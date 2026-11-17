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

## Domain and scope

`BeginJob(SearchFoldersSemantic {query, limit})` runs in the bounded Discovery
lane. Direct synchronous execution returns `background_required`. Ordinary
`SearchFolders` is unchanged. A successful semantic reply contains paths,
`indexed_count`, `scope` and explicit `incomplete`. Selection still passes through
the existing `ChooseFolder` validation/history path.

This initial version embeds **names and paths only**, from a bounded Everything
directory snapshot, ordinary-search candidates and saved favorites/history. It indexes at most **8192**
unique paths with a frame-byte budget; reaching either bound is explicitly
incomplete. It does not read document contents, traverse directories itself,
guarantee full-disk semantic recall or persist a private-path vector index.
Literal exact-name hits keep priority after successful model inference.

The installed optional multilingual E5 runtime supplies Python, tokenizer and
weights through the existing integrity-checked resolver. The Folder worker is
separate embedded code and does not change emoji protocol, manifests or indexes.
No model is downloaded automatically and no queries/paths leave the computer.
Without a working local runtime the API/UI shows an error; it does not label
ordinary lexical results as successful AI. Return to ordinary mode with `\`.
See [local runtime installation](../emoji-ai.md).

## Responsiveness and limitations

The first query loads the model and embeds the bounded snapshot. Later queries
reuse the resident encoder and RAM vectors for unchanged paths. Reordering does
not re-encode them; only newly added paths are encoded, and removed paths are
evicted. This also permits query-specific lexical candidates without rebuilding
the entire index on each query or history update.
Query changes cancel old jobs and stale replies are ignored. Neural input uses
a 350ms debounce; ordinary search keeps its 120ms debounce. Initial indexing can
take substantially longer than a warm query and the UI shows preparation/loading.

CPU inference uses four threads, batches of 64 and 128-token descriptors with the
folder basename first. Worker requests/results are bounded by 4MiB. Startup,
cold and warm stages have explicit timeout budgets (45s, 180s and 30s respectively);
cancellation is checked while waiting and failed/cancelled workers are killed.
Everything acquisition remains separately bounded by its existing helper timeout.

This is an initial bounded local feature, not a quality guarantee. Candidate
coverage and ambiguous-language ranking remain limitations. See the dated
[research](FOLDER-SEMANTIC-RESEARCH.md) for model selection reasoning and the
[verification ledger](VERIFICATION.md) for actual end-to-end measurements.
