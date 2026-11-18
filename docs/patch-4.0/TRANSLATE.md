# Translate and BULL in Katran 4.0

Lilia accepted the local Translate and BULL experience after trying batch
translation, numbered references and the updated viewer. The accepted
launcher runtime was `c6114a60454a`, source `422b708`. The benchmark below
uses the supplied pages and includes publication to Translate.

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

The collection library also defines a local v1 portable PNG container. The
container can hold any sequence of PNG pages. Katran uses it for successful
batch outputs and reports failed input positions in the job receipt. See
[the collection contract](BULL.md).

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

Select an image and open Ctrl+K → AI Commands → Translate image from Japanese,
Simplified Chinese, Traditional Chinese or Korean. Each action is independently
rebindable. Both forms of Chinese use the installed `PP-OCRv5_server_rec`.
Traditional Chinese uses its own translation prompt and groups adjacent
vertical columns from right to left.
Settings → AI → Image translation reports local availability by language and the English
output language. Output is a separate, managed PNG in Translate. The renderer
keeps text placements disjoint and uses readable spacing and a minimum font
size. If it cannot safely replace a region, it can add a white card nearby
or put the numbered translation on a white side margin. Remaining cards can
use the lower panel. Margins may increase PNG dimensions within the same
size limits. The result reports margin entries, panel entries and skips.
Bright matching numbers identify translations on the margins and the
corresponding recognized areas on the image. When the image has no safe
space for a number, its card shows the original coordinates.
Enable **Numbered annotations only** in Settings → AI → Image translation
to keep original lettering and put every translation in a numbered card.
The renderer bypasses letter removal and in-bubble layout in this mode.
The setting applies to newly submitted single-page and batch jobs. The
hybrid layout remains the default.
Nothing is published if no lettering can be placed safely.
Cancellation before publication leaves no result. Source bytes stay intact.
The Katran UI opens a newly saved single-page PNG in QuickLook. QuickLook
receives a named snapshot of the saved revision and handles Space dismissal.
If external preview fails, the published result stays in Translate.

For a sequence of 2–32 images, use Ctrl to choose the order by clicking, or
Shift to select from the first page to the last in either direction. The
same translation command accepts an image BULL and uses its saved page
order. The action steps through that order with progress and cancellation.
Once published, the collection appears as one card in Translate and opens
in the Katran page viewer. The original BULL remains available, and even a
BULL with one page produces a new BULL. A partially translated BULL keeps
failed pages as originals at their existing positions and reports stage
errors in the completion dialog. If every page fails, nothing is published.
The separate QuickLook action uses its collection viewer when installed.

The action uses PP-OCRv5, Manga OCR and the installed `qwen3:4b-instruct` tag.
Use the dedicated Instruct variant to avoid generating reasoning tokens.
Source language is chosen by the command. Automatic source detection is
outside this checkpoint.
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
The optional recognizers live under the same `paddlex/official_models` path
as the detector. Use folders named `PP-OCRv5_server_rec` for Simplified
Chinese and `korean_PP-OCRv5_mobile_rec` for Korean, each containing
`inference.json`, `inference.pdiparams` and `inference.yml`. The model
packages are listed in the [PaddleOCR recognition documentation](https://www.paddleocr.ai/latest/en/version3.x/module_usage/text_recognition.html).

## Single-page performance report · 2026-11-18

The timings below come from local Windows tests using the provided Japanese
pages, RTX 5090 for Qwen and Manga OCR, and CPU for Paddle. Full API timing
starts at action submission and includes the saved result in Translate.
Stage timings come from a separate instrumented reproduction.

| Warm complex page | Previous | Optimized |
| --- | ---: | ---: |
| Paddle detection and initial recognition | 15.1 s | 0.6 s |
| Manga OCR | 1.3 s | 0.8 s |
| Qwen translation | 1.6 s | 1.5 s |
| Rendering in the stage probe | 1.9 s | 0.9 s |
| Full API, including the later side-margin layout | 21.6 s | 4.1 s |

The largest change was in Japanese OCR. The previous Paddle pipeline detected
and read each crop, then Manga OCR read the crops again. Now Paddle runs only
the detector, with oneDNN on CPU, and Manga OCR reads up to four crops in one
GPU call. Both versions found the same 53 polygons and produced the same 19
grouped transcriptions on the complex fixture. This comparison measures speed
at the same OCR output quality on these pages.

Ollama receives all region IDs and text for the page in one structured call.
The earlier warm Instruct implementation made five calls on the complex
fixture. The table shows the effect of combining them after the separate
change from reasoning-enabled Qwen3 4B to Qwen3 4B Instruct. The private
Ollama process and both OCR processes stay alive after the first operation,
so repeated pages reuse loaded models until Katran exits.

The most recent cold simple-page API run before marker changes took 15.1 s.
Repeating it in the same session took 1.3 s. The complex page's full warm
path was about 5.2 times as fast. The detector stage by itself was about
25 times as fast. These are fixture measurements from one machine.
With the numbered markers, a later API run measured 14.2 s cold simple,
1.2 s warm simple, and 3.7 s warm complex. All twelve margin cards in the
complex fixture had matching markers. Cold initialization varied between runs.
On the complex fixture, a renderer-only probe measured 1.24 s for the
hybrid layout and 0.53 s for numbered-only. The latter produced 19 paired
markers and preserved every source pixel outside the markers in that probe.

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
translation, and one overlapping block. That intermediate output contained five
in-page placements and fourteen entries in the panel, with zero skips.
This covers recognized regions. OCR grouping and accuracy on dense artwork
still limit the result.
The full API check then measured 15.5 s for the first simple page, 1.3 s
for the repeated simple page, and 3.9 s for the warm complex page, including
publication. Both simple translations remained inside their bubbles.

The next layout pass on the same complex input placed seven translations in
the artwork and twelve in the side margins, with none below the page. Separate
synthetic pages verify the Chinese and Korean command routes using horizontal
dialogue. Dense, vertical or stylized input requires separate quality checks.
The same session completed the complex Japanese page in 4.1 s, the first
Chinese page in 3.1 s, and the first Korean page in 2.5 s. Both synthetic
pages yielded two in-place translations and no skips.

## Local one-page probe

The standalone probe lives in `scripts/image-translate`. Test images, model packs,
intermediate JSON and generated previews stay outside Git.

1. `ocr_page.py` uses PaddlePaddle 3.2.2 / PaddleOCR 3.3.3 in a separate Python
   environment. It reads the PP-OCRv5 server detector from
   `target/build/translate-probe/ocr/paddlex/official_models`. It uses oneDNN
   on CPU, falling back to the regular CPU backend if oneDNN is unavailable
   at inference. Simplified Chinese and Korean load their matching local
   Paddle recognizers. `--prepare` explicitly permits first-time fetching
   for the standalone Japanese detector probe.
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
