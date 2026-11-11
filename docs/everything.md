# Everything folder search

`src/everything.rs` searches the existing Everything index through the official
`es.exe` helper adjacent to the running Katran executable. It does not depend on
Afio at runtime, build an index, or fall back to scanning the filesystem. The
backend was adapted from Afiocira's `crates/photo-gui/src/everything.rs`; its
MIT copyright and permission notice is retained in `AFIO-LICENSE`.

Everything itself must be installed and running with IPC support; the helper is
separate. Packaging has not yet been implemented. Afio's existing packaging
uses voidtools ES 1.1.0.38 x64, with its own license and provenance files; a
Katran distribution must include these notices alongside the helper. See
`../Afiocira/scripts/setup-everything.ps1` for the pinned archive hashes.

The backend accepts a native Everything expression and a limit of 1–100; an
empty expression returns no results without starting a process. It wraps the
expression with a folder filter and also checks the directory attribute on each
result. A malformed expression that changes the filter cannot return file items,
but may consume result slots. Query text is a single argument after `--`, never
shell code. UTF-8 output, isolated ES settings, 32 MiB output and a three-second
deadline are enforced. Missing helper, unavailable Everything, timeout, invalid
response and other process failures are distinct errors, not empty results.

Items are paths from an index snapshot, not proofs of existence or access. The
caller must validate the chosen folder when opening it. Calls block while the
helper runs, so a caller must use a bounded background worker and reject stale
results. There is no total count or pagination; refine a broad query. Parser and
injectable-runner tests are synthetic; actual IPC, packaging and UI remain to be
verified on Windows.
