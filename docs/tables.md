# Tables: clipboard value grids

Tables is an optional feature after the accepted 1.5.0 baseline. Its eye in
Settings → Tabs enables collection and a numbered tab. `tables_enabled` defaults
to false. Turning it off stops collection and hides stored Table events from
the active shelf without deleting their data.

## Capture and scope

The Windows adapter recognizes a complete HTML table in the selected CF_HTML
fragment. File references take precedence; a confirmed table takes precedence
over a bitmap representation; other payloads retain the existing image, Unicode
Text and Links handling. A disabled Tables category is skipped. Ordinary rich
HTML without a table keeps the existing fallback. Tab-separated plain text is
ambiguous, so automatic capture requires HTML table evidence. Explicit TSV
import is available through the domain API, including single-column input.

This targets the interoperable formats produced by spreadsheet and browser
applications. It does not imply physical acceptance in Excel, Google Sheets or
Notion before the owner checks those applications. Existing CSV/XLSX file
references keep their ordinary file behavior.

The stored model is a rectangular grid of Unicode strings: values, empty cells,
line breaks, tabs and quotes. Formula evaluation, workbook tabs, cell styling,
column widths and number-format metadata are outside this first slice. HTML
rowspan/colspan becomes an anchor value with empty covered cells. Nested tables,
multiple separate tables in one fragment and ambiguous partial table fragments
are rejected rather than silently including unselected neighbouring cells.
No remote content, HTML scripts or images are loaded or executed.

## API and storage

`IngestTable {rows,source}`, `IngestTableTsv {text,source}` and
`IngestTableHtml {html,source}` validate values through the shared dispatcher.
`GetTable {id}` returns the exact grid, revision and CSV source. `CopyTables
{items}` and the tables-only branch of `CopySmart` use exact selection IDs and
revisions. GUI, capture and other clients use these same paths.

A managed CSV blob is the authoritative transfer content. CSV uses UTF-8 BOM,
commas, RFC 4180 quoting and CRLF record endings; the last empty row is retained.
Schema v6 adds per-event `table_fragments` metadata and content-keyed
`table_contents` search. Its TSV search representation is separate from Text
search even when identical CSV bytes share a blob. Existing Text and Link events
are not reclassified. An existing catalogue
gets a consistent `index-before-v6` backup before migration.

A table and combined table copy are limited to 1 MiB of cell values, 10,000 rows,
256 columns and 100,000 cells. Encoded input/clipboard formats have a separate
8 MiB limit. Selection commands accept at most 1,000 items and reject oversized
combined results before publication. Preview limits affect display only.

## Copy, drag and ordering

Ctrl+C on a tables-only selection publishes HTML cells and Unicode TSV together.
Multiple selected tables are stacked vertically, separated by one empty row;
shorter tables are padded with empty columns to the widest selected table.
Ctrl-click preserves explicit click order; Shift uses current visible order.
Drag produces a separate CSV file per table. Mixed selections keep the existing
individual-file transfer behavior. Enter opens the CSV using the default app.

Space uses Katran's internal grid through both synchronous and background-job
preview routes. Wheel zooms toward the pointer; Ctrl-wheel/trackpad zoom uses
the canvas centre. Drag with the left or middle mouse button pans the grid.
The scale button resets to 100%; “Вписать” fits the full table. Ctrl Left/Right
selects grid/CSV source without moving to another item. Card drag remains CSV.

`TableViewGeometry`, `ZoomTableView`, `PanTableView`, `FitTableView` and
`ResetTableView` share process-local `Interaction.table_view`. The domain bounds
zoom to 0.1–400%, keeps the pointer anchor stable except where content bounds
require clamping, and resets the view on changing the preview item. Ordinary
zoom renders visible cells only. The overview uses sparse grid lines without
per-cell text or hover widgets, preserving bounded rendering for large tables.

Publication validates and prepares both formats before clearing the clipboard.
Cancellation or stale selection prevents publication. Windows does not provide a
transactional rollback for multiple clipboard formats: failure after clearing
returns explicit `html_written`, `text_written` and warnings. Katran updates its
own capture sequence even on a partial write, preventing self-capture.

Own publications also carry a unique registered clipboard-format marker.
Katran retains up to 64 successful publication tokens and checks them under
the same clipboard lock and sequence as capture. This suppresses repeated own
notifications even when the sequence changes after publication. Missing or
unknown markers retain normal capture, including identical external values;
there is no content-based deduplication. Marker write failure is explicit and
retains the exact-sequence fallback. Copy preparation and stale capture tickets
still obey the dispatcher cancellation fence.

The native contract follows Microsoft's [HTML clipboard format](https://learn.microsoft.com/en-us/windows/win32/dataxchg/html-clipboard-format)
and [SetClipboardData ownership rules](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setclipboarddata).
CF_HTML offsets count UTF-8 bytes from the beginning of the complete payload.
Parsing and publication have synthetic boundaries; tests never use real clipboard
contents or personal documents.

## Manual Windows acceptance

Enable Tables and clipboard capture. Copy a small range with a heading, Unicode,
numbers, an empty last column and an empty last row from Google Sheets or a
Notion simple table. Open it with Space, then paste with Ctrl+C into a fresh
table and compare the grid. Drag it and inspect the CSV independently. Repeat
with two tables selected in a deliberate Ctrl order and a Shift range after
sorting; copied tables should stack in that order, while drag creates two CSVs.
Hide Tables and verify collection stops. Excel remains unverified until a user
can perform the equivalent check there.

## Installed candidate · 2026-11-10

Release candidate `a1c71f601a6b` is installed. Its executable hash and all 96
source manifest entries were verified against the working tree at installation.
The full API/headless suite passed 317 tests with 0 failures and 3 existing
manual/performance tests ignored. All-target Clippy with `-D warnings`,
formatting, whitespace and the synthetic installation contract passed. The
incremental-cache hard-link warning is environmental. No real clipboard contents,
personal source files, desktop automation or application launches were used for
verification. No GitHub push was performed.

Ctrl+K now hides direct tab navigation, numbered-tab and tab enable/disable
commands. All ↔ previous tab and opening tab settings remain visible. The
underlying dispatcher routes, numeric keys and user shortcut overrides remain
available. This is a working candidate awaiting owner acceptance of actual
spreadsheet/browser clipboard formats, not a newly accepted release baseline.

The owner subsequently confirmed actual Google Sheets → Notion and Notion →
Google Sheets table transfer. She reported self-capture after table Ctrl+C and
Space routing to QuickLook; the next candidate adds the ownership marker and
unifies internal preview capability checks for synchronous/background routes.

## Zoom and ownership-fix candidate · 2026-11-10

Installed release candidate `e0c6ba72930d` adds table zoom, pan, fit and reset,
fixes Space routing through background jobs, and suppresses own table clipboard
publications using ownership tokens. The executable and all 97 source manifest
entries were verified at installation. The complete suite passed 325 tests
(0 failures, 3 existing manual/performance tests ignored). All-target Clippy
with `-D warnings`, formatting and the synthetic installation contract passed.
No application or real clipboard was operated for these checks. Owner mouse
and clipboard acceptance is pending; no GitHub push was performed.
