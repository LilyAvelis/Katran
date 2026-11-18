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


def load_recognizer(language):
    """Build lazily, then retain this language's pipeline across requests."""
    names = {"zh": "PP-OCRv5_server_rec", "ko": "korean_PP-OCRv5_mobile_rec"}
    name = names[language]
    det = MODELS / "PP-OCRv5_server_det"
    rec = MODELS / name
    for folder in (det, rec):
        if any(not (folder / part).is_file() for part in
               ("inference.json", "inference.pdiparams", "inference.yml")):
            raise FileNotFoundError(f"Local PP-OCR model is incomplete: {folder.name}")
    from paddleocr import PaddleOCR

    def create(accelerated):
        return PaddleOCR(
            text_detection_model_name="PP-OCRv5_server_det",
            text_detection_model_dir=str(det),
            text_recognition_model_name=name,
            text_recognition_model_dir=str(rec),
            device="cpu", enable_mkldnn=accelerated,
            use_doc_orientation_classify=False, use_doc_unwarping=False,
            use_textline_orientation=False,
        )

    pipeline = None
    accelerated = True

    def predict(image, out_json):
        nonlocal pipeline, accelerated
        try:
            if pipeline is None:
                pipeline = create(accelerated)
            pages = list(pipeline.predict(str(image)))
        except (RuntimeError, NotImplementedError):
            if not accelerated:
                raise
            accelerated = False
            pipeline = None
            pipeline = create(False)
            pages = list(pipeline.predict(str(image)))
        if len(pages) != 1:
            raise ValueError("Expected exactly one image page")
        result = pages[0]
        # Keep Paddle's recognition order paired with its polygon and score.
        blocks = [
            {"polygon": polygon.astype(int).tolist(), "text": str(text).strip(), "confidence": float(score)}
            for polygon, text, score in zip(
                result["rec_polys"], result["rec_texts"], result["rec_scores"], strict=True
            )
            if str(text).strip()
        ]
        with Path(out_json).open("x", encoding="utf-8") as output:
            json.dump(blocks, output, ensure_ascii=False, indent=2)

    return predict


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("image", nargs="?")
    parser.add_argument("out_json", nargs="?")
    parser.add_argument("--lang", choices=["japan", "china", "korean"])
    parser.add_argument("--prepare", action="store_true", help="Explicitly allow the first model download")
    parser.add_argument("--worker", action="store_true")
    parser.add_argument("--multilingual-worker", action="store_true")
    args = parser.parse_args()
    if args.multilingual_worker:
        if args.worker or args.image is not None or args.out_json is not None or args.prepare or args.lang is not None:
            parser.error("--multilingual-worker does not accept other options or positional arguments")
        detector = None
        recognizers = {}
        for line in sys.stdin:
            value = json.loads(line)
            if (not isinstance(value, dict) or set(value) != {"dir", "language"}
                    or not isinstance(value["dir"], str) or not Path(value["dir"]).is_absolute()
                    or value["language"] not in ("ja", "zh", "zh-hant", "ko")):
                raise ValueError("worker request must contain an absolute dir and language ja, zh, zh-hant or ko")
            staging = Path(value["dir"])
            language = {"zh-hant": "zh"}.get(value["language"], value["language"])
            if language == "ja":
                if detector is None:
                    detector = load_detector(False, parser)
                detect(detector, staging / "input.png", staging / "ocr.json")
            else:
                if language not in recognizers:
                    recognizers[language] = load_recognizer(language)
                recognizers[language](staging / "input.png", staging / "ocr.json")
            with (staging / "paddle.done").open("x", encoding="utf-8"):
                pass
        return
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
        if args.lang == "japan":
            detector = load_detector(args.prepare, parser)
            detect(detector, args.image, args.out_json)
        else:
            if args.prepare:
                parser.error("Explicit model preparation is not supported for china or korean")
            load_recognizer({"china": "zh", "korean": "ko"}[args.lang])(args.image, args.out_json)


if __name__ == "__main__":
    main()
