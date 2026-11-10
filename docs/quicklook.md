# QuickLook external preview · current 1.2.5 contract

`PreviewExternal {id}` and the Actions command «Открыть через QuickLook»
(Ctrl Enter, rebindable) use the injectable Launcher boundary. Space/Enter route
supported images to Katran and other regular files to external QuickLook. Source
existence and recorded content revision are checked first. The selected file is
never executed; no file association is invoked.

## Transport

Katran delegates to the documented Windows desktop CLI:
`QuickLook.exe <one separately encoded absolute path>`. It discovers the executable
in LOCALAPPDATA/Programs/QuickLook, Program Files/QuickLook, Program Files (x86),
or the standard Scoop installation. Missing executables return quicklook_not_found.
Store-only installations are not supported by this CLI discovery path. Windows
canonical verbatim drive/UNC paths are normalized for the receiving application.
`requested:true` confirms process creation, not completed rendering or foreground
focus. QuickLook controls its own plugins, preview window and single-instance IPC.

Katran no longer opens, probes, configures or writes QuickLook named pipes.
The previous direct pipe transport is retired: QuickLook4.5's uncaught reader task
can fail on an empty client connection. A Katran connection failure before a full
message was a plausible trigger, not a proven cause of every observed failure.
The old synthetic pipe-byte tests did not establish real interoperability.

Official [integration guide](https://github.com/QL-Win/QuickLook/wiki/Develop,-build-and-integrate)
and [4.5 source](https://github.com/QL-Win/QuickLook/blob/4.5.0/QuickLook/PipeServerManager.cs).

## Local recovery and verification

On2026-11-10 the owner authorized QuickLook recovery. A clean4.5 configuration
restored TXT preview, but subsequent PDF/CLI checks produced a fresh ViewerWindow
NullReferenceException. Official4.4 ZIP was checked against GitHub's SHA256 and
verified on a syntheticPDF. The ordinary installation was backed up as a sibling
QuickLook.backup-4.5.0-* and replaced with stock4.4 files (without portable.lock).
Original and clean4.5 configurations were backed up under APPDATA/pooi.moe.
The duplicate Store package was not removed. This is a local recovery observation,
not proof that every4.5 installation fails.

Four requests via Katran's actual stdio API alternated syntheticTXT/PDF using the
new CLI transport. One QuickLook process and its channel remained alive; no new
exception log appeared. The owner checks actual on-screen content and plugins.
Unit tests use fake launchers; UI tests disable real external launches. Recovery
checks were separately explicitly authorized and used synthetic files only.
