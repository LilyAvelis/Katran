param(
    [Parameter(Mandatory = $true)][string]$Profile,
    [string]$Python = 'python'
)
$ErrorActionPreference = 'Stop'
$profilePath = (Resolve-Path -LiteralPath $Profile).Path
if (-not (Test-Path -LiteralPath $profilePath -PathType Container)) { throw 'Profile must be an existing directory.' }
$runtime = Join-Path $profilePath 'folder-ai-gpu-runtime'
$markerName = '.katran-gpu-installing'
$markerText = 'Katran Folder GPU installation v1'
if (-not (Test-Path -LiteralPath (Join-Path $runtime 'manifest.json'))) {
    $fallback = Join-Path $profilePath 'folder-ai-gpu-runtime-owned'
    $primaryEntry = if (Test-Path -LiteralPath $runtime) { Get-Item -LiteralPath $runtime -Force } else { $null }
    if ($primaryEntry -and (($primaryEntry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or -not $primaryEntry.PSIsContainer)) { throw 'Invalid runtime directory.' }
    if ((Test-Path -LiteralPath (Join-Path $fallback 'manifest.json')) -or ($primaryEntry -and -not (Test-Path -LiteralPath (Join-Path $runtime $markerName)) -and @(Get-ChildItem -LiteralPath $runtime -Force).Count -gt 0)) { $runtime = $fallback }
}
if (Test-Path -LiteralPath $runtime) {
    $entry = Get-Item -LiteralPath $runtime -Force
    if (($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or -not $entry.PSIsContainer) { throw 'Invalid runtime directory.' }
    if (Test-Path -LiteralPath (Join-Path $runtime 'manifest.json')) { throw 'A completed Folder GPU runtime already exists; it was not replaced.' }
} else { New-Item -ItemType Directory -Path $runtime | Out-Null }
$marker = Join-Path $runtime $markerName
if (Test-Path -LiteralPath $marker) {
    $entry = Get-Item -LiteralPath $marker -Force
    if ($entry.PSIsContainer -or ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or [IO.File]::ReadAllText($marker) -cne $markerText) { throw 'Invalid Folder GPU installation marker; existing files were not changed.' }
} else {
    # Old installers wrote no ownership marker. Never run pip in an unknown
    # environment, even if its directory happens to look like our old venv.
    if (@(Get-ChildItem -LiteralPath $runtime -Force).Count -ne 0) { throw 'An unmarked incomplete Folder GPU runtime exists; its files were not changed.' }
    $stream = [IO.File]::Open($marker, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try { $bytes = [Text.UTF8Encoding]::new($false).GetBytes($markerText); $stream.Write($bytes, 0, $bytes.Length) } finally { $stream.Dispose() }
}
$venv = Join-Path $runtime '.venv'
$scripts = Join-Path $venv 'Scripts'
$pythonExe = Join-Path $scripts 'python.exe'
foreach ($path in @($venv, $scripts, $pythonExe)) {
    if (Test-Path -LiteralPath $path) {
        $entry = Get-Item -LiteralPath $path -Force
        if (($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or
            $entry.PSIsContainer -ne ($path -ne $pythonExe)) {
            throw 'Invalid Folder GPU environment; existing files were not changed.'
        }
    }
}
$repairVenv = -not (Test-Path -LiteralPath $pythonExe -PathType Leaf)
if (-not $repairVenv) {
    try { & $pythonExe -m pip --version 2>$null | Out-Null; $repairVenv = $LASTEXITCODE -ne 0 }
    catch { $repairVenv = $true }
}
if ($repairVenv) {
    # venv without --clear can restore incomplete bootstrap files and keep installed packages.
    & $Python -m venv $venv
    if ($LASTEXITCODE -ne 0) { throw 'Could not prepare Folder GPU environment.' }
}
& $pythonExe -m pip --isolated install --no-input --disable-pip-version-check 'pip==25.3'
if ($LASTEXITCODE -ne 0) { throw 'Could not prepare the Python package installer.' }
# CUDA 12.9 Windows wheels and cuDNN 9; no global CUDA or driver changes.
$packages = @(
    'onnxruntime-gpu==1.23.2', 'numpy==2.2.6', 'tokenizers==0.23.3',
    'nvidia-cuda-nvrtc-cu12==12.9.86', 'nvidia-cuda-runtime-cu12==12.9.79',
    'nvidia-cufft-cu12==11.4.1.4', 'nvidia-curand-cu12==10.3.10.19',
    'nvidia-cudnn-cu12==9.28.1.4', 'nvidia-cublas-cu12==12.9.1.4'
)
& $pythonExe -m pip install --no-input --disable-pip-version-check @packages
if ($LASTEXITCODE -ne 0) { throw 'Folder GPU package installation failed; no completed runtime was registered.' }
& $pythonExe -m pip check
if ($LASTEXITCODE -ne 0) { throw 'Folder GPU dependencies failed validation.' }
# Registration is deliberately last; Python startup/provider verification is
# performed by the worker for each session, not inferred from this marker.
$manifest = '{"version":1,"backend":"onnxruntime-cuda","onnxruntime":"1.23.2"}'
$manifestTemp = Join-Path $runtime ('.manifest-' + [guid]::NewGuid().ToString('N') + '.tmp')
try {
    [IO.File]::WriteAllText($manifestTemp, $manifest, [Text.UTF8Encoding]::new($false))
    [IO.File]::Move($manifestTemp, (Join-Path $runtime 'manifest.json'))
} finally { if (Test-Path -LiteralPath $manifestTemp -PathType Leaf) { Remove-Item -LiteralPath $manifestTemp } }
Remove-Item -LiteralPath $marker
Write-Output 'Folder GPU component installed. CUDA is checked when a GPU search starts.'
