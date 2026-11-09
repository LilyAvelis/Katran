$ErrorActionPreference = 'Stop'
$projectRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
Push-Location -LiteralPath $projectRoot
try {
    cargo build --offline --release --features ui,clipboard-events --bin katran-ui --bin katran-launcher --target-dir target/build
    if ($LASTEXITCODE -ne 0) { throw 'Katran build failed' }
    $builtExecutable = Join-Path $projectRoot 'target/build/release/katran-ui.exe'
    $buildVersion = (Get-FileHash -LiteralPath $builtExecutable -Algorithm SHA256).Hash.Substring(0,12).ToLowerInvariant()
    $versionDirectory = Join-Path $projectRoot "runtime/versions/$buildVersion"
    New-Item -ItemType Directory -Path $versionDirectory -Force | Out-Null
    $installedExecutable = Join-Path $versionDirectory 'katran-ui.exe'
    if (-not (Test-Path -LiteralPath $installedExecutable)) {
        Copy-Item -LiteralPath $builtExecutable -Destination $installedExecutable
    }
    $sourceFiles = @((Get-ChildItem -LiteralPath (Join-Path $projectRoot 'src') -Recurse -File).FullName) + @((Get-ChildItem -LiteralPath (Join-Path $projectRoot 'assets/branding') -Recurse -File).FullName) + @('Cargo.toml','Cargo.lock','build.rs','scripts/build-working.ps1' | ForEach-Object { Join-Path $projectRoot $_ })
    $sourceHashes = @($sourceFiles | ForEach-Object { [ordered]@{ path = $_.Substring($projectRoot.Length + 1); sha256 = (Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant() } })
    [ordered]@{ profile = 'release'; version = '1.0.7'; capture_backend = 'windows-events'; executable_sha256 = (Get-FileHash -LiteralPath $installedExecutable -Algorithm SHA256).Hash.ToLowerInvariant(); sources = $sourceHashes } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $versionDirectory 'build-manifest.json') -Encoding utf8
    Set-Content -LiteralPath (Join-Path $projectRoot 'runtime/current-version.txt') -Value $buildVersion -Encoding ascii
    $launcher = Join-Path $projectRoot 'runtime/Katran.exe'
    Copy-Item -LiteralPath (Join-Path $projectRoot 'target/build/release/katran-launcher.exe') -Destination $launcher -Force
    # Headless registration only: writes a current-user Start Menu link, no UI launch.
    & $launcher --register
    if ($LASTEXITCODE -ne 0) { throw 'Katran Start Menu registration failed' }
    Write-Output "Prepared Katran 1.0.7 Release: $buildVersion"
} finally {
    Pop-Location
}
