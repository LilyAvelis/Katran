# Downloads live source

Downloads resolves the Windows `FOLDERID_Downloads` Known Folder, including redirected locations. `downloads_folder` is an optional folder override for configuration and synthetic tests; `downloads_enabled` controls scanning. There is no guessed `USERPROFILE/Downloads` fallback.

`ScanDownloads` uses the shared `IngestPath` dispatcher with source `downloads`. All regular file types, including empty files, are eligible after two unchanged readable metadata observations. Temporary suffixes `.crdownload`, `.part`, `.partial`, `.tmp`, `.download` are ignored case-insensitively. Symlinks are skipped. Recursive traversal is capped at 20,000 entries; enumeration failures preserve prior scanner state. Content receipts and events commit together. Changed bytes create a new arrival, with live browsing retaining the newest available record for each path.

Downloads original deletion belongs to `DeleteSelection`, with one item immediate and multiple distinct IDs requiring confirmation. The domain boundary checks the configured folder, resolved containment, regular-file status and recorded content hash before calling Windows Shell recycling. It never falls back to permanent filesystem deletion. `DeleteEvents` remains shelf-only. Copy and drag use the shared file-reference clients.

Synthetic tests inject a fake recycler; they do not touch personal Downloads files or empty the Recycle Bin. The owner checks native Windows recycle/copy/drag behavior manually. Metadata stability does not guarantee an application has finished writing, and changes that preserve both size and modification time cannot be noticed by this scanner. Hash checks at ingestion and deletion prevent acting on a mismatching recorded revision.


The GUI uses native `FolderWatcher` preparation plus `CommitFolderCandidates`,
not recurring ScanDownloads calls. Hashing/discovery runs outside the dispatcher;
Clipboard capture gets priority and one prepared file commits per turn. The
serialized prepared hash is an internal local-worker contract, not proof of
content from untrusted API clients; operations still verify recorded revisions.
See latency.md for measurements and explicit limits. Windows can show a warning
if recycling is impossible; Katran has no permanent filesystem fallback.
