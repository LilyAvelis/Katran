# 1.3 hardening · implementation and acceptance

Owner-authorized checkpoint: `585df0c` (1.2.6 feature line and Azuros audit).
No GitHub push before 2.0. Audit: `reviews/azuros-1.3-readiness.md`.

The user authorized fixing and optimizing the audited boundaries after this
checkpoint, and integrating the successful fast QuickLook experiment with
co-start/readiness when absent. Implementation is covered by domain-jobs.md; physical Windows acceptance remains with the owner.

## Integration ownership

One integrator owns API/dispatcher/schema/catalogue integration. Workers own
disjoint implementation modules and synthetic tests. API proof before GUI.

1. Explicit source membership/deletion policy independent of provenance labels.
   Generic IngestPath remains shelf-only. Trusted supervisor commits create live
   membership; raw serialized prepared hashes/classification pairs are rejected.
2. Materialized SQL catalogue projection: source configuration/generation,
   latest path/event, availability, verified classification. Filter/page before
   LIMIT; no unrelated filesystem work in BrowseShelf.
3. Domain SourceSupervisor owns watchers, restart/health/generation and bounded
   commits; GUI/CLI drive the same start/poll/rescan/status commands. No worker
   business lifecycle or second ingest implementation inside widgets.
4. Classification uses a continuing persisted work queue and digest index;
   129/257 items must drain. No public pair assertion can hide unrelated images.
5. Long I/O preparation must not take the interactive Service/UI with it. Typed
   jobs/status and bounded lanes; preserve freshness and batch error semantics.
6. ActionRow/KeyCaps own both real measurement and paint with production-style
   bounds tests, shared vertical axis and ancestor clipping.
7. Ordered schema migrations reject future versions; explicit table retention.
   Central action/type policy, sampled diagnostics, atomic verified install.
8. Fast QuickLook: separate external GPL fork, hardened legacy receiver plus
   versioned ACK endpoint; normal Explorer behavior; no duplicate app, no empty
   readiness probes. Background co-start and explicit compatibility/failure.

## Current QuickLook experiment (not installed)

Separate lab: `C:/Users/lilya/Lilia Avelis/QuickLook-Latency-Lab`.
`REPORT.md` and `DESIGN-NEXT.md` contain evidence and integration gates. Same
isolated server: warm 30 samples/type/mode, full CLI medians259–270ms versus
minimal client84–88ms, p95 client94–103ms. First66-call run had a PNG393ms outlier;
do not hide it. All252 calls passed visible-title/process criteria, not complete
rendering. Seven actual-server failure/recovery cases passed before and after.
Lab PID31992 stopped; installed QuickLook PID48004 and executable unchanged.
Working Katran pointer is still `09c3bce0319b` until a verified new build.

## Release acceptance

Close Azuros counterexamples with regression tests, then combined fmt/test/clippy.
Repeat fixed-page scale checks and end-to-end latency boundaries. Build a runnable
1.3 candidate locally; owner performs Windows UI/clipboard/QuickLook checks.
Do not claim a headless clipping test proves appearance or an ACK proves rendering.

## Implemented candidate

The listed boundaries are implemented in the1.3.0 Test candidate. Domain contracts
and remaining operational limits are documented in domain-jobs.md. Combined tests
and Clippy are verified by the integrator; the fresh reviewer follow-up records
closed counterexamples. No claim of unlimited source folders or a universal100ms
render guarantee. Candidate activation does not terminate the owner's old apps.

## Owner steering · replace the ordinary QuickLook

The owner rejected preserving a separate slow active installation. The fast normal
build replaced the ordinary QuickLook executable tree, the obsolete Store package
was removed, and production status reports fast_bridge_v1/ready. The backend's
normal default is now Programs/QuickLook. Candidate/lab source artifacts remain
build evidence, not a second installation for the owner to select.
