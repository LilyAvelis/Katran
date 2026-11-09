"""Mechanical PNG/ICO export; generated artwork is never altered in place.

Optional authoring dependency: Pillow. Normal Rust builds use checked-in assets.
"""
from pathlib import Path
from PIL import Image

directory = Path(__file__).resolve().parent.parent / "assets" / "branding"
source = Image.open(directory / "katran-logo.png").convert("RGBA")
if source.width != source.height:
    raise ValueError("Katran logo must be square")
source.resize((256, 256), Image.Resampling.LANCZOS).save(directory / "katran-icon.png")
source.save(
    directory / "katran.ico",
    format="ICO",
    sizes=[(edge, edge) for edge in (16, 20, 24, 32, 40, 48, 64, 96, 128, 256)],
)
