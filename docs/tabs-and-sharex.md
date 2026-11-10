# Tabs and sources · 1.3.0 Test

All is pinned;1..9 select the other tabs in their saved order. Drag tabs to reorder.
Tab toggles All and the remembered non-All tab (initially Images). Ctrl K and Tab
are reserved; other shortcuts can be rebound. Text input and dialogs retain local
input. Gallery/Stream and their Ctrl−/Ctrl+ density are independent of tab order.

Images contains ordinary clipboard/manual image arrivals. Screenshots contains
originals in the configured ShareX folder, including monthly/date subfolders.
Downloads contains original regular files in its configured/default Known Folder,
including arbitrary extensions. All combines these projected views. Classification
keeps known/verified ShareX copies out of the ordinary image stream. Selection can
span hidden tabs; copy/drag use the exact saved IDs and content revisions.

In Settings choose the same absolute screenshot root ShareX saves into. Katran
does not change ShareX preferences or assume its installation directory. Empty
ShareX root disables that source. Downloads can be disabled or pointed at a
synthetic/custom absolute root. Native workers enumerate existing stable files and
watch subsequent changes; discovery is independent of the capture switch. The
switch still controls ordinary Clipboard capture.

Delete in Screenshots removes the original permanently. Delete in Downloads uses
Windows Recycle Bin; there is no permanent fallback. Images removes only its shelf
object/cache. One file needs no question; multiple files require Enter confirmation.
External removals hide originals after reconciliation. Unchanged deleted shelf
records do not resurrect after restart. Generic provenance labels do not confer
original-file ownership.

API: SetTab/ToggleTab/BrowseShelf, SelectTabPosition/MoveTab, EnsureSources/
PollSources/SourceStatus/RescanSource. Legacy ScanSharex/ScanDownloads schedule the
same asynchronous domain runtime. Wire examples, generations, private receipts,
batch errors and explicit limits are in [domain-jobs.md](domain-jobs.md).

Each source root is limited to20,000 entries, recursively skipping links. A failed
or truncated traversal reports an error and preserves the previous availability
view. This is not an unlimited filesystem index. Stable-file checks use repeated
size/mtime observations; brief/intermediate revisions can be missed.

Manual acceptance: close the previous Katran, launch1.3.0 Test; verify Tab round
trip and positional/mouse tab order, capture pause, a ShareX screenshot in a nested
date folder, multiple-file copying, single/group deletion and an external removal.
Use disposable files when checking original deletion. The owner checks actual
Windows input/rendering; automated tests use only synthetic roots and adapters.
