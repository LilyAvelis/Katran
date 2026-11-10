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
on Windows. Tag builds additionally compile setup and publish assets only after
checks pass. Default workflow permissions are read-only; only the tag release job
can write releases. Checkout is pinned to an official action commit. Inno Setup is
provided by the [official Windows runner image](https://github.com/actions/runner-images/blob/main/images/windows/Windows2022-Readme.md).
Synthetic checks: `powershell -NoProfile -ExecutionPolicy Bypass -File tests/release-package-contract.ps1`.

## CLI and launcher compatibility

Manual in-app updates replace the versioned UI/domain executable and its notices.
The stable launcher and root `katran-cli.exe` are retained. Install/extract a new
full package to update the CLI or launcher as well; future launcher protocol
changes require a full package. An old CLI explicitly rejects unsupported newer
data schemas instead of silently modifying them. The initial 2.0.0 package ships
UI and CLI from the same source revision.
