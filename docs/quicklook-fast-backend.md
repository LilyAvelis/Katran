# Maintained QuickLook bridge · integration contract

Katran's Rust backend uses a separate, versioned QuickLook endpoint. The maintained QuickLook application is a separate GPL artifact. Its source is outside this MIT repository in `C:\Users\lilya\Lilia Avelis\Katran\local-experiments\QuickLook-Latency-Lab\production-source`; its normal candidate is `production-candidate` beside that checkout. Nothing from QuickLook's viewer implementation is embedded in Katran.

## Backend API

`quicklook_backend::warmup()`, `status()` and `toggle(&Path)` return the standard Katran Result with a JSON result. The domain dispatcher owns their invocation; GUI clients submit background jobs. `toggle` is called only after the existing domain existence/content-revision checks. Supported native images retain Katran's own preview.

- `warmup`: keep a ready backend, or start the configured maintained fork when no standard QuickLook instance exists. The per-process gate prevents concurrent Katran launches; QuickLook's own mutex resolves external starts. Join an already-initializing maintained server without respawning it, or wait after a new launch, with the same8second readiness budget. Early exit/timeout is explicit, and no process is killed.
- `status`: read-only, with `ready`, `transport`, version/instance/PID for the fast backend; a normal existing stock instance reports explicit `desktop_cli` compatibility. Initializing is distinct from ready. No legacy-pipe readiness connection is made.
- `toggle`: returns `requested:true`, transport and acknowledgement state. Fast `accepted` means a bounded UI request was queued; `dispatched` means its UI action ran. Neither means painted/content-ready. Stock `process_created` retains the current CLI initiation contract.

`KATRAN_QUICKLOOK_EXE` may explicitly configure the maintained executable; otherwise it is `%LOCALAPPDATA%\Programs\QuickLook\QuickLook.exe`. Cold-start requires an adjacent `katran-bridge-manifest.json` with `protocol:1`, `flavor:normal` and a matching executable SHA256. This verifies local package consistency, not a signed publisher identity. A running bridge's reported PID is checked against the kernel's pipe-server PID, and its process image must equal the configured executable.

A stock instance already running is left alone and uses its own supported desktop CLI. Only an absent bridge permits that compatibility decision; busy, denied, invalid, ambiguous or incompatible bridge responses produce explicit failures. Never start a parallel normal server or terminate the owner's existing instance. Katran exit leaves normal QuickLook alive for Explorer. If the maintained package is absent and no QuickLook runs, co-start reports `quicklook_not_found` rather than silently launching an unknown program.

## Protocol

The normal endpoint is `QuickLook.KatranBridge.v1.<SID>`, separate from the legacy `QuickLook.App.Pipe.<SID>`. The server grants pipe access to the current user. The native Rust client creates no helper process, uses overlapped I/O with cancellation/deadlines, and never targets the legacy endpoint. Frames are a four-byte little-endian length followed by strict UTF-8 JSON, maximum131072bytes and depth16 server-side. One request/response is exchanged per connection.

Requests have `protocol:1` and a command:

```json
{"protocol":1,"command":"status"}
{"protocol":1,"command":"preview","request_id":"unique-id","path":"C:\\synthetic\\example.txt"}
{"protocol":1,"command":"request_status","request_id":"unique-id"}
```

Replies include protocol, PID, instance nonce, version, `ok`, state, and request_id/error where applicable. A preview ID is deduplicated against its original path; a conflicting path is rejected. At most four pending UI requests and256 recent results are retained. Queue overflow is rejected explicitly; completed entries are evicted first. After eviction, request_status returns unknown. Dispatch uses normal WPF priority; legacy integration remains compatible and uses its existing scheduling semantics.

Toggle must never be replayed after an uncertain write/ACK failure. The client queries request_status for the same ID and requires the original instance nonce. Failed lookup, changed instance or unknown result becomes `quicklook_outcome_unknown`; there is no CLI retry. A known unsubmitted request also reports its failure instead of secretly changing transports. The one-way laboratory client's written-only result is not used by this integration.

Every malformed/stalled/oversized connection is bounded and rearmed independently. The standard legacy receiver is hardened too: empty EOF and incomplete lines never kill its loop. Bridge ACK data remains connected until the client consumes it and closes, avoiding DisconnectNamedPipe dropping a buffered ACK. The client permits a25ms absent-pipe rearm grace; a busy bridge can use the bounded I/O deadline.

## Normal candidate and activation

The normal fork restores upstream Explorer/global keyboard/native/tray/focus behavior. It shares the normal QuickLook single-instance mutex and legacy endpoint. It starts the bridge before plugin initialization, but reports initializing until initialization completes. The stock update channel is disabled; its menu explains that this maintained build is updated with Katran.

The candidate uses `portable.lock` and its own adjacent UserData; ordinary installed QuickLook files and profile are not overwritten. The full existing stock plugin tree is copied unchanged. Custom settings are not silently migrated. The source ZIP, source commits, notices and source build command are adjacent to the candidate.

The external installer validates the normal candidate hash and source package. Its no-flags mode is read-only; new installs target the ordinary QuickLook directory and refuse an existing destination. Future replacements need an explicitly authorized update rather than silent overwriting.

## Activated on this host · 2026-11-10

The owner explicitly requested replacing the slow installation instead of keeping two versions. The verified normal build now occupies `%LOCALAPPDATA%\Programs\QuickLook\QuickLook.exe`; the old Store4.5 package was removed. Temporary sibling installation files were moved back into the existing development lab. API PreviewBackendStatus confirmed `ready:true`, `transport:fast_bridge_v1`, `version:4.4.0-katran-bridge1` from this ordinary path. This is the active default, not a manual side-by-side selection.

The normal application and its matching GPL source archive/notices are together in the installed directory. Existing QuickLook launch links using the ordinary path now open the fast build. No new Windows login autostart rule is added; Katran co-starts the backend when absent. Automatic co-start never terminates an existing process; the replacement/restart above was explicitly authorized by the owner.

## Verification and limits

Synthetic/fake checks run without touching the installed server or its clipboard:

```powershell
cargo test --offline --lib quicklook_backend --target-dir target/build
cargo test --offline --test quicklook_bridge --target-dir target/build
cargo clippy --offline --lib --test quicklook_bridge --target-dir target/build -- -D warnings
```

Backend tests cover manifest integrity/lab rejection, executable identity/readiness, existing/concurrent normal instances, existing-maintained initialization and its timeout without respawning, delayed readiness, one spawn on timeout, early child exit and lost-ACK lookup without replay. Native pipe tests cover request limits, forbidden legacy targets, absent endpoints, fragmented responses, kernel-PID validation, malformed replies and cancelled read deadlines.

The external .NET462 `bridge-tests` harness links the exact production bridge source and tests malformed/empty/truncated/UTF-8/stalled connections, subsequent status recovery, ACK loss/deduplication, four pending requests/overflow,270 request bursts, bounded result eviction and shutdown. The real WPF binary was built with `KATRAN_BRIDGE_LAB`: it uses unique lab mutex/pipe names and disables global hooks/tray/focus for isolation. The explicit ignored Rust integration test targets only that lab endpoint and verifies status, accepted/dispatched replies and duplicate-ID handling.

The normal build is now running and its production endpoint reports readiness. Explorer Space, actual preview rendering and physical Windows input still require the owner's visual acceptance. Acknowledged dispatch is not a content-ready signal. Previous warm-title improvements establish the opportunity but are not a new latency guarantee for this production endpoint. Large-file revision hashing remains unchanged and may dominate request time. No size/mtime-only cache weakens that correctness check. No GitHub publication before the owner's2.0 release instruction.

## Background job wire example

Use BeginJob for GUI startup warmup and external preview. The initial queued reply is pending work, not a completed preview. Poll until a terminal job state. The actual wire names are `succeeded`, `failed` and `cancelled`; queued/running/ready remain pending.

```json
{"api_version":1,"request_id":"warm","command":{"type":"begin_job","command":{"type":"ensure_preview_backend"}}}
{"api_version":1,"request_id":"poll-warm","command":{"type":"poll_job","job_id":1}}
{"api_version":1,"request_id":"preview","command":{"type":"begin_job","command":{"type":"preview_external","id":123}}}
```

Use the returned job_id rather than assuming the example's1. PollJob returns a job status with job_id, lane, state, result and error. The outer API `ok:true` confirms the poll itself succeeded; inspect the job state/error before using result. A succeeded preview result contains its initiation receipt under `result.receipt`. Job success and `acknowledgement:accepted` still do not assert painted content. Immediate commands can return `state:succeeded` and result without a job_id. Cancellation after an external effect starts cannot claim that a QuickLook request was undone.

The installed backend is already active. The next owner check is launching Katran1.3.0 Test and checking Explorer Space, Katran TXT/PDF/image previews and command-palette geometry. No manual switch to a different QuickLook installation is required. The stock CLI compatibility branch remains for other installations; this host uses fast_bridge_v1.

## Startup readiness and repeated requests · 2026-11-12

Katran submits EnsurePreviewBackend immediately after opening its service, before font materialization and shelf setup. Warmup remains asynchronous; the GUI does not wait for QuickLook to construct its first frame.

A normal QuickLook mutex with no bridge is ambiguous. When the configured installation includes a maintained-package manifest, status reports bridge_pending and waits for the ready handshake instead of treating the mutex as stock-ready and spawning another CLI invocation. If a genuinely stock instance is running alongside that maintained installation, readiness times out explicitly; no process is terminated. Without a maintained manifest the stock compatibility path remains available.

While startup warmup is pending, repeated UI open/QuickLook requests are reduced to the most recent request. Selection/tab/folder context must still match before it runs. Warmup failure clears that pending request and reports one error. Startup completion/error is recorded in diagnostics. Synthetic tests cover mutex-before-pipe, readiness timeout, click coalescing and stale selection; no real preview window is opened by these tests.
