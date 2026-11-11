#!/usr/bin/env python3
"""Build deterministic, single-strike private Apple emoji resource (fontTools 4.63.0)."""
import argparse
import hashlib
import io
from pathlib import Path
import zipfile
import fontTools
from fontTools.ttLib import TTFont

SOURCE_SHA256 = "18e48f1785564fbf511241e0963b265057bfe742036d8543406c6ce07e48ec0b"

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    if fontTools.__version__ != "4.63.0":
        raise SystemExit("fontTools 4.63.0 required for deterministic output")
    with args.source.open("rb") as f:
        digest = hashlib.file_digest(f, "sha256").hexdigest()
    if digest != SOURCE_SHA256:
        raise SystemExit("Apple emoji source SHA-256 mismatch")
    font = TTFont(args.source, recalcTimestamp=False)
    strikes = font["CBLC"].strikes
    indexes = [i for i, s in enumerate(strikes) if (s.bitmapSizeTable.ppemX, s.bitmapSizeTable.ppemY) == (96, 96)]
    if len(indexes) != 1 or len(strikes) != len(font["CBDT"].strikeData):
        raise SystemExit("Unexpected bitmap strikes")
    idx = indexes[0]
    font["CBLC"].strikes = [strikes[idx]]
    font["CBDT"].strikeData = [font["CBDT"].strikeData[idx]]
    buf = io.BytesIO()
    font.save(buf, reorderTables=True)
    data = buf.getvalue()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    info = zipfile.ZipInfo("apple-bundled96.ttf", (1980, 1, 1, 0, 0, 0))
    info.compress_type = zipfile.ZIP_DEFLATED
    info.external_attr = 0o600 << 16
    with zipfile.ZipFile(args.output, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as out:
        out.writestr(info, data, compresslevel=9)
    print(f"font bytes={len(data)} sha256={hashlib.sha256(data).hexdigest()}")
    print(f"zip bytes={args.output.stat().st_size} sha256={hashlib.sha256(args.output.read_bytes()).hexdigest()}")
if __name__ == "__main__":
    main()
