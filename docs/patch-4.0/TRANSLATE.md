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

Select a Japanese image and open Ctrl+K → AI Commands → Translate image
from Japanese. The action is independently rebindable. Settings → AI →
Image translation selects Interface language, Russian or English and reports
the locally detected components. The default follows Katran's interface
language at submission. Output is a separate, managed PNG in Translate.
Cancellation before publication leaves no result. Source bytes stay intact.

The action uses PP-OCRv5, Manga OCR and the installed `qwen3:1.7b` tag.
Japanese is the only exposed recognition language for this integration.
The worker embeds the tested scripts, starts an isolated loopback Ollama
with cloud disabled, and contains its Windows process tree. Nothing is
downloaded on invoking the action. Setup/Portable can point
`KATRAN_TRANSLATE_ROOT` at an existing component directory with `venv`,
`paddlex/official_models`, and `manga-model`, or use the active profile's
`translate-runtime` directory. The verified local development runtime can
reuse the existing probe files. Manga recognition and rendering use the
existing portable ComfyUI Python. `KATRAN_OLLAMA_EXE` can point at a local
executable, and `OLLAMA_MODELS` selects its model store.
If the active profile has the model under `translate-ollama`, Katran uses
that store first and passes its exact path to its private Ollama process.
This avoids dependence on environment variables inherited from the launcher.

## Local one-page probe

The standalone probe lives in `scripts/image-translate`. Test images, model packs,
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
