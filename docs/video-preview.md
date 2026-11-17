# Native video preview with audio

Local follow-up after the immutable v3.8.0 release tag, authorized 18 November2026.
Space preview and Mini stream share one explicit playback session. No public
release or consumer update is implied by the local implementation.

## Product contract

Video opens inside Katran with explicit Play/Pause, timeline/seek, elapsed and
duration, and volume. Browsing Mini stream does not autoplay audio. Closing the
preview, changing the item/tab/view/surface, hiding the window or opening Settings
closes the old session. Full preview and Mini stream are distinct surfaces even
when showing the same item. A late prepare result cannot reopen an abandoned
session after leaving and returning. Metadata-only tag/pin changes keep the same
content session. Ordinary image/Text/Pods interactions retain their routes.

## Domain and native boundary

`media_playback.rs` owns one session, monotonic tokens, validated controls and
bounded frames through an injected Backend. Ordinary constructors are lazy;
read-only preview Services cannot create or control native playback.
`media_service.rs` resolves an actual current id/revision and authorized surface.
`BeginJob(OpenMedia)` prepares large-file hash validation on a worker, then checks
epoch/context, record revision/path and file metadata before committing. Direct
OpenMedia is synchronous for explicit API callers. Frame polls use cheap file
metadata rather than rehashing a large video on every frame.

`media_playback_windows.rs` uses Windows Media Foundation IMFMediaEngine in
frame-server mode. Windows supplies the audio clock/output, and frames are
copied through WIC to bounded RGBA buffers (up to1280x720, about30fps). This is
not zero-copy or a promise of full-resolution/high-refresh playback. No extra
player/runtime download is required. Source codec availability depends on the
Windows installation; opening a recognized extension does not guarantee decoding.

Controls: OpenMedia{id,revision}; ControlMedia{session,action}; MediaState;
MediaFrame{session,after_serial}. Actions are play, pause, seek(position_ms),
set_volume(volume0..1), stop. Wirev2 preserves large IDs/session/serial values.
Frames are bounded and serial/session checked before texture upload. Reuse one
UI TextureHandle; do not recreate it for each frame.

The native worker has a1.5-second client-poll lease while loading/playing, so an
abandoned client normally closes audio even if the UI cannot issue Stop. Clients
must poll state/frames during playback. This supplements explicit domain Stop.
OS codec/COM calls may themselves block; the thread watchdog cannot interrupt a
hung system call, and a strict worst-case Shutdown deadline is not established.
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

Initial muted native feasibility probe passed with synthetic H264/AAC MP4,
metadata/audio stream detection, decoded frame, pause/seek, close and file-handle
release. The final gate also checks MKV, non-black pixels, poll-lease expiry,
dispatcher context cancellation and UI lifecycle; see TEAM/VERIFICATION for final
results. Native appearance and audible output use owner manual acceptance, not
desktop automation. No personal media or Clipboard is used in fixtures.

Primary references:
- https://learn.microsoft.com/en-us/windows/win32/api/mfmediaengine/nf-mfmediaengine-imfmediaengineclassfactory-createinstance
- https://learn.microsoft.com/en-us/windows/win32/api/mfmediaengine/nf-mfmediaengine-imfmediaengine-transfervideoframe
- https://learn.microsoft.com/en-us/windows/win32/api/mfmediaengine/nf-mfmediaengine-imfmediaengine-onvideostreamtick
