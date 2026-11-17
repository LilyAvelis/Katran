param(
    [Parameter(Mandatory = $true)][string]$Profile,
    [string]$Python = 'python'
)
$ErrorActionPreference = 'Stop'
$profilePath = (Resolve-Path -LiteralPath $Profile).Path
if (-not (Test-Path -LiteralPath $profilePath -PathType Container)) { throw 'Profile must be an existing directory.' }
$runtime = Join-Path $profilePath 'folder-ai-gpu-runtime'
if (Test-Path -LiteralPath $runtime) {
    $entry = Get-Item -LiteralPath $runtime -Force
    if (($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or -not $entry.PSIsContainer) { throw 'Invalid runtime directory.' }
    if (Test-Path -LiteralPath (Join-Path $runtime 'manifest.json')) { throw 'A completed Folder GPU runtime already exists; it was not replaced.' }
} else { New-Item -ItemType Directory -Path $runtime | Out-Null }
$venv = Join-Path $runtime '.venv'
$pythonExe = Join-Path $venv 'Scripts/python.exe'
if (-not (Test-Path -LiteralPath $pythonExe -PathType Leaf)) {
    & $Python -m venv $venv
    if ($LASTEXITCODE -ne 0) { throw 'Could not create Folder GPU environment.' }
}
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
[IO.File]::WriteAllText((Join-Path $runtime 'manifest.json'), $manifest, [Text.UTF8Encoding]::new($false))
Write-Output 'Folder GPU component installed. CUDA is checked when a GPU search starts.'
