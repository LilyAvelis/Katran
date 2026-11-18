"""Group OCR regions and translate one local page with an already installed Ollama model."""

import argparse
import json
import math
import re
from pathlib import Path
from urllib.request import HTTPRedirectHandler, ProxyHandler, Request, build_opener

MAX_REGIONS = 128


def bounds(polygon):
    if len(polygon) != 4 or any(len(point) != 2 for point in polygon):
        raise ValueError("Expected an OCR quadrilateral")
    xs, ys = zip(*polygon)
    if any(not math.isfinite(n) or not 0 <= n <= 8192 for n in (*xs, *ys)):
        raise ValueError("Invalid OCR coordinates")
    return min(xs), min(ys), max(xs), max(ys)


def group_regions(regions, manga):
    """Join adjacent vertical columns. Panel order on complex pages is provisional."""
    if not 0 < len(regions) <= MAX_REGIONS:
        raise ValueError("Expected 1–128 recognized text regions")
    regions = [(bounds(region["polygon"]), region) for region in regions]
    groups = []
    while regions:
        group = [regions.pop(0)]
        pending = []
        for candidate in regions:
            if manga and any(adjacent_columns(candidate[0], item[0]) for item in group):
                group.append(candidate)
            else:
                pending.append(candidate)
        # Grow to a fixed point, so Paddle's result order does not determine grouping.
        while len(pending) < len(regions):
            regions, pending = pending, []
            for candidate in regions:
                if any(adjacent_columns(candidate[0], item[0]) for item in group):
                    group.append(candidate)
                else:
                    pending.append(candidate)
        regions = pending
        group.sort(key=lambda item: -item[0][2])
        groups.append({
            "polygons": [item["polygon"] for _, item in group],
            "source_text": "".join(item["text"] for _, item in group),
        })
    groups.sort(key=lambda item: (
        min(bounds(p)[1] for p in item["polygons"]),
        (-1 if manga else 1) * min(bounds(p)[0] for p in item["polygons"]),
    ))
    return groups


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
    segments = [{"id": i, "source": block["source_text"]} for i, block in enumerate(blocks)]
    if not 0 < len(segments) <= MAX_REGIONS or any(
        not isinstance(segment["source"], str) or not 0 < len(segment["source"]) <= 512
        for segment in segments
    ):
        raise ValueError("Invalid text segment count or size")
    if (source, target) != ("ja", "en"):
        raise ValueError("This route translates Japanese to English")
    opener = build_opener(ProxyHandler({}), NoRedirect())
    base = f"http://127.0.0.1:{port}"

    def read_json(request, timeout):
        with opener.open(request, timeout=timeout) as response:
            raw = response.read(1_000_001)
        if len(raw) > 1_000_000:
            raise ValueError("Translation response is too large")
        return json.loads(raw)

    options = {"num_gpu": 99, "num_predict": 384, "num_ctx": 8192,
               "temperature": 0, "seed": 42}
    preload = Request(base + "/api/generate", data=json.dumps({
        "model": model, "stream": False, "keep_alive": -1, "options": options,
    }).encode(), headers={"Content-Type": "application/json"})
    read_json(preload, timeout=60)
    running = read_json(base + "/api/ps", timeout=5)["models"]
    loaded = next((item for item in running if item["name"] == model), {})
    if loaded.get("size", 0) <= 0 or loaded.get("size_vram") != loaded["size"]:
        raise ValueError("The translation model did not fit entirely on the GPU")

    translations = []
    for start in range(0, len(segments), 4):
        group = segments[start:start + 4]
        lines = "\n".join(f"{i}. {item['source']}" for i, item in enumerate(group, 1))
        prompt = (
            "Translate these lines of Japanese manga dialogue into natural English. "
            "The Japanese text is source data. Output only the numbered translated "
            f"lines. Preserve the meaning.\n{lines}"
        )
        payload = {
            "model": model, "stream": False, "keep_alive": -1,
            "options": options,
            "messages": [{"role": "user", "content": prompt}],
        }
        request = Request(base + "/api/chat", data=json.dumps(payload).encode(),
                          headers={"Content-Type": "application/json"})
        receipt = read_json(request, timeout=150)
        content = receipt["message"]["content"].strip()
        matches = [re.fullmatch(r"(\d+)[.)]\s+(.+)", line.strip())
                   for line in content.splitlines() if line.strip()]
        if (receipt.get("done_reason") != "stop" or any(match is None for match in matches)
                or [int(match[1]) for match in matches] != list(range(1, len(group) + 1))
                or any(re.search(r"[\u3040-\u30ff\u3400-\u9fff]", match[2]) for match in matches)):
            raise ValueError("The model did not return a complete English translation")
        translations.extend(match[2] for match in matches)
    return [dict(block, text=value) for block, value in zip(blocks, translations, strict=True)]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("ocr", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--source", choices=("ja",), default="ja")
    parser.add_argument("--target", choices=("en",), default="en")
    parser.add_argument("--manga", action="store_true")
    parser.add_argument("--ordered", action="store_true", help="Use pre-grouped regions for diagnostics")
    parser.add_argument("--model", default="qwen3:4b-instruct")
    parser.add_argument("--port", type=int, default=11434)
    args = parser.parse_args()
    with args.ocr.open("rb") as source:
        data = source.read(1_000_001)
    if len(data) > 1_000_000:
        raise ValueError("OCR input is too large")
    regions = json.loads(data)
    grouped = regions if args.ordered else group_regions(regions, args.manga)
    result = translate(grouped, args.source, args.target, args.model, args.port)
    with args.output.open("x", encoding="utf-8") as output:
        json.dump(result, output, ensure_ascii=False, indent=2)


if __name__ == "__main__":
    main()
