# Catalogue sorting contract

Sorting is domain state. The complete current source and search filter are
ordered before pagination; visual clients never reorder a received page.

`SetSort { mode, reverse, seed }` selects arrival, extension, modification date, shuffle,
natural name, size, alphabetical name or origin. Arrival remains the backward-compatible
default. Name and extension keys are Unicode lowercase. Natural name compares
ASCII digit runs numerically without an integer-size limit (`file2` before
`file10`). Equal primary keys use lowercase name and event ID as deterministic
tie-breaks. Reverse changes the complete known-value order. Unknown modification
dates remain last in either direction.

Origin sorts recorded provenance: source domain first, then source application,
then a known adapter label (Downloads, ShareX, clipboard or manual). Unknown
origins remain last even in reverse. A Downloads entry without recorded website
metadata is labelled Downloads; no browser history or website is inferred.

Modification date means the original file's recorded modification time, never
arrival time. It must be recorded during domain ingest/source receipt commit.
Historical metadata is unknown until a subsequent verified source receipt;
browsing must not stat historical files. Text and clipboard byte payloads have
no original-file modification date.

Shuffle uses a stored session seed and stable event-ID hash. Pagination keeps
the same seed. Shift does not reverse shuffle. A newly requested shuffle can
choose a new seed; refreshing or loading another page must not do so.

An ordered cursor must bind the sort specification, source/tab and search filter
and carry the last row's ordering keys. The existing descending-ID cursor alone
cannot represent other sort orders. Changing any bound input resets pagination.

The native key adapter uses comma followed by `e`, `m`, `r`, `n`, `s`,
`a`, or `o` (origin); Shift on the second key reverses the corresponding order except shuffle.
Text inputs keep ordinary comma/letter input. Ctrl comma remains Settings.
All sort operations are also available through Ctrl K Actions.

Range selection follows the displayed top-to-bottom order. Ctrl selection
retains explicit click order, independently of the current sort mode.

`BrowseShelfSorted { query, limit, cursor }` uses the current domain sort and
returns `events`, `sort`, and an opaque string `next_cursor` (or null). The
cursor is versioned and bound to source configuration generations, enabled
Images/Text filters, tab, search and the complete sort specification. Malformed
or oversized cursors produce `invalid_cursor`; changed context produces
`stale_cursor`. Clients restart from a null cursor after a context change.
Deleting the last page anchor does not invalidate its recorded key.

The existing `BrowseShelf { after_id, ... }` retains its descending arrival
contract for compatibility. GUI clients use the sorted command. Arrival's normal
order is newest first; other modes begin ascending. Shift reverses all known
keys and deterministic tie-breaks. Missing values remain at the end.

Schema v4 adds nullable `events.modified_at_ms` and ordering indexes, preserving
a backup before migrating an existing database. Trusted source commits refresh
the recorded modification timestamp even if content hashes are unchanged;
this does not create another arrival. Existing entries are never backfilled by
scanning original paths at migration or browsing time.

The default arrival query retains the indexed compound SQL merge path. Other
modes calculate ordering in SQL across matching rows, then decode only the
requested page; natural/origin/shuffle keys can require a SQLite temporary sort
for a large matching catalogue. Browsing performs no filesystem metadata reads
for sorting. Concurrent changes to a row's ordering metadata can move it across
a cursor boundary; paging is a live view rather than a pinned snapshot.

The comma prefix expires after two seconds and is cancelled by Escape, a
modified shortcut, or entering a text field/dialog. Ctrl comma opens Settings.
Commands in Actions can be rebound individually through the existing catalogue;
comma sequences are an input adapter and do not alter the shortcut chord grammar.

An explicitly rebound bare `Comma` shortcut takes precedence over the sequence
adapter. Sorting remains available through Actions in that case.

## Working build verification · 2026-11-10

Installed release candidate `e68c1216bf87` includes sorting, smart text copying
and Markdown preview. At that installation, the executable hash and all 91
source manifest entries matched the working tree. No application was launched
for verification.

The full API/headless suite passed 273 tests. After the final custom-Comma
precedence fix, all 51 UI tests passed (one additional test). Clippy with
`-D warnings`, formatting and the synthetic installation contract passed.
Windows visual acceptance remains with the owner; this is not a new accepted
release or a GitHub publication.

## Sorting feedback and evidence in the UI

Actions show sequence hints such as `, e` and `, Shift+E` in the bounded right
key area. These hints are presentation only. A custom assigned shortcut is shown
instead when present; the tooltip still explains the sequence.

A successful sort action confirms the mode and direction in the reserved bottom
status line, for example “Сортировка по размеру · большие → маленькие”. Cards and
stream rows show the active sort criterion for size, original modification date,
extension or provenance. Unknown original modification dates are explicitly
unknown. Dates are displayed in UTC, and sizes use binary units. Full metadata
is available in the card tooltip; narrow cards retain clipped single-line text.
No filesystem reads are performed while formatting or painting these values.

The input adapter accepts ordinary comma followed by Shift plus a letter,
physical key positions under another keyboard layout, and Shift comma followed
by a letter as an additional reverse form. A text-only `<` / uppercase-letter
fallback handles adapters that provide text rather than physical key events.
Synthetic RawInput checks include key press plus uppercase Text, a mismatched
logical/physical key with Russian Text, and Shift comma plus `<` Text. These
checks do not substitute for the owner's physical Windows keyboard acceptance.

## Feedback and Links candidate · 2026-11-10

Installed release candidate `e0c4aff23a8d` adds these presentation/input changes,
Ctrl-arrow document modes and the Links feature. The executable hash and all
92 source entries were verified against the working tree at installation.
Full API/headless suite: 289 passed, 0 failed, 3 existing manual/performance
tests ignored. The final UI-only rerun passed 58 tests. All-target Clippy with
`-D warnings`, formatting, whitespace and the synthetic installation contract
passed. The existing incremental-cache hard-link warning is environmental.
No desktop automation, application launch or GitHub push was performed.
