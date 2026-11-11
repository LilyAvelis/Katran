# Local preview artifacts

No GitHub push, installer, Start Menu registration or installed-runtime pointer change was performed.

| Preview | Product commit | Executable SHA-256 | State |
| --- | --- | --- | --- |
| First P1 | ea92a3e | e9365303e144b8f0965388bf9bb3a12dcbea8d7b8c7a63765dc903ee636850f0 | Retained comparison snapshot; predates completed workflows |
| Integrated 2.5 | d9deb80c0c104674d6f90f433906b5c494458f10 | 821c1744002e624a92ee2df9cf9db5ff7c6b83b2c38dc3dac8decbb72d5ced80 | Retained pre-UI-feedback snapshot; its data profile is reused below |
| UI/Shift+Tab feedback patch | c014984 | c6964da9fb3abef03e7c46f65e23cfce49a931ae5ec70ae084ce95fb479226ce | Latest; locally fast-forwarded into main with owner authorization |

For the latest feedback patch run `runtime/preview/c6964da9fb3abef03e7c46f65e23cfce49a931ae5ec70ae084ce95fb479226ce/Start Katran Updated.cmd`. It deliberately uses the preceding 821c… preview's existing data profile, preserving the owner's pins and objects. Close the previous 2.5 window first; otherwise same-profile activation will show the already running old executable. No application was launched by the agent and no data was modified during packaging.

Feedback patch evidence: four direct-transfer API cases with a Clipboard source that panics on any read, one same-ID ordinary/pinned preview parity case, shortcut contracts, headless geometry and keyboard round-trip checks, and all-target clippy. The UI suite passed 79 cases in one run and its corrected key-release fixture passed its focused rerun. Native QuickLook latency itself remains unmeasured; the foreground operation indicator now has the same bottom-bar presentation for both slices.

Each is in `runtime/preview/<SHA-256>/`. Run `Start Katran Preview.cmd`; it passes a separate `data` directory to that immutable executable. The bundled ES helper and licenses are alongside it. Nothing launches automatically.

Both builds used Windows x64, Rust/Cargo 1.95.0, release profile and `ui,clipboard-events`, with the existing `target/build` cache. The per-artifact build-manifest.json records exact source/asset hashes, Cargo.lock and toolchain. The final build took 15.06 seconds; no comparative speedup is claimed.

Verification: full serial regression followed by focused reruns of all eleven intentionally changed legacy targets, targeted Folder/pin/transfer/import/recovery/timestamp checks, headless UI and all-target clippy with `-D warnings`. Native recycle passed on a disposable fixture outside the sandbox; the sandbox denial restored the fixture. Do not sum overlapping runs. Source logs are local under target/katran-2.5-*.log.

Manual acceptance remains open: native Windows feel, real browser bookmark import and selected browser invocation. Raycast service-emoji filtering awaits reliable source evidence. Folder cut and external clipboard cut are unavailable; ordinary file copying is supported. See CURRENT.md for precise scope and limits.

For a regression, choose an earlier checkpoint or preview explicitly. Do not use Git reset or replace the installed binary automatically; schema 8 rollback requires the recorded pre-migration backup and stopped writers.
