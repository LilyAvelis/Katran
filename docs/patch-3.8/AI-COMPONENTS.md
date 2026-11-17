# Optional AI components · local 3.8 · 2026-11-17

Settings → AI contains separate Emoji search, Folder neural search and CUDA
cards. Nothing downloads on opening Settings. Install and Verify are explicit
actions that open a visible PowerShell console, kept open after completion.
The existing GPU preference moved here from General; it is still optional.
The two search features use multilingual-e5-small, but their environments are
independent. Installing Folder does not build the emoji catalogue/index.
CUDA is a separate acceleration component, not another search model.

## API and terminal contract

`AiComponentsStatus` only reads metadata and the last terminal receipt. Files
found does not mean inference works; a successful verification is displayed
with its completion time. `LaunchAiComponent {component, action}` accepts enums
`emoji|folder|folder_gpu` and `install|verify`. Launch acknowledgement only means
the visible process started. Injected Launcher/Installation boundaries let API
and UI tests avoid real consoles, downloads and installed environments.

The Windows adapter uses CREATE_NEW_CONSOLE without inheriting GUI standard
handles; arguments are separately quoted, with no interpolated shell command.
The console receives `<profile>/ai-installations/<id>/request.json` and writes
`install.log` plus an atomic `result.json` before returning to the open console.
Receipts include component/action, state, verified, code/message and timestamp.
A per-component exclusive file handle prevents concurrent terminal operations.
Closing early produces an explicit missing-result failure while the owning
Service is alive; after an app restart an unfinished operation is shown as
unknown, not success. Another attempt still respects the process-level lock.

Installer helpers, workers, public CLDR/Unicode data are embedded in the
executable. On explicit launch they materialize into a content-addressed,
verified bundle under `<profile>/ai-installers/<hash>`. No model weights are
embedded. This also supplies helpers when an existing installation receives an
executable-only update. Portable/setup packages separately carry the manual
scripts. Changed materialized helpers fail validation rather than being silently
overwritten; a local assistant can inspect/correct a script and run it manually.

## Components and installation

- Emoji keeps the existing installer, isolated environment, CLDR index and
  manifest under `emoji-ai-runtime`. Existing completed components are verified
  rather than replaced.
- Folder installs `folder-ai-runtime`, containing its own CPU environment,
  pinned model/tokenizer and manifest. Hashes and canonical containment are
  checked by Rust before use. An absent Folder runtime may use the existing
  verified emoji runtime for compatibility; an invalid Folder runtime fails
  explicitly. Legacy environment-variable/adjacent/dev resolver overrides are
  outside these profile-local install cards.
- CUDA installs `folder-ai-gpu-runtime` with pinned isolated ONNX/CUDA/cuDNN
  packages. It does not replace CPU environments, system CUDA or the driver.
  It can install before a model; then the receipt is installed/unverified with
  `model_missing`. A compatible NVIDIA driver remains necessary.

Model downloads use the fixed E5 revision and SHA-256 checks. A compatible
already installed model is copied locally rather than downloaded again. Missing
Python 3.10 x64 can be bootstrapped into the profile using the fixed official
Python installer, after validating its Authenticode signature/publisher. It
does not change PATH or install a launcher. Failure leaves an actionable log;
partial installations are not silently erased or replaced.

The console displays the journal path and a command to read it. Logs may contain
local paths and dependency output; the user chooses what to share with another
assistant. No diagnostic is uploaded automatically.

## Verification and remaining limits

The installer checks dependencies and performs a real synthetic worker query.
Emoji validates its manifest and worker response; Folder uses two invented
paths and a temporary index. GPU additionally executes inference and requires
actual CUDA nodes in the ONNX profile, with no silent CPU-only success.

Evidence: new API 1/1, bundle/argument unit checks 2/2, CPU resolver 2/2,
Folder semantic API 3/3, offline Python contract 3/3 and synthetic terminal/
package contracts passed. Headless UI: 250/251 in the restricted run; the sole
unrelated file-move fixture returned access denied and passed its authorized
isolated rerun. New AI click test passed without launching a real installer.
All-targets Clippy passed. Real installed emoji verification and CUDA
verification passed; CUDA executed 349 nodes and the actual Folder worker
returned the two synthetic results with an isolated cache.

Clean-machine Python download/setup, fresh CPU/CUDA pip installation and visible
console acceptance are not yet tested on another PC. These are remaining manual
acceptance checks, not claims of universal compatibility. Existing installed
models/environments were not reinstalled for this work. Enter paste behavior
remains separate; see [Azuros's audit](PASTE-AUDIT.md).
