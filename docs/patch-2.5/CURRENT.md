# Katran 2.5 — current engineering handoff

Latest runnable artifact and verified product commit: [ARTIFACTS.md](ARTIFACTS.md).

## Manual-feedback patch

Lilia's screenshots showed Folder service details expanding the header. These
details now live in a fixed bottom information bar with bounded click-open
panels. Header/footer geometry is verified across Images/Folder/pinned.

Shift+Tab is the primary direct transfer/import action, implemented as
TransferSelection in the shared API without any Clipboard reads. Category to
Folder moves unpinned items and copies pinned ones; Folder to category imports
supported formats, preserving originals and reporting unsupported items.
API checkpoint: 8366a8a. Four synthetic direct-transfer scenarios, the headless
keyboard round trip and no-Folder/text-field cases passed. Ordinary Tab stays
navigation-only. The information pane and shortcuts follow in the next checkpoint.

The same-ID/revision preview regression passed for external QuickLook and an
internal PNG in ordinary/pinned slices. No slice-specific QuickLook branch was
found. The generic job label moved into the information bar; the reported native
latency difference is not yet measured or claimed fixed.

2026-11-11. This supersedes onboarding and intermediate status in HANDOFF.md. The local implementation is unaccepted; public release remains 2.0.0. Read TEAM.md for reusable sessions and PIPELINE.md for the agreed order.

## Checkpoints

- ecb30e5: all pre-existing local fixes and onboarding.
- 1544666: P1 domain.
- ea92a3e: first runnable P1 slice; executable SHA-256 e9365303e144b8f0965388bf9bb3a12dcbea8d7b8c7a63765dc903ee636850f0. This older preview remains available for comparison.
- fff5309: P2 workflows and P3 properties before final regression reconciliation.
- The following checkpoint contains final integration/tests; obtain its actual SHA with git log. Nothing was pushed.

Lilia authorized autonomous implementation and local checkpoints. The installed runtime pointer and existing user database were not changed. Native GUI acceptance remains with Lilia; tests use synthetic data and fake boundaries, except one explicit disposable native recycle smoke.

## Implementation

Polly owns Command/dispatcher/schema/catalogue integration. Fenna owns Folder state and UI clients. Mira owns Windows/Everything/transfer/recycle and source-worker exclusion. Nemi owns persistence/import/default-handler policy and protected deletion. Vela owns revision-bound negative-ID previews. Rhea prepared immutable preview packaging. Kiki supplies acceptance scenarios; Pax supplies Folder deletion/timestamp tests; Lune maintains contracts; Walter reviewed risky boundaries and order. Reuse the actual sessions in TEAM.md.

Folder replaces All: legacy internal ShelfTab::All and wire alias all now route to Folder. Negative IDs are session-only and never reused. Context changes clear stale selection. Pinned/tags stay category-local; one tag filter is active. Unpin retains metadata but disables editing/filtering it. Display names do not rename files or change URLs. Application choices are opaque Shell handler IDs, not executable commands.

Schema 8 backs up before migration, preserves existing records, initializes owned modified times and converts historical permanent managed-original policies to recycle. History/favorites persist independently; current Folder does not.

Ctrl+X stages intent only. Managed moves retain the same verified source handle through copy/removal and a verified destination handle through the destructive effect. Pinned items copy. Partial receipts distinguish failed source removal from source removed/shelf retained. Consumed intents are not silently retried. Shared blobs survive while any event references them.

Folder/Downloads/Screenshots deletion uses handle-bound same-volume staging with JSON phase journals and explicit ITransferSource::RecycleItem. There is no permanent fallback. Failure restores without overwrite or returns a verified recovery path. Folder aliases of pinned external paths are protected. Pin changes are refused while background jobs are active. Recovery staging never enters source projections. Recovery is manual; directory-rename durability under power loss is not proven.

Ordinary Folder Ctrl+C/drag exports independent named files, with source and snapshot verification. Changing Folder invalidates old internal IDs and paste can consume the independent clipboard files. External file paste copies and never implicitly imports. Add to Katran explicitly handles images, UTF-8 TXT/MD, CSV and Netscape bookmarks with per-URL outcomes; unsupported types return errors and preserve originals. Own export timestamps preserve arrival time; external timestamps remain intact.

## Verification

- Full serial no-fail-fast regression identified eleven legacy targets affected by All/schema/delete-policy changes. Every affected target subsequently passed its focused rerun, preserving its coverage under the new contract. Successful unaffected targets were reused.
- Headless UI covers dialogs/navigation, Copy event/key deduplication and Ctrl+X → Tab → Ctrl+V. A real button-focus bug was reproduced and fixed; text fields retain local input.
- Folder copy/delete/import, mixed pinned moves, aliases, recovery success/failure, stale revisions, source-worker exclusion and late-export timestamps passed targeted API checks.
- Native recycle first returned ACCESS_DENIED in sandbox and restored its source. The same prebuilt binary passed outside sandbox on disposable data. Native Shell tests are opt-in in the normal suite.
- All-target clippy with -D warnings passed; logs are under target/katran-2.5-*.log. Do not sum overlapping targeted runs into an invented count. Environmental hard-link cache warnings did not trigger cache clearing.
- Everything answered a real read-only query for the project folder outside sandbox.

Final packaging checks source hashes, commit, toolchain, lockfile, features/profile and executable SHA-256. The latest preview remains isolated from the installed version.

## Remaining acceptance and limits

1. Lilia's Windows feel, real browser bookmarks import and chosen-browser URL invocation need feedback. Link choices now use HTML/browser associations and pass the exact URL; fake-provider API behavior is verified, actual browser invocation is not.
2. Folder cut is explicitly unavailable; use ordinary copy. Same-folder staged moves are no-ops. External clipboard cut is not performed.
3. Unsupported import formats have no new category. Historical tags, identity and application settings are not reconstructed from files.
4. No reliable marker distinguishes Raycast emoji service copies. No blanket emoji filter was installed; real source evidence or an explicit compromise remains necessary.
5. Afio application integration, recursive folder browsing, content editors and cloud/account features stay out of scope.

Next: run the latest isolated preview, collect short owner feedback, then continue the existing sessions. Do not automatically install, publish, push or reset the branch.

## Owner feedback: refocus and category arrows
Tab now routes in raw_input_hook before egui begin_pass can queue widget focus traversal. Deliberate search focus and modal Tab navigation remain local; unrelated stale focus is surrendered on window return. Afiocira photo-gui/main.rs consume_plain_tab confirms the same queued-egui-traversal cause.
NavigateTab API and tab_previous/tab_next actions implement Ctrl+Shift+ArrowLeft/ArrowRight: visible saved category order, clamped boundaries, no Folder entry, exit pinned/tag view even at a boundary. Folder is unaffected. Digits and tab clicks retain repeated-selection pinned behavior; arrows do not enter pinned. GUI chord encoding explicitly maps egui arrow names to the API canonical Arrow names.
Verification: category_navigation, shortcuts_contract, palette_tab_visibility passed; full UI suite 83/83 passed, including focus loss/return, deliberate search/modal ownership, pinned boundaries and existing double-digit behavior. Owner performs native Windows acceptance. Folder-return product decision and redesign remain deferred.
