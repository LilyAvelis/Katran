# Extract audio from video

Select one video and open Ctrl+K → Video → Extract audio. Track numbers start
at 1 and the first track is preselected. The background job decodes that
track and encodes AAC into a separate M4A file. That output is audio-only.
The original file and current player session stay intact. Supported source
extensions are MP4, M4V, MOV, MKV and WebM.

Results are saved below `audio-extractions` in the active data profile. Each
successful job creates its own directory and returns the complete file path.
Katran registers the file through its ordinary Files pointer catalogue, then
copies it as a file. If copying fails or the clipboard changes while extraction
runs, Files still contains the result and the dialog reports partial success.
Removing the Files pointer does not delete the audio file.
Setup normally uses `%LOCALAPPDATA%\Katran\data`; Portable uses its adjacent
`katran-data` profile. A custom launcher can select a different data profile.
The operation uses the existing FFmpeg/FFprobe runtime. Prepare missing
components in Settings → Themes → Video background.

The Rust command `ExtractVideoAudio { audio_track }` captures the single
selection when submitted. A missing ordinal selects zero; explicit ordinals
are zero-based among audio streams. The result includes `path`, `audio_track`,
the Files pointer receipt, and an independent clipboard receipt.
Cancellation before publication leaves no final file. Invalid track, no audio,
and media processing failure are separate job errors. The worker checks the
source revision and stages the file privately. The dispatcher checks the same
source and its file stamp again before authorizing publication. The staged file
is bounded by 2 GiB, and a process is stopped after two hours or on cancellation.
Forced process termination can leave a temporary staging directory on disk.

Synthetic API fixtures use the packaged FFmpeg 9.0.2 tools. They verify both
track choices by decoded frequency, Unicode filenames, AAC/M4A output,
repeat extraction preserving earlier files, silent and damaged input, invalid
track and cancellation before commit. The source is compared byte-for-byte.
The Windows UI requires separate owner acceptance.
