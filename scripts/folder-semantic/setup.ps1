[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$Profile, [Parameter(Mandatory=$true)][string]$Python, [string]$ModelFile, [string]$TokenizerFile)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$profilePath = (Resolve-Path -LiteralPath $Profile).Path
$destination = Join-Path $profilePath 'folder-ai-runtime'
if (Test-Path -LiteralPath $destination) { throw 'Folder AI runtime already exists and will not be replaced.' }
if (-not (Test-Path -LiteralPath $Python -PathType Leaf)) { throw 'Python executable is unavailable.' }
$info = & $Python -I -c 'import sys;print(sys.version_info[:2], sys.maxsize.bit_length() + 1)'
if ($LASTEXITCODE -ne 0 -or $info -cne '(3, 10) 64') { throw '64-bit Python 3.10 is required.' }
$stage = Join-Path $profilePath ('.folder-ai-stage-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path (Join-Path $stage 'model') -Force | Out-Null
try {
    $base = 'https://huggingface.co/intfloat/multilingual-e5-small/resolve/614241f622f53c4eeff9890bdc4f31cfecc418b3'
    $files = @(
        @{ Local=$ModelFile; Url="$base/onnx/model.onnx?download=true"; Name='model.onnx'; Hash='ca456c06b3a9505ddfd9131408916dd79290368331e7d76bb621f1cba6bc8665' },
        @{ Local=$TokenizerFile; Url="$base/tokenizer.json?download=true"; Name='tokenizer.json'; Hash='0b44a9d7b51c3c62626640cda0e2c2f70fdacdc25bbbd68038369d14ebdf4c39' }
    )
    if (!$ModelFile -or !$TokenizerFile) {
        . (Join-Path $PSScriptRoot '../ai-components/model-download.ps1')
    }
    foreach ($inputFile in $files) {
        $target = Join-Path $stage ('model/' + $inputFile.Name)
        if ($inputFile.Local) { Copy-Item -LiteralPath $inputFile.Local -Destination $target }
        else {
            $asset = Get-KatranModelAsset -Profile $profilePath -Name $inputFile.Name -Source $inputFile.Url -Sha256 $inputFile.Hash
            Copy-Item -LiteralPath $asset -Destination $target
        }
        if ((Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash.ToLowerInvariant() -cne $inputFile.Hash) { throw "Checksum mismatch: $($inputFile.Name)" }
    }
    & $Python -m venv (Join-Path $stage '.venv')
    if ($LASTEXITCODE -ne 0) { throw 'Folder Python environment creation failed.' }
    $pythonExe = Join-Path $stage '.venv/Scripts/python.exe'
    & $pythonExe -m pip --isolated install --no-input --disable-pip-version-check 'pip==25.3'
    if ($LASTEXITCODE -ne 0) { throw 'Could not prepare the Python package installer.' }
    & $pythonExe -m pip install --no-input --disable-pip-version-check 'numpy==2.2.6' 'onnxruntime==1.23.2' 'tokenizers==0.23.3'
    if ($LASTEXITCODE -ne 0) { throw 'Folder AI dependency installation failed.' }
    & $pythonExe -m pip check
    if ($LASTEXITCODE -ne 0) { throw 'Folder AI dependency check failed.' }
    [IO.File]::WriteAllText((Join-Path $stage 'manifest.json'), '{"version":1,"backend":"onnxruntime-cpu","onnxruntime":"1.23.2"}', [Text.UTF8Encoding]::new($false))
    [IO.Directory]::Move($stage, $destination)
    Write-Host 'Folder AI runtime installed; a functional check follows.'
} catch {
    Write-Warning "Installation did not complete. Staged files remain at: $stage"
    throw
}
