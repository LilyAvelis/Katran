$ErrorActionPreference = 'Stop'
$projectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
Push-Location -LiteralPath $projectRoot
try {
    # Do not invoke build-working: it changes the stable launcher/Start Menu entry.
    cargo build --offline --release --features ui,clipboard-events-test --bin katran-ui --target-dir target/build
    if ($LASTEXITCODE -ne 0) { throw 'Clipboard events experimental build failed' }
    $built = Join-Path $projectRoot 'target/build/release/katran-ui.exe'
    $hash = (Get-FileHash -LiteralPath $built -Algorithm SHA256).Hash.ToLowerInvariant()
    $id = $hash.Substring(0,12)
    $experiment = Join-Path $projectRoot 'runtime/experiments/clipboard-events'
    $directory = Join-Path $experiment $id
    New-Item -ItemType Directory -Path $directory -Force | Out-Null
    $installed = Join-Path $directory 'katran-ui.exe'
    if (-not (Test-Path -LiteralPath $installed)) { Copy-Item -LiteralPath $built -Destination $installed }
    if ((Get-FileHash -LiteralPath $installed -Algorithm SHA256).Hash.ToLowerInvariant() -ne $hash) { throw 'Experimental binary hash mismatch' }
    $files = @((Get-ChildItem -LiteralPath (Join-Path $projectRoot 'src') -File -Recurse).FullName) + @((Get-ChildItem -LiteralPath (Join-Path $projectRoot 'assets/branding') -File -Recurse).FullName) + @('Cargo.toml','Cargo.lock','build.rs','scripts/build-events-test.ps1' | ForEach-Object { Join-Path $projectRoot $_ })
    $sources = @($files | ForEach-Object { [ordered]@{ path=$_.Substring($projectRoot.Length + 1); sha256=(Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant() } })
    [ordered]@{ channel='clipboard-events-experiment'; features='ui,clipboard-events-test'; version='1.0.6-events-test'; executable_sha256=$hash; sources=$sources } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $directory 'build-manifest.json') -Encoding utf8
    Set-Content -LiteralPath (Join-Path $experiment 'current-version.txt') -Value $id -Encoding ascii
    Write-Output "Prepared EVENTS TEST: $id"
    Write-Output 'Stable Katran launcher and Start Menu entry were not changed.'
} finally { Pop-Location }
