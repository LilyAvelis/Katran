# 2.5 preview build

This is an unaccepted preview, separate from the installed runtime. Freeze inputs and serialize Cargo jobs. From the project root:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/prepare-preview.ps1 snapshot
cargo build --offline --release --features ui,clipboard-events --bin katran-ui --target-dir target/build
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/prepare-preview.ps1 package
```

Only package after successful build. Snapshot checks source hashes and toolchain before and after; timestamp alone cannot prove which command produced the binary. Record the actual build command and result. The resulting path is printed; run its `katran-ui.exe` manually with an explicit disposable data-directory argument. Do not use personal files for testing. Everything search requires the separately installed and running Everything application. Preview packaging does not register or change the installed launcher.
