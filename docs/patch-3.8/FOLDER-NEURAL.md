# Optional local neural Folder search · 2026-11-17

Open the folder picker with **Ctrl+Shift+K**. In its Search section, press the
physical **backslash (`\`)** key to switch ordinary/neural mode. It works while
the query field has focus and consumes the corresponding text event, including
another keyboard layout. Holding the key does not repeatedly switch. Pasted
Windows paths remain intact. The key keeps its existing meanings outside this
picker, including Pods. Mode is session-only.

Neural mode shows a gold field border, a local-neural heading and a descriptive
query placeholder. Press `\` again for ordinary Everything search. History,
Favorites and Standard folders remain separate picker sections. The configured
Standard-folders shortcut (default Ctrl+Y) now works inside the already open
picker, including while its query field is focused; it remains rebindable.

Owner UX refinement: neural search starts **only on Enter**, not while typing.
The first reply contains the top **10** results. Press Down on the last row for
the next 10; existing rows remain and the cursor moves to the first new result.
Enter on a completed result opens the selected folder. Changing the query cancels
the old job and waits for a new Enter. Ordinary search keeps its prior behavior.

## Domain and scope

`BeginJob(SearchFoldersSemantic {query, limit})` runs in the bounded Discovery
lane. Direct synchronous execution returns `background_required`. Ordinary
`SearchFolders` is unchanged. A successful semantic reply contains paths,
`indexed_count`, `scope` and explicit `incomplete`. Selection still passes through
the existing `ChooseFolder` validation/history path.

The picker uses `SearchFoldersSemanticPage {query,offset,limit,snapshot}` with
limit 10. Replies add query/offset, next_offset, has_more and an opaque snapshot.
The token binds query, candidate order, CPU/GPU choice and Service instance;
changed inputs reject continuation with `folder_ai_snapshot_changed`. Enter
starts a fresh search after that error. Top-k limits presentation of ranked
matches; it is not a calibrated confidence threshold or a quality guarantee.

This initial version embeds **names and paths only**, from a bounded Everything
directory snapshot, ordinary-search candidates and saved favorites/history. It indexes at most **8192**
unique paths with a frame-byte budget; reaching either bound is explicitly
incomplete. It does not read document contents, traverse directories itself,
guarantee full-disk semantic recall. A bounded derived vector cache is now
persisted locally across launches; see below.
Literal exact-name hits keep priority after successful model inference.

The installed optional multilingual E5 runtime supplies Python, tokenizer and
weights through the existing integrity-checked resolver. The Folder worker is
separate embedded code and does not change emoji protocol, manifests or indexes.
No model is downloaded automatically and no queries/paths leave the computer.
Without a working local runtime the API/UI shows an error; it does not label
ordinary lexical results as successful AI. Return to ordinary mode with `\`.
See [local runtime installation](../emoji-ai.md).

## Responsiveness and limitations

The index survives Katran/worker restarts in the active profile's
`folder-ai-index/cpu.sqlite3` or `cuda.sqlite3`. Compatible vectors load from
disk; only names/path descriptors missing from that cache are encoded. Renaming
or moving a folder changes its descriptor; deleted or evicted entries are pruned
when the next bounded discovery snapshot is saved. File contents and folder
modification times are not embedding inputs, so changing files inside a folder
does not require re-embedding its unchanged name/path.

The cache stores hashed descriptors and normalized float32 vectors, never query
history or plain folder paths. Writes are atomic SQLite transactions, bounded
to 8192 vectors per device. Model/tokenizer/optional external tensor hashes,
descriptor/pooling/prefix versions, token limit, dimensions, ORT and CPU/GPU mode
bind compatibility; incompatible vectors are rebuilt rather than reused.
Corrupt, locked or unwritable caches fall back to the working RAM index with
an explicit diagnostic status. The main catalogue schema remains v13.

This avoids full re-embedding after restart; the local model/runtime still has
to load to encode the query. Entries outside the bounded retained snapshot can
be encoded again if they return later. No file-content crawl is introduced.

The first query loads the model and embeds the bounded snapshot. Later queries
reuse the resident encoder and RAM vectors for unchanged paths. Reordering does
not re-encode them; only newly added paths are encoded, and removed paths are
evicted. This also permits query-specific lexical candidates without rebuilding
the entire index on each query or history update.
Query changes cancel old jobs and stale replies are ignored. Neural input waits
for Enter; ordinary search keeps its 120ms debounce. Initial indexing can
take substantially longer than a warm query and the UI shows preparation/loading.

CPU inference uses four threads, batches of 64 and 128-token descriptors with the
folder basename first. Worker requests/results are bounded by 4MiB. Startup,
cold and warm stages have explicit timeout budgets (45s, 180s and 30s respectively);
cancellation is checked while waiting and failed/cancelled workers are killed.
Everything acquisition remains separately bounded by its existing helper timeout.

## Optional GPU component

Settings → General → Folders and storage → **Folder neural index · GPU** selects
CUDA for this feature; default is off. `folder_ai_gpu` persists only when true.
The separate `folder-ai-gpu-runtime` in the active profile owns its own Python
environment; existing emoji CPU environment, manifests and model weights are
unchanged. Weights still pass the existing integrity resolver and are reused.
`scripts/folder-semantic/setup-gpu.ps1 -Profile <absolute profile>` installs pinned
ONNX Runtime GPU 1.23.2 and CUDA/cuDNN wheels locally, without changing drivers
or installing a global CUDA toolkit. Model download is not part of this setup.
The resolver checks the isolated runtime, and worker startup must activate CUDA.
Unavailable CUDA returns `folder_gpu_unavailable`; it is never labeled GPU while
silently using CPU. Turn the checkbox off to use CPU again.

Older binaries can reject settings with the new true GPU field; controlled
rollback must restore/adjust settings as with the new `details` view. Runtime
installation itself does not enable the setting or restart Katran. Actual CUDA
availability/performance evidence is recorded in `VERIFICATION.md`, not inferred
from the checkbox or installed package names.

This is an initial bounded local feature, not a quality guarantee. Candidate
coverage and ambiguous-language ranking remain limitations. See the dated
[research](FOLDER-SEMANTIC-RESEARCH.md) for model selection reasoning and the
[verification ledger](VERIFICATION.md) for actual end-to-end measurements.
