"""Second-pass manga recognition over Paddle region groups; outputs ordered geometry."""
import argparse
import json
import math
import os
import sys
from pathlib import Path

os.environ["HF_HUB_OFFLINE"] = "1"
os.environ["TRANSFORMERS_OFFLINE"] = "1"

import torch
from PIL import Image
from transformers import GenerationMixin, ViTImageProcessor, VisionEncoderDecoderModel
sys.path.insert(0, str(Path(__file__).resolve().parent))
from translate_page import group_regions, bounds

# Official kha-white/manga-ocr-base, snapshot aa6573bd10b0d446cbf622e29c3e084914df9741.
MODEL_DIR = Path(os.environ.get("KATRAN_MANGA_MODEL", Path(__file__).resolve().parents[2] / "target/build/translate-probe/ocr/manga-model"))


class MangaModel(VisionEncoderDecoderModel, GenerationMixin):
    pass


def load_model():
    model_path = str(MODEL_DIR)
    processor = ViTImageProcessor.from_pretrained(model_path, local_files_only=True, trust_remote_code=False)
    model = MangaModel.from_pretrained(model_path, local_files_only=True, trust_remote_code=False, weights_only=True).eval()
    device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
    model.to(device)
    # Character vocabulary; inference only decodes, so MeCab is unnecessary.
    vocab = (MODEL_DIR / "vocab.txt").read_text(encoding="utf-8").splitlines()
    return processor, model, device, vocab


def recognize(image_path, ocr_path, output_path, processor, model, device, vocab):
    if ocr_path.stat().st_size > 1_000_000:
        raise ValueError("OCR input too large")
    groups = group_regions(json.loads(ocr_path.read_text(encoding="utf-8")), True)
    image = Image.open(image_path)
    if getattr(image, "n_frames", 1) != 1 or max(image.size) > 8192 or image.width * image.height > 32_000_000:
        raise ValueError("Unsupported image")
    image = image.convert("L").convert("RGB")
    output = []
    # Fixed-size batches bound accelerator memory and keep page order intact.
    # Each crop uses the same processor and generation settings as a single run.
    batch_size = 4
    with torch.inference_mode():
        for offset in range(0, len(groups), batch_size):
            batch = groups[offset:offset + batch_size]
            crops = []
            for group in batch:
                rects = [bounds(p) for p in group["polygons"]]
                x0 = max(0, math.floor(min(r[0] for r in rects)) - 3)
                y0 = max(0, math.floor(min(r[1] for r in rects)) - 3)
                x1 = min(image.width, math.ceil(max(r[2] for r in rects)) + 3)
                y1 = min(image.height, math.ceil(max(r[3] for r in rects)) + 3)
                if x1 <= x0 or y1 <= y0:
                    raise ValueError("Invalid crop")
                crops.append(image.crop((x0, y0, x1, y1)))
            pixels = processor(crops, return_tensors="pt").pixel_values.to(device)
            sequences = model.generate(pixels, max_length=300).tolist()
            if len(sequences) != len(batch):
                raise ValueError("Recognition returned inconsistent batch size")
            for group, ids in zip(batch, sequences):
                if any(i < 0 or i >= len(vocab) for i in ids):
                    raise ValueError("Invalid recognition token")
                text = "".join(vocab[i].removeprefix("##") for i in ids if i not in (0, 2, 3, 4))
                output.append({"source_text": "".join(text.split()), "polygons": group["polygons"]})
    with output_path.open("x", encoding="utf-8") as f:
        json.dump(output, f, ensure_ascii=False, indent=2)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("image", type=Path, nargs="?")
    parser.add_argument("ocr", type=Path, nargs="?")
    parser.add_argument("output", type=Path, nargs="?")
    parser.add_argument("--worker", action="store_true")
    args = parser.parse_args()
    if args.worker:
        if any(value is not None for value in (args.image, args.ocr, args.output)):
            parser.error("--worker does not accept positional arguments")
        model = load_model()
        for line in sys.stdin:
            value = json.loads(line)
            if not isinstance(value, str) or not Path(value).is_absolute():
                raise ValueError("worker request must be an absolute staging directory path")
            staging = Path(value)
            recognize(staging / "input.png", staging / "ocr.json", staging / "grouped.json", *model)
            with (staging / "manga.done").open("x", encoding="utf-8"):
                pass
    else:
        if any(value is None for value in (args.image, args.ocr, args.output)):
            parser.error("image, ocr and output are required")
        model = load_model()
        recognize(args.image, args.ocr, args.output, *model)


if __name__ == "__main__":
    main()
