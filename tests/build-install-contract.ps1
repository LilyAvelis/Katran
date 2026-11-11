$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '../scripts/install-build.ps1')
$root = Join-Path ([IO.Path]::GetTempPath()) ('katran-contract-' + [guid]::NewGuid().ToString('N'))
try {
    foreach ($dir in @('src','assets/branding','scripts','target/build/release','runtime')) { New-Item -ItemType Directory -Path (Join-Path $root $dir) -Force | Out-Null }
    foreach ($crate in @('avilon-actions','avilon-egui')) {
        foreach ($dir in @("crates/$crate/src/nested", "crates/$crate/src/target", "crates/$crate/target")) { New-Item -ItemType Directory -Path (Join-Path $root $dir) -Force | Out-Null }
        foreach ($file in @('Cargo.toml','LICENSE','src/lib.rs','src/nested/module.rs','src/target/private.rs','target/privatefile','Cargo.lock')) { Set-Content (Join-Path $root "crates/$crate/$file") "synthetic-$file" }
    }
    Set-Content (Join-Path $root 'Cargo.toml') "[package]`nname = `"synthetic`"`nversion = `"9.8.7`""
    foreach ($file in @('Cargo.lock','build.rs','src/a.rs','assets/branding/a','scripts/build-working.ps1','scripts/install-build.ps1','target/build/release/katran-ui.exe','target/build/release/katran-launcher.exe')) { Set-Content (Join-Path $root $file) $file }
    $pointer = Join-Path $root 'runtime/current-version.txt'
    Set-Content $pointer 'old'
    function Assert-Old { if ((Get-Content $pointer -Raw).Trim() -ne 'old') { throw 'Pointer changed on failure' } }
    try { Invoke-KatranInstall $root { throw 'synthetic build failure' } {}; throw 'expected failure' } catch { if ($_.Exception.Message -eq 'expected failure') { throw } }; Assert-Old
    try { Invoke-KatranInstall $root { Add-Content (Join-Path $root 'src/a.rs') 'change' } {}; throw 'expected failure' } catch { if ($_.Exception.Message -eq 'expected failure') { throw } }; Assert-Old
    try { Invoke-KatranInstall $root { Add-Content (Join-Path $root 'crates/avilon-actions/src/lib.rs') 'change' } {}; throw 'expected failure' } catch { if ($_.Exception.Message -eq 'expected failure') { throw } }; Assert-Old
    try { Invoke-KatranInstall $root {} { throw 'synthetic registration failure' }; throw 'expected failure' } catch { if ($_.Exception.Message -eq 'expected failure') { throw } }; Assert-Old
    Invoke-KatranInstall $root {} {} | Out-Null
    $id = (Get-Content $pointer -Raw).Trim()
    $manifest = Get-Content (Join-Path $root "runtime/versions/$id/build-manifest.json") -Raw | ConvertFrom-Json
    if ($manifest.version -ne '9.8.7') { throw 'Incorrect package version' }
    if ($manifest.update_format -ne 1 -or $manifest.minimum_launcher_protocol -ne 1) { throw 'Missing updater protocol' }
    if ($manifest.executable_sha256 -ne (Get-FileHash (Join-Path $root "runtime/versions/$id/katran-ui.exe") -Algorithm SHA256).Hash.ToLowerInvariant()) { throw 'Incorrect executable hash' }
    $sourcePaths = @($manifest.sources | ForEach-Object { $_.path.Replace('\','/') })
    foreach ($crate in @('avilon-actions','avilon-egui')) {
        foreach ($file in @('Cargo.toml','LICENSE','src/lib.rs','src/nested/module.rs')) {
            $path = "crates/$crate/$file"
            $entry = @($manifest.sources | Where-Object { $_.path.Replace('\','/') -eq $path })
            if ($entry.Count -ne 1 -or $entry[0].sha256 -cne (Get-FileHash -LiteralPath (Join-Path $root $path) -Algorithm SHA256).Hash.ToLowerInvariant()) { throw "Missing or incorrect source hash: $path" }
        }
        foreach ($file in @('src/target/private.rs','target/privatefile','Cargo.lock')) { if ("crates/$crate/$file" -in $sourcePaths) { throw "Unexpected crate input: $file" } }
    }
    Invoke-KatranInstall $root {} {} | Out-Null
    Write-Output 'Synthetic installation contract passed.'
} finally {
    if ((Split-Path -Parent $root) -eq [IO.Path]::GetTempPath().TrimEnd('\') -and (Split-Path -Leaf $root) -match '^katran-contract-[0-9a-f]{32}$') { Remove-Item -LiteralPath $root -Recurse -Force }
}
