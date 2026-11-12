param([string]$Label = '3.2')
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'install-build.ps1')
$projectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..')).TrimEnd([IO.Path]::DirectorySeparatorChar)
$binary = Join-Path $projectRoot 'target/build/release/katran-ui.exe'
$launcher = Join-Path $projectRoot 'target/build/release/katran-launcher.exe'
if (-not (Test-Path -LiteralPath $binary) -or -not (Test-Path -LiteralPath $launcher)) {
    throw 'Build both release binaries before writing the preview manifest'
}
$sourceFiles = @(Get-KatranSources $projectRoot)
$manifest = [ordered]@{
    label = "Katran $Label local preview"
    package_version = '3.1.1'
    commit = (& git rev-parse HEAD).Trim()
    branch = (& git branch --show-current).Trim()
    working_tree_modified = [bool]((& git status --porcelain) | Select-Object -First 1)
    os_arch = "$([Environment]::OSVersion.Platform)/$([Runtime.InteropServices.RuntimeInformation]::OSArchitecture)"
    rustc = (& rustc --version).Trim()
    cargo = (& cargo --version).Trim()
    features = @('ui', 'clipboard-events')
    lock_sha256 = (Get-FileHash -LiteralPath (Join-Path $projectRoot 'Cargo.lock') -Algorithm SHA256).Hash.ToLowerInvariant()
    ui_sha256 = (Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLowerInvariant()
    launcher_sha256 = (Get-FileHash -LiteralPath $launcher -Algorithm SHA256).Hash.ToLowerInvariant()
    sources = $sourceFiles
}
$output = Join-Path $projectRoot "target/build/release/preview-$Label-manifest.json"
$manifest | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $output -Encoding utf8
Write-Output $output
Write-Output $manifest.ui_sha256
