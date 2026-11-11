param(
    [string]$Root,
    [string]$BinaryDirectory = 'target/build/release',
    [Parameter(Mandatory)][string]$OutputDirectory,
    [string]$Version,
    [Parameter(Mandatory)][string]$DependencyNoticesPath,
    [string]$MediaRuntimeArchive,
    [string]$AppleBundleSha256,
    [switch]$BuildInstaller,
    [string]$IsccPath = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe"
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ([string]::IsNullOrWhiteSpace($Root)) { $Root = Join-Path $PSScriptRoot '..' }
$Root = [IO.Path]::GetFullPath($Root)
if (!$Version) {
    $cargo = Get-Content -LiteralPath (Join-Path $Root 'Cargo.toml') -Raw
    if ($cargo -notmatch '(?ms)^\[package\].*?^version\s*=\s*"([^"]+)"') { throw 'Missing package version' }
    $Version = $Matches[1]
}
if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw 'Release version must be numeric major.minor.patch' }
if (![IO.Path]::IsPathRooted($BinaryDirectory)) { $BinaryDirectory = Join-Path $Root $BinaryDirectory }
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $OutputDirectory) { throw 'OutputDirectory must not exist; packaging never replaces existing files' }
if (!(Test-Path -LiteralPath $DependencyNoticesPath -PathType Leaf) -or (Get-Item -LiteralPath $DependencyNoticesPath).Length -eq 0) { throw 'A nonempty dependency notices file is required' }
if ((Get-Item -LiteralPath $DependencyNoticesPath).Length -gt 8MB) { throw 'Dependency notices exceed the updater 8 MiB limit' }
foreach ($file in @('katran-ui.exe','katran-launcher.exe','katran.exe')) {
    if (!(Test-Path -LiteralPath (Join-Path $BinaryDirectory $file) -PathType Leaf)) { throw "Missing release binary: $file" }
}
foreach ($file in @('es.exe','LICENSE','NOTICE.txt')) {
    if (!(Test-Path -LiteralPath (Join-Path $Root "assets/everything/$file") -PathType Leaf)) { throw "Missing Everything asset: $file" }
}
New-Item -ItemType Directory -Path $OutputDirectory | Out-Null
$utf8 = New-Object Text.UTF8Encoding($false)
function Write-Json($Path, $Value) { [IO.File]::WriteAllText($Path, (ConvertTo-Json -InputObject $Value -Depth 8) + "`n", $utf8) }
function Hash($Path) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
function Write-Zip($Directory, $Destination) {
    Add-Type -AssemblyName System.IO.Compression
    $stream = [IO.File]::Open($Destination, [IO.FileMode]::CreateNew)
    $zip = New-Object IO.Compression.ZipArchive($stream, [IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($file in (Get-ChildItem -LiteralPath $Directory -Recurse -File | Sort-Object FullName)) {
            $name = $file.FullName.Substring($Directory.Length + 1).Replace('\','/')
            $entry = $zip.CreateEntry($name, [IO.Compression.CompressionLevel]::Optimal)
            $entry.LastWriteTime = [DateTimeOffset]::new(2020,1,1,0,0,0,[TimeSpan]::Zero)
            $inputStream = [IO.File]::OpenRead($file.FullName)
            $outputStream = $entry.Open()
            try { $inputStream.CopyTo($outputStream) } finally { $inputStream.Dispose(); $outputStream.Dispose() }
        }
    } finally { $zip.Dispose(); $stream.Dispose() }
}
$hash = Hash (Join-Path $BinaryDirectory 'katran-ui.exe')
$id = $hash.Substring(0,12)
$portable = Join-Path $OutputDirectory 'portable'
$update = Join-Path $OutputDirectory 'update'
$versionDir = Join-Path $portable "runtime/versions/$id"
New-Item -ItemType Directory -Path $versionDir,$update | Out-Null
if ($MediaRuntimeArchive) {
    & python (Join-Path $PSScriptRoot 'media-runtime.py') $MediaRuntimeArchive (Join-Path $versionDir 'media-runtime')
    if ($LASTEXITCODE -ne 0) { throw 'Media runtime validation failed' }
    Copy-Item -LiteralPath $MediaRuntimeArchive -Destination (Join-Path $OutputDirectory "Katran-$Version-media-runtime.zip")
    Write-Json (Join-Path $OutputDirectory 'media-runtime.json') ([ordered]@{schema_version=1;version='9.0.2';url="https://github.com/LilyAvelis/Katran/releases/download/v$Version/Katran-$Version-media-runtime.zip";sha256=(Hash $MediaRuntimeArchive);size_bytes=(Get-Item -LiteralPath $MediaRuntimeArchive).Length;source='https://github.com/FFmpeg-Builds/FFmpeg-Builds/releases/tag/autobuild-2026-11-11-12-39';license='LGPL shared build'})
}
# Explicit whitelist: never copy a working tree, runtime tree, user data or build directory.
Copy-Item -LiteralPath (Join-Path $BinaryDirectory 'katran-launcher.exe') -Destination (Join-Path $portable 'Katran.exe')
Copy-Item -LiteralPath (Join-Path $BinaryDirectory 'katran-launcher.exe') -Destination (Join-Path $portable 'runtime/Katran.exe')
Copy-Item -LiteralPath (Join-Path $BinaryDirectory 'katran.exe') -Destination (Join-Path $portable 'katran-cli.exe')
[IO.File]::WriteAllText((Join-Path $portable 'portable.flag'), '', $utf8)
$combinedNotices = (Get-Content -LiteralPath $DependencyNoticesPath -Raw) + "`n`nEverything ES:`n" + (Get-Content -LiteralPath (Join-Path $Root 'assets/everything/NOTICE.txt') -Raw) + "`n" + (Get-Content -LiteralPath (Join-Path $Root 'assets/everything/LICENSE') -Raw)
if ($AppleBundleSha256) {
    $combinedNotices += "`n`nApple emoji artwork: Apple Inc. Source conversion: https://github.com/samuelngs/apple-emoji-ttf/releases/tag/macos-26-20260722-484daf4e . Compact resource retains the 96px bitmap strike. The converter code license does not grant rights to Apple's artwork. Source SHA256: 18e48f1785564fbf511241e0963b265057bfe742036d8543406c6ce07e48ec0b . Resource SHA256: $AppleBundleSha256 .`n"
}
foreach($assetNotice in @('assets/emoji-flags/LICENSE-GRAPHICS','assets/emoji-flags/ATTRIBUTION.md','assets/unicode/LICENSE.txt','assets/unicode/SOURCE.md')) {
    $combinedNotices += "`n`n$assetNotice`n" + (Get-Content -LiteralPath (Join-Path $Root $assetNotice) -Raw)
}
[IO.File]::WriteAllText((Join-Path $portable 'THIRD-PARTY-NOTICES.txt'), $combinedNotices, $utf8)
if ((Get-Item -LiteralPath (Join-Path $portable 'THIRD-PARTY-NOTICES.txt')).Length -gt 8MB) { throw 'Combined notices exceed updater limit' }
[IO.File]::WriteAllText((Join-Path $portable 'runtime/current-version.txt'), $id + "`n", $utf8)
$sourceFiles = @('Cargo.toml','Cargo.lock','build.rs') + @(Get-ChildItem -LiteralPath (Join-Path $Root 'src') -Recurse -File -Filter '*.rs' | ForEach-Object { $_.FullName.Substring($Root.Length + 1) })
foreach ($crate in @('avilon-actions','avilon-egui')) {
    $crateRoot = Join-Path $Root "crates/$crate"
    $sourceFiles += @("crates/$crate/Cargo.toml", "crates/$crate/LICENSE")
    $sourceFiles += @(Get-ChildItem -LiteralPath (Join-Path $crateRoot 'src') -Recurse -File -Filter '*.rs' | Where-Object { $_.FullName.Substring($crateRoot.Length + 1) -notmatch '(^|[\\/])(target|build)[\\/]' } | ForEach-Object { $_.FullName.Substring($Root.Length + 1) })
}
$sourceFiles += @('assets/everything/es.exe','assets/everything/LICENSE','assets/everything/NOTICE.txt')
$sourceFiles += @('assets/branding/katran.ico','assets/branding/katran.rc' | Where-Object { Test-Path -LiteralPath (Join-Path $Root $_) -PathType Leaf })
$sources = @($sourceFiles | Sort-Object | ForEach-Object { [ordered]@{path=$_.Replace('\','/'); sha256=(Hash (Join-Path $Root $_))} })
$manifest = [ordered]@{profile='release';version=$Version;update_format=1;minimum_launcher_protocol=1;capture_backend='windows-events';shell_visual_protocol=1;executable_sha256=$hash;third_party_notices_sha256=(Hash (Join-Path $portable 'THIRD-PARTY-NOTICES.txt'));everything_es_sha256=(Hash (Join-Path $Root 'assets/everything/es.exe'));everything_license_sha256=(Hash (Join-Path $Root 'assets/everything/LICENSE'));everything_notice_sha256=(Hash (Join-Path $Root 'assets/everything/NOTICE.txt'));sources=$sources}
if ($AppleBundleSha256) {
    if ($AppleBundleSha256 -cne '8cb7eb78303459c10a90d70cd2ee85adae6c331985692570024487f3656c7474') { throw 'Unexpected Apple resource digest' }
    $manifest.bundled_apple_sha256 = $AppleBundleSha256
    $manifest.apple_source_sha256 = '18e48f1785564fbf511241e0963b265057bfe742036d8543406c6ce07e48ec0b'
}
Copy-Item -LiteralPath (Join-Path $BinaryDirectory 'katran-ui.exe') -Destination (Join-Path $versionDir 'katran-ui.exe')
Copy-Item -LiteralPath (Join-Path $portable 'THIRD-PARTY-NOTICES.txt') -Destination (Join-Path $versionDir 'THIRD-PARTY-NOTICES.txt')
Copy-Item -LiteralPath (Join-Path $Root 'assets/everything/es.exe') -Destination (Join-Path $versionDir 'es.exe')
Copy-Item -LiteralPath (Join-Path $Root 'assets/everything/LICENSE') -Destination (Join-Path $versionDir 'Everything-ES-LICENSE.txt')
Copy-Item -LiteralPath (Join-Path $Root 'assets/everything/NOTICE.txt') -Destination (Join-Path $versionDir 'Everything-ES-NOTICE.txt')
Write-Json (Join-Path $versionDir 'build-manifest.json') $manifest
if ((Get-Item -LiteralPath (Join-Path $versionDir 'build-manifest.json')).Length -gt 64KB) { throw 'Build manifest exceeds the updater 64 KiB limit' }
Copy-Item -LiteralPath (Join-Path $versionDir 'katran-ui.exe'),(Join-Path $versionDir 'build-manifest.json'),(Join-Path $versionDir 'THIRD-PARTY-NOTICES.txt') -Destination $update
foreach ($notice in @('AFIO-LICENSE','LICENSE')) {
    if (Test-Path -LiteralPath (Join-Path $Root $notice) -PathType Leaf) { Copy-Item -LiteralPath (Join-Path $Root $notice) -Destination $portable }
}
$portableZip = Join-Path $OutputDirectory "Katran-$Version-windows-x64-portable.zip"
$updateName = "Katran-$Version-update.zip"
$updateZip = Join-Path $OutputDirectory $updateName
Write-Zip $portable $portableZip
Write-Zip $update $updateZip
if ((Get-Item -LiteralPath $updateZip).Length -ge 100MB -or (Get-Item -LiteralPath (Join-Path $BinaryDirectory 'katran-ui.exe')).Length -ge 200MB) { throw 'Legacy update ZIP or UI executable exceeds updater limits' }
Write-Json (Join-Path $OutputDirectory 'release.json') ([ordered]@{schema_version=1;data_schema_version=6;minimum_launcher_protocol=1;version=$Version;tag="v$Version";repository='LilyAvelis/Katran';update=[ordered]@{url="https://github.com/LilyAvelis/Katran/releases/download/v$Version/$updateName";size_bytes=(Get-Item -LiteralPath $updateZip).Length;sha256=(Hash $updateZip)}})
if ($BuildInstaller) {
    if (!(Test-Path -LiteralPath $IsccPath -PathType Leaf)) { throw 'Inno Setup 6 compiler missing; pass -IsccPath' }
    & $IsccPath "/DAppVersion=$Version" "/DPackageRoot=$portable" "/DOutputRoot=$OutputDirectory" (Join-Path $Root 'installer/katran.iss')
    if ($LASTEXITCODE -ne 0) { throw "Inno Setup failed: $LASTEXITCODE" }
}
$checksums = @(Get-ChildItem -LiteralPath $OutputDirectory -File | Where-Object { $_.Extension -in @('.zip','.exe','.json') } | Sort-Object Name | ForEach-Object { "$(Hash $_.FullName)  $($_.Name)" })
[IO.File]::WriteAllText((Join-Path $OutputDirectory 'SHA256SUMS.txt'), ($checksums -join "`n") + "`n", $utf8)
Write-Output "Prepared Katran $Version release artifacts in $OutputDirectory"
