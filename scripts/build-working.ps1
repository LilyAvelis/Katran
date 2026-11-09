$ErrorActionPreference = 'Stop'
$projectRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
Push-Location -LiteralPath $projectRoot
try {
    cargo build --offline --release --features ui --bin katran-ui --target-dir target/build
    if ($LASTEXITCODE -ne 0) { throw 'Katran build failed' }
    $builtExecutable = Join-Path $projectRoot 'target/build/release/katran-ui.exe'
    $buildVersion = (Get-FileHash -LiteralPath $builtExecutable -Algorithm SHA256).Hash.Substring(0,12).ToLowerInvariant()
    $versionDirectory = Join-Path $projectRoot "runtime/versions/$buildVersion"
    New-Item -ItemType Directory -Path $versionDirectory -Force | Out-Null
    $installedExecutable = Join-Path $versionDirectory 'katran-ui.exe'
    if (-not (Test-Path -LiteralPath $installedExecutable)) {
        Copy-Item -LiteralPath $builtExecutable -Destination $installedExecutable
    }
    $sourceFiles = @((Get-ChildItem -LiteralPath (Join-Path $projectRoot 'src') -Recurse -File).FullName) + @('Cargo.toml','Cargo.lock','scripts/build-working.ps1' | ForEach-Object { Join-Path $projectRoot $_ })
    $sourceHashes = @($sourceFiles | ForEach-Object { [ordered]@{ path = $_.Substring($projectRoot.Length + 1); sha256 = (Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant() } })
    [ordered]@{ profile = 'release'; version = '1.0.3'; executable_sha256 = (Get-FileHash -LiteralPath $installedExecutable -Algorithm SHA256).Hash.ToLowerInvariant(); sources = $sourceHashes } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $versionDirectory 'build-manifest.json') -Encoding utf8
    Set-Content -LiteralPath (Join-Path $projectRoot 'runtime/current-version.txt') -Value $buildVersion -Encoding ascii
    Write-Output "Prepared Katran 1.0.3 Release: $buildVersion"
} finally {
    Pop-Location
}
