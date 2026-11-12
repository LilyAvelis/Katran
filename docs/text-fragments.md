# Text fragments

Text and Markdown behavior described here are working changes beyond the
owner-accepted 1.5.0 baseline. Windows UI acceptance is pending.

Text is an optional shelf feature. `Settings.text_enabled` defaults to `false`;
turn it on with the Text eye in Settings → Tabs. Clipboard capture must also be
enabled. When Text is off, clipboard Unicode text is not ingested. Older saved
tab orders keep their existing positions; Text is appended as a disabled tab.

## Capture and storage

For a clipboard snapshot, Katran checks file references first, then bitmap
images when Images is enabled, then plain Unicode text. When Images is disabled,
the bitmap path is skipped and Katran can continue to Unicode text. Text capture
uses the Unicode clipboard format only; it does not extract HTML or tables.
An exact single HTTP(S) URL is routed to the independently enabled Links feature;
when Links is disabled it is skipped. Prose containing URLs remains ordinary text.
Empty text is ignored. Existing Text records are never reclassified.

Exact repeated text reuses an existing text-fragment event and its ID. If older
duplicates exist, the pinned event wins, then the newest ID; existing duplicates
are not merged or deleted. The chosen event retains its pin, tags, properties,
timestamps and origin. Text and Links remain distinct event kinds even when their
bytes match. No whitespace, line-ending or Unicode normalization is applied. Search
matches the full content, while the shelf card keeps a short preview. The source
text is preserved as Unicode, including its line endings; title trimming and
preview truncation are presentation only.

Limits are 1 MiB per fragment, 8 MiB for a merged result, 4 KiB for a merge
separator, and 1,000 items per merge/copy. A failed item or size check makes a
copy all-or-none: per-item results identify failures and the clipboard is left
unchanged. Deleting the last event that references a shared text blob allows
the normal owned-blob cleanup to reclaim it. There is no separate Text retention
or privacy blacklist.

Schema version 3 introduced `text_contents` (content keyed by blob hash) and
`text_fragments` (event-specific preview metadata). Schema v4 records file
modification times for sorting; v5 adds Link event metadata and v6 adds Tables.
Migration creates a SQLite backup named for the target version
(`index-before-v6` for this build).
Older binaries reject newer schemas. Keep that migration backup as a recovery
artifact; do not restore it casually over newer work.

## Preview and transfer

Space opens a text fragment in Katran's internal preview. The preview worker is
read-only and starts lazily; hiding Text stops the worker and clears its current
preview state. The UI displays at most 65,536 characters. `GetText` returns the
full validated content independently of that display limit.

Ctrl+C uses `CopySelectedObjects` / `CopySmart`. When every selected item is a
Text fragment, it copies their contents as plain text joined by `\n\n`, in
selection order. Ctrl-click toggles an item: adding puts it at the end and
removing then re-adding moves it to the end. Shift range selection follows the
current visible order from top to bottom regardless of which endpoint is
active. When sorting changes that visible order, range results follow the new
order while Ctrl-built selection order remains intact.

If a selection includes an ordinary file or any non-Text item, Ctrl+C follows
the existing file-copy path. Drag always uses file transfer, including for
Text fragments; saved copies are independent `.txt` files. The Actions command
**Копировать тексты одним текстом** remains a text-only operation, joining
fragments in selection order with `\n\n`. `CopyText` and
`CopySelectedText` validate and merge first, then publish once to the clipboard
only if the whole batch succeeds. `CopyEventsChecked` is the explicit API for
file copying regardless of item kind.

Space also previews an existing `.md` or `.markdown` file internally, even when
the Text tab is disabled. `GetDocumentText{id}` accepts Text fragments or UTF-8
Markdown files up to 1 MiB. For Markdown references, it validates the recorded
revision and source content before and after reading. The preview displays up
to 65,536 characters and offers **Markdown / Исходный текст** views; the stored
source remains unchanged.

Inside the text/Markdown preview, Ctrl Left selects Markdown and Ctrl Right
selects the original source. These keys do not navigate to another item. Ordinary
arrow keys keep item navigation, and text fields retain their editing shortcuts.

The conservative Markdown renderer styles headings, unordered list items,
emphasis, strong text, inline code, fenced code, and link labels. Link labels
are styled only: Katran does not navigate to destinations or fetch content.
Unsupported constructs fall back to literal source text. HTML is displayed as
text, never interpreted or executed. This is not a full CommonMark parser.

## Domain API

These commands use the same versioned dispatcher as the UI and other clients:

```json
{"api_version":1,"request_id":"ingest","command":{"type":"ingest_text","text":"Первая строка\r\nВторая строка 🐄","source":"manual"}}
{"api_version":1,"request_id":"read","command":{"type":"get_text","id":42}}
{"api_version":1,"request_id":"read-document","command":{"type":"get_document_text","id":43}}
{"api_version":1,"request_id":"merge","command":{"type":"merge_text","items":[{"id":42,"revision":"blob:…"}],"separator":"\n\n"}}
{"api_version":1,"request_id":"copy","command":{"type":"copy_text","items":[{"id":42,"revision":"blob:…"}],"separator":"\n\n"}}
{"api_version":1,"request_id":"copy-selection","command":{"type":"copy_selected_text"}}
{"api_version":1,"request_id":"copy-smart","command":{"type":"copy_smart","items":[{"id":42,"revision":"blob:…"}]}}
{"api_version":1,"request_id":"copy-objects","command":{"type":"copy_selected_objects"}}
```

`IngestText` requires Text to be enabled. `GetText` returns the full body with
its revision. `GetDocumentText` reads Text fragments or UTF-8 Markdown files
and validates the source revision. `MergeText` returns the combined string
without changing the system clipboard. Merge/copy commands require exact item
IDs and revisions so a stale selection cannot silently publish different
content. `CopySmart` selects the text-copy path only when all items are Text
fragments; otherwise it dispatches to file-copy validation. `CopySelectedObjects`
uses the domain's current selection or cursor.

Embedded NUL characters supplied through the API remain intact in TXT storage
and export. Plain Windows text Clipboard cannot represent them, so copying such
a result returns `clipboard_unrepresentable` without changing Clipboard.

Text merge preparation runs off-thread. Before publishing a prepared copy the
dispatcher revalidates selected event revisions, fences cancellation, and updates
the capture sequence in the same finalization step so its own copied text is not
recaptured.

## Windows owner acceptance

In a normal installed build, the owner can check the feature without desktop
automation:

1. Confirm Text starts hidden and clipboard text is not captured. Enable Text,
   enable capture, and copy Unicode text containing CRLF and non-Latin
   characters; confirm one Text item appears with the full original body.
2. Copy the same text again and confirm the existing card remains the only one.
   Search for text beyond the card preview and confirm it
   matches.
3. Confirm file references take precedence over bitmap/text, and with Images
   enabled, bitmap images take precedence over plain Unicode text. Confirm
   exact single HTTP(S) URLs are handled by Links independently of Text.
4. Open a text item with Space; confirm preview is capped while GetText or an
   exported `.txt` retains the full content.
5. Ctrl+C a selection containing only Text fragments and confirm it writes
   joined plain text. Add an ordinary file and confirm Ctrl+C uses file copy;
   drag Text fragments and confirm it always creates file payloads.
6. Toggle selection with Ctrl, remove and re-add an item, then range-select
   upward and downward. Confirm Ctrl ordering is retained and range ordering
   follows the visible order after sort changes.
7. Open a UTF-8 `.md` file with Space while Text is disabled. Switch between
   **Markdown** and **Исходный текст**; check common headings, lists, emphasis,
   code and link styling, and confirm HTML and unsupported constructs stay
   literal. Confirm the full saved source remains unchanged.
8. Include a stale or invalid item in a batch copy and confirm per-item failure
   with no change to the system clipboard. Hide Text and confirm its preview
   worker stops; re-enable it and confirm the saved items remain available.

API tests use synthetic data and fake clipboard sinks. This is a checklist, not
a claim of physical Windows UI acceptance.

Rich prose HTML conversion and Markdown export remain later roadmap stages.
Markdown link syntax is styled in preview only. Exact URL capture and link
transfer follow the [Links contract](links-design.md); HTML table grids follow
the separate [Tables contract](tables.md).
Combined sorting/API/build verification for the current working changes is
tracked separately; this document does not identify an installed build.
