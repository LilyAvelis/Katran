"""Detect text on one page with PP-OCRv5, in original image coordinates."""
import argparse
import json
import os
import sys
from pathlib import Path

ROOT = Path(os.environ.get("KATRAN_PADDLE_ROOT", Path(__file__).resolve().parents[2] / "target" / "build" / "translate-probe" / "ocr"))
MODELS = Path(os.environ.get("KATRAN_PADDLE_MODELS", ROOT / "paddlex" / "official_models"))
# Keep all model and hub caches inside the explicit probe directory.
for key, child in (("PADDLE_PDX_CACHE_HOME", "paddlex"), ("HF_HOME", "huggingface"), ("MODELSCOPE_CACHE", "modelscope"), ("XDG_CACHE_HOME", "cache")):
    os.environ[key] = str(Path(os.environ.get("KATRAN_PROCESS_CACHE", ROOT)) / child)
os.environ["PADDLE_PDX_DISABLE_MODEL_SOURCE_CHECK"] = "True"
os.environ["FLAGS_enable_pir_api"] = "0"


def load_detector(prepare, parser):
    det = MODELS / "PP-OCRv5_server_det"
    if not prepare and any(
        not (det / name).is_file()
        for name in ("inference.json", "inference.pdiparams", "inference.yml")
    ):
        parser.error("PP-OCRv5 weights are missing; use --prepare to download them explicitly")
    from paddleocr import TextDetection

    def create(accelerated):
        return TextDetection(
            model_dir=None if prepare else str(det),
            model_name="PP-OCRv5_server_det", device="cpu", enable_mkldnn=accelerated,
            limit_side_len=64, limit_type="min", thresh=0.3, box_thresh=0.6,
            unclip_ratio=1.5,
        )

    detector = None
    accelerated = True

    def predict(image):
        nonlocal detector, accelerated
        try:
            if detector is None:
                detector = create(accelerated)
            return list(detector.predict(str(image), max_side_limit=4000))
        except (RuntimeError, NotImplementedError):
            if not accelerated:
                raise
            # Some Paddle builds cannot run the oneDNN backend.
            accelerated = False
            detector = None
            detector = create(False)
            return list(detector.predict(str(image), max_side_limit=4000))

    return predict


def detect(detector, image, out_json):
    pages = detector(image)
    if len(pages) != 1:
        raise ValueError("Expected exactly one image page")
    result = pages[0]
    blocks = [
        {"polygon": polygon.astype(int).tolist(), "text": "", "confidence": float(score)}
        for polygon, score in zip(result["dt_polys"], result["dt_scores"], strict=True)
    ]
    with Path(out_json).open("x", encoding="utf-8") as output:
        json.dump(blocks, output, ensure_ascii=False, indent=2)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("image", nargs="?")
    parser.add_argument("out_json", nargs="?")
    parser.add_argument("--lang", choices=["japan"])
    parser.add_argument("--prepare", action="store_true", help="Explicitly allow the first model download")
    parser.add_argument("--worker", action="store_true")
    args = parser.parse_args()
    if args.worker:
        if args.image is not None or args.out_json is not None or args.prepare:
            parser.error("--worker does not accept positional arguments or --prepare")
        detector = load_detector(False, parser)
        for line in sys.stdin:
            value = json.loads(line)
            if not isinstance(value, str) or not Path(value).is_absolute():
                raise ValueError("worker request must be an absolute staging directory path")
            staging = Path(value)
            detect(detector, staging / "input.png", staging / "ocr.json")
            with (staging / "paddle.done").open("x", encoding="utf-8"):
                pass
    else:
        if args.image is None or args.out_json is None or args.lang is None:
            parser.error("image, out_json and --lang are required")
        detector = load_detector(args.prepare, parser)
        detect(detector, args.image, args.out_json)


if __name__ == "__main__":
    main()
