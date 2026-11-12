# Links contract

Links is an optional feature after the owner-accepted 1.5.0 baseline. Enable its
eye in Settings → Tabs. `Settings.links_enabled` defaults to false. Disabling
Links hides the tab, closes its numbered position, and stops new link capture;
existing events remain stored. Links works independently of Text.

## Recognition and storage

Clipboard precedence remains files, enabled bitmap capture, then Unicode.
An entire explicit HTTP(S) URL is classified as a Link. Whitespace, control
characters, backslashes, missing authority and parser repairs such as
`http:example.com` are rejected. Prose containing URLs remains Text when Text
is enabled. When Links is disabled, exact URLs are skipped rather than stored
as Text. Existing Text events are never reclassified.

The exact original URL is retained, including Unicode, query and fragment.
Limits are 1 MiB per link, 8 MiB per combined copy/export and 1,000 selected
items. An exact repeated URL reuses an existing Link ID, preferring a pinned
match and then the newest ID. Pin, tags, properties, timestamps and origin stay
unchanged; existing duplicates are not merged or deleted. Text and Links remain
distinct kinds, and URL spelling is not normalized for identity.
Schema v5 adds event-specific `link_fragments` metadata and reuses content-keyed
search metadata. Migration makes a consistent `index-before-v5` backup before
changing an existing catalogue. Older binaries reject the newer schema.

Recognition, search, preview and export never fetch destinations, titles,
favicons or browser history. The destination domain is not evidence of the
clipboard source domain. Recorded source provenance remains separate.

## Domain API and transfer

`IngestLink {url, source}` validates and stores an exact URL while Links is
enabled. `GetLink {id}` returns its URL and revision. `GetDocumentText {id}` also
returns the URL for the read-only preview client. `CopyLinks {items}` explicitly
copies links; `CopySmart` and `CopySelectedObjects` choose this behavior for a
links-only selection. Items carry exact IDs and revisions.

Ctrl+C writes plain URL values separated by LF. Ctrl-click keeps explicit
click order; Shift selection follows the current visible top-to-bottom order.
A links-only drag creates exactly one UTF-8 `links.txt` with those URLs, one
per line, in the same order. A single link also produces `links.txt`. Text drag
continues to produce one TXT per fragment. Mixed selections use the existing
individual-file transfer path.

Grouped drag uses the existing owned snapshot plan and two-phase drag session,
not file writes in a widget. Generated files use create-new semantics within
Katran's export cache. Quota, source revision and cancellation checks apply to
the original selected events even though the resulting payload is one file.
Failed grouping does not publish an incomplete list. Copy failures leave the
clipboard unchanged; item failures retain their IDs and error details.

Enter opens the validated URL in the default browser through the injectable
Windows Shell boundary. Space shows the URL locally, without Markdown controls.
Other file-opening operations keep their existing file-based contract.

The pure module also contains Markdown, InternetShortcut and bookmark HTML
serializers from research. They are not exposed as working GUI export modes.
Tables and rich HTML conversion remain future work.

## Verification and owner acceptance

Synthetic API tests cover opt-in capture classification, exact data, search,
restart, old Text identity, selection order, fake clipboard/browser/drag sinks,
grouped and mixed drag, stale revisions, cancellation and deletion before async
commit. Headless UI tests cover Links preview while Text is disabled and worker
shutdown when Links is hidden. No personal files or real clipboard payloads are
used for these tests.

For manual Windows acceptance: enable Links and capture, copy two URLs, select
them in reverse order with Ctrl, then paste with Ctrl+C. Drag the same selection
and inspect the single `links.txt`. Repeat using Shift under a different sort.
Use Enter to open a URL, Space to inspect it, then hide Links and verify new URLs
are not collected. Existing Text records should retain their original kind.

Release candidate `e0c4aff23a8d` was installed on 2026-11-10. Its executable and
all 92 source manifest entries were verified. The complete suite passed 289
tests (3 existing manual/performance tests ignored), and the final UI rerun
passed 58 tests. Clippy and the synthetic installation contract passed.
Physical Windows acceptance remains with the owner.

Owner feedback on 2026-11-10 confirmed real multi-link plain-text copying and
the single-file drag workflow in `e0c4aff23a8d`. Repeated URL values remained
separate selected arrivals. This records acceptance of those flows, rather than
declaring every later working change a released baseline.
