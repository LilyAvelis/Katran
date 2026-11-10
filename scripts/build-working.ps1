$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'install-build.ps1')
$projectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
Push-Location -LiteralPath $projectRoot
try {
    Invoke-KatranInstall -Root $projectRoot -Build {
        cargo build --offline --release --features ui,clipboard-events --bin katran-ui --bin katran-launcher --target-dir target/build
        if ($LASTEXITCODE -ne 0) { throw 'Katran build failed' }
    } -Register {
        param($launcher)
        # The release launcher uses the GUI subsystem: explicitly wait instead
        # of reading a possibly stale LASTEXITCODE from the previous cargo call.
        $registration = Start-Process -FilePath $launcher -ArgumentList '--register' -Wait -PassThru -WindowStyle Hidden
        if ($registration.ExitCode -ne 0) { throw "Katran Start Menu registration failed (exit $($registration.ExitCode))" }
    }
} finally { Pop-Location }
