# Windows distribution

Katran 2.0 release artifacts are produced from the three optimized Rust binaries,
never by archiving a developer checkout or an existing runtime/data directory.
QuickLook is an optional separately installed application and is not bundled.

Build with `cargo build --locked --release --features ui,clipboard-events --bin katran-ui --bin katran-launcher --bin katran --target-dir target/build`, then:

```powershell
python scripts/third-party-notices.py --output C:\path\to\THIRD-PARTY-NOTICES.txt
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/package-release.ps1 -OutputDirectory C:\path\to\new-output -BuildInstaller -DependencyNoticesPath C:\path\to\THIRD-PARTY-NOTICES.txt
```

The output directory must not already exist. Inno Setup 6's `ISCC.exe` is required
only for the installer (`-IsccPath` overrides its normal Program Files path).
The script compiles setup but never launches setup or Katran.
Dependency notices are generated offline from Cargo's cached locked package
metadata and verbatim license/notice files. Missing texts stop packaging for
review. Portable and installed distributions both carry these notices and MIT
license files. The collector also includes development/build dependencies rather
than risk omitting a transitive attribution. Generated notices contain package
names, versions and notice text, never local Cargo-cache paths.

The portable ZIP contains `Katran.exe`, `katran-cli.exe`, `portable.flag`, license
notices, and `runtime`: a compatibility launcher, a 12-hex current-version pointer,
and one immutable version containing `katran-ui.exe`, `build-manifest.json` and
`THIRD-PARTY-NOTICES.txt`.
Run the root launcher. Portable data defaults to `katran-data` beside it.
Do not extract a new portable release over an existing installation: use a new
directory and explicitly transfer data while Katran is closed, or use Settings updates.

Setup installs per user into `%LOCALAPPDATA%\Programs\Katran`, creates a Start Menu
shortcut and `installed.flag`. Data lives separately in `%LOCALAPPDATA%\Katran\data`
and is preserved by installation/uninstallation. Close Katran before running setup.
Setup never runs the app, changes autostart, deletes old versions wholesale, or
imports a developer's data. Windows visual installation checks belong to the owner.

`Katran-VERSION-update.zip` contains exactly `katran-ui.exe`, its build manifest,
and `THIRD-PARTY-NOTICES.txt`. The manifest binds notices with
`third_party_notices_sha256`; license text remains readable beside each immutable
executable. Notice text is limited to 8 MiB; JSON manifests remain limited to 64 KiB.
The manual updater installs an immutable version and changes the launch pointer;
it never overwrites the running executable or stable launcher. `release.json` has
`schema_version: 1`, `data_schema_version: 6`, `minimum_launcher_protocol: 1`,
`version`, `tag`, `repository`, and `update: {url, size_bytes, sha256}`. Its URL is
the version-tagged GitHub Releases asset in `LilyAvelis/Katran`. Launcher protocol
changes require full setup/portable replacement. Data schema metadata is informational;
the new executable owns migrations. No data downgrade is supported.

`SHA256SUMS.txt` covers final ZIP, EXE and JSON release assets. These checksums
detect corruption; GitHub repository ownership and TLS remain the trust anchor.
No signing key/certificate is configured, so binaries/installers are unsigned.
Windows may display an unknown-publisher warning. Never claim signed provenance.

CI pins Rust 1.95.0 for reproducible checks and release builds. It runs fmt,
API/headless tests, Clippy and synthetic whitelist/determinism tests
on Windows. After checks pass, release builds produce three binaries and notices;
a separate job packages them and compiles setup, then a publish job uploads the
finished assets. Default workflow permissions are read-only; only the publish job
can write releases. Official checkout, cache and artifact actions are pinned to
commits. Inno Setup is
provided by the [official Windows runner image](https://github.com/actions/runner-images/blob/main/images/windows/Windows2022-Readme.md).
Synthetic checks: `powershell -NoProfile -ExecutionPolicy Bypass -File tests/release-package-contract.ps1`.

## CLI and launcher compatibility

Manual in-app updates replace the versioned UI/domain executable and its notices.
The stable launcher and root `katran-cli.exe` are retained. Install/extract a new
full package to update the CLI or launcher as well; future launcher protocol
changes require a full package. An old CLI explicitly rejects unsupported newer
data schemas instead of silently modifying them. The initial 2.0.0 package ships
UI and CLI from the same source revision.

## Retrying release infrastructure

If an existing stable tag has not produced a published release, the workflow on
main can be dispatched with `release_tag` set to that existing tag. Checks resolve
the tag to a commit and the build and packaging jobs check out that same commit;
the tag itself is never moved. Publishing remains gated by checks and exact
Cargo/tag version matching. Asset publication
refuses an existing release rather than replacing its files. Release preparation
prefetches all locked dependencies because offline license metadata includes
development dependencies that a release-only binary build may not download.
The build checks that the installer compiler exists before compiling and records
the commit, toolchain, target, features, profile, lockfile hash and four input
file hashes in a receipt. Packaging verifies that receipt against its checkout and
the downloaded files; publication verifies the final asset whitelist and checksums
and confirms that the tag still resolves to the checked commit before creating a release.

The workflow stores immutable build and packaged artifacts for 30 days, named with
the checked commit and run attempt. Within that retention period, GitHub's rerun
failed jobs can retry packaging using the successful build artifact, or publication
using the successful packaged artifact, without compiling again. A new dispatch
is a new run and performs its checks and build again. A partially created release
still requires manual investigation: publication never replaces existing assets.

Cargo registry/git and `target/build` caches are keyed by Windows image,
architecture, Rust version, features, profile, lockfile and commit, with a
same-lockfile fallback for later commits. Pull requests may restore but never save
caches. No working tree, runtime/data directory, or local credentials are cached.
Warm-cache speed depends on runner and cache transfer costs; no benchmark is claimed.
Only obsolete main/PR checks share cancellation groups; tag and dispatched release
runs have separate groups and cannot cancel one another.
