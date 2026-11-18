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
            original_box = record["box"]
            others = [item["region"] for pos, item in enumerate(records) if pos != index]
            candidates = []
            if record["explicit"]:
                candidates.append(original_box)
            else:
                candidates.append(_bubble_box(result, record["region"], others, original_box))
                left, top, right, bottom = original_box
                # A nearly uniform light OCR rectangle is itself a safe text
                # area after erasing; do not extend into nearby artwork.
                sample = result[top:bottom, left:right]
                if (sample.size and np.mean(np.min(sample, axis=2) >= 235) >= 0.995
                        and not any(np.any(other[top:bottom, left:right]) for other in others)):
                    candidates.append(original_box)
                candidates.append(_local_box(result, record["region"], others, original_box))
            accepted = False
            for box in candidates:
                if box is None:
                    continue
                left, top, right, bottom = box
                if np.any(protected[top:bottom, left:right]) or any(_overlaps(box, old[0]) for old in placements):
                    continue
                try:
                    fitted = _fit(record["text"], font, draw, box)
                except ValueError:
                    continue
                placements.append((box, fitted))
                accepted = True
                break
            if not accepted:
                rejected.add(index)
        if rejected:
            active.difference_update(rejected)
            continue
        break
    else:
        result_image = Image.fromarray(rgb.copy())
        draw = ImageDraw.Draw(result_image)
        placements = []

    # Secondary placements use only completely plain, opaque white pixels near
    # the original region. They cannot cover any recognized region or placed text.
    panel_font = ImageFont.truetype(str(font), 16)
    panel_draw = ImageDraw.Draw(Image.new("RGB", (1, 1), "white"))
    line_height = max(22, panel_draw.textbbox((0, 0), "Ag", font=panel_font)[3] + 7)
    occupied = np.zeros((height, width), dtype=np.uint8)
    for record in records:
        occupied = cv2.bitwise_or(occupied, record["region"])
    for (x0, y0, x1, y1), _ in placements:
        occupied[y0:y1, x0:x1] = 255
    opaque = np.asarray(alpha)
    blocked = (np.min(rgb, axis=2) < 250) | (opaque < 250) | (occupied != 0)
    blocked = cv2.dilate(blocked.astype(np.uint8), np.ones((5, 5), dtype=np.uint8))
    integral = cv2.integral(blocked, sdepth=cv2.CV_32S)
    near = []
    for index, record in enumerate(records):
        if index in active or not isinstance(record["text"], str) or not record["text"].strip():
            continue
        # Ambiguous overlapping source areas always remain unchanged.
        if any(np.any((record["region"] != 0) & (other["region"] != 0))
               for pos, other in enumerate(records) if pos != index):
            continue
        x0, y0, x1, y1 = record["box"]
        best = None
        for box_width in (80, 120, 160, 200, 240):
            if box_width > width:
                continue
            lines = _panel_wrap(record["text"], panel_font, panel_draw, box_width - 16)
            if not lines:
                continue
            box_height = len(lines) * line_height + 12
            if box_height > min(200, height):
                continue
            # Check a bounded grid around the source; all pixels must be white.
            for y in range(max(0, y0 - 160), min(height - box_height, y1 + 160) + 1, 12):
                for x in range(max(0, x0 - 160), min(width - box_width, x1 + 160) + 1, 12):
                    right, bottom = x + box_width, y + box_height
                    if integral[bottom, right] - integral[y, right] - integral[bottom, x] + integral[y, x]:
                        continue
                    if any(_overlaps((x, y, right, bottom), old[0]) for old in near):
                        continue
                    distance = abs((x + right) / 2 - (x0 + x1) / 2) + abs((y + bottom) / 2 - (y0 + y1) / 2)
                    if best is None or distance < best[0]:
                        best = (distance, (x, y, right, bottom), lines)
        if best is not None:
            _, box, lines = best
            near.append((box, lines, index, panel_font, False))
        else:
            # Compact opaque label directly over a recognized text rectangle.
            # Any darkness outside this OCR region may be art: do not cover it.
            box = (x0, y0, x1, y1)
            w, h = x1 - x0, y1 - y0
            patch = rgb[y0:y1, x0:x1]
            own = record["region"][y0:y1, x0:x1] != 0
            others = any(np.any(item["region"][y0:y1, x0:x1])
                         for pos, item in enumerate(records) if pos != index)
            collision = any(_overlaps(box, item[0]) for item in placements + near)
            if (w >= 55 and h >= 35 and w * h <= min(24000, width * height * 0.02)
                    and not others and not collision and np.all(opaque[y0:y1, x0:x1] >= 250)
                    and np.mean(np.min(patch, axis=2) >= 235) >= 0.90
                    and np.mean((np.min(patch, axis=2) < 220) & ~own) <= 0.005):
                face = ImageFont.truetype(str(font), 14)
                lines = _panel_wrap(record["text"], face, panel_draw, w - 12)
                compact_line_height = 19
                if lines and len(lines) * compact_line_height + 10 <= h:
                    near.append((box, lines, index, face, True))

    for (left, top, right, bottom), (face, text, spacing, text_bounds) in placements:
        x = left + (right - left - (text_bounds[2] - text_bounds[0])) / 2 - text_bounds[0]
        y = top + (bottom - top - (text_bounds[3] - text_bounds[1])) / 2 - text_bounds[1]
        draw.multiline_text((x, y), text, font=face, fill=(0, 0, 0), spacing=spacing, align="center")
    for (left, top, right, bottom), lines, index, face, cover in near:
        if cover:
            draw.rectangle((left, top, right - 1, bottom - 1), fill=(255, 255, 255), outline=(100, 100, 100), width=1)
        y = top + 6
        for line in lines:
            draw.text((left + 6, y), line, font=face, fill=(0, 0, 0))
            y += 19 if cover else line_height
    result_image.putalpha(alpha)

    # Place remaining translations alongside the page, close to their source y.
    # Side allocation is lazy: no side means no extra width.
    side_width = 220
    gap = 8
    sides = {"left": [], "right": []}
    side_height = {"left": 0, "right": 0}
    side_indices = set()
    near_indices = {placed[2] for placed in near}

    pending = sorted((i for i in range(len(records)) if i not in active and i not in near_indices),
                     key=lambda i: records[i]["box"][1] + records[i]["box"][3])
    for index in pending:
        record = records[index]
        text = record["text"]
        if not isinstance(text, str) or not text.strip():
            continue
        x0, y0, x1, y1 = record["box"]
        lines = _panel_wrap(text, panel_font, panel_draw, side_width - 24)
        if not lines:
            continue
        item_height = (len(lines) + 1) * line_height + 12
        desired = max(0, (y0 + y1 - item_height) // 2)
        primary = "left" if x0 + x1 < width else "right"
        secondary = "right" if primary == "left" else "left"
        for side in (primary, secondary):
            added = side_width if not sides[side] else 0
            current_extra = side_width * sum(bool(v) for v in sides.values())
            proposed_width = width + current_extra + added
            if proposed_width > 8192 or proposed_width * height > 32_000_000:
                continue
            needed = item_height + (gap if sides[side] else 0)
            if side_height[side] + needed > height:
                continue
            side_height[side] += needed
            sides[side].append((desired, index, lines, item_height))
            side_indices.add(index)
            break

    side_items = []
    for side, items in sides.items():
        tail_height = side_height[side]
        next_top = 0
        for desired, index, lines, item_height in items:
            y = min(max(desired, next_top), height - tail_height)
            side_items.append((side, y, index, lines, item_height))
            next_top = y + item_height + gap
            tail_height -= item_height + gap

    left_pad = side_width if sides["left"] else 0
    right_pad = side_width if sides["right"] else 0
    output_width = width + left_pad + right_pad
    max_height = min(8192, 32_000_000 // output_width)
    panel_items, panel_height = [], 12
    for index, record in enumerate(records):
        if index in active or index in near_indices or index in side_indices:
            continue
        text = record["text"]
        if not isinstance(text, str) or not text.strip():
            continue
        x, y, _, _ = record["box"]
        lines = _panel_wrap(f"#{index + 1} ({x}, {y}): {text}", panel_font, panel_draw, output_width - 24)
        if not lines:
            continue
        block_height = len(lines) * line_height + 10
        if height + panel_height + block_height + 8 > max_height:
            continue
        panel_items.append(lines)
        panel_height += block_height
    fallback = len(panel_items) + len(side_items)
    if not active and not near and not fallback:
        raise ValueError("No text blocks could be rendered safely")
    output_height = height + panel_height + 8 if panel_items else height
    if side_items or panel_items:
        canvas = Image.new("RGBA", (output_width, output_height), "white")
        canvas.paste(result_image, (left_pad, 0))
        canvas_draw = ImageDraw.Draw(canvas)
        for side, y, index, lines, item_height in side_items:
            sx = 0 if side == "left" else left_pad + width
            # Number and original coordinates serve as an unambiguous pointer.
            source_x, source_y, _, _ = records[index]["box"]
            canvas_draw.rectangle((sx + 4, y, sx + side_width - 5, y + item_height - 1),
                                  outline=(185, 185, 185, 255), width=1)
            prefix = f"#{index + 1}  {'→' if side == 'left' else '←'} ({source_x}, {source_y})"
            yy = y + 6
            canvas_draw.text((sx + 12, yy), prefix, font=panel_font, fill=(70, 70, 70, 255))
            yy += line_height
            for line in lines:
                canvas_draw.text((sx + 12, yy), line, font=panel_font, fill=(0, 0, 0, 255))
                yy += line_height
        y = height + 12
        for lines in panel_items:
            for line in lines:
                canvas_draw.text((12, y), line, font=panel_font, fill=(0, 0, 0, 255))
                y += line_height
            y += 10
        result_image = canvas
    with open(output, "xb") as target:
        result_image.save(target, format="PNG")
    return {"rendered": len(active) + len(near) + fallback,
            "skipped": len(records) - len(active) - len(near) - fallback,
            "fallback": fallback, "side": len(side_items), "left_pad": left_pad}


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
