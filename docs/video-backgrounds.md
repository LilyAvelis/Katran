# Experimental video backgrounds

The Blue theme accepts a profile-local video instead of lily/static artwork.
Select a saved video or import one in Settings → Themes. A single selected video
in Downloads offers **Сделать видеофоном** in Actions.
The domain validates the selected content revision, creates a managed copy and
activates the theme. Sources are never modified. Image and video backgrounds
are mutually exclusive; switching to a picture or a lily stops the video.

## API and storage

- `ImportVideoBackground {path}` prepares and commits a video without selecting
  it; BeginJob/PollJob also supports it.
- `UseSelectedVideoAsBackground` imports the one selected video and activates it,
  with the same asynchronous preparation lane as image backgrounds.
- `VideoBackgrounds` returns `{items:[...]}`; `GetVideoBackground {id}` returns
  bounded metadata and the managed path.
- `ThemePreferences.video_background` selects its SHA256 ID. SetTheme/SetSettings
  validate it and reject conflicting background sources.

`video-backgrounds/<sha>.mp4` and a bounded metadata sidecar live inside the
profile, outside Git and packages. Import uses an immutable bounded source copy,
checks H.264, maximum128MiB,120seconds and3840×2160, then remuxes only the first
video stream with `-an -sn -dn -c:v copy`. Audio is removed from the stored copy,
not merely muted during playback. FFmpeg tools have process timeouts. File
names/paths are passed as separate arguments without a command shell.

## Rendering and limits

Requires the user's `ffmpeg` and `ffprobe` on PATH; these tools are not bundled
or downloaded by the application. The first local prototype supports H.264.
The player explicitly requests D3D11VA hardware surfaces. Hardware/tool failure
is an explicit error rather than an implicit software decoding fallback.

One process decodes one looping, silent stream, capped at24fps and1280×720 with
aspect ratio preserved. One bounded frame queue feeds a reused egui texture;
the same current texture is sampled by the background and optical card/menu
shader. Short loops may use the bounded decoded cache described below; larger clips keep only streaming frame buffers. This is **not zero-copy**:
`hwdownload` stages decoded frames in CPU memory and egui uploads RGBA to OpenGL.
Eliminating this transfer requires a separate D3D11/OpenGL interop implementation.
No whole-application GPU/CPU load percentage is claimed without measurement.

Hiding the main window in the tray or minimizing stops and reaps this owned
decoder process. The last displayed frame remains cached; showing starts a new
loop from its beginning. Selecting another source drops the former texture and
worker. There is no poster extraction yet; before the first frame or on initial
hardware failure, ordinary theme fallback remains available. A failed source
can be cleared by choosing a static background.

## Verification

Synthetic imports verify audio removal, unchanged originals, metadata bounds,
tamper rejection, API activation from Downloads, persistence and conflicting
settings. Headless GUI tests route one matching frame texture through both
background and optics and reject stale IDs. The explicit
`KATRAN_TEST_VIDEO_D3D11VA=1` worker test uses synthetic H.264 to check a real
decoded frame, nonblocking pause and child-process cleanup. Ordinary headless
UI tests do not launch media processes.

The owner's local Grok clip was probed as H.264,1168×784,15.041667seconds. The
exact D3D11VA/download/scale pipeline decoded it successfully. Its rendering
size is1072×720; its source file remained unchanged after the managed import.
Native composition, looping appearance and combined optics await owner feedback.

## Decoded short-loop cache and explicit enable switch

The video checkbox now appears separately in Themes. File selection appears
only while enabled. Disabling retains the saved ID; `video_enabled:None`
preserves existing videos from the first build and stays off when no video is
selected. `Some(false)` prevents both playback and optical sampling, and
`Some(true)` allows choosing a file before an ID exists.

For loops whose decoded NV12 frame payload fits a512MiB estimate, FFmpeg now
decodes a single input pass, scales to even dimensions and caches NV12 frames
with its loop filter. Output timestamps are regenerated continuously. The
reader uses a monotonic24fps schedule with no burst catch-up, so playback does
not depend on input seek/timestamp pacing. Larger clips keep streaming loops
with the same output pacing; they are never silently truncated to fit the cache.

The owner's clip now renders at1072×720. Including aligned rows and a small
frame-count margin, estimated frame-cache payload is about417MiB. This is a
RAM/decoding tradeoff; the limit describes cached frame payload, not total
process memory. Cache and decoder are released on hide/minimize; resuming
starts a fresh first pass. RGBA conversion and OpenGL upload still happen
through CPU memory. Hardware decode is avoided on subsequent cached loops,
not on initial playback. Visual source seams and repeating identical boundary
frames are not rewritten or cross-faded.

An explicit hardware test consumed110 frames from a48-frame synthetic source
across multiple loops, kept one process ID, verified cadence and stopped/reaped
the worker. Direct FFmpeg validation reported48 input frames decoded and110
output frames. No post-change GPU utilization percentage is claimed.
