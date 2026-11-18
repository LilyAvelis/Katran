# Katran 4.0 run · 2026-11-18

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
