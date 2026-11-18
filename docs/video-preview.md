# Video preview with audio

Local follow-up after the immutable v3.8.0 release tag, authorized 18 November2026.
Space preview and Mini stream share one explicit playback session. No public
release or consumer update is implied by the local implementation.

## Product contract

Space starts the chosen video automatically; Mini stream waits for Play so list
browsing does not autoplay audio. Play/Pause/Replay, timeline, time and volume
sit in a translucent overlay inside the full available video canvas. Original
proportions are retained. The timeline spans its width and the compact controls
are centered. Controls hide after two seconds without pointer activity while
playing, remaining visible during interaction/focus, pause, loading or errors.
Volume is remembered within the UI session. Closing the
preview, changing the item/tab/view/surface, hiding the window or opening Settings
closes the old session. Full preview and Mini stream are distinct surfaces even
when showing the same item. A late prepare result cannot reopen an abandoned
session after leaving and returning. Metadata-only tag/pin changes keep the same
content session. Ordinary image/Text/Pods interactions retain their routes.

The owner confirmed playback/navigation in Folder and Desktop. Follow-up UI
corrections size the overlay from actual widgets with a bottom inset for large
controls. Seek holds the requested position until observed confirmation or a
bounded timeout/error. Physical P pauses/resumes only on a video surface; text
editing and modals retain input, and autorepeat does not toggle.

## Domain and native boundary

`media_playback.rs` owns one session, monotonic tokens, validated controls and
bounded frames through an injected Backend. Ordinary constructors are lazy;
read-only preview Services cannot create or control native playback.
`media_service.rs` resolves an actual current id/revision and authorized surface.
`BeginJob(OpenMedia)` prepares large-file hash validation on a worker, then checks
epoch/context, record revision/path and file metadata before committing. Direct
OpenMedia is synchronous for explicit API callers. Frame polls use cheap file
metadata rather than rehashing a large video on every frame.

`media_playback_mpv.rs` replaces the first Windows Media Foundation candidate
with libmpv, informed by Afio's existing Rust/egui integration. libmpv itself is
C code. It owns decoding, playback timing and audio. The default bridge uses
software rendering into bounded RGBA (up to 1280x720, about 30fps). Conversion/
scaling is single-threaded and can be slow for large videos. Settings → General
offers optional video GPU acceleration, **off by default**. A private, never-shown
WGL context on the worker renders through libmpv OpenGL into an FBO, then reads
bounded RGBA back into the existing texture path. Hardware decoding is requested
with auto-safe; actual hwdec-current can still report CPU. This is not zero-copy,
a full-resolution/HDR promise or a performance benchmark.

MediaState exposes optional Snapshot.acceleration: requested_gpu, actual
renderer (cpu/gpu), observed decoder and fallback_reason. The player shows actual
mode with the controls. GPU initialization failure falls back to software before
loading the file and reports the reason. Mid-playback GL failure stops with an
error; seamless recovery or immunity to a hung/crashing driver is not promised.
The user can disable the option. Changing it invalidates sessions/pending opens;
the next open selects the configured backend. Injected test backends stay owned
by the test.

The owner saw the first candidate work in Desktop but fail in Folder with
Media Engine code4 / HRESULT0x80070003. Synthetic Cyrillic names reproduced
failure0x80070002, while ASCII names including spaces/#/% passed. These checks
do not prove the exact owner's source failure, but invalidate the earlier
candidate's acceptance. Initial gates missed a relevant filename class.

Controls: OpenMedia{id,revision}; ControlMedia{session,action}; MediaState;
MediaFrame{session,after_serial}. Actions are play, pause, seek(position_ms),
set_volume(volume0..1), stop. Wirev2 preserves large IDs/session/serial values.
Frames are bounded and serial/session checked before texture upload. Reuse one
UI TextureHandle; do not recreate it for each frame.

The native worker has a1.5-second client-poll lease while loading/playing, so an
abandoned client normally closes audio even if the UI cannot issue Stop. Clients
must poll state/frames during playback. This supplements explicit domain Stop.
Library/system calls may themselves block; the thread watchdog cannot interrupt
a hung call, and a strict worst-case shutdown deadline is not established.
Normal synthetic close/release is tested; do not claim process-isolated cleanup.

## Source and lifecycle limits

Only local regular files are accepted. Revision/content checks protect the open
boundary; source changes or disappearance invalidate playback. Reparse/symlink
paths are rejected, including ancestors, which can exclude junction-backed or
cloud-placeholder files. Unsupported codec and unavailable source should remain
distinct outcomes. Existing external Open With/QuickLook actions remain available.
No schema migration, persisted player state, playlist or audio-only file mode
is introduced.

## Evidence

The first muted Media Foundation probe passed ASCII-named H264/AAC MP4 and MKV
but missed the subsequently reproduced Cyrillic failure. Replacement libmpv
gates must cover Unicode paths, real decoded frames and observed controls;
see TEAM/VERIFICATION for dated results and installed identity.
Native appearance and audible output use owner manual acceptance, not
desktop automation. No personal media or Clipboard is used in fixtures.

## Optional runtime

Windows x64 uses Afio's pinned build20261107-git-9028187538. Load only an absolute
KATRAN_MPV_DLL override or libmpv-2.dll next to katran-ui.exe. Exact SHA256 is
verified under a read handle denying replacement; native dependency lookup is
restricted to its directory and System32, without PATH/CWD engine discovery.
Scripts/configuration and reference autoloading are disabled for local playback.

Install through a visible terminal:
`./scripts/setup-video-player.ps1 -OutputDirectory 'C:/Katran/runtime/versions/current-id'`.
Pass a directory, not an executable filename. The script downloads the pinned
approximately30MiB archive, checks it and the extracted approximately116MiB DLL,
and saves a receipt/notice. An explicit -Archive can reuse verified cached bytes.
Restart after installation; verified bytes alone do not certify device playback.

Archive SHA256: df7c433a2d481fdaf6163b201f26abbb0eff0c215c51368397088c6d8f7a001b
DLL SHA256: c91949577ae9cc2d500208da51f2c4a24cfa7a02d0ce6e130bf67ec738b450bb

The DLL has separate GPL/LGPL/component terms; do not label it MIT or assume
LGPL-only. This setup obtains upstream bytes separately. Public redistribution
needs exact build/component/source compliance review in the later release scope.
Afio source integration is MIT. No new publication is authorized by this task.

Primary references:
- https://github.com/mpv-player/mpv/blob/master/include/mpv/render.h
- https://github.com/mpv-player/mpv/tree/master/include/mpv
- https://github.com/shinchiro/mpv-winbuild-cmake/releases/tag/20261107
- https://github.com/mpv-player/mpv/blob/master/Copyright
