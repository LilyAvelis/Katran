"""Local bounded folder-name embeddings; JSONL protocol, no filesystem traversal."""
import hashlib
import json
import math
import os
import sqlite3
import struct
import sys

MAX_FRAME = 4 * 1024 * 1024
MAX_ITEMS = 8192
MAX_QUERY = 512

def emit(obj):
    payload = (json.dumps(obj, ensure_ascii=True, allow_nan=False, separators=(",", ":")) + "\n").encode("ascii")
    if len(payload) > MAX_FRAME:
        raise ValueError("output limit")
    sys.stdout.buffer.write(payload)
    sys.stdout.buffer.flush()

def descriptor(path):
    normalized = path.replace("\\", "/").rstrip("/")
    name = normalized.rsplit("/", 1)[-1]
    return "passage: folder name: " + name[:512] + " path: " + normalized[-1024:]

def refresh_cache(paths, cache, encode, np):
    # Keep only current paths and own each small vector independently: a view
    # into a batch would retain all evicted entries of that batch in memory.
    current = {path: cache[path] for path in paths if path in cache}
    missing = list(dict.fromkeys(path for path in paths if path not in current))
    for start in range(0, len(missing), 64):
        batch = missing[start:start + 64]
        vectors = encode([descriptor(path) for path in batch])
        if len(vectors) != len(batch):
            raise ValueError("vector count")
        for path, vector in zip(batch, vectors):
            current[path] = vector.copy()
    return current, np.stack([current[path] for path in paths], axis=0) if paths else None

CACHE_VERSION = 1
DESCRIPTOR_VERSION = "folder-name-path-v1"
VECTOR_DIM = 384
MAX_TOKENS = 128

def file_digest(path):
    digest = hashlib.sha256()
    with open(path, "rb") as file:
        for block in iter(lambda: file.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()

def cache_identity(model, tokenizer, mode, ort_version):
    model_files = {"model": file_digest(model), "tokenizer": file_digest(tokenizer)}
    for suffix in ("_data", ".data"):
        sidecar = model + suffix
        if os.path.isfile(sidecar):
            model_files[suffix] = file_digest(sidecar)
    return json.dumps({"version": CACHE_VERSION, "model": model_files, "descriptor": DESCRIPTOR_VERSION,
                       "tokens": MAX_TOKENS, "dim": VECTOR_DIM, "provider": mode, "ort": ort_version,
                       "pooling": "attention-mask-mean-l2-v1", "passage_prefix": "passage: ", "query_prefix": "query: "},
                      sort_keys=True, separators=(",", ":"))

def cache_key(path):
    return hashlib.sha256(descriptor(path).encode("utf-8")).hexdigest()

def valid_vector(blob):
    if not isinstance(blob, bytes) or len(blob) != VECTOR_DIM * 4:
        return None
    values = struct.unpack("<384f", blob)
    if not all(math.isfinite(value) for value in values):
        return None
    norm2 = sum(value * value for value in values)
    return values if abs(norm2 - 1.0) <= 0.002 else None

class DiskCache:
    def __init__(self, path, identity):
        self.path = path
        self.identity = identity

    def load(self, paths, np):
        if len(paths) > MAX_ITEMS:
            return {}, "unavailable"
        if not self.path:
            return {}, "disabled"
        if not os.path.isabs(self.path):
            return {}, "unavailable"
        if not os.path.exists(self.path):
            return {}, "empty"
        try:
            connection = sqlite3.connect(self.path, timeout=0.1)
            try:
                meta = connection.execute("SELECT CASE WHEN length(identity)<=8192 THEN identity END FROM metadata WHERE id=1").fetchone()
                if not meta or meta[0] != self.identity:
                    return {}, "identity_mismatch"
                # Bound both row count and BLOB size before Python receives data.
                rows = connection.execute("SELECT CASE WHEN length(key)=64 THEN key END, CASE WHEN length(vector)=1536 THEN vector END, CASE WHEN length(checksum)=64 THEN checksum END FROM embeddings LIMIT ?", (MAX_ITEMS + 1,)).fetchall()
                if len(rows) > MAX_ITEMS:
                    return {}, "corrupt"
                by_key = {}
                for key, blob, checksum in rows:
                    if not isinstance(key, str) or not isinstance(checksum, str) or valid_vector(blob) is None or hashlib.sha256(key.encode("ascii") + blob).hexdigest() != checksum:
                        return {}, "corrupt"
                    by_key[key] = np.asarray(struct.unpack("<384f", blob), dtype=np.float32)
                return {path: by_key[cache_key(path)] for path in paths if cache_key(path) in by_key}, "loaded"
            finally:
                connection.close()
        except (OSError, sqlite3.Error, ValueError, TypeError, UnicodeError):
            return {}, "unavailable"

    def save(self, paths, vectors):
        if len(paths) > MAX_ITEMS:
            return False
        if not self.path or not os.path.isabs(self.path):
            return False
        try:
            os.makedirs(os.path.dirname(self.path), exist_ok=True)
            connection = sqlite3.connect(self.path, timeout=0.1)
            try:
                connection.execute("BEGIN IMMEDIATE")
                connection.execute("DROP TABLE IF EXISTS metadata")
                connection.execute("DROP TABLE IF EXISTS embeddings")
                connection.execute("CREATE TABLE metadata(id INTEGER PRIMARY KEY CHECK(id=1), identity TEXT NOT NULL)")
                connection.execute("CREATE TABLE embeddings(key TEXT PRIMARY KEY, vector BLOB NOT NULL, checksum TEXT NOT NULL)")
                connection.execute("INSERT INTO metadata(id,identity) VALUES(1,?)", (self.identity,))
                rows = []
                for path in dict.fromkeys(paths):
                    key = cache_key(path)
                    blob = struct.pack("<384f", *vectors[path])
                    if valid_vector(blob) is None:
                        raise ValueError("invalid embedding")
                    rows.append((key, blob, hashlib.sha256(key.encode("ascii") + blob).hexdigest()))
                connection.executemany("INSERT OR REPLACE INTO embeddings(key,vector,checksum) VALUES(?,?,?)", rows)
                connection.commit()
                return True
            except (sqlite3.Error, ValueError, TypeError, OverflowError, struct.error):
                connection.rollback()
                return False
            finally:
                connection.close()
        except (OSError, sqlite3.Error, ValueError):
            return False

def run():
    import numpy as np
    import onnxruntime as ort
    from tokenizers import Tokenizer
    model, tokenizer_file, mode = sys.argv[1:4]
    if mode not in ("cpu", "gpu"):
        raise ValueError("mode")
    tokenizer = Tokenizer.from_file(tokenizer_file)
    pad = tokenizer.token_to_id("<pad>")
    if pad is None:
        raise ValueError("tokenizer")
    tokenizer.enable_truncation(max_length=MAX_TOKENS)
    tokenizer.enable_padding(pad_id=pad, pad_token="<pad>")
    opts = ort.SessionOptions()
    opts.intra_op_num_threads = 4
    opts.inter_op_num_threads = 1
    if mode == "gpu":
        if hasattr(ort, "preload_dlls"):
            ort.preload_dlls(directory="")
        if "CUDAExecutionProvider" not in ort.get_available_providers():
            raise ValueError("CUDA unavailable")
        # E5 keeps Gather/Unsqueeze shape work on CPU; neural kernels use CUDA.
        # Reject a whole-session CPU fallback, not legitimate shape nodes.
        session = ort.InferenceSession(model, sess_options=opts, providers=["CUDAExecutionProvider"])
        session.disable_fallback()
        if "CUDAExecutionProvider" not in session.get_providers():
            raise ValueError("CUDA inactive")
    else:
        session = ort.InferenceSession(model, sess_options=opts, providers=["CPUExecutionProvider"])
    inputs = {x.name: x for x in session.get_inputs()}
    if not {"input_ids", "attention_mask"}.issubset(inputs) or set(inputs) - {"input_ids", "attention_mask", "token_type_ids"}:
        raise ValueError("model inputs")
    output = session.get_outputs()[0].name
    def encode(texts):
        tokens = tokenizer.encode_batch(texts, add_special_tokens=True)
        ids = np.asarray([e.ids for e in tokens], dtype=np.int64)
        mask = np.asarray([e.attention_mask for e in tokens], dtype=np.int64)
        feeds = {"input_ids": ids, "attention_mask": mask}
        if "token_type_ids" in inputs:
            feeds["token_type_ids"] = np.zeros_like(ids)
        for name, data in feeds.items():
            if inputs[name].type == "tensor(int32)":
                feeds[name] = data.astype(np.int32)
        hidden = session.run([output], feeds)[0]
        if hidden.ndim == 3:
            weights = mask[..., None].astype(np.float32)
            vectors = (hidden.astype(np.float32) * weights).sum(axis=1) / np.maximum(weights.sum(axis=1), 1)
        elif hidden.ndim == 2:
            vectors = hidden.astype(np.float32)
        else:
            raise ValueError("model output")
        if vectors.shape != (len(texts), 384) or not np.isfinite(vectors).all():
            raise ValueError("vectors")
        norms = np.linalg.norm(vectors, axis=1, keepdims=True)
        if np.any(norms == 0):
            raise ValueError("vectors")
        return vectors / norms
    cache_path = sys.argv[4] if len(sys.argv) > 4 else None
    cache_error = False
    try:
        identity = cache_identity(model, tokenizer_file, mode, ort.__version__) if cache_path else ""
    except (OSError, ValueError):
        identity = ""
        cache_error = True
    disk = DiskCache(None if cache_error else cache_path, identity)
    loaded_once = False
    emit({"type":"ready", "protocol":1})
    cached_key = None
    cache = {}
    matrix = None
    while True:
        line = sys.stdin.buffer.readline(MAX_FRAME + 1)
        if not line:
            break
        if len(line) > MAX_FRAME or not line.endswith(b"\n"):
            raise ValueError("frame")
        request = json.loads(line)
        q, paths, limit = request["query"], request["paths"], request["limit"]
        if not isinstance(q,str) or not q.strip() or len(q)>MAX_QUERY or not isinstance(paths,list) or len(paths)>MAX_ITEMS or not all(isinstance(p,str) and p and "\0" not in p for p in paths) or type(limit) is not int or not 1<=limit<=MAX_ITEMS:
            raise ValueError("request")
        key = hashlib.sha256(json.dumps(paths, ensure_ascii=True, separators=(",", ":")).encode("ascii")).digest()
        loaded = 0
        status = "memory"
        persisted = False
        if not loaded_once:
            cache, status = disk.load(paths, np)
            loaded = len(cache)
            loaded_once = True
        reused = sum(path in cache for path in paths)
        encoded = 0
        if key != cached_key:
            encoded = len(set(paths) - set(cache))
            cache, matrix = refresh_cache(paths, cache, encode, np)
            persisted = disk.save(paths, cache)
            if cache_error or (disk.path and not persisted):
                status = "unavailable"
            elif not disk.path:
                status = "disabled"
            cached_key = key
        indices = []
        if matrix is not None:
            scores = matrix @ encode(["query: " + q.strip()])[0]
            indices = [int(i) for i in np.argsort(-scores, kind="stable")[:limit]]
        emit({"id":request["id"], "indices":indices,
              "cache":{"loaded":loaded,"reused":reused,"encoded":encoded,"persisted":persisted,"status":status}})

if __name__ == "__main__":
    try:
        run()
    except Exception:
        # Never print paths, prompts, exception details, or tracebacks.
        try:
            code = "folder_gpu_unavailable" if len(sys.argv) > 3 and sys.argv[3] == "gpu" else "folder_ai_unavailable"
            emit({"type":"error", "code":code})
        except Exception:
            pass
        sys.exit(1)
