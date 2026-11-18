# Katran 4.0 run · 2026-11-18

## Final handoff

Lilia closed the local 4.0 run after confirming Translate and batch order,
numbered references, image upscale, Lens, collection workflows and video
playback inside the same BULL that had failed before. She confirmed the prior
clip stops on a page turn. The final viewer polish at `c4fc0a6` names the
collection, centers the pager and opens the player after the entrance.
Its 27 targeted headless tests passed. The last animation timing awaits a
Windows check. Audio extraction and Files/clipboard publication have
synthetic API checks. See [local release notes](../releases/next.md#katran-40--accepted-local-build).

The last verified primary Raycast runtime is `23df78286fa2`. Its manifest
binds the executable to its inputs and records package version `3.8.0`.
The retained `4bda0a94ca40` runtime is the immediate rollback option for the
viewer polish. A rollback across catalogue v15 must also restore the matching
pre-migration profile backup. Public 4.0 distribution has its own authorization
and verification. See [development and agent map](../development.md).

- Collections, typed payloads, and Folder safety. [BULL](BULL.md) · [BS](BS.md)
- Warm/cold timings, OCR, one Qwen request per page, and batch receipts. [Translate](TRANSLATE.md)
- Model discovery, exact scaling and audio publication. [Image upscale](IMAGE-UPSCALE.md) · [Audio extraction](AUDIO-EXTRACTION.md)
- Windows QuickLook adapter scope and setup. [Image BULL plugin](../../integrations/quicklook/QuickLook.Plugin.BullViewer/README.md)

## Session history and ownership

Polly closed Astra's two P2 findings with Nemi on Text/Links identity and
Fenna on batch translation feedback. Synthetic pack/reingest/unpack tests
verify visible cards and retained ownership. A synthetic renderer receipt
and a headless UI test cover successful pages with skipped lettering
regions. Polly also verified both Chinese commands on the owner-supplied
visible manga excerpt. The dense screen tone remained in the input. Both
routes published a PNG with two rendered groups and zero skipped regions.

For Traditional Chinese and partial translation, Fenna owned Settings and
the result dialog. Polly wired the command, reused the Chinese Paddle
recognizer, added vertical grouping and preserved failed BULL pages.
A local OCR and Qwen run translated pages 1 and 3 of a four-page synthetic
BULL. Pages 2 and 4 contained no text and remained exact PNG copies at their
original positions. The source BULL was unchanged.

For BULL as batch input, Fenna owned the Upscale completion UI and Polly
owned ordered input preparation, domain routing, publication and verification.
The local Real-ESRGAN ×2 API fixture transformed a two-page synthetic BULL,
verified exact dimensions in stored order, checked both cards in Images,
and compared the original archive byte-for-byte. The Folder API fixture
verified availability for image BULL, rejected video BULL from the image
action list, and cancelled a queued BULL translation.

For unified packing, Fenna owned the viewer and action wiring, Vela owned
Folder output publication, and Nemi investigated and fixed completed recycle
stage retention. Polly owned type selection, command integration, shelf
deletion and local verification.

For the standalone BULL shelf follow-up, Fenna owned the rebindable action and
preview wiring, Nemi owned Images and its pin/tag catalogue projection, and
Polly owned the command, pack/unpack jobs, storage and local verification.

The replacement follow-up keeps these owners. Fenna handled first-page cards
and their previews. Nemi handled catalogue v15 and hidden-member queries.
Polly integrated merge ordering, pin/tag inheritance and reversible storage.

For Links BULL, Nemi owned the v2 codec, Vela owned metadata retrieval, Fenna
owned the pager and input, and Polly owned shelf integration, copy/open and QA.

Nemi added the Text codec, Fenna added Markdown and entry input, Vela traced
Links latency, and Polly integrated Text storage and the previous-window lease.

For the Links speed change, Vela limited enrichment to HTML, Nemi owned the
revision-checked blob replacement, Fenna integrated detached UI polling, and
Polly integrated the independent job lane and verification.

For BS, Nemi owned the portable codec, Vela owned file preparation and
previews, Fenna owned UI, and Polly integrated the Files shelf and API.

Polly leads the current session and owns the shared Rust command, dispatcher,
integration, verification and installation. The Notion 4.0 card owns the run
order. The installed baseline is `09244a0440d7` (source `5eb3f93`), with local
main starting at `7d8cc7a`. Native video playback and frame capture are already
present. The video-background dependency diagnosis was separate from this run.

| Session | Model | Stage 1 ownership |
| --- | --- | --- |
| `/root/vela` | Terra / medium | Audio worker, FFmpeg and temporary output |
| `/root/nemi` | Terra / high | Independent synthetic API fixtures and storage checks |
| `/root/fenna` | Terra / high | Actions and extraction dialog after the API gate |
| Kiki Click | Luna / high requested | No session. The runtime rejected the fourth worker with the thread limit. |

The prior 3.8 sessions were not available in this chat. Garden's local mirror
reported a connection failure and did not resolve model names from the grade
labels. Model selection followed the owner-specified mapping and the full
Legion cards read directly in Notion. No model was substituted for Kiki.

The reconnaissance remains ★, audio ★★ (existing runtime and job scheduler,
with a new publication boundary), and AI components ★★★★ (setup and discovery
must converge across existing installations). Image upscale remains ★★★★
pending identification of the exact workflow, translation ★★★★★ until
language/page acceptance, and BULL/QuickLook ★★★★★★ because it adds a
standalone format and an external reader. No later-stage implementation began.

Stage 1 checkpoint `c3f2072` is on local main. The worker, independent API
fixtures, and UI wiring are complete. Polly verified 3 worker tests, 3 API
tests with FFmpeg 9.0.2, and 3 headless UI/palette tests. One optimized build
was staged as `8221aa080f1d`. The previous runtime and a consistent v13 backup
are recorded in the local rollback receipt. Owner Windows acceptance remains
open. Stage 2 is next.

## Scope refinement · static image upscale first

Static image upscale is the first standalone AI feature. A saved video frame
is an ordinary image input. Translation follows later, BULL and QuickLook stay
last, and video processing is outside this run. Limit the preceding Settings → AI
work to installing, discovering, and validating the selected model/runtime
through the same path, while preserving the existing Emoji and Folder features.

Local ComfyUI reconnaissance found `Lily Therapy` and several V33 workflows
referencing `UpscaleModelLoader` with `4x_foolhardy_Remacri.pth` and
`UltimateSDUpscale`. The latter also takes a diffusion model, conditioning,
and a VAE. The weights alone therefore do not reproduce that full pipeline.
Other local graphs use `ImageUpscaleWithModel` with `RealESRGAN_x4.pth`.
Both weight files exist in the configured external upscale model directory.
The owner subsequently chose to use the available model weights directly.
This stage runs the standalone models, so the diffusion pass from
`UltimateSDUpscale` is outside its contract.

Stage 3 defines four independent, rebindable actions under AI Commands in
Ctrl+K: Remacri ×2/×4 and Real-ESRGAN ×2/×4. No model picker or custom size
field is required for this scope. Output is a separate managed image for
the existing wallpaper action. Remacri ×2 uses its native ×4 model followed by
downsampling to exactly twice the input dimensions.

Polly integrated stages 2–3 at `b1e2cc1` and staged runtime `536b7a16621d`
for the primary launcher. Nemi owned bounded local discovery, Vela owned the
inference worker, and Fenna owned the Actions and Settings integration. The
default local discovery found all three weights without an environment
override. The API fixture verified each action against real weights, exact PNG
dimensions, unchanged source and cancellation before publication. Three
discovery and four headless UI/palette checks passed. Native Windows interaction
and visual quality on the owner's images remain open.

The owner confirmed Upscale in Windows and approved the audio Files/clipboard
follow-up. Translate starts with a distinct shelf and a reusable Rust
collection. Russian and English are primary. OCR must also recognize
Japanese, Korean and Chinese. The first functional provider will be verified
on a single page before batching. See [Translate](TRANSLATE.md).

The audio Files/clipboard follow-up and Translate foundation are in
`f55b9cc`, staged as `6fed2bc35739`. Audio API fixtures use a synthetic
clipboard sink; they cover successful file publication and preservation of the
Files pointer on clipboard failure. Collection, tab and v13→v14 migration
fixtures passed. The primary profile is migrated on next launch and has a
verified v13 rollback copy. Native Windows acceptance remains open.

The first Translate probe used the owner's two Japanese samples. Vela set
up isolated PP-OCRv5 and the offline Manga OCR second pass, Fenna implemented
the raster cleanup/typesetting, and Polly verified reading order and local
Ollama translation. Simple-page Russian output is reproducible without a
manual transcript; complex grouping still mixes dialogue and other page text.
Prototype scripts are checkpointed separately from the app build.

Polly integrated the Japanese action and committed at `06d3c01`. Vela
owned subprocess preparation, Nemi owned runtime discovery, and Fenna owned
the Settings → AI card. The approved simple page passed via the normal
dispatcher using the English interface default, and the PNG appeared in
Translate without modifying input. Settings round-trip verified the Russian
override. Targeted headless palette and Settings checks passed. The primary
launcher selects `909c59a69df4`; the v14 backup and prior runtime remain
available. Owner Windows acceptance and complex pages remain open.

For the multilingual and lettering follow-up, the existing sessions were
reused. Vela owned raster placement and side margins, Nemi owned the Paddle
worker, and Fenna owned command discovery, shortcuts and Settings. Polly
owned the API and runtime integration, model placement and end-to-end checks.

For ordered translation batches and collection previews, Nemi owned the
standalone Rust codec, Vela owned marker layout and the external QuickLook
adapter, and Fenna owned Katran preview UX. Polly owned selection order,
batch jobs, publication, package integration and verification. The requested
Luna profile for Kiki was unavailable in this session.

After owner acceptance of batch output and numbering, Fenna owned the Katran
BULL viewport, Nemi the BK badge, and Vela the numbered-only lettering mode
and QuickLook viewport. Polly owned the persisted preference, job routing,
and verification.
