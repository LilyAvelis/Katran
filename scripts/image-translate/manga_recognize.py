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
MODEL_DIR = Path(__file__).resolve().parents[2] / "target/build/translate-probe/ocr/manga-model"


class MangaModel(VisionEncoderDecoderModel, GenerationMixin):
    pass


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("image", type=Path)
    parser.add_argument("ocr", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    if args.ocr.stat().st_size > 1_000_000:
        raise ValueError("OCR input too large")
    groups = group_regions(json.loads(args.ocr.read_text(encoding="utf-8")), True)
    model_path = str(MODEL_DIR)
    processor = ViTImageProcessor.from_pretrained(model_path, local_files_only=True, trust_remote_code=False)
    model = MangaModel.from_pretrained(model_path, local_files_only=True, trust_remote_code=False, weights_only=True).eval()
    device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
    model.to(device)
    # Character vocabulary; inference only decodes, so MeCab is unnecessary.
    vocab = (MODEL_DIR / "vocab.txt").read_text(encoding="utf-8").splitlines()
    image = Image.open(args.image)
    if getattr(image, "n_frames", 1) != 1 or max(image.size) > 8192 or image.width * image.height > 32_000_000:
        raise ValueError("Unsupported image")
    image = image.convert("L").convert("RGB")
    output = []
    with torch.inference_mode():
        for group in groups:
            rects = [bounds(p) for p in group["polygons"]]
            x0 = max(0, math.floor(min(r[0] for r in rects)) - 3)
            y0 = max(0, math.floor(min(r[1] for r in rects)) - 3)
            x1 = min(image.width, math.ceil(max(r[2] for r in rects)) + 3)
            y1 = min(image.height, math.ceil(max(r[3] for r in rects)) + 3)
            if x1 <= x0 or y1 <= y0:
                raise ValueError("Invalid crop")
            pixels = processor(image.crop((x0, y0, x1, y1)), return_tensors="pt").pixel_values.to(device)
            ids = model.generate(pixels, max_length=300)[0].tolist()
            if any(i < 0 or i >= len(vocab) for i in ids):
                raise ValueError("Invalid recognition token")
            text = "".join(vocab[i].removeprefix("##") for i in ids if i not in (0, 2, 3, 4))
            output.append({"source_text": "".join(text.split()), "polygons": group["polygons"]})
    with args.output.open("x", encoding="utf-8") as f:
        json.dump(output, f, ensure_ascii=False, indent=2)


if __name__ == "__main__":
    main()
