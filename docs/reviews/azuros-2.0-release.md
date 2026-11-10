# Azuros review: Katran 2.0 release

Review date: 2026-11-10. Reviewer: Azuros Claudapi, Senior, gpt-6.1-sol.

Scope: the intentional tracked and untracked functional changes after the
accepted 1.5.0 baseline, through candidate `e0c6ba72930d`. This is an independent
source review, not a repetition of the owner's Windows acceptance. Release
versioning, packaging and updater changes being prepared concurrently require
a separate follow-up below.

## Findings and resolution

### P2: Do not write exact Link payloads to diagnostics

Location: `src/diagnostics.rs:263`, fallback in `summarize_command`; the missing
special case is `Command::IngestLink { url, source }`.

`Service::execute` records the command summary before dispatch
(`src/lib.rs:284`). Text and Table ingests replace their content with counts,
but Link ingest falls through to `serde_json::to_value(command)`. Consequently
the complete URL, including user information, query parameters and fragment,
is persisted in `logs/events.jsonl`. The recursive `BeginJob` summarizer has
the same omission. This affects even a rejected ingest: logging precedes
validation and the Links feature switch. The stdio client enables this logger
by default (`src/main.rs:23`). Token redaction only recognizes a few credential
prefixes and does not protect ordinary signed-URL or application token values.

Deterministic synthetic reproduction: with diagnostics attached, submit

```json
{"api_version":1,"request_id":"synthetic","command":{"type":"ingest_link","url":"https://example.test/download?access_token=SYNTHETIC_PRIVATE_VALUE#private-fragment","source":"synthetic"}}
```

The command summary contains `url` verbatim, whether the request succeeds or
returns `tab_disabled`. The same result follows from directly evaluating
`diagnostics::summarize_command` on that command. No native clipboard or network
access is involved in this reproduction.

Fix: add an explicit Link ingest summary containing type, source and byte count,
without URL content. Add a synthetic regression test for direct and nested
`BeginJob` Link ingest; check that the secret URL cannot occur in the serialized
summary or flushed log. This is a bounded privacy fix recommended before push.

Resolution reviewed: the lead added an explicit `IngestLink` count/source branch
and `link_diagnostics_omit_url_in_direct_and_nested_commands`. Direct and nested
summaries now omit the URL and its query/fragment. This finding is closed in the
reviewed working tree; the lead owns the final test run.

### P2: Keep update finalization off the UI dispatcher

The initial release integration called `updater.commit(prepared)` from
`Service::poll_job`. Commit rereads and hashes the current executable repeatedly,
verifies the new executable/manifest/notices, synchronizes files and replaces the
pointer with Windows write-through semantics. With a permitted 200 MiB executable
or slow storage, this synchronous finalization blocks the UI dispatcher, including
copy/capture input, even though download/extraction already uses a worker.

Resolution reviewed: `UpdateReady` now crosses `authorize_claimed_effect` and
uses `continue_claimed` on its original Update worker, returning
`PreparedWork::UpdateInstalled`. The final dispatcher phase only updates small
status JSON. The dedicated lazy Update lane also prevents HTTP waits from
occupying Foreground or Capture workers. This finding is closed in the reviewed
working tree; the lead owns final integrated verification.

## Other reviewed boundaries

- Schema v2-to-v6 migration ordering and pre-migration backup; old setting/tab
  defaults; per-event fragment kinds sharing content-addressed blobs; last
  reference deletion and foreign-key cleanup.
- Shared dispatcher sync/background commands; Text/Link/Table ingest, read,
  merge and smart copy; partial table format publication; exact revisions;
  cancellation at preparation/dispatcher/Windows effect boundaries.
- Capture priority, feature gates at preparation and commit, stale capture
  tickets, clipboard sequence checks, bounded ownership tokens and native
  marker reads. Unsupported/ambiguous table input remains explicit.
- CSV/TSV/HTML and CF_HTML encoding/decoding limits, escaping and byte offsets;
  combined table row/cell/content limits; independent copy/drag/open snapshots;
  grouped Link drag; copy-only OLE export lifetime.
- SQL search and visibility before LIMIT, sorted keyset pagination, reverse
  order, missing values, shuffle seed, cursor context and recorded modification
  timestamps; range selection order versus explicit Ctrl-click order.
- File open/reveal, cached named snapshots, associated-app enumeration,
  session binding and bounded handler/icon storage; no public executable-path
  capability was introduced by the picker.
- Lazy Text/Table read-only workers, conservative Markdown rendering, no HTML
  execution or remote fetching, grid virtualization/overview and domain-owned
  zoom/pan/fit state; GUI dispatch routing and tab settings.

No additional actionable P1/P2 regression or data-loss finding was established
in these boundaries. This statement is limited to the examined source snapshot.

## Verification and limits

`git diff --check` passed for the examined tree. The owner/lead supplied the
candidate's 325-passed / 0-failed / 3-ignored API/headless result and all-target
Clippy result; these are supplied release evidence, not tests rerun by this
review. This reviewer did not run a duplicate full suite. The finding above is
directly established by the command-summary branch and the logging call site.

No personal roots, original files, real clipboard contents, native apps or
desktop were operated. No executable was installed/launched and no Git push was
performed. The owner's reported Windows acceptance remains the evidence for
actual clipboard target compatibility, native application invocation and
physical keyboard/mouse behavior. Excel interoperability remains outside the
reported Google Sheets/Notion acceptance. Unkillable third-party Shell/COM calls
and live-view cursor movement during concurrent metadata changes remain the
documented limitations, not new findings from this review.

## Release follow-up

Reviewed Cargo 2.0.0 versioning; `src/updater.rs`, shared API/job integration and
the fourth lazy Update lane; launcher layout/registration changes;
`scripts/package-release.ps1`, `installer/katran.iss`, the Windows check/tag
publication workflow, dependency notice collection and its pinned license-text
overrides. At this review point the fixes above are closed and no additional
actionable P1/P2 finding was established in these files.

The updater checks a fixed public repository, stable semantic versions, HTTPS
GitHub hosts (including redirects), version-tagged asset URLs, metadata sizes and
SHA-256. Its trusted candidate is Serialize-only and cannot be supplied over the
wire. ZIP handling bounds entry count before indexing, checks local/central
header consistency, requires exactly the three named root regular files and
bounds each streamed member. The executable, JSON manifest and UTF-8 notices
are bound by hashes; notices are capped at 8 MiB and manifests at 64 KiB.
Preparation uses an OS file lock and an owned staging directory; cancellation
drops preparation. Commit verifies files and the prior pointer, publishes an
immutable version, then atomically replaces only the launcher pointer. Stage
cleanup refuses reparse paths and removes only its three known files. No updater
path enters the shelf database or original source roots.

Packaging copies an explicit binary/license whitelist, emits relative source
hashes, refuses an existing output directory and uses fixed ZIP timestamps.
CI checkout is commit-pinned with persisted credentials disabled; default token
permission is read-only and release write permission is limited to the checked
tag publication job. Installer data is outside the application directory;
uninstallation does not recursively remove user data. TLS and GitHub repository
ownership are the declared trust anchor; no signing provenance is claimed.

Remaining explicit limits: the first actual published asset download, native
setup execution and real restart/update cycle were not exercised by this
review. Full launcher replacement requires a full package. The three-file
in-app update does not replace the separately packaged CLI binary; future CLI
API/schema changes require a full package as well. Prior immutable versions and
crash-orphan stages are not reclaimed by this updater.

## Final GUI integration and verdict

The final `src/bin/updates_ui.rs` and Settings integration were reviewed after
the API and worker changes. Opening Settings/Updates reads status only; network
access begins solely after an explicit Check action. Install is offered only
for an available trusted candidate and all work enters `BeginJob`. The client
retains/polls its owned job before the Settings-closed early return, so closing
Settings does not abandon the request. Cancellation polls the real terminal
result and cannot claim to undo a committed pointer switch. Installed state
asks for a manual restart and prevents another check/install before that
restart. The GUI contains no independent download, extraction or storage path.

Also reviewed the local build manifest's launcher/update protocol fields and
the matching installed/portable data-path resolution. A current development
build may omit redistribution notices; incoming release updates still require
all three verified archive members and the notices hash.

Final source-review verdict: no outstanding P1/P2 findings. Both established
P2 findings above were fixed and their implementations rechecked. The examined
2.0 functional and release integration is suitable to proceed to the lead's
final full suite, Clippy, build/package checks and authorized publication.
This is a source-review verdict; it does not claim execution of the new
installer or a real published-release update/restart cycle. The lead owns
recording those final build/test results and the publication artifact hashes.
