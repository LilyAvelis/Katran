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
    for size in range(min(160, int(height)), 7, -1):
        font = ImageFont.truetype(str(font_path), size)
        lines = _wrap(text, font, draw, width)
        if lines is None:
            continue
        spacing = max(1, round(size * 0.12))
        content = "\n".join(lines)
        bounds = draw.multiline_textbbox((0, 0), content, font=font, spacing=spacing, align="center")
        if bounds[2] - bounds[0] <= width and bounds[3] - bounds[1] <= height:
            return font, content, spacing, bounds
    raise ValueError("translated text cannot fit in the supplied text area at minimum font size 8")


def _bubble_box(cleaned, region, other_regions, fallback):
    """Return an interior rectangle only for an isolated, light component."""
    height, width = region.shape
    light = (cv2.cvtColor(cleaned, cv2.COLOR_RGB2GRAY) >= 220).astype(np.uint8)
    _, labels, stats, _ = cv2.connectedComponentsWithStats(light, 8)
    candidates = labels[(region != 0) & (labels != 0)]
    if not candidates.size:
        return fallback
    counts = np.bincount(candidates)
    label = int(np.argmax(counts[1:]) + 1)
    x, y, w, h, area = stats[label]
    # A region touching the canvas edge or another text block is not a
    # confidently bounded bubble. Limit unusually large connected whitespace.
    if x == 0 or y == 0 or x + w >= width or y + h >= height or area > width * height * 0.45:
        return fallback
    component = (labels == label).astype(np.uint8)
    if any(np.any(component & (other != 0)) for other in other_regions):
        return fallback
    if counts[label] < cv2.countNonZero(region) * 0.65:
        return fallback
    # Erode to keep text away from outlines/nearby art. The largest all-light
    # rectangle must contain the original block centre, not a nearby blank area.
    cleaned_gray = cv2.cvtColor(cleaned, cv2.COLOR_RGB2GRAY)
    closed = cv2.morphologyEx(component, cv2.MORPH_CLOSE, np.ones((7, 7), dtype=np.uint8))
    component = np.where((region != 0) & (cleaned_gray >= 195), closed, component).astype(np.uint8)
    safe = cv2.erode(component, np.ones((9, 9), dtype=np.uint8))
    cx = (fallback[0] + fallback[2]) // 2
    cy = (fallback[1] + fallback[3]) // 2
    if not safe[cy, cx]:
        return fallback
    heights = np.zeros(width, dtype=np.int32)
    best = None
    best_area = 0
    for row in range(height):
        heights = np.where(safe[row], heights + 1, 0)
        stack = []
        for col in range(width + 1):
            current = int(heights[col]) if col < width else 0
            start = col
            while stack and stack[-1][1] > current:
                start, rect_height = stack.pop()
                top = row + 1 - rect_height
                if start <= cx < col and top <= cy <= row:
                    rect_area = (col - start) * rect_height
                    if rect_area > best_area:
                        best_area = rect_area
                        best = (start, top, col, row + 1)
            if current and (not stack or stack[-1][1] < current):
                stack.append((start, current))
    if best is None or best_area <= (fallback[2] - fallback[0]) * (fallback[3] - fallback[1]):
        return fallback
    return best


def render(source, blocks, output, font):
    image = Image.open(source).convert("RGB")
    rgb = np.asarray(image)
    height, width = rgb.shape[:2]
    if not isinstance(blocks, list):
        raise ValueError("blocks must be an ordered list")
    if not Path(font).is_file():
        raise ValueError("font file not found")
    gray = cv2.cvtColor(rgb, cv2.COLOR_RGB2GRAY)
    erase = np.zeros((height, width), dtype=np.uint8)
    regions = []
    # Prepare and validate all regions before producing an output file.
    for index, block in enumerate(blocks):
        try:
            if not isinstance(block, dict) or not isinstance(block.get("text"), str) or not block["text"].strip():
                raise ValueError("text must be a nonempty string")
            raw = block.get("polygons")
            if raw is None:
                raw = [block["polygon"]]
            if not isinstance(raw, list) or not raw:
                raise ValueError("polygons must be a nonempty list")
            polygons = [_polygon(value, width, height) for value in raw]
            region = np.zeros_like(erase)
            for polygon in polygons:
                cv2.fillPoly(region, [polygon], 255)
            samples = gray[region != 0]
            # Select dark ink relative to the local light background. Reject an
            # ambiguous area rather than erasing a whole bubble or illustration.
            background = float(np.percentile(samples, 85))
            if background < 90:
                raise ValueError("region background is too dark to isolate dark lettering")
            threshold = min(background - 25, 190)
            ink = ((gray < threshold) & (region != 0)).astype(np.uint8) * 255
            coverage = cv2.countNonZero(ink) / len(samples)
            if coverage < 0.001 or coverage > 0.55:
                raise ValueError("cannot isolate dark lettering safely inside polygon")
            # A one-pixel dilation includes antialiasing; clip it to the polygons.
            ink = cv2.dilate(ink, np.ones((3, 3), dtype=np.uint8), iterations=1)
            erase = cv2.bitwise_or(erase, cv2.bitwise_and(ink, region))
            if "text_box" in block:
                box = block["text_box"]
                if not isinstance(box, list) or len(box) != 4 or not all(isinstance(v, (int, float)) and math.isfinite(v) for v in box):
                    raise ValueError("text_box must be [left, top, right, bottom]")
                box = tuple(int(v) for v in box)
            else:
                all_points = np.concatenate(polygons)
                x, y, w, h = cv2.boundingRect(all_points)
                box = (x, y, x + w, y + h)
            left, top, right, bottom = box
            if not (0 <= left < right <= width and 0 <= top < bottom <= height):
                raise ValueError("text_box exceeds image bounds or is empty")
            regions.append((region, box, "text_box" in block, block["text"]))
        except (KeyError, TypeError, ValueError, cv2.error) as exc:
            raise ValueError(f"block {index + 1}: {exc}") from exc
    result = cv2.inpaint(rgb, erase, 3, cv2.INPAINT_TELEA) if cv2.countNonZero(erase) else rgb.copy()
    result_image = Image.fromarray(result)
    draw = ImageDraw.Draw(result_image)
    placements = []
    for index, (region, box, explicit, text) in enumerate(regions):
        if not explicit:
            others = [item[0] for pos, item in enumerate(regions) if pos != index]
            box = _bubble_box(result, region, others, box)
        try:
            placements.append((box, _fit(text, font, draw, box)))
        except ValueError as exc:
            raise ValueError(f"block {index + 1}: {exc}") from exc
    for (left, top, right, bottom), (face, text, spacing, bounds) in placements:
        x = left + (right - left - (bounds[2] - bounds[0])) / 2 - bounds[0]
        y = top + (bottom - top - (bounds[3] - bounds[1])) / 2 - bounds[1]
        draw.multiline_text((x, y), text, font=face, fill=(0, 0, 0), spacing=spacing, align="center")
    result_image.save(output)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source")
    parser.add_argument("blocks_json")
    parser.add_argument("output")
    parser.add_argument("font")
    args = parser.parse_args()
    with open(args.blocks_json, encoding="utf-8") as stream:
        blocks = json.load(stream)
    render(args.source, blocks, args.output, args.font)


if __name__ == "__main__":
    main()
