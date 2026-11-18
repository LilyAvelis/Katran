# Translate foundation

Translate is a dedicated shelf view for managed images explicitly published
as translation results. Catalogue v14 adds the result link and a separate tag
scope. Ordinary clipboard images, upscale results and original inputs do not
gain Translate membership through their filename or source label.
Published results are exclusive to Translate. Tag sorting includes Translate
and uses its own tag assignments, including in background tab previews.

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
Image translation reports the locally detected components and the English
output language. Output is a separate, managed PNG in Translate. The renderer
keeps text placements disjoint and uses readable spacing and a minimum font
size. If it cannot safely replace a region, it preserves the original and
adds its numbered translation to a white panel below the artwork. The panel
can increase PNG height within the same dimension and pixel limits. The
result reports panel entries and skips separately. Nothing is published if
no lettering can be placed safely in either location.
Cancellation before publication leaves no result. Source bytes stay intact.

The action uses PP-OCRv5, Manga OCR and the installed `qwen3:4b-instruct` tag.
Use the dedicated Instruct variant to avoid generating reasoning tokens.
Japanese is the only exposed recognition language for this integration.
The translator requires full GPU placement and a complete English answer.
The English-only rollout follows the owner's acceptance scope for this stage.
The worker embeds the tested scripts and lazily starts both OCR workers and
a private loopback Ollama with cloud disabled. Paddle stays in RAM, Manga OCR
stays on its selected device, and Qwen stays on the GPU between translations.
Closing the application releases the owned Windows process trees. Cancelling a
busy OCR stage or a worker failure can require that worker to load again. Nothing is
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

On the approved simple page, separate cold workers spent 7.6 s in Paddle
detection and recognition, 9.6 s in Manga OCR, and 0.5 s rendering. The earlier
Qwen3 4B reasoning run spent 14.8 s generating 4,578 tokens. With the same OCR
input on RTX 5090, Instruct returned both numbered translations in 2.2 s,
including weight load. On the complex page it returned all 19 recognized
blocks in 1.7 s with the model already loaded. Grouping and transcription on
that page still need work. These measurements cover separate stages.

With resident workers, the normal Katran API processed the simple page in
18.7 s on first use and 4.5 s on immediate repeat. The same session processed
the complex page in 21.6 s, lettering 14 blocks and leaving 5 original regions.

The subsequent stage probe on the same warm complex page measured Paddle
15.1 → 0.6 s, Manga OCR 1.3 → 0.8 s, Qwen 1.6 → 1.5 s, and rendering
1.9 → 0.9 s. The detector and Manga OCR produced equal region geometry and
transcripts on both test pages. Ollama logged five translation requests for
the original complex page and one for the updated page. The old five layout
skips came from three ambiguous text masks, one area too narrow for the
translation, and one overlapping block. The current output contains five
in-page placements and fourteen entries in the panel, with zero skips.
This covers recognized regions. OCR grouping and accuracy on dense artwork
still limit the result.
The full API check then measured 15.5 s for the first simple page, 1.3 s
for the repeated simple page, and 3.9 s for the warm complex page, including
publication. Both simple translations remained inside their bubbles.

## Local one-page probe

The standalone probe lives in `scripts/image-translate`. Test images, model packs,
intermediate JSON and generated previews stay outside Git.

1. `ocr_page.py` uses PaddlePaddle 3.2.2 / PaddleOCR 3.3.3 in a separate Python
   environment. It reads the PP-OCRv5 server detector from
   `target/build/translate-probe/ocr/paddlex/official_models`. It uses oneDNN
   on CPU, falling back to the regular CPU backend if oneDNN is unavailable
   at inference. `--prepare` explicitly permits first-time model fetching.
2. `manga_recognize.py` regroups vertical columns and rereads the cropped
   Japanese dialogue with `kha-white/manga-ocr-base` at revision
   `aa6573bd10b0d446cbf622e29c3e084914df9741`. The model pack is opened
   offline, with weights-only loading. Up to four crops from the same page
   are read together. It does not infer panel order.
3. `translate_page.py` uses the already downloaded
   `qwen3:4b-instruct` model over the local Ollama interface, with cloud
   disabled and the service bound to loopback. A single structured request
   translates the page, and the worker validates the complete set of IDs.
   It accepts a known transcript (`--ordered`) for separating translation and
   lettering defects from recognition errors.
4. `render_page.py` erases detected lettering and draws the translated blocks
   with a local font. Preview output is a separate PNG. Its text-mask heuristic
   targets dark lettering and lighter backgrounds. In-page placement uses
   a safe light region and leaves other lettering regions clear.

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
