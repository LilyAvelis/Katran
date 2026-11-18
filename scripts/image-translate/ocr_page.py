"""One-page Japanese PP-OCRv5 probe. Coordinates are in original image pixels."""
import argparse
import json
import os
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2] / "target" / "build" / "translate-probe" / "ocr"
MODELS = ROOT / "paddlex" / "official_models"
# Keep all model and hub caches inside the explicit probe directory.
for key, child in (("PADDLE_PDX_CACHE_HOME", "paddlex"), ("HF_HOME", "huggingface"), ("MODELSCOPE_CACHE", "modelscope"), ("XDG_CACHE_HOME", "cache")):
    os.environ[key] = str(ROOT / child)
os.environ["PADDLE_PDX_DISABLE_MODEL_SOURCE_CHECK"] = "True"
os.environ["FLAGS_enable_pir_api"] = "0"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("image")
    parser.add_argument("out_json")
    parser.add_argument("--lang", choices=["japan"], required=True)
    parser.add_argument("--prepare", action="store_true", help="Explicitly allow the first model download")
    args = parser.parse_args()
    det = MODELS / "PP-OCRv5_server_det"
    rec = MODELS / "PP-OCRv5_server_rec"
    if not args.prepare and any(
        not (folder / name).is_file()
        for folder in (det, rec)
        for name in ("inference.json", "inference.pdiparams", "inference.yml")
    ):
        parser.error("PP-OCRv5 weights are missing; use --prepare to download them explicitly")
    from paddleocr import PaddleOCR
    models = (
        {"lang": args.lang, "ocr_version": "PP-OCRv5"}
        if args.prepare
        else {"text_detection_model_dir": str(det), "text_recognition_model_dir": str(rec)}
    )
    ocr = PaddleOCR(
        **models, device="cpu",
        use_doc_orientation_classify=False, use_doc_unwarping=False,
        use_textline_orientation=False, enable_mkldnn=False,
    )
    pages = list(ocr.predict(args.image))
    if len(pages) != 1:
        raise ValueError("Expected exactly one image page")
    result = pages[0]
    polygons, texts, scores = result["rec_polys"], result["rec_texts"], result["rec_scores"]
    if not (len(polygons) == len(texts) == len(scores)):
        raise ValueError("OCR returned inconsistent results")
    blocks = []
    for polygon, text, score in zip(polygons, texts, scores):
        blocks.append({"polygon": [[int(p[0]), int(p[1])] for p in polygon], "text": str(text), "confidence": float(score)})
    with Path(args.out_json).open("x", encoding="utf-8") as output:
        json.dump(blocks, output, ensure_ascii=False, indent=2)


if __name__ == "__main__":
    main()
