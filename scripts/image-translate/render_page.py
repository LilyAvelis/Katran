"""Render translated text into supplied image polygons, without OCR or translation.

Input JSON is an ordered list of objects with ``text`` and either ``polygon`` or
``polygons`` (each polygon is a list of image-space [x, y] points). An optional
``text_box`` [left, top, right, bottom] selects a larger safe placement rectangle;
it is intended for diagnostics and does not expand the inpainting mask.
"""
import argparse
import json
import math
from pathlib import Path

import cv2
import numpy as np
from PIL import Image, ImageDraw, ImageFont


def _polygon(value, width, height):
    points = np.asarray(value, dtype=np.float64)
    if points.ndim != 2 or points.shape[1] != 2 or len(points) < 3 or not np.isfinite(points).all():
        raise ValueError("polygon must contain at least three finite [x, y] points")
    if (points[:, 0] < 0).any() or (points[:, 0] >= width).any() or (points[:, 1] < 0).any() or (points[:, 1] >= height).any():
        raise ValueError("polygon exceeds image bounds")
    points = np.rint(points).astype(np.int32)
    if abs(cv2.contourArea(points)) < 4:
        raise ValueError("polygon is too small")
    return points


def _wrap(text, font, draw, width):
    lines = []
    for paragraph in text.split("\n"):
        line = ""
        for word in paragraph.split():
            trial = f"{line} {word}" if line else word
            if draw.textlength(trial, font=font) <= width:
                line = trial
            else:
                if not line or draw.textlength(word, font=font) > width:
                    return None
                lines.append(line)
                line = word
        lines.append(line)
    return lines


def _fit(text, font_path, draw, box):
    left, top, right, bottom = box
    width, height = right - left, bottom - top
    # Use actual text bounds; never shrink below a readable diagnostic minimum.
    for size in range(min(160, int(height)), 13, -1):
        font = ImageFont.truetype(str(font_path), size)
        lines = _wrap(text, font, draw, width)
        if lines is None:
            continue
        spacing = max(3, round(size * 0.28))
        content = "\n".join(lines)
        bounds = draw.multiline_textbbox((0, 0), content, font=font, spacing=spacing, align="center")
        if bounds[2] - bounds[0] <= width and bounds[3] - bounds[1] <= height:
            return font, content, spacing, bounds
    raise ValueError("translated text cannot fit in the supplied text area at minimum font size 14")


def _bubble_box(cleaned, region, other_regions, fallback):
    """Conservatively find an interior text rectangle near the source region."""
    height, width = region.shape
    x0, y0, x1, y1 = fallback
    pad = max(48, min(256, max(x1 - x0, y1 - y0) * 2))
    left, top = max(0, x0 - pad), max(0, y0 - pad)
    right, bottom = min(width, x1 + pad), min(height, y1 + pad)
    local = cleaned[top:bottom, left:right]
    selected = region[top:bottom, left:right]
    light = (cv2.cvtColor(local, cv2.COLOR_RGB2GRAY) >= 220).astype(np.uint8)
    _, labels, stats, _ = cv2.connectedComponentsWithStats(light, 8)
    candidates = labels[(selected != 0) & (labels != 0)]
    if not candidates.size:
        return None
    counts = np.bincount(candidates)
    label = int(np.argmax(counts[1:]) + 1)
    x, y, w, h, area = stats[label]
    if x == 0 or y == 0 or x + w >= right - left or y + h >= bottom - top or area > width * height * 0.45:
        return None
    component = (labels == label).astype(np.uint8)
    if any(np.any(component & (other[top:bottom, left:right] != 0)) for other in other_regions):
        return None
    if counts[label] < cv2.countNonZero(selected) * 0.65:
        return None
    gray = cv2.cvtColor(local, cv2.COLOR_RGB2GRAY)
    closed = cv2.morphologyEx(component, cv2.MORPH_CLOSE, np.ones((7, 7), dtype=np.uint8))
    component = np.where((selected != 0) & (gray >= 195), closed, component).astype(np.uint8)
    safe = cv2.erode(component, np.ones((9, 9), dtype=np.uint8))
    cx, cy = (x0 + x1) // 2 - left, (y0 + y1) // 2 - top
    if not safe[cy, cx]:
        return None
    best, best_area = _largest_box(safe, cx, cy)
    if best is None or best_area <= (x1 - x0) * (y1 - y0):
        return None
    x0, y0, x1, y1 = best
    return x0 + left, y0 + top, x1 + left, y1 + top


def _largest_box(safe, cx, cy):
    # Horizontal intervals containing the centre, one per row. The search then
    # combines intervals with NumPy rather than looping over every canvas pixel.
    row_left = np.full(safe.shape[0], cx + 1, dtype=np.int32)
    row_right = np.full(safe.shape[0], cx, dtype=np.int32)
    for row in np.flatnonzero(safe[:, cx]):
        a = np.flatnonzero(safe[row, :cx] == 0)
        b = np.flatnonzero(safe[row, cx + 1:] == 0)
        row_left[row] = int(a[-1] + 1) if a.size else 0
        row_right[row] = int(cx + 1 + b[0]) if b.size else safe.shape[1]
    best, best_area = None, 0
    for top_row in range(cy, -1, -1):
        if row_left[top_row] > cx:
            break
        left_edge = int(np.max(row_left[top_row:cy + 1]))
        right_edge = int(np.min(row_right[top_row:cy + 1]))
        ls = np.maximum.accumulate(np.maximum(row_left[cy:], left_edge))
        rs = np.minimum.accumulate(np.minimum(row_right[cy:], right_edge))
        areas = np.maximum(0, rs - ls) * (np.arange(cy, safe.shape[0]) - top_row + 1)
        offset = int(np.argmax(areas))
        if int(areas[offset]) > best_area:
            best_area = int(areas[offset])
            best = (int(ls[offset]), top_row, int(rs[offset]), cy + offset + 1)
    return best, best_area



def _local_box(cleaned, region, others, fallback):
    """Find a wholly light rectangle near this region, avoiding all other text."""
    height, width = region.shape
    x0, y0, x1, y1 = fallback
    margin = min(32, max(8, min(x1 - x0, y1 - y0) // 4))
    left, top = max(0, x0 - margin), max(0, y0 - margin)
    right, bottom = min(width, x1 + margin), min(height, y1 + margin)
    gray = cv2.cvtColor(cleaned[top:bottom, left:right], cv2.COLOR_RGB2GRAY)
    safe = (gray >= 235).astype(np.uint8)
    for other in others:
        safe[other[top:bottom, left:right] != 0] = 0
    safe = cv2.erode(safe, np.ones((5, 5), dtype=np.uint8))
    selected = region[top:bottom, left:right] != 0
    ys, xs = np.nonzero((safe != 0) & selected)
    if not len(xs):
        return None
    wanted_x, wanted_y = (x0 + x1) // 2 - left, (y0 + y1) // 2 - top
    nearest = int(np.argmin((xs - wanted_x) ** 2 + (ys - wanted_y) ** 2))
    cx, cy = int(xs[nearest]), int(ys[nearest])
    best, _ = _largest_box(safe, cx, cy)
    if best is None:
        return None
    x0, y0, x1, y1 = best
    return x0 + left, y0 + top, x1 + left, y1 + top


def _overlaps(a, b):
    return a[0] < b[2] and b[0] < a[2] and a[1] < b[3] and b[1] < a[3]


def _panel_wrap(text, font, draw, width):
    """Wrap even long unbroken tokens without shrinking the font."""
    lines = []
    current = ""
    for word in text.split():
        trial = (current + " " + word).strip()
        if draw.textlength(trial, font=font) <= width:
            current = trial
            continue
        if current:
            lines.append(current)
            current = ""
        for ch in word:
            if current and draw.textlength(current + ch, font=font) > width:
                lines.append(current)
                current = ""
            if draw.textlength(ch, font=font) > width:
                return None
            current += ch
    if current:
        lines.append(current)
    return lines or None


def render(source, blocks, output, font):
    original = Image.open(source).convert("RGBA")
    alpha = original.getchannel("A")
    rgb = np.asarray(original.convert("RGB"))
    height, width = rgb.shape[:2]
    if not isinstance(blocks, list):
        raise ValueError("blocks must be an ordered list")
    if not Path(font).is_file():
        raise ValueError("font file not found")
    gray = cv2.cvtColor(rgb, cv2.COLOR_RGB2GRAY)
    records = []
    # Geometry must be valid even for skipped blocks so we can protect it.
    for index, block in enumerate(blocks):
        try:
            raw = block.get("polygons")
            if raw is None:
                raw = [block["polygon"]]
            if not isinstance(raw, list) or not raw:
                raise ValueError("polygons must be a nonempty list")
            polygons = [_polygon(value, width, height) for value in raw]
            region = np.zeros((height, width), dtype=np.uint8)
            for polygon in polygons:
                cv2.fillPoly(region, [polygon], 255)
            if "text_box" in block:
                box = block["text_box"]
                if not isinstance(box, list) or len(box) != 4 or not all(isinstance(v, (int, float)) and math.isfinite(v) for v in box):
                    raise ValueError("text_box must be [left, top, right, bottom]")
                box = tuple(int(v) for v in box)
            else:
                x, y, w, h = cv2.boundingRect(np.concatenate(polygons))
                box = (x, y, x + w, y + h)
            left, top, right, bottom = box
            if not (0 <= left < right <= width and 0 <= top < bottom <= height):
                raise ValueError("text_box exceeds image bounds or is empty")
            records.append({"region": region, "box": box, "explicit": "text_box" in block,
                            "text": block.get("text"), "ink": None})
        except (AttributeError, KeyError, TypeError, ValueError, cv2.error) as exc:
            raise ValueError(f"block {index + 1}: invalid geometry: {exc}") from exc

    active = set()
    for index, record in enumerate(records):
        try:
            if not isinstance(record["text"], str) or not record["text"].strip():
                raise ValueError("text must be nonempty")
            region = record["region"]
            samples = gray[region != 0]
            background = float(np.percentile(samples, 85))
            if background < 90:
                raise ValueError("region background is too dark")
            ink = ((gray < min(background - 25, 190)) & (region != 0)).astype(np.uint8) * 255
            coverage = cv2.countNonZero(ink) / len(samples)
            if coverage < 0.001 or coverage > 0.55:
                raise ValueError("cannot isolate lettering")
            ink = cv2.dilate(ink, np.ones((3, 3), dtype=np.uint8), iterations=1)
            record["ink"] = cv2.bitwise_and(ink, region)
            active.add(index)
        except (TypeError, ValueError, cv2.error):
            continue

    # Failed blocks remain untouched on the page and move to a bottom panel.
    while active:
        protected = np.zeros((height, width), dtype=np.uint8)
        erase = np.zeros_like(protected)
        for index, record in enumerate(records):
            if index in active:
                erase = cv2.bitwise_or(erase, record["ink"])
            else:
                protected = cv2.bitwise_or(protected, record["region"])
        rejected = {i for i in active if np.any((records[i]["region"] != 0) & (protected != 0))}
        if rejected:
            active.difference_update(rejected)
            continue
        result = cv2.inpaint(rgb, erase, 3, cv2.INPAINT_TELEA) if cv2.countNonZero(erase) else rgb.copy()
        result_image = Image.fromarray(result)
        draw = ImageDraw.Draw(result_image)
        placements = []
        for index in sorted(active):
            record = records[index]
            box = record["box"]
            if not record["explicit"]:
                others = [item["region"] for pos, item in enumerate(records) if pos != index]
                box = _bubble_box(result, record["region"], others, box)
                if box is None:
                    box = _local_box(result, record["region"], others, record["box"])
            try:
                if box is None:
                    raise ValueError("no safe bubble area")
                left, top, right, bottom = box
                if np.any(protected[top:bottom, left:right]) or any(_overlaps(box, old[0]) for old in placements):
                    raise ValueError("placement overlaps another block")
                fitted = _fit(record["text"], font, draw, box)
                placements.append((box, fitted))
            except ValueError:
                rejected.add(index)
        if rejected:
            active.difference_update(rejected)
            continue
        break
    else:
        result_image = Image.fromarray(rgb.copy())
        draw = ImageDraw.Draw(result_image)
        placements = []

    for (left, top, right, bottom), (face, text, spacing, text_bounds) in placements:
        x = left + (right - left - (text_bounds[2] - text_bounds[0])) / 2 - text_bounds[0]
        y = top + (bottom - top - (text_bounds[3] - text_bounds[1])) / 2 - text_bounds[1]
        draw.multiline_text((x, y), text, font=face, fill=(0, 0, 0), spacing=spacing, align="center")
    result_image.putalpha(alpha)

    # Coordinate labels identify original regions without drawing on the artwork.
    panel_font = ImageFont.truetype(str(font), 16)
    panel_draw = ImageDraw.Draw(Image.new("RGB", (1, 1), "white"))
    line_height = max(20, panel_draw.textbbox((0, 0), "Ag", font=panel_font)[3] + 5)
    max_height = min(8192, 32_000_000 // width)
    panel_items, panel_height = [], 12
    for index, record in enumerate(records):
        if index in active:
            continue
        text = record["text"]
        if not isinstance(text, str) or not text.strip():
            continue
        x, y, _, _ = record["box"]
        lines = _panel_wrap(f"#{index + 1} ({x}, {y}): {text}", panel_font, panel_draw, width - 24)
        if not lines:
            continue
        block_height = len(lines) * line_height + 10
        if height + panel_height + block_height + 8 > max_height:
            continue
        panel_items.append(lines)
        panel_height += block_height
    fallback = len(panel_items)
    if not active and not fallback:
        raise ValueError("No text blocks could be rendered safely")
    if fallback:
        canvas = Image.new("RGBA", (width, height + panel_height + 8), "white")
        canvas.paste(result_image, (0, 0))
        panel = ImageDraw.Draw(canvas)
        y = height + 12
        for lines in panel_items:
            for line in lines:
                panel.text((12, y), line, font=panel_font, fill=(0, 0, 0, 255))
                y += line_height
            y += 10
        result_image = canvas
    with open(output, "xb") as target:
        result_image.save(target, format="PNG")
    return {"rendered": len(active) + fallback, "skipped": len(records) - len(active) - fallback,
            "fallback": fallback}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source")
    parser.add_argument("blocks_json")
    parser.add_argument("output")
    parser.add_argument("font")
    parser.add_argument("--report", help="Write rendered and skipped block counts as JSON")
    args = parser.parse_args()
    with open(args.blocks_json, encoding="utf-8") as stream:
        blocks = json.load(stream)
    report = render(args.source, blocks, args.output, args.font)
    if args.report:
        with open(args.report, "x", encoding="utf-8") as receipt:
            json.dump(report, receipt)


if __name__ == "__main__":
    main()
