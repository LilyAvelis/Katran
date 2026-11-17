"""Run bounded local functional checks with synthetic input; no folder index is read."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

class ModelMissing(Exception):
    pass


MODEL_SHA = "ca456c06b3a9505ddfd9131408916dd79290368331e7d76bb621f1cba6bc8665"
TOKENIZER_SHA = "0b44a9d7b51c3c62626640cda0e2c2f70fdacdc25bbbd68038369d14ebdf4c39"


def check_hash(path, expected):
    digest = hashlib.sha256()
    with open(path, "rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    if digest.hexdigest() != expected:
        raise ValueError("model or tokenizer checksum mismatch")


def legacy_files(profile):
    root = profile / "emoji-ai-runtime"
    base = root / "local-experiments" / "emoji-ai"
    model, tokenizer = base / "model/onnx/model.onnx", base / "model/onnx/tokenizer.json"
    check_hash(model, MODEL_SHA)
    check_hash(tokenizer, TOKENIZER_SHA)
    if not (base / ".venv/Scripts/python.exe").is_file():
        raise ValueError("Emoji Python runtime is unavailable")
    manifest = json.loads((base / "katran-e-runtime.json").read_text(encoding="utf-8-sig"))
    if manifest.get("version") != 1 or manifest.get("protocol") != 1:
        raise ValueError("Emoji runtime manifest is incompatible")
    required = {
        "local-experiments/emoji-ai/worker.py", "local-experiments/emoji-ai/semantic_search.py",
        "local-experiments/emoji-ai/model/onnx/model.onnx", "local-experiments/emoji-ai/model/onnx/tokenizer.json",
        "local-experiments/emoji-ai/cache-cldr/manifest.json", "local-experiments/emoji-ai/cache-cldr/passages.npy",
        "assets/unicode/emoji-test.txt",
        *(f"local-experiments/emoji-ai/cldr/cldr-annotations-{kind}-{language}.json" for kind in ("full", "derived-full") for language in ("en", "ru")),
    }
    files = manifest.get("files")
    optional = {"local-experiments/emoji-ai/model/onnx/model.onnx_data", "local-experiments/emoji-ai/model/onnx/model.onnx.data"}
    if not isinstance(files, dict) or not required.issubset(files) or set(files) - required - optional:
        raise ValueError("Emoji runtime manifest is incomplete")
    for relative, expected in files.items():
        path = (root / relative).resolve(strict=True)
        if not path.is_relative_to(root.resolve(strict=True)):
            raise ValueError("Emoji runtime input is outside its directory")
        check_hash(path, expected)
    return model, tokenizer


def model_files(profile):
    runtime = profile / "folder-ai-runtime"
    if runtime.exists():
        manifest = json.loads((runtime / "manifest.json").read_text(encoding="utf-8-sig"))
        if manifest != {"version": 1, "backend": "onnxruntime-cpu", "onnxruntime": "1.23.2"}:
            raise ValueError("Folder runtime manifest is incompatible")
        if not (runtime / ".venv/Scripts/python.exe").is_file():
            raise ValueError("Folder Python runtime is unavailable")
        model, tokenizer = runtime / "model/model.onnx", runtime / "model/tokenizer.json"
        check_hash(model, MODEL_SHA)
        check_hash(tokenizer, TOKENIZER_SHA)
        return model, tokenizer
    legacy = profile / "emoji-ai-runtime"
    if legacy.exists():
        return legacy_files(profile)
    raise ModelMissing("Install Folder or Emoji AI to provide a model")


def emoji(profile):
    legacy_files(profile)
    base = profile / "emoji-ai-runtime" / "local-experiments" / "emoji-ai"
    request = [{"id": 1, "op": "search", "query": "cow", "offset": 0, "limit": 1}, {"id": 2, "op": "shutdown"}]
    result = subprocess.run([sys.executable, str(base / "worker.py")], input="".join(json.dumps(x) + "\n" for x in request), capture_output=True, text=True, encoding="utf-8", timeout=120, cwd=str(base))
    frames = [json.loads(line) for line in result.stdout.splitlines()]
    if result.returncode != 0 or len(frames) < 2 or frames[0].get("type") != "ready" or frames[0].get("protocol") != 1 or not frames[1].get("ok") or not frames[1].get("result", {}).get("results"):
        raise ValueError("Emoji AI worker failed its synthetic search")
    print("Emoji AI: synthetic worker search succeeded.")


def folder(profile, gpu, job, bundle):
    import numpy as np
    import onnxruntime as ort
    from tokenizers import Tokenizer
    model, tokenizer_file = model_files(profile)
    if ort.__version__ != "1.23.2":
        raise ValueError("ONNX Runtime version is incompatible")
    options = ort.SessionOptions()
    options.intra_op_num_threads = 4
    with tempfile.TemporaryDirectory(prefix="gpu-check-", dir=job) as tmp:
        if gpu:
            ort.preload_dlls(directory="")
            if "CUDAExecutionProvider" not in ort.get_available_providers():
                raise ValueError("CUDA execution provider is unavailable")
            options.enable_profiling = True
            options.profile_file_prefix = str(Path(tmp) / "profile")
        provider = "CUDAExecutionProvider" if gpu else "CPUExecutionProvider"
        session = ort.InferenceSession(str(model), sess_options=options, providers=[provider])
        if gpu:
            session.disable_fallback()
        if provider not in session.get_providers():
            raise ValueError("Requested execution provider is inactive")
        tokenizer = Tokenizer.from_file(str(tokenizer_file))
        tokenizer.enable_truncation(max_length=128)
        tokenizer.enable_padding(pad_id=tokenizer.token_to_id("<pad>"), pad_token="<pad>")
        encoded = tokenizer.encode("query: synthetic folder")
        ids = np.asarray([encoded.ids], dtype=np.int64)
        feeds = {"input_ids": ids, "attention_mask": np.asarray([encoded.attention_mask], dtype=np.int64)}
        for item in session.get_inputs():
            if item.name == "token_type_ids":
                feeds[item.name] = np.zeros_like(ids)
            if item.name in feeds and item.type == "tensor(int32)":
                feeds[item.name] = feeds[item.name].astype(np.int32)
        value = session.run(None, feeds)[0]
        if value.size == 0 or not np.isfinite(value).all() or value.shape[-1] != 384:
            raise ValueError("Synthetic inference returned an invalid vector")
        if gpu:
            records = json.loads(Path(session.end_profiling()).read_text(encoding="utf-8"))
            count = sum(1 for event in records if event.get("cat") == "Node" and event.get("args", {}).get("provider") == "CUDAExecutionProvider")
            if count == 0:
                raise ValueError("No model operation executed on CUDA")
            print("Folder GPU: synthetic inference succeeded; CUDA operations:", count)
        else:
            print("Folder CPU: synthetic inference succeeded.")
        request = {"id": 1, "query": "synthetic folder", "paths": ["C:/synthetic/folder-one", "C:/synthetic/folder-two"], "limit": 2}
        mode = "gpu" if gpu else "cpu"
        worker = bundle / "scripts/folder-semantic/worker.py"
        completed = subprocess.run([sys.executable, str(worker), str(model), str(tokenizer_file), mode, str(Path(tmp) / "index.sqlite3")], input=json.dumps(request) + "\n", capture_output=True, text=True, encoding="utf-8", timeout=120)
        frames = [json.loads(line) for line in completed.stdout.splitlines()]
        if completed.returncode != 0 or len(frames) < 2 or frames[0].get("type") != "ready" or frames[0].get("protocol") != 1 or sorted(frames[1].get("indices", [])) != [0, 1]:
            raise ValueError("Folder AI worker failed its synthetic search")
        print("Folder AI: synthetic worker search succeeded.")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("component", choices=("emoji", "folder", "folder_gpu"))
    parser.add_argument("--profile", type=Path, required=True)
    parser.add_argument("--job", type=Path, required=True)
    parser.add_argument("--bundle", type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.component == "emoji":
            emoji(args.profile)
        else:
            folder(args.profile, args.component == "folder_gpu", args.job, args.bundle)
    except ModelMissing as exc:
        print("Verification needs a model:", str(exc))
        return 2
    except Exception as exc:
        # Only our fixed validation messages are shared; other errors may contain paths.
        detail = str(exc) if type(exc) is ValueError else type(exc).__name__
        print("Verification failed:", detail)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
