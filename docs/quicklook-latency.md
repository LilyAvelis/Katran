# QuickLook latency observation · 2026-11-10

The installed QuickLook desktop CLI (4.4.0) and the Katran 1.2.5 release stdio API were measured against three small synthetic files. QuickLook was already running; this is a warm-process observation, not an application cold-start test. The Katran executable SHA-256 was `B02AECC7D7528656F525CF698B0E07B19E9137C27DF43F79959088284CAFE333`.

Each mode sent six cycles of TXT, PDF and PNG in that order. The first cycle is reported separately; the remaining five samples per file form the warm summary below. Times are milliseconds; p95 uses nearest-rank and is consequently the maximum of five observations. The small sample size makes differences and tail estimates exploratory, not performance guarantees.

| Fixture (bytes) | CLI visible-title p50 / p95 | API visible-title p50 / p95 | CLI creation p50 | API response p50 |
| --- | ---: | ---: | ---: | ---: |
| TXT (187) | 270.9 / 277.3 | 274.7 / 286.7 | 3.5 | 3.7 |
| PDF (1,856) | 271.8 / 315.2 | 275.9 / 301.2 | 3.2 | 3.9 |
| PNG (14,959) | 254.2 / 281.3 | 282.1 / 380.3 | 3.3 | 3.8 |

First-cycle visible-title times were CLI 277.5 / 284.2 / 254.4 ms and API 262.4 / 260.6 / 440.0 ms, respectively. All 36 title observations arrived within the three-second deadline; all API responses reported `requested: true`, and all direct CLI child processes exited with code zero. Direct CLI child exit is distinct from process creation: warm medians were approximately 230–249 ms depending on file type.

The visible-title clock begins immediately before request dispatch and ends when a visible window owned by the existing QuickLook server PID has a title containing the requested fixture's basename. The script polls Windows window metadata every 5 ms; scheduling and polling add uncertainty. Alternating distinct basenames avoids mistaking the previous title for the requested one. A title can appear before content is rendered or a frame is painted: these measurements cannot establish content-ready latency. `requested: true` indicates process creation, not QuickLook rendering or focus.

The direct CLI and API phases ran sequentially, so background load and warming can affect comparisons. These figures are not Explorer timings. They do not include Katran's physical keyboard input, GUI event handling, dispatcher contention, or any subsequent GUI refresh; the PNG API command explicitly invokes external preview even though the normal image preview is internal. They use tiny cached files and do not measure the cost of checking a large file's revision. The result suggests that for these fixtures the request returns well before the title changes, but it does not identify the cause of perceived latency or prove there is no optimization opportunity.

To repeat, with the existing QuickLook server PID and an isolated, synthetic fixture folder:

```powershell
python scripts/measure-quicklook-latency.py --katran target/build/release/katran.exe --fixtures 'C:\Users\lilya\Downloads\QuickLook-Test-Files-20261110-102249' --pid <QuickLook-server-PID> --rounds 6 --output "$env:TEMP\katran-quicklook-latency.json"
```

The tool creates a temporary Katran data directory, ingests references to the synthetic fixtures and uses one long-lived stdio process. It records JSON and CSV, then toggles the last synthetic preview through the documented CLI. It does not open QuickLook's named pipe. The measurement output from this run is in `%TEMP%\katran-quicklook-latency-20261110.json` and the adjacent `.csv`. Do not use personal files for benchmark inputs.


## Subsequent Katran-only cleanup · 1.2.6 Test

The UI no longer performs full BrowseShelf/Status refresh after open/preview/
quicklook actions; it updates Interaction only. A headless diagnostics regression
checks the actual command sequence and retains full refresh for shelf refresh.
This change does not alter transport or source freshness checks. It was not
included in the preceding API/CLI benchmark and has no claimed measured window
latency improvement; that benchmark excludes GUI refresh entirely.

## QuickLook source investigation

The [4.4 startup path](https://github.com/QL-Win/QuickLook/blob/4.4.0/QuickLook/App.xaml.cs)
initializes application/static state and checks for an existing instance before
forwarding the preview. The source explicitly notes CLI performance as an area
for improvement. This is consistent with, but does not by itself prove the cause
of, the roughly230–249ms CLI child lifetimes observed above.

The [4.4 IPC handler](https://github.com/QL-Win/QuickLook/blob/4.4.0/QuickLook/PipeServerManager.cs)
uses ApplicationIdle dispatch priority and does not guard null lines or restart
a failed reader task. An experimental QuickLook fork could harden this listener,
then compare a minimal launcher/persistent client, startup path, and UI scheduling
without rewriting format viewers. Katran stays on its proven CLI path until such
a change passes repeated real-server failure and latency tests. No QuickLook
source or installed files were changed during this source investigation.
