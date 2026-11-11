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
shader. The complete movie is never expanded into RAM. This is **not zero-copy**:
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
size is1073×720; its source file remained unchanged after the managed import.
Native composition, looping appearance and combined optics await owner feedback.
