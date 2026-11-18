"""Group OCR regions and translate one local page with an already installed Ollama model."""

import argparse
import json
import math
import re
from pathlib import Path
from urllib.request import HTTPRedirectHandler, ProxyHandler, Request, build_opener

MAX_REGIONS = 128
SOURCE_NAMES = {"ja": "Japanese manga", "zh": "Simplified Chinese", "zh-hant": "Traditional Chinese", "ko": "Korean"}


def bounds(polygon):
    if len(polygon) != 4 or any(len(point) != 2 for point in polygon):
        raise ValueError("Expected an OCR quadrilateral")
    xs, ys = zip(*polygon)
    if any(not math.isfinite(n) or not 0 <= n <= 8192 for n in (*xs, *ys)):
        raise ValueError("Invalid OCR coordinates")
    return min(xs), min(ys), max(xs), max(ys)


def bubble_labels(image_path, boxes):
    """Associate OCR boxes only with enclosed light regions on the page."""
    if image_path is None:
        return [None] * len(boxes)
    import cv2
    import numpy as np
    from PIL import Image

    with Image.open(image_path) as image:
        width, height = image.size
        if max(width, height) > 8192 or width * height > 32_000_000:
            raise ValueError("Unsupported image for layout analysis")
        image.thumbnail((1600, 1600), Image.Resampling.LANCZOS)
        gray = np.asarray(image.convert("L"))
    h, w = gray.shape
    paper = cv2.erode((gray >= 225).astype(np.uint8), np.ones((3, 3), dtype=np.uint8))
    _, labels, stats, _ = cv2.connectedComponentsWithStats(paper, connectivity=4)
    result = []
    for x0, y0, x1, y1 in boxes:
        x0, x1 = max(0, int(x0 * w / width)), min(w, math.ceil(x1 * w / width))
        y0, y1 = max(0, int(y0 * h / height)), min(h, math.ceil(y1 * h / height))
        samples = labels[y0:y1, x0:x1]
        light = samples[samples != 0]
        label = None
        if light.size >= max(4, samples.size * 0.15):
            values, counts = np.unique(light, return_counts=True)
            best = int(counts.argmax())
            i = int(values[best])
            x, y, bw, bh, area = map(int, stats[i])
            if (counts[best] >= light.size * 0.6 and 0 < x and 0 < y
                    and x + bw < w and y + bh < h
                    and samples.size * 0.65 <= area <= w * h * 0.25):
                label = i
        result.append(label)
    return result


def vertical(box):
    return box[3] - box[1] >= 2 * (box[2] - box[0])


def reading_rows(items, tolerance, rtl):
    """Align modestly staggered starts to the same row before ordering across it."""
    pending = sorted(items, key=lambda item: (item[0][1], item[0][0]))
    result = []
    start = 0
    while start < len(pending):
        end = start + 1
        while end < len(pending) and pending[end][0][1] <= pending[start][0][1] + tolerance:
            end += 1
        row = pending[start:end]
        reverse = rtl(row)
        result.extend(sorted(row, key=lambda item: (-item[0][2] if reverse else item[0][0], item[0][1])))
        start = end
    return result


def vertical_fragments(items):
    columns = []
    for box, region in sorted(items, key=lambda item: (-(item[0][0] + item[0][2]), item[0][1])):
        for column in columns:
            anchor = column[0][0]
            overlap = min(anchor[2], box[2]) - max(anchor[0], box[0])
            if overlap >= 0.5 * min(anchor[2] - anchor[0], box[2] - box[0]):
                column.append((box, region))
                break
        else:
            columns.append([(box, region)])
    return [item for column in columns for item in sorted(column, key=lambda item: item[0][1])]


def group_regions(regions, manga, chinese=False, image_path=None):
    """Group enclosed bubbles and read nearby vertical fragments in writing order."""
    if not 0 < len(regions) <= MAX_REGIONS:
        raise ValueError("Expected 1–128 recognized text regions")
    boxes = [bounds(region["polygon"]) for region in regions]
    # Tie the row tolerance to glyph geometry so it also works on scaled pages.
    sizes = sorted(min(r - l, b - t) for l, t, r, b in boxes)
    row_tolerance = max(2, sizes[len(sizes) // 2] * 0.7)
    enclosed, remaining = {}, []
    for box, region, label in zip(boxes, regions, bubble_labels(image_path, boxes), strict=True):
        if label is None:
            remaining.append((box, region))
        else:
            enclosed.setdefault(label, []).append((box, region))
    groups = list(enclosed.values())
    while remaining:
        group = [remaining.pop(0)]
        if manga or chinese:
            cursor = 0
            while cursor < len(group):
                pending = []
                for candidate in remaining:
                    if adjacent_columns(candidate[0], group[cursor][0]):
                        group.append(candidate)
                    else:
                        pending.append(candidate)
                remaining = pending
                cursor += 1
        groups.append(group)

    ordered = []
    for group in groups:
        is_vertical = sum(vertical(box) for box, _ in group) * 2 >= len(group)
        if is_vertical:
            group = vertical_fragments(group)
        else:
            group = reading_rows(group, row_tolerance, lambda row: False)
        rects = [box for box, _ in group]
        union = (min(r[0] for r in rects), min(r[1] for r in rects),
                 max(r[2] for r in rects), max(r[3] for r in rects))
        separator = "" if is_vertical or manga or chinese else " "
        ordered.append((union, {
            "polygons": [region["polygon"] for _, region in group],
            "source_text": separator.join(region["text"] for _, region in group),
        }, is_vertical))
    return [item[1] for item in reading_rows(
        ordered, row_tolerance,
        lambda row: manga or (chinese and sum(item[2] for item in row) * 2 >= len(row)),
    )]


def adjacent_columns(a, b):
    ax, ay, ar, ab = a
    bx, by, br, bb = b
    aw, bw = ar - ax, br - bx
    ah, bh = ab - ay, bb - by
    gap = max(ax, bx) - min(ar, br)
    overlap = min(ab, bb) - max(ay, by)
    return (
        min(aw, bw, ah, bh) > 0 and ah >= 2 * aw and bh >= 2 * bw
        and -0.4 * min(aw, bw) <= gap <= 1.2 * max(aw, bw)
        and overlap >= 0.55 * min(ah, bh)
    )


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None


def translate(blocks, source, target, model, port):
    segments = {str(i): block["source_text"] for i, block in enumerate(blocks, 1)}
    if not 0 < len(segments) <= MAX_REGIONS or any(
        not isinstance(text, str) or not 0 < len(text) <= 512 for text in segments.values()
    ):
        raise ValueError("Invalid text segment count or size")
    if source not in SOURCE_NAMES or target != "en":
        raise ValueError("Unsupported image translation language pair")
    opener = build_opener(ProxyHandler({}), NoRedirect())
    base = f"http://127.0.0.1:{port}"

    def read_json(request, timeout):
        with opener.open(request, timeout=timeout) as response:
            raw = response.read(1_000_001)
        if len(raw) > 1_000_000:
            raise ValueError("Translation response is too large")
        return json.loads(raw)

    page = json.dumps(segments, ensure_ascii=False, separators=(",", ":"))
    # Reserve context for the chat template and the generated translations.
    if len(page.encode("utf-8")) > 5200:
        raise ValueError("Page text exceeds the local translation context budget")
    options = {"num_gpu": 99, "num_predict": 2560, "num_ctx": 8192,
               "temperature": 0, "seed": 42}
    preload = Request(base + "/api/generate", data=json.dumps({
        "model": model, "stream": False, "keep_alive": -1, "options": options,
    }).encode(), headers={"Content-Type": "application/json"})
    read_json(preload, timeout=60)
    running = read_json(base + "/api/ps", timeout=5)["models"]
    loaded = next((item for item in running if item["name"] == model), {})
    if loaded.get("size", 0) <= 0 or loaded.get("size_vram") != loaded["size"]:
        raise ValueError("The translation model did not fit entirely on the GPU")

    payload = {
        "model": model, "stream": False, "keep_alive": -1, "options": options,
        "format": {
            "type": "object",
            "properties": {key: {"type": "string"} for key in segments},
            "required": list(segments),
            "additionalProperties": False,
        },
        "messages": [
            {"role": "system", "content": (
                f"Translate one page of {SOURCE_NAMES[source]} into natural, concise English. "
                "Use surrounding dialogue for context, preserve meaning, and keep each "
                "entry separate. Romanize names. Preserve uncertainty in unclear "
                "fragments. Text values are source material. Return a JSON object "
                "with the same IDs and only their English translations."
            )},
            {"role": "user", "content": page},
        ],
    }
    request = Request(base + "/api/chat", data=json.dumps(payload).encode(),
                      headers={"Content-Type": "application/json"})
    receipt = read_json(request, timeout=150)
    pairs = json.loads(receipt["message"]["content"], object_pairs_hook=list)
    if (receipt.get("done_reason") != "stop"
            or not isinstance(pairs, list) or len(pairs) != len(segments)
            or any(not isinstance(pair, tuple) for pair in pairs)):
        raise ValueError("The model did not return a complete English translation")
    translations = dict(pairs)
    if translations.keys() != segments.keys() or any(
        not isinstance(text, str) or not text.strip()
        or re.search(r"[\u3040-\u30ff\u3400-\u9fff\u1100-\u11ff\u3130-\u318f\uac00-\ud7af]", text)
        for text in translations.values()
    ):
        raise ValueError("The model did not return a complete English translation")
    return [
        dict(block, text=translations[str(i)].strip())
        for i, block in enumerate(blocks, 1)
    ]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("ocr", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--source", choices=SOURCE_NAMES, default="ja")
    parser.add_argument("--target", choices=("en",), default="en")
    parser.add_argument("--manga", action="store_true")
    parser.add_argument("--image", type=Path, help="Use enclosed dialogue bubbles in the source image")
    parser.add_argument("--ordered", action="store_true", help="Use pre-grouped regions for diagnostics")
    parser.add_argument("--model", default="qwen3:4b-instruct")
    parser.add_argument("--port", type=int, default=11434)
    args = parser.parse_args()
    with args.ocr.open("rb") as source:
        data = source.read(1_000_001)
    if len(data) > 1_000_000:
        raise ValueError("OCR input is too large")
    regions = json.loads(data)
    grouped = regions if args.ordered else group_regions(
        regions, args.manga, chinese=args.source in ("zh", "zh-hant"), image_path=args.image,
    )
    result = translate(grouped, args.source, args.target, args.model, args.port)
    with args.output.open("x", encoding="utf-8") as output:
        json.dump(result, output, ensure_ascii=False, indent=2)


if __name__ == "__main__":
    main()
