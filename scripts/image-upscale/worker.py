"""Private one-shot inference worker. No downloads or service required."""
import json
import sys
from pathlib import Path

import numpy as np
import torch
from PIL import Image, ImageOps
from spandrel import ModelLoader

MAX_INPUT_PIXELS = 16_000_000
MAX_OUTPUT_PIXELS = 32_000_000
MAX_NATIVE_PIXELS = 40_000_000
MAX_EDGE = 8192
Image.MAX_IMAGE_PIXELS = MAX_INPUT_PIXELS


def main():
    source, output, receipt, model_path, requested = sys.argv[1:]
    requested = int(requested)
    if requested not in (2, 4):
        raise ValueError("Invalid upscale factor")
    with Image.open(source) as original:
        if getattr(original, "n_frames", 1) != 1:
            raise ValueError("Animated images are not supported")
        original = ImageOps.exif_transpose(original)
        width, height = original.size
        ow, oh = width * requested, height * requested
        if min(width, height) < 1 or width * height > MAX_INPUT_PIXELS or max(ow, oh) > MAX_EDGE or ow * oh > MAX_OUTPUT_PIXELS:
            raise ValueError("Image dimensions exceed the upscale limit")
        rgba = original.convert("RGBA")
        alpha = rgba.getchannel("A")
        rgb = np.array(rgba.convert("RGB"), dtype=np.uint8)
    state = torch.load(model_path, map_location="cpu", weights_only=True)
    model = ModelLoader().load_from_state_dict(state)
    scale = int(model.scale)
    if scale not in (2, 4) or scale < requested or model.input_channels != 3 or model.output_channels != 3:
        raise ValueError("Model does not support the requested scale")
    if max(width * scale, height * scale) > MAX_EDGE or width * height * scale * scale > MAX_NATIVE_PIXELS:
        raise ValueError("Native model output exceeds the upscale limit")
    device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
    model = model.to(device).eval()
    # Work in overlapping tiles to bound GPU memory; crop away padded borders.
    native = Image.new("RGB", (width * scale, height * scale))
    tile, pad = 256, 24
    with torch.inference_mode():
        for y in range(0, height, tile):
            for x in range(0, width, tile):
                x1, y1 = min(x + tile, width), min(y + tile, height)
                left, top = max(0, x - pad), max(0, y - pad)
                right, bottom = min(width, x1 + pad), min(height, y1 + pad)
                a = np.ascontiguousarray(rgb[top:bottom, left:right].transpose(2, 0, 1))
                tensor = torch.from_numpy(a).unsqueeze(0).to(device=device, dtype=torch.float32).div_(255)
                result = model(tensor).clamp_(0, 1).mul_(255).round_().byte().cpu().squeeze(0).permute(1, 2, 0).numpy()
                expected = ((bottom - top) * scale, (right - left) * scale, 3)
                if result.shape != expected:
                    raise ValueError("Unexpected model output dimensions")
                cropped = result[(y - top) * scale:(y1 - top) * scale, (x - left) * scale:(x1 - left) * scale]
                native.paste(Image.fromarray(cropped), (x * scale, y * scale))
    if scale != requested:
        native = native.resize((ow, oh), Image.Resampling.LANCZOS)
    native.putalpha(alpha.resize((ow, oh), Image.Resampling.LANCZOS))
    native.save(output, format="PNG")
    Path(receipt).write_text(json.dumps({"input_width": width, "input_height": height, "width": ow, "height": oh}), encoding="utf-8")


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:
        if len(sys.argv) == 6:
            Path(sys.argv[3]).write_text(json.dumps({"error": str(exc)[:500]}), encoding="utf-8")
        sys.exit(1)
