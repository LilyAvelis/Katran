# Build and release discipline

Owner decision, 2026-11-10: optimize release delivery and agent time across projects. Global behavioral rules are installed in `~/.codex/AGENTS.md`; this document provides Katran-specific execution guidance. The global file is local user configuration, not part of the distributed application.

## What caused the long 2.0 release

The original workflow used a fresh Windows runner for checks and another for release compilation, with no Cargo/target cache or binary artifact handoff. Repeated infrastructure fixes restarted both stages. One recorded release compile alone took 6m49s in [run 42888269192](https://github.com/LilyAvelis/Katran/actions/runs/42888269192). That successful compile was followed by a license-metadata failure: release-only compilation had not downloaded every manifest required by offline Cargo metadata. There was no surviving binary artifact to reuse. A moving stable toolchain also introduced new Clippy warnings after local validation.

Agent orchestration added avoidable overhead: repeated clean runs, duplicate branch/tag checks, and minute-by-minute status turns. Those are distinct from compiler runtime and must be addressed separately. The owner reported a two-hour release session; that is not a measured duration of one compilation.

## Recurring lesson and accepted release scope

The owner reports the same pattern previously occurred in Afio. This is a recurring process defect: a newly noticed inaccuracy led to a complete release rebuild, even when it did not invalidate the binary. The repeated Afio case is owner testimony, not a new audit of that project.

The owner explicitly accepts releasing a working version with known bounded shortcomings and fixing them in patches. Do not add optional pre-release work merely to obtain reassurance. A concrete defect must be connected to the affected behavior; unease alone is not a reason to invalidate successful checks.

Before another expensive command, identify the changed input, the evidence it invalidates, the minimum affected stage, and the existing result that remains reusable. No changed input or concrete new defect means no full rebuild. This is a work-selection rule, not an additional approval gate.

The global entry point also requires reading `~/.codex/guides/devops.md` before release/CI work. [Notion lesson and principle](https://app.notion.com/p/41449fda32968163a218d5a0184af262). Instructions improve discoverability but do not mechanically prevent a new full dispatch. The implemented artifact/job boundaries remove compilation from package/publish retries; they are not a universal guarantee against repeated agent work.

## Execution contract

1. Identify the product SHA and build inputs: Windows x64, Rust 1.95.0, Cargo.lock, `ui,clipboard-events`, and profile. For workflow_dispatch, the workflow/main SHA is different from the product tag SHA; use the checked source output, never assume github.sha is the product revision.
2. Before expensive compilation, verify tag/version, required packaging tools, locked dependency availability, license metadata and notice generation. Keep errors visible. The checks gate remains mandatory.
3. Reuse the existing local `target/build` and CI dependency/target caches. Caches are disposable accelerators, not evidence of a particular successful release. Never cache .env, credentials, user data or exports.
4. Save exact built binaries and notices as an immutable artifact with the checked source revision and hashes. Packaging consumes that artifact; publication consumes the prepared package artifact. A cache miss must not change product semantics.
5. For a transient packaging/publication failure, rerun the failed job and its dependents. Do not restart passed checks/build jobs. A workflow-definition change and a new dispatch are a new run: cross-run artifact reuse is not automatic and needs separately verified producer/artifact identity. Do not claim that capability unless implemented.
6. Keep already-published tags/assets immutable. Current release workflow runs finish using their original definition; optimizing the next definition does not justify cancelling a release in progress.

## Verification by change

| Change | Required next action |
| --- | --- |
| Product Rust, dependency lock, features, toolchain or compiled assets | Relevant API tests and required CI checks; rebuild affected artifact |
| Packaging script / installer definition | Packaging contract and affected package; reuse an identical verified binary artifact |
| Publication/network only | Retry publication of existing verified packages |
| Docs only | Review links/text and diff; no product rebuild |
| CI cache/job wiring only | Parse workflow, review SHA handoff, conditions, permissions and artifact paths; observe the next real run |

Do not weaken mandatory checks to make a pipeline green. Once they pass, repeat them only for a changed input, a failure or a specific unresolved concern. Batch infrastructure corrections before retrying. After two failed attempts at the same stage, diagnose the cause instead of launching another blind full cycle.

## Waiting and completion

Use completion events or one bounded background watcher when supported. Do useful independent work while machines run. Do not spend an LLM turn on each status poll or send repeated unchanged commentary. Do not create a recurring automation to monitor one run. If the environment cannot wake the chat, state that limitation and leave the run URL plus the exact remaining action; do not promise a future message.

A passed check, published tag, generated package and published release are separate states. Confirm the required outcome once. For Katran 2.0, publication is followed by a bounded official-asset hash check and an isolated updater smoke using synthetic data, without launching the GUI or installer. Never describe those pending steps as already passed.

## Measuring improvement

Use job/step elapsed times for setup/download, compilation, tests, packaging and publication. Compare equivalent cold/warm inputs. No numerical speedup is claimed before a real warm run. Artifact reuse should remove compilation from a package/publish retry structurally; cache benefits depend on hits and restore overhead.

Prefer hosted-runner caches and artifact reuse first. Installing a self-hosted runner/service on the owner's machine is a separate explicitly approved decision, not an automatic release optimization.

Sources: [Codex global instructions](https://learn.chatgpt.com/docs/agent-configuration/agents-md), [GitHub caches versus artifacts](https://docs.github.com/en/actions/concepts/workflows-and-actions/dependency-caching), [rerunning failed jobs](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/re-run-workflows-and-jobs).
