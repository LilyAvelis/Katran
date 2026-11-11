param([Parameter(Mandatory)][string]$PackageDirectory)
$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip=Get-ChildItem -LiteralPath $PackageDirectory -Filter '*-update.zip' | Select-Object -First 1
if(!$zip){throw 'Update ZIP missing'}
$archive=[IO.Compression.ZipFile]::OpenRead($zip.FullName)
try {
 $names=@($archive.Entries | ForEach-Object FullName)
 $expected=@('katran-ui.exe','build-manifest.json','THIRD-PARTY-NOTICES.txt')
 if($names.Count -ne 3 -or @(Compare-Object $names $expected).Count -ne 0){throw 'Legacy updater requires exactly three named files'}
} finally {$archive.Dispose()}
$version=Get-ChildItem -LiteralPath (Join-Path $PackageDirectory 'portable/runtime/versions') -Directory | Select-Object -First 1
foreach($name in @('es.exe','Everything-ES-LICENSE.txt','Everything-ES-NOTICE.txt','THIRD-PARTY-NOTICES.txt','build-manifest.json')){
 if(!(Test-Path -LiteralPath (Join-Path $version.FullName $name) -PathType Leaf)){throw "Missing $name"}
}
$manifest=Get-Content -LiteralPath (Join-Path $version.FullName 'build-manifest.json') -Raw | ConvertFrom-Json
if((Get-FileHash -LiteralPath (Join-Path $version.FullName 'es.exe')).Hash -ine $manifest.everything_es_sha256){throw 'ES hash mismatch'}
if((Get-FileHash -LiteralPath (Join-Path $version.FullName 'THIRD-PARTY-NOTICES.txt')).Hash -ine $manifest.third_party_notices_sha256){throw 'Notices hash mismatch'}
Write-Output 'Portable helper/notices and legacy three-file update contracts passed.'
