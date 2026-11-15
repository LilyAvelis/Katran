"""Offline, local-only Unicode 16 semantic emoji search.

The model and tokenizer are supplied as local files. Nothing imports a hub
client, makes network requests, or records queries.
"""
from __future__ import annotations

import argparse
from dataclasses import dataclass
import hashlib
import json
import math
import os
from pathlib import Path
import statistics
import sys
import time
from typing import Iterable

# PowerShell consoles can default to a legacy code page that cannot print emoji.
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8")
if hasattr(sys.stderr, "reconfigure"):
    sys.stderr.reconfigure(encoding="utf-8")

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
UNICODE = ROOT / "assets" / "unicode" / "emoji-test.txt"
CACHE = HERE / "cache"
CATALOGUE_SHA256 = "24f0c534e86cf142e2496953e8f0e46a3e702392911eddcd29c6cced85139697"
DESCRIPTOR_VERSION = 1
MAX_TOKENS = 128


@dataclass(frozen=True)
class Emoji:
    text: str
    name: str
    group: str
    subgroup: str

    def passage(self) -> str:
        return f"passage: {self.name}; {self.subgroup.replace('-', ' ')}; {self.group}"


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def catalogue(path: Path = UNICODE) -> list[Emoji]:
    if sha256(path) != CATALOGUE_SHA256:
        raise ValueError("Unicode source hash does not match Unicode 16.0")
    items = []
    group = subgroup = ""
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.startswith("# group: "):
            group = line.removeprefix("# group: ")
            continue
        if line.startswith("# subgroup: "):
            subgroup = line.removeprefix("# subgroup: ")
            continue
        if line.startswith("#") or "#" not in line or ";" not in line:
            continue
        fields, comment = line.split("#", 1)
        cps, qualification = fields.split(";", 1)
        if qualification.strip() != "fully-qualified":
            continue
        parts = comment.strip().split(" ", 2)
        if len(parts) != 3 or not group or not subgroup:
            raise ValueError("Malformed official Unicode entry")
        text = "".join(chr(int(cp, 16)) for cp in cps.split())
        items.append(Emoji(text, parts[2], group, subgroup))
    if len(items) != 3781 or len({e.text for e in items}) != 3781:
        raise ValueError(f"Expected 3781 unique fully-qualified entries, got {len(items)}")
    return items


def load_annotations(directory: Path):
    """Read local official CLDR annotations; match emoji ignoring only VS16."""
    annotations = {}
    files = []
    for lang in ("en", "ru"):
        merged = {}
        for kind, root in (("full", "annotations"), ("derived-full", "annotationsDerived")):
            path = directory / f"cldr-annotations-{kind}-{lang}.json"
            payload = json.loads(path.read_text(encoding="utf-8"))
            if payload[root]["identity"]["language"] != lang:
                raise ValueError(f"Unexpected CLDR language in {path}")
            for emoji, data in payload[root]["annotations"].items():
                normalized = emoji.replace("\ufe0f", "")
                if normalized in merged and merged[normalized] != data:
                    raise ValueError(f"Ambiguous CLDR emoji key: {emoji}")
                merged[normalized] = data
            files.append(path)
        annotations[lang] = merged
    return annotations, files


def cldr_passages(entry: Emoji, annotations: dict) -> list[str]:
    result = []
    key = entry.text.replace("\ufe0f", "")
    for lang in ("en", "ru"):
        data = annotations[lang].get(key, {})
        title = data.get("tts", [entry.name])[0]
        words = data.get("default", [])
        if not isinstance(title, str) or not isinstance(words, list) or not all(isinstance(x, str) for x in words):
            raise ValueError("Invalid CLDR annotation")
        result.append(f"passage: {title}; {', '.join(words)}; {entry.name}; {entry.subgroup.replace('-', ' ')}; {entry.group}")
    return result


def fingerprint(model: Path, tokenizer: Path, cldr_files=()) -> dict:
    # Include separately stored ONNX tensor files when present. Model files
    # must be local and immutable for a cache to remain valid.
    external = sorted(p for p in model.parent.iterdir() if p.is_file() and
                      (p.name == model.name + "_data" or p.name == model.name + ".data"))
    files = [model, tokenizer, *external]
    result = {"unicode_sha256": CATALOGUE_SHA256, "descriptor_version": 2 if cldr_files else DESCRIPTOR_VERSION,
              "max_tokens": MAX_TOKENS,
              "model_files": [{"name": p.name, "sha256": sha256(p)} for p in files]}
    if cldr_files:
        result["locales"] = ["en", "ru"]
        result["cldr_files"] = [{"name": p.name, "sha256": sha256(p)} for p in cldr_files]
    return result


class Encoder:
    def __init__(self, model: Path, tokenizer: Path, threads: int = 0):
        import numpy as np
        import onnxruntime as ort
        from tokenizers import Tokenizer

        self.np = np
        self.tokenizer = Tokenizer.from_file(str(tokenizer))
        pad = self.tokenizer.token_to_id("<pad>")
        if pad is None:
            raise ValueError("XLM-R tokenizer does not contain <pad>")
        self.tokenizer.enable_truncation(max_length=MAX_TOKENS)
        self.tokenizer.enable_padding(pad_id=pad, pad_token="<pad>")
        options = ort.SessionOptions()
        if threads:
            options.intra_op_num_threads = threads
        self.session = ort.InferenceSession(str(model), sess_options=options,
                                           providers=["CPUExecutionProvider"])
        self.inputs = {x.name: x for x in self.session.get_inputs()}
        if not {"input_ids", "attention_mask"}.issubset(self.inputs):
            raise ValueError(f"Unexpected model inputs: {list(self.inputs)}")
        unknown = set(self.inputs) - {"input_ids", "attention_mask", "token_type_ids"}
        if unknown:
            raise ValueError(f"Unsupported model inputs: {sorted(unknown)}")
        self.output = self.session.get_outputs()[0].name

    def encode(self, texts: list[str]):
        np = self.np
        encoded = self.tokenizer.encode_batch(texts, add_special_tokens=True)
        ids = np.asarray([e.ids for e in encoded], dtype=np.int64)
        mask = np.asarray([e.attention_mask for e in encoded], dtype=np.int64)
        feeds = {"input_ids": ids, "attention_mask": mask}
        if "token_type_ids" in self.inputs:
            feeds["token_type_ids"] = np.zeros_like(ids)
        for name, array in feeds.items():
            if self.inputs[name].type == "tensor(int32)":
                feeds[name] = array.astype(np.int32)
        hidden = self.session.run([self.output], feeds)[0]
        if hidden.ndim == 3:
            weights = mask[..., None].astype(np.float32)
            vectors = (hidden.astype(np.float32) * weights).sum(axis=1) / np.maximum(weights.sum(axis=1), 1)
        elif hidden.ndim == 2:
            vectors = hidden.astype(np.float32)
        else:
            raise ValueError(f"Unexpected embedding output shape: {hidden.shape}")
        if vectors.shape != (len(texts), 384):
            raise ValueError(f"Expected 384-dimensional E5 embeddings, got {vectors.shape}")
        norms = np.linalg.norm(vectors, axis=1, keepdims=True)
        if not np.all(np.isfinite(vectors)) or np.any(norms == 0):
            raise ValueError("Model produced invalid embeddings")
        return vectors / norms


def build(encoder: Encoder, entries: list[Emoji], ident: dict, batch: int, cache: Path = CACHE, annotations=None):
    np = encoder.np
    cache.mkdir(parents=True, exist_ok=True)
    start = time.perf_counter()
    chunks = []
    texts = [text for item in entries for text in (cldr_passages(item, annotations) if annotations is not None else [item.passage()])]
    for index in range(0, len(texts), batch):
        chunks.append(encoder.encode(texts[index:index + batch]))
        if index % (batch * 20) == 0:
            print(f"Indexing {min(index + batch, len(texts))}/{len(texts)}", file=sys.stderr, flush=True)
    matrix = np.concatenate(chunks, axis=0).astype(np.float32)
    if annotations is not None:
        matrix = matrix.reshape(len(entries), 2, 384)
    temp = cache / "passages.tmp.npy"
    np.save(temp, matrix, allow_pickle=False)
    os.replace(temp, cache / "passages.npy")
    meta_temp = cache / "manifest.tmp.json"
    meta_temp.write_text(json.dumps({"identity": ident, "count": len(entries), "dimensions": 384,
                                     "embedding_sha256": sha256(cache / "passages.npy")}, indent=2), encoding="utf-8")
    os.replace(meta_temp, cache / "manifest.json")
    return matrix, time.perf_counter() - start


def load_cache(np, entries: list[Emoji], ident: dict, cache: Path = CACHE):
    path, meta_path = cache / "passages.npy", cache / "manifest.json"
    if not path.is_file() or not meta_path.is_file():
        raise ValueError("Index not found; run the build command first")
    meta = json.loads(meta_path.read_text(encoding="utf-8"))
    if meta.get("identity") != ident or meta.get("count") != len(entries):
        raise ValueError("Cached index does not match model, tokenizer, or Unicode data; rebuild it")
    if sha256(path) != meta.get("embedding_sha256"):
        raise ValueError("Cached embeddings failed integrity check; rebuild them")
    matrix = np.load(path, allow_pickle=False)
    expected_shape = (len(entries), 2, 384) if ident.get("locales") else (len(entries), 384)
    if matrix.shape != expected_shape or matrix.dtype != np.float32 or not np.isfinite(matrix).all():
        raise ValueError("Cached embeddings are invalid")
    return matrix


def search(encoder: Encoder, entries: list[Emoji], matrix, query: str, offset: int = 0, limit: int = 10):
    np = encoder.np
    if not query.strip() or not (0 <= offset < len(entries)) or not (1 <= limit <= 100):
        raise ValueError("Query must be non-empty; offset and page size must be in range")
    begin = time.perf_counter()
    q = encoder.encode(["query: " + query.strip()])[0]
    embedded = time.perf_counter()
    scores = matrix @ q
    if scores.ndim == 2:
        scores = scores.max(axis=1)
    # Stable tie-breaking uses official source order, preventing page overlap.
    order = np.argsort(-scores, kind="stable")[offset:offset + limit]
    results = [{"rank": offset + rank + 1, "emoji": entries[i].text, "name": entries[i].name,
                "group": entries[i].group, "subgroup": entries[i].subgroup,
                "score": float(scores[i])} for rank, i in enumerate(order)]
    done = time.perf_counter()
    return {"results": results, "offset": offset,
            "next_offset": offset + len(results) if offset + len(results) < len(entries) else None,
            "timing_ms": {"embed": (embedded - begin) * 1000, "rank": (done - embedded) * 1000,
                          "total": (done - begin) * 1000}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model", type=Path, required=True, help="Local ONNX model file")
    parser.add_argument("--tokenizer", type=Path, required=True, help="Local tokenizer.json")
    parser.add_argument("--threads", type=int, default=0, help="ONNX CPU threads; 0 is runtime default")
    parser.add_argument("--cldr", type=Path, help="Optional local CLDR annotation directory (EN/RU, full/derived)")
    sub = parser.add_subparsers(dest="command", required=True)
    create = sub.add_parser("build"); create.add_argument("--batch", type=int, default=32)
    q = sub.add_parser("search"); q.add_argument("query"); q.add_argument("--offset", type=int, default=0); q.add_argument("--limit", type=int, default=10)
    bench = sub.add_parser("bench"); bench.add_argument("--runs", type=int, default=10)
    sub.add_parser("eval")
    args = parser.parse_args()
    if not args.model.is_file() or not args.tokenizer.is_file():
        parser.error("Model and tokenizer must already exist locally")
    if args.threads < 0:
        parser.error("Threads cannot be negative")
    entries = catalogue()
    start = time.perf_counter()
    annotations, cldr_files = load_annotations(args.cldr) if args.cldr else (None, ())
    cache = HERE / "cache-cldr" if annotations is not None else CACHE
    ident = fingerprint(args.model, args.tokenizer, cldr_files)
    encoder = Encoder(args.model, args.tokenizer, args.threads)
    init_ms = (time.perf_counter() - start) * 1000
    if args.command == "build":
        if not 1 <= args.batch <= 256: parser.error("Batch must be 1..256")
        matrix, seconds = build(encoder, entries, ident, args.batch, cache, annotations)
        print(json.dumps({"entries": len(entries), "dimensions": matrix.shape[-1], "variants": matrix.shape[1] if matrix.ndim == 3 else 1, "build_seconds": seconds,
                          "index_bytes": (cache / "passages.npy").stat().st_size, "init_ms": init_ms}, ensure_ascii=False, indent=2))
        return
    cache_start = time.perf_counter()
    matrix = load_cache(encoder.np, entries, ident, cache)
    cache_load_ms = (time.perf_counter() - cache_start) * 1000
    if args.command == "search":
        result = search(encoder, entries, matrix, args.query, args.offset, args.limit)
        result["init_ms"] = init_ms
        result["cache_load_ms"] = cache_load_ms
    else:
        cases = json.loads((HERE / "evaluation.json").read_text(encoding="utf-8"))
        if args.command == "eval":
            rows = []
            for case in cases:
                found = search(encoder, entries, matrix, case["query"])
                ranks = [item["rank"] for item in found["results"] if item["emoji"] in case["expected"]]
                rows.append({"query": case["query"], "expected": case["expected"], "hit_at_10": bool(ranks),
                             "first_hit": min(ranks) if ranks else None, "top": found["results"][:5]})
            result = {"cases": rows, "hits_at_10": sum(row["hit_at_10"] for row in rows), "total": len(rows), "init_ms": init_ms}
        else:
            if not 1 <= args.runs <= 1000: parser.error("Runs must be 1..1000")
            search(encoder, entries, matrix, cases[0]["query"])  # warm-up
            timings = []
            for i in range(args.runs):
                timings.append(search(encoder, entries, matrix, cases[i % len(cases)]["query"])["timing_ms"])
            def stats(metric):
                values = sorted(item[metric] for item in timings)
                return {"min": values[0], "median": statistics.median(values),
                        "p95": values[math.ceil(0.95 * len(values)) - 1], "max": values[-1]}
            result = {"provider": "CPUExecutionProvider", "runs": args.runs, "init_ms": init_ms,
                      "cache_load_ms": cache_load_ms,
                      "warm_ms": {metric: stats(metric) for metric in ("embed", "rank", "total")}}
    print(json.dumps(result, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, ImportError) as error:
        print(f"Error: {error}", file=sys.stderr)
        sys.exit(2)
