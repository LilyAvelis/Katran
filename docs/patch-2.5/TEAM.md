# Katran 2.5 — team and handoff

Owner: Lilia. Coordinator: Polly (`/root`). Onboarding started 2026-11-11.
Canonical product scope: https://app.notion.com/p/5c5eb801699d40b88420f27ab42d0d4e
Legion: https://app.notion.com/p/41449fda3296813aa14ed7596d68b1b9

## Runtime constraints

Four concurrent slots including the coordinator; onboarding runs in waves of three.
Completed sessions are retained for targeted followup, not claimed to be running.
Fenna, Mira, Nemi, Vela and Rhea completed read-only technical onboarding;
Kiki completed onboarding with 11 acceptance scenarios, and Pax completed
read-only onboarding. Completed sessions are available for targeted followup,
not concurrently active workers. All nine sessions are created, but four slots
include Polly, so they are not all active at once.
Full fetched persona cards and source scope are in `onboarding/`, named by Notion ID.
garden.py list/profile used cached profiles after reporting a network error; full cards fetched successfully using Notion connector.

## Actual sessions

| Persona | Session | Model / effort | Area | Status |
| --- | --- | --- | --- | --- |
| Fenna Ferrule | /root/fenna | gpt-6.1-terra / high | Application/domain contracts, Folder and pinned state | Read-only onboarding complete; targeted followup available |
| Mira Socket | /root/mira | gpt-6.1-terra / high | Windows, transfer, capture, Everything | Read-only onboarding complete; targeted followup available |
| Nemi Index | /root/nemi | gpt-6.1-terra / high | SQLite, metadata and migration | Read-only onboarding complete; targeted followup available |
| Vela Frame | /root/vela | gpt-6.1-terra / medium | Preview and cache | Read-only onboarding complete; targeted followup available |
| Rhea Pipeline | /root/rhea | gpt-6.1-terra / medium | Build and verification coordination | Read-only onboarding complete; targeted followup available |
| Kiki Click | /root/kiki | gpt-6-luna / xhigh | Acceptance cases | 11 scenarios prepared; GUI remains owner-operated |
| Lune Ledger | /root/lune | gpt-6-luna / high | Team map and handoff | Read-only onboarding complete; handoff recorded |
| Pax Gauge | /root/pax | gpt-6-luna / high | Measurement and regression | Read-only onboarding complete; targeted followup available |
| Walter Wallace | /root/walter | gpt-6.1-sol / low | Original-file safety review | Read-only safety review complete; targeted followup available |

## Boundaries

Onboarding is read-only except these source snapshots and team/handoff documents. No runtime or personal data writes. No GUI/clipboard automation; owner performs native Windows checks. No builds or publishing during onboarding.

Existing uncommitted changes at entry: README.md, src/api.rs, src/bin/ui.rs, src/downloads.rs, src/interaction.rs, src/lib.rs, tests/downloads.rs, tests/interaction.rs. Preserve them and inspect before implementation.

Autonomous local implementation and Git checkpoints are authorized; GitHub push is not. Polly owns shared API/dispatcher/schema integration. API behavior is verified before GUI implementation. Native Windows acceptance remains owner-operated.

Scope: Folder replaces All; nonrecursive live files; Everything/history/favorites; copy/move materialization with pin protection; explicit return import; category-local pinned/properties/tags; owned timestamps; Raycast investigation. Afio app integration deferred to 3.0.

New 2.5 recycle-delete requirement for managed folders supersedes historical permanent ShareX behavior for this patch; make the behavior change explicit in implementation documentation. No file deletion during onboarding.

## Current implementation checkpoint

Baseline checkpoint `ecb30e5`; P1 checkpoint `1544666`; P1 preview checkpoint `ea92a3e`; P2/P3 checkpoint `fff5309`.
Implementation and regression reconciliation are complete for the documented scope. Early P1 remains immutable for comparison; the final preview is packaged separately. Read [CURRENT.md](CURRENT.md) before the historical handoff.
Version 2.5.0 is not accepted. See `HANDOFF.md` for gates and
[`PIPELINE.md`](PIPELINE.md) for artifact handling.

## Resume

Read this file, source scope and relevant handoff. Use followup_task with the existing canonical session when available. A saved row does not preserve an executing process. If runtime session is unavailable, explicitly record replacement with the same persona and transfer its handoff.
