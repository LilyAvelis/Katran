"""Local-only, bounded JSONL semantic search worker. Protocol: WORKER.md."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys

import semantic_search as search

HERE = Path(__file__).resolve().parent
MAX_LINE = 16384  # UTF-8 bytes excluding newline
MAX_QUERY = 512  # Unicode code points
MAX_ID = 128


def emit(out, value):
    data = (json.dumps(value, ensure_ascii=True, allow_nan=False, separators=(",", ":")) + "\n").encode("ascii")
    out.write(data)
    out.flush()


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate key")
        result[key] = value
    return result


def valid_id(value):
    return (isinstance(value, str) and 0 < len(value) <= MAX_ID and not any(0xD800 <= ord(c) <= 0xDFFF for c in value)) or (type(value) is int and -(2**53 - 1) <= value <= 2**53 - 1)


def read_line(inp):
    """Read at most one bounded frame, discarding excess without retaining it."""
    raw = inp.readline(MAX_LINE + 2)
    if not raw:
        return None, False
    too_long = len(raw.rstrip(b"\n")) > MAX_LINE or (len(raw) == MAX_LINE + 2 and not raw.endswith(b"\n"))
    if too_long:
        while not raw.endswith(b"\n"):
            raw = inp.readline(MAX_LINE + 2)
            if not raw:
                break
        return b"", True
    return raw, False


def serve(inp, out, entries, search_one):
    emit(out, {"type": "ready", "protocol": 1, "count": len(entries)})
    while True:
        raw, too_long = read_line(inp)
        if raw is None:
            return
        if too_long:
            emit(out, {"id": None, "ok": False, "error": {"code": "frame_too_large"}})
            continue
        request_id = None
        try:
            if not raw.endswith(b"\n"):
                raise ValueError("unterminated frame")
            request = json.loads(raw.decode("utf-8", errors="strict"), object_pairs_hook=unique_object,
                                 parse_constant=lambda _: (_ for _ in ()).throw(ValueError("non-finite number")))
            if not isinstance(request, dict):
                raise ValueError("request must be an object")
            if valid_id(request.get("id")):
                request_id = request["id"]
            else:
                raise ValueError("invalid id")
            op = request.get("op")
            if op == "shutdown" and set(request) == {"id", "op"}:
                emit(out, {"id": request_id, "ok": True, "result": {"shutdown": True}})
                return
            if op != "search" or not set(request).issubset({"id", "op", "query", "offset", "limit"}):
                raise ValueError("invalid operation or fields")
            query, offset, limit = request.get("query"), request.get("offset", 0), request.get("limit", 10)
            if (not isinstance(query, str) or not query.strip() or len(query) > MAX_QUERY
                    or any(0xD800 <= ord(c) <= 0xDFFF for c in query)
                    or type(offset) is not int or not 0 <= offset < len(entries)
                    or type(limit) is not int or not 1 <= limit <= 100):
                raise ValueError("invalid search parameters")
        except (ValueError, UnicodeError, RecursionError):
            emit(out, {"id": request_id, "ok": False, "error": {"code": "invalid_request"}})
            continue
        try:
            result = search_one(query, offset, limit)
            emit(out, {"id": request_id, "ok": True, "result": result})
        except Exception:
            # Never include query, exception text, or traceback in the protocol.
            emit(out, {"id": request_id, "ok": False, "error": {"code": "search_failed"}})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model", type=Path, default=HERE / "model/onnx/model.onnx")
    parser.add_argument("--tokenizer", type=Path, default=HERE / "model/onnx/tokenizer.json")
    parser.add_argument("--cldr", type=Path, default=HERE / "cldr")
    parser.add_argument("--cache", type=Path, default=HERE / "cache-cldr")
    parser.add_argument("--threads", type=int, default=8)
    args = parser.parse_args()
    try:
        if not 0 <= args.threads <= 256:
            raise ValueError("invalid threads")
        entries = search.catalogue()
        _, cldr_files = search.load_annotations(args.cldr)
        identity = search.fingerprint(args.model, args.tokenizer, cldr_files)
        encoder = search.Encoder(args.model, args.tokenizer, args.threads)
        matrix = search.load_cache(encoder.np, entries, identity, args.cache)
    except Exception:
        emit(sys.stdout.buffer, {"type": "startup_error", "protocol": 1, "error": {"code": "initialization_failed"}})
        return 2
    serve(sys.stdin.buffer, sys.stdout.buffer, entries,
          lambda query, offset, limit: search.search(encoder, entries, matrix, query, offset, limit))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
