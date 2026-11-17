# Folder search · 3.8 evidence

## Scope

FolderX uses the existing local Everything index. Search and ranking are algorithmic; no AI model or cloud service is involved. Absolute folder queries still use the existing direct-path flow and are validated when selected.

## Acquisition and ranking

The adapter independently queries an exact basename, a general basename, the full path, and, for eligible short ASCII queries, a two-substitution approximate basename. The approximate expression is anchored to the basename. Each stream requests up to 513 rows and retains at most 512: the extra row detects that the stream was truncated. Candidates are deduplicated and ranked locally before applying the requested display limit. An `incomplete` result means at least one stream exceeded its bound; `false` is not a guarantee that all semantically relevant or fuzzy matches were found. The approximate stream handles substitutions only, not arbitrary edit distance, and is omitted for non-ASCII alphabetic queries and compact queries outside 4–12 ASCII alphanumerics. Technical directories remain eligible results.

The installed Everything accepted `nopath:` for basename matching. The current documentation uses `no-path:`, but that spelling returned zero results in this installation; the adapter uses the verified spelling. Exact and approximate regex queries below were tested against this installation. This is not a general compatibility guarantee for every Everything version or configuration.

Up to four helper calls are sequential. Each retains the existing 1.5-second IPC timeout and 3-second process deadline; single-run measurements are recorded below; they are not a latency benchmark. Errors are reported instead of presenting a failed helper call as an empty result.

## Real helper evidence, 2026-11-17

The helper was run read-only against the local index after the execution environment allowed it. No personal paths are recorded here. The target is this repository's `Katran` directory. These are **raw helper candidate positions**, not the final order returned by Katran's ranked API.

| Query | Raw helper observation |
| --- | --- |
| Old `folder: <Katran>` | At least 513 results; the repository was absent from those first 513. The first five were recycle/recovery directories. The original UI requested only the first 100. |
| Old `folder: <cat run>` | 11 results; the repository was absent. |
| New exact basename, `Katran` | Two candidates; the repository was second. |
| New anchored two-substitution basename, `cat run` | Two candidates; the repository was second. |

## Ranked API snapshot, 2026-11-17

The debug CLI (`ui` feature, debug profile) was exercised through `serve-stdio` with a new isolated temporary profile under `target`; no source workers, capture or UI were started. CLI SHA-256: `3485e78e36df1864ad9b0a64c64eaf133e189a1fe9e68339412a14e3bbed7352`. Timing is one local observation, measured from request write to response read; it is not a benchmark. The old helper comparison used a limit of 100 and measured process invocation through response.

| Query | Old helper: count, target rank, time | Ranked API: count, target rank, time | API incomplete |
| --- | --- | --- | --- |
| `Katran` | 100, absent, 75 ms | 100, **1**, 617 ms | yes |
| `cat run` | 11, absent, 87 ms | 13, **12**, 733 ms | no |

For `Katran`, the old top three were recycle/recovery folders; the ranked API top three were the target repository, another folder named Katran, then a recycle/recovery folder. For `cat run`, the old top three comprised a technical folder and two other folders; the ranked API top three were other folders. Thus `cat run` now finds the repository in this snapshot, but its rank of 12 leaves a visible ranking limitation. No real path names or identifiers are recorded here.

Candidate acquisition may be truncated, and these examples do not establish complete recall for arbitrary queries or indices.
