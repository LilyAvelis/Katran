# File visuals · 1.2.0 Test

`PreviewFile {id,max_edge}` is the read-only dispatcher API for gallery and stream
visuals, with `max_edge` in 32..512. It does not restrict which regular file types
can be ingested, copied or dragged. An unavailable visual does not remove a file
or change transfer behavior. Existing `PreviewImage` and the large image preview
retain their image-only contract.

The response contains `width`, `height`, straight RGBA `rgba_base64`, `revision`,
`cache_hit`, `visual_kind` and `fallback_reasons`. The ordered attempts are:

1. Katran's existing bounded image decoder (`visual_kind: image`).
2. Windows Shell thumbnail (`shell_thumbnail`).
3. Shell file/type/associated application icon (`shell_icon`).
4. Built-in generic document icon (`generic_icon`).

Unsupported image decoding and unavailable Shell visuals are explicit in the
successful fallback response. Missing files, changed recorded contents, invalid
dimensions and unknown IDs remain explicit API errors. The generic icon also
works on non-Windows platforms. Gallery and stream render icons at up to 96
logical pixels; thumbnails use the available image region.

Shell availability depends on Windows file associations and installed thumbnail
providers. PDF, Office, video, archive and other files are transferable regardless
of their visual. Katran bundles no PDF/Office renderer or media player and does
not promise thumbnails for every format. It never executes the target file or
launches its associated application to obtain a visual. Windows may activate an
installed Shell extension while extracting a thumbnail or icon.

The native boundary initializes COM on each calling worker with balanced cleanup,
also respecting a previously initialized apartment. COM interfaces, bitmap and
device-context handles are released on every return. Requested and accepted
bitmaps are at most 512×512; buffers are bounded to 1 MiB. Shell extraction occurs
only in the existing two background visual workers, with a 64-item pending queue.
Third-party Shell providers can be slow or hang: the queue/concurrency bounds are
not a provider execution timeout or process sandbox.

Image previews retain their existing 128 MiB persistent cache. File visuals use
a separate 128 MiB cache, keyed by path, name, recorded content revision and size;
the UI retains its existing 48 MiB/96-texture limit. Cached file visuals may remain
unchanged after file associations/providers change until that cache is cleared.
No source file is changed during visual extraction or cache maintenance. Existing
reference ingest and freshness checks hash complete source contents, which can
cost time for very large files; this feature does not alter that contract.

Synthetic tests inject the provider boundary to check fallback order, malformed
dimensions, cache reuse, source errors and arbitrary-file export/drag preparation.
The native Windows test extracts only a synthetic temporary text file's icon,
without launching apps, running a native drag loop or changing the clipboard.

Windows API references:
[IShellItemImageFactory::GetImage](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ishellitemimagefactory-getimage),
[CoInitializeEx](https://learn.microsoft.com/en-us/windows/win32/api/combaseapi/nf-combaseapi-coinitializeex).
