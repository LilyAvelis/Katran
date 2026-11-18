# BullViewer draft

QuickLook `IViewer` plugin for BALOHCOL v1 portable PNG collections. The host
provides `QuickLook.Common.dll`; this project does not bundle or replace it.
The viewer validates all framing, checksums and PNG structure while indexing,
then reads and checks only the selected page for WPF decoding. Previous/Next
navigate without external programs. Input is bounded to 32 pages, 64 MiB per
page, 256 MiB per collection and 8192 px / 32 million pixels per image.
Errors are shown in the viewer. Large collections can take time to index, but
indexing and decoding run outside the UI thread.

Build against the matching installed QuickLook Common assembly; set
`QuickLookCommonPath` to its absolute path and keep intermediate/output paths
under the workspace's `target/build`. The local verified build used .NET SDK
8.0.428, target `net462`, and the installed 4.4.0-katran-bridge1 Common DLL.
Source and binary compatibility with other QuickLook releases is not established.

Do not install or restart QuickLook automatically. A visual native test remains
necessary after explicit installation; synthetic parsing and WPF decoding do not
prove that the running application loaded the plugin.
