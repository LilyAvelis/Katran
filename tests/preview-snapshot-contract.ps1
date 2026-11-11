$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$root = Join-Path ([IO.Path]::GetTempPath()) ('katran-preview-test-' + [guid]::NewGuid().ToString('N'))
try {
    foreach ($dir in @('scripts','src','assets','target')) { New-Item -ItemType Directory -Path (Join-Path $root $dir) -Force | Out-Null }
    Copy-Item -LiteralPath (Join-Path $repo 'scripts/prepare-preview.ps1') -Destination (Join-Path $root 'scripts/prepare-preview.ps1')
    foreach ($file in @('Cargo.toml','Cargo.lock','build.rs','src/lib.rs','assets/fixture')) { Set-Content -LiteralPath (Join-Path $root $file) -Value "synthetic-$file" }
    foreach ($crate in @('garden-actions','garden-egui')) {
        foreach ($dir in @("crates/$crate/src/nested", "crates/$crate/src/target", "crates/$crate/target")) { New-Item -ItemType Directory -Path (Join-Path $root $dir) -Force | Out-Null }
        foreach ($file in @('Cargo.toml','LICENSE','src/lib.rs','src/nested/module.rs','src/target/private.rs','target/privatefile','Cargo.lock')) { Set-Content -LiteralPath (Join-Path $root "crates/$crate/$file") -Value "synthetic-$file" }
    }
    # Override metadata commands; this exercises only snapshotting synthetic files.
    $wrapper = Join-Path $root 'snapshot-wrapper.ps1'
    @'
function git { 'synthetic-commit' }
function rustc { 'synthetic-rustc' }
function cargo { 'synthetic-cargo' }
& (Join-Path $PSScriptRoot 'scripts/prepare-preview.ps1') snapshot
'@ | Set-Content -LiteralPath $wrapper
    & powershell -NoProfile -ExecutionPolicy Bypass -File $wrapper
    if ($LASTEXITCODE -ne 0) { throw 'Snapshot failed' }
    $snapshot = Get-Content -LiteralPath (Join-Path $root 'target/preview-inputs.json') -Raw | ConvertFrom-Json
    $paths = @($snapshot.inputs | ForEach-Object { $_.path.Replace('\','/') })
    foreach ($crate in @('garden-actions','garden-egui')) {
        foreach ($file in @('Cargo.toml','LICENSE','src/lib.rs','src/nested/module.rs')) {
            $path = "crates/$crate/$file"
            $entry = @($snapshot.inputs | Where-Object { $_.path.Replace('\','/') -eq $path })
            if ($entry.Count -ne 1 -or $entry[0].sha256 -ine (Get-FileHash -LiteralPath (Join-Path $root $path) -Algorithm SHA256).Hash) { throw "Missing or incorrect source hash: $path" }
        }
        foreach ($file in @('src/target/private.rs','target/privatefile','Cargo.lock')) { if ("crates/$crate/$file" -in $paths) { throw "Unexpected crate input: $file" } }
    }
    Write-Output 'Synthetic preview snapshot contract passed.'
} finally {
    if ((Split-Path -Parent $root) -eq [IO.Path]::GetTempPath().TrimEnd('\') -and (Split-Path -Leaf $root) -match '^katran-preview-test-[0-9a-f]{32}$') { Remove-Item -LiteralPath $root -Recurse -Force }
}
