"""Create a Katran emoji AI runtime integrity manifest from an assembled local bundle."""
import argparse
import hashlib
import json
from pathlib import Path

REQUIRED = [
    "local-experiments/emoji-ai/worker.py",
    "local-experiments/emoji-ai/semantic_search.py",
    "local-experiments/emoji-ai/model/onnx/model.onnx",
    "local-experiments/emoji-ai/model/onnx/tokenizer.json",
    "local-experiments/emoji-ai/cache-cldr/manifest.json",
    "local-experiments/emoji-ai/cache-cldr/passages.npy",
    "assets/unicode/emoji-test.txt",
    *[f"local-experiments/emoji-ai/cldr/cldr-annotations-{kind}-{lang}.json"
      for lang in ("en", "ru") for kind in ("full", "derived-full")],
]
OPTIONAL = [f"local-experiments/emoji-ai/model/onnx/{name}"
            for name in ("model.onnx_data", "model.onnx.data")]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path, help="absolute root of the assembled runtime")
    args = parser.parse_args()
    if not args.root.is_absolute():
        parser.error("root must be absolute")
    root = args.root.resolve(strict=True)
    files = {}
    for name in REQUIRED + [x for x in OPTIONAL if (root / x).exists()]:
        path = (root / name).resolve(strict=True)
        if not path.is_relative_to(root) or not path.is_file():
            raise ValueError(f"runtime input outside root or not a file: {name}")
        digest = hashlib.sha256()
        with path.open("rb") as source:
            for block in iter(lambda: source.read(1024 * 1024), b""):
                digest.update(block)
        files[name] = digest.hexdigest()
    destination = root / "local-experiments/emoji-ai/katran-e-runtime.json"
    with destination.open("x", encoding="utf-8", newline="\n") as output:
        json.dump({"version": 1, "protocol": 1, "files": files}, output, indent=2, sort_keys=True)
        output.write("\n")
    print(f"Created {destination}")


if __name__ == "__main__":
    main()
