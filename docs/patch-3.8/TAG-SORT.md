# Tag-order sorting · local 3.8 · 2026-11-18

Owner requested an additional mode alongside custom pinned order. In the sort
menu opened by comma, ordinary Slash selects `sort_tags` (sequence `, /`). Slash
outside that menu retains the tag palette. Actions → Sorting also exposes the
mode; the existing rebindable domain action path is used.

## Domain contract

`SortMode::Tags` persists independently per tab. Current tag positions 1…9 define
groups, untagged items come last; within groups normal shelf arrival-descending
(stable ID) order applies. Desktop uses detected-at descending and stable ID.
Reordering complete tags atomically remaps assignments, so this sort follows the
new tag positions. It does not overwrite custom pinned ranks. Reverse normalizes
to false, matching custom pinned/shuffle behavior.

Ordinary shelf grouping occurs in SQL before LIMIT with keyset pagination.
Cursor identity includes tag-definition revision and a deterministic assignment
fingerprint read in the same SQLite snapshot as page rows. A separate Service's
assignment/reorder invalidates the old cursor. Desktop uses its existing
metadata-only projection and reads the assignment map once, without per-item
queries or file decoding. The UI performs no local page-only sorting.

Folder has no tag slots: SetSort rejects Tags with `folder_has_no_tags`, settings
validation rejects it, SortActions and its keyboard menu omit the choice.
Virtual emoji catalogue keeps its existing fixed Unicode order, as with other
sort modes, and virtual entries cannot receive tags.

## Verification and compatibility

API tests: ordinary grouping/order/reorder, reverse normalization, cross-Service
cursor invalidation, Folder rejection; Desktop grouping/stable ties and cursor
invalidation. Headless input test checks comma→Slash, no interception outside the
menu, and Folder choice/index alignment. Existing sequence-hint contract covers
all registered sort actions.

Schema stays v13. Older binaries do not know settings enum `sort.mode: tags`:
before controlled rollback select an older supported sort with this build or
restore compatible preferences/backup. Do not assume switching only the runtime
pointer is sufficient. No public push/tag/release is authorized.

## Installed artifact

Source `5135443`, runtime `efc8c786e60f`, one 82.31s build. Targeted API regression
20/20 passed. UI 265/266 plus isolated timing-fixture retry 1/1, all new sort/cache
checks passed. All-targets Clippy and formatting passed. Native owner acceptance
of ordering and visual cache behavior remains pending. See VERIFICATION.md for
artifact identity and controlled rollback.
