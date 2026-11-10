$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$sandbox = Join-Path ([IO.Path]::GetTempPath()) ('katran-release-test-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path (Join-Path $sandbox 'src'),(Join-Path $sandbox 'binaries'),(Join-Path $sandbox 'katran-data'),(Join-Path $sandbox 'runtime/old') | Out-Null
try {
    foreach ($file in @('katran-ui.exe','katran-launcher.exe','katran.exe')) { [IO.File]::WriteAllText((Join-Path $sandbox "binaries/$file"), "synthetic-$file") }
    foreach ($file in @('Cargo.toml','Cargo.lock','build.rs','AFIO-LICENSE','THIRD-PARTY-NOTICES.txt','src/lib.rs','.env.local','katran-data/private.txt','runtime/old/private.exe','binaries/private.pdb')) { [IO.File]::WriteAllText((Join-Path $sandbox $file), 'synthetic-private-canary') }
    $out = Join-Path $sandbox 'output'
    & (Join-Path $root 'scripts/package-release.ps1') -Root $sandbox -BinaryDirectory 'binaries' -OutputDirectory $out -Version '2.0.0' -DependencyNoticesPath (Join-Path $sandbox 'THIRD-PARTY-NOTICES.txt')
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead((Join-Path $out 'Katran-2.0.0-windows-x64-portable.zip'))
    try {
        $names = @($zip.Entries.FullName)
        $hash = (Get-FileHash -LiteralPath (Join-Path $sandbox 'binaries/katran-ui.exe') -Algorithm SHA256).Hash.ToLowerInvariant()
        $expected = @('AFIO-LICENSE','THIRD-PARTY-NOTICES.txt','Katran.exe','katran-cli.exe','portable.flag','runtime/Katran.exe','runtime/current-version.txt',"runtime/versions/$($hash.Substring(0,12))/katran-ui.exe","runtime/versions/$($hash.Substring(0,12))/build-manifest.json","runtime/versions/$($hash.Substring(0,12))/THIRD-PARTY-NOTICES.txt")
        if (@(Compare-Object ($names | Sort-Object) ($expected | Sort-Object)).Count) { throw 'Portable whitelist differs' }
    } finally { $zip.Dispose() }
    $release = Get-Content -LiteralPath (Join-Path $out 'release.json') -Raw | ConvertFrom-Json
    $update = Join-Path $out 'Katran-2.0.0-update.zip'
    if ($release.update.sha256 -cne (Get-FileHash -LiteralPath $update -Algorithm SHA256).Hash.ToLowerInvariant() -or $release.update.size_bytes -ne (Get-Item -LiteralPath $update).Length) { throw 'Update metadata does not match ZIP' }
    $zip = [IO.Compression.ZipFile]::OpenRead($update)
    try {
        if (@(Compare-Object @($zip.Entries.FullName | Sort-Object) @('build-manifest.json','katran-ui.exe','THIRD-PARTY-NOTICES.txt' | Sort-Object)).Count) { throw 'Update whitelist differs' }
        $reader = New-Object IO.StreamReader($zip.GetEntry('build-manifest.json').Open())
        try { $manifest = $reader.ReadToEnd() | ConvertFrom-Json } finally { $reader.Dispose() }
        if ($manifest.third_party_notices_sha256 -cne (Get-FileHash -LiteralPath (Join-Path $sandbox 'THIRD-PARTY-NOTICES.txt') -Algorithm SHA256).Hash.ToLowerInvariant()) { throw 'Dependency notice digest differs' }
    } finally { $zip.Dispose() }
    $second = Join-Path $sandbox 'second'
    & (Join-Path $root 'scripts/package-release.ps1') -Root $sandbox -BinaryDirectory 'binaries' -OutputDirectory $second -Version '2.0.0' -DependencyNoticesPath (Join-Path $sandbox 'THIRD-PARTY-NOTICES.txt')
    if ((Get-FileHash -LiteralPath $update).Hash -cne (Get-FileHash -LiteralPath (Join-Path $second 'Katran-2.0.0-update.zip')).Hash) { throw 'ZIP bytes are not deterministic' }
    if ((Get-FileHash -LiteralPath (Join-Path $out 'Katran-2.0.0-windows-x64-portable.zip')).Hash -cne (Get-FileHash -LiteralPath (Join-Path $second 'Katran-2.0.0-windows-x64-portable.zip')).Hash) { throw 'Portable ZIP bytes are not deterministic' }
    $refused = $false
    try { & (Join-Path $root 'scripts/package-release.ps1') -Root $sandbox -BinaryDirectory 'binaries' -OutputDirectory $out -Version '2.0.0' -DependencyNoticesPath (Join-Path $sandbox 'THIRD-PARTY-NOTICES.txt') } catch { $refused = $true }
    if (!$refused) { throw 'Packaging overwrote existing output' }
    Write-Output 'Release package contract passed (synthetic files only; no application launched).'
} finally {
    $resolved = [IO.Path]::GetFullPath($sandbox)
    if ((Split-Path -Parent $resolved) -eq [IO.Path]::GetTempPath().TrimEnd('\') -and (Split-Path -Leaf $resolved) -match '^katran-release-test-[a-f0-9]{32}$') { Remove-Item -LiteralPath $resolved -Recurse -Force }
}
