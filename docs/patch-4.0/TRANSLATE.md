# Translate foundation

Translate is a dedicated shelf view for managed images explicitly published
as translation results. Catalogue v14 adds the result link and a separate tag
scope. Ordinary clipboard images, upscale results and original inputs do not
gain Translate membership through their filename or source label.

`crates/baloh-collection` is a separate Rust library for ordered payloads
with stable entry keys. An image translation job will use one entry per
original page. It can fill each entry's result or error independently while
preserving the input order. Katran owns image storage, job state and shelf
publication. The collection library is independent of the Katran catalogue
and may later be reused by Avilon or another project.

The collection library defines in-memory behavior. File encoding, validation
of standalone .bull archives, and QuickLook are later work. A translation
batch should reopen as a single ordered result, with successful pages and
per-page failures associated with their originals.

## First functional gate

Russian and English are the primary translation languages. Recognition must
also cover Japanese, Korean, and Chinese. Start with one page containing
identified regions and a known reading order. Record the detected regions,
transcription, ordered dialogue, translation, cleaned background and final
lettering as independently inspectable intermediate results. Evaluate vertical
and right-to-left page order with a separately labelled manga example.

[PP-OCRv5](https://www.paddleocr.ai/v3.6.0/en/version3.x/algorithm/PP-OCRv5/PP-OCRv5_multi_languages.html)
is a candidate recognizer with models for these languages. A model's
listed language is coverage evidence only; the project still needs a real
page run to verify mixed scripts and vertical dialogue. The local ComfyUI
environment already has PyTorch, Transformers and OpenCV, but no working
OCR and inpainting translation pipeline has been verified. Select the
translation backend and explicit optional component setup after the fixtures.

The Translate shelf can be opened before any results exist. It displays
only accepted managed outputs once that publishing path is implemented.

## Local one-page probe

The prototype lives in `scripts/image-translate`; it is intentionally separate
from the Katran tab until the page gate passes. Test images, model packs,
intermediate JSON and generated previews stay outside Git.

1. `ocr_page.py` uses PaddlePaddle 3.2.2 / PaddleOCR 3.3.3 in a separate Python
   environment. The cached Japanese PP-OCRv5 server detector and recognizer are
   read from `target/build/translate-probe/ocr/paddlex/official_models`.
   `--prepare` explicitly permits first-time model fetching.
2. `manga_recognize.py` regroups vertical columns and rereads the cropped
   Japanese dialogue with `kha-white/manga-ocr-base` at revision
   `aa6573bd10b0d446cbf622e29c3e084914df9741`. The model pack is opened
   offline, with weights-only loading. It does not infer panel order.
3. `translate_page.py` uses an already
   downloaded model over the local Ollama interface. The test uses the installed
   `qwen3:30b` tag, with cloud disabled and the service bound to loopback.
   It accepts a known transcript (`--ordered`) for separating translation and
   lettering defects from recognition errors.
4. `render_page.py` erases detected lettering and draws the translated blocks
   with a local font. Preview output is a separate PNG. Its text-mask heuristic
   targets dark lettering and lighter backgrounds. It expands horizontal
   text only inside a safely detected enclosed light region.

On the approved simple Japanese page PP-OCRv5 detected five vertical columns,
and the prototype grouped them into two dialogue regions in reading order.
Recognition dropped characters in both regions and caused a mistranslation.
Using a checked Japanese transcript with the same detected geometry isolated
the downstream steps and produced a readable Russian preview. With the
Manga OCR second pass both dialogue blocks matched the Japanese source
on manual inspection of the simple page. The same automatically recognized
blocks were translated to Russian by the local model and rendered directly
into both bubbles. The lettering stayed within the contours, with some pale
stripes remaining from background reconstruction. The larger
multi-panel page produced 53 detections (51 nonempty) and 19 provisional groups.
Its second pass still contains incorrect merges. Whole-page panel order,
furigana and lettering over shaded art still require
separate verification. The local 30B translator is a probe, not a release
requirement.
