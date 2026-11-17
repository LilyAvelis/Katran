"""Local bounded folder-name embeddings; JSONL protocol, no filesystem traversal."""
import hashlib
import json
import os
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

def run():
    import numpy as np
    import onnxruntime as ort
    from tokenizers import Tokenizer
    model, tokenizer_file = sys.argv[1:3]
    tokenizer = Tokenizer.from_file(tokenizer_file)
    pad = tokenizer.token_to_id("<pad>")
    if pad is None:
        raise ValueError("tokenizer")
    tokenizer.enable_truncation(max_length=128)
    tokenizer.enable_padding(pad_id=pad, pad_token="<pad>")
    opts = ort.SessionOptions()
    opts.intra_op_num_threads = 4
    opts.inter_op_num_threads = 1
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
        if not isinstance(q,str) or not q.strip() or len(q)>MAX_QUERY or not isinstance(paths,list) or len(paths)>MAX_ITEMS or not all(isinstance(p,str) and p and "\0" not in p for p in paths) or type(limit) is not int or not 1<=limit<=100:
            raise ValueError("request")
        key = hashlib.sha256(json.dumps(paths, ensure_ascii=True, separators=(",", ":")).encode("ascii")).digest()
        if key != cached_key:
            cache, matrix = refresh_cache(paths, cache, encode, np)
            cached_key = key
        indices = []
        if matrix is not None:
            scores = matrix @ encode(["query: " + q.strip()])[0]
            indices = [int(i) for i in np.argsort(-scores, kind="stable")[:limit]]
        emit({"id":request["id"], "indices":indices})

if __name__ == "__main__":
    try:
        run()
    except Exception:
        # Never print paths, prompts, exception details, or tracebacks.
        try:
            emit({"type":"error", "code":"folder_ai_unavailable"})
        except Exception:
            pass
        sys.exit(1)
