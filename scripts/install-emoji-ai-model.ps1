[CmdletBinding()]
param(
    [string]$Profile,
    [string]$ModelFile,
    [string]$TokenizerFile,
    [string]$Python
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if (!$Profile) {
    if (Test-Path -LiteralPath (Join-Path $root 'installed.flag') -PathType Leaf) {
        if (!$env:LOCALAPPDATA) { throw 'LOCALAPPDATA is unavailable; specify -Profile.' }
        $Profile = Join-Path $env:LOCALAPPDATA 'Katran/data'
    } elseif (Test-Path -LiteralPath (Join-Path $root 'portable.flag') -PathType Leaf) {
        $Profile = Join-Path $root 'katran-data'
    } else { throw 'Cannot identify the Katran profile; specify -Profile.' }
}
$Profile = [IO.Path]::GetFullPath($Profile)
$destination = Join-Path $Profile 'emoji-ai-runtime'
if (Test-Path -LiteralPath $destination) { throw "Runtime already exists: $destination. It will not be replaced." }
$required = @('scripts/emoji-ai/worker.py','scripts/emoji-ai/semantic_search.py','scripts/create-emoji-ai-runtime-manifest.py','assets/unicode/emoji-test.txt','assets/emoji-ai/cldr/LICENSE')
foreach ($lang in @('en','ru')) { foreach ($kind in @('full','derived-full')) { $required += "assets/emoji-ai/cldr/cldr-annotations-$kind-$lang.json" } }
foreach ($file in $required) { if (!(Test-Path -LiteralPath (Join-Path $root $file) -PathType Leaf)) { throw "Missing installation input: $file" } }
foreach ($file in @($ModelFile, $TokenizerFile)) { if ($file -and !(Test-Path -LiteralPath $file -PathType Leaf)) { throw "Local model input not found: $file" } }
$pythonArgs = @()
if ($Python) {
    if (!(Test-Path -LiteralPath $Python -PathType Leaf)) { throw "Python executable not found: $Python" }
    $pythonCommand = [IO.Path]::GetFullPath($Python)
} else {
    $launcher = Get-Command py.exe -ErrorAction SilentlyContinue
    if (!$launcher) { throw 'Python launcher not found. Install 64-bit Python 3.10 or specify -Python.' }
    $pythonCommand = $launcher.Source
    $pythonArgs = @('-3.10-64')
}
$pythonInfo = & $pythonCommand @pythonArgs -c 'import struct,sys; print("%s.%s %s" % (sys.version_info.major, sys.version_info.minor, struct.calcsize("P") * 8))'
if ($LASTEXITCODE -ne 0 -or $pythonInfo -cne '3.10 64') { throw '64-bit Python 3.10 is required.' }
New-Item -ItemType Directory -Path $Profile -Force | Out-Null
$stage = Join-Path $Profile ('.emoji-ai-stage-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $stage | Out-Null
try {
    $worker = Join-Path $stage 'local-experiments/emoji-ai'
    $modelDir = Join-Path $worker 'model/onnx'
    $cldr = Join-Path $worker 'cldr'
    $unicode = Join-Path $stage 'assets/unicode'
    New-Item -ItemType Directory -Path $worker,$modelDir,$cldr,$unicode -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $root 'scripts/emoji-ai/worker.py'),(Join-Path $root 'scripts/emoji-ai/semantic_search.py') -Destination $worker
    foreach ($file in (Get-ChildItem -LiteralPath (Join-Path $root 'assets/emoji-ai/cldr') -File | Where-Object { $_.Name -in @('LICENSE','cldr-annotations-full-en.json','cldr-annotations-derived-full-en.json','cldr-annotations-full-ru.json','cldr-annotations-derived-full-ru.json') })) { Copy-Item -LiteralPath $file.FullName -Destination $cldr }
    Copy-Item -LiteralPath (Join-Path $root 'assets/unicode/emoji-test.txt') -Destination $unicode
    $base = 'https://huggingface.co/intfloat/multilingual-e5-small/resolve/614241f622f53c4eeff9890bdc4f31cfecc418b3'
    $inputs = @(
        @{ Local=$ModelFile; Remote="$base/onnx/model.onnx"; Name='model.onnx'; Hash='ca456c06b3a9505ddfd9131408916dd79290368331e7d76bb621f1cba6bc8665' },
        @{ Local=$TokenizerFile; Remote="$base/tokenizer.json"; Name='tokenizer.json'; Hash='0b44a9d7b51c3c62626640cda0e2c2f70fdacdc25bbbd68038369d14ebdf4c39' }
    )
    foreach ($inputFile in $inputs) {
        $target = Join-Path $modelDir $inputFile.Name
        if ($inputFile.Local) { Copy-Item -LiteralPath $inputFile.Local -Destination $target }
        else { Write-Host "Downloading $($inputFile.Name)..."; Invoke-WebRequest -Uri $inputFile.Remote -OutFile $target -UseBasicParsing }
        if ((Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash.ToLowerInvariant() -cne $inputFile.Hash) { throw "Checksum mismatch: $($inputFile.Name)" }
    }
    & $pythonCommand @pythonArgs -m venv (Join-Path $worker '.venv')
    if ($LASTEXITCODE -ne 0) { throw 'Python environment creation failed.' }
    $python = Join-Path $worker '.venv/Scripts/python.exe'
    & $python -m pip install 'numpy==2.2.6' 'onnxruntime==1.23.2' 'tokenizers==0.23.3'
    if ($LASTEXITCODE -ne 0) { throw 'Python dependency installation failed.' }
    & $python (Join-Path $worker 'semantic_search.py') --model (Join-Path $modelDir 'model.onnx') --tokenizer (Join-Path $modelDir 'tokenizer.json') --cldr $cldr build
    if ($LASTEXITCODE -ne 0) { throw 'Emoji index creation failed.' }
    & $python (Join-Path $root 'scripts/create-emoji-ai-runtime-manifest.py') $stage
    if ($LASTEXITCODE -ne 0) { throw 'Runtime manifest creation failed.' }
    # Same-volume rename fails if another installation has already created the destination.
    [IO.Directory]::Move($stage, $destination)
    Write-Output "Emoji AI runtime installed: $destination"
} catch {
    Write-Warning "Installation did not complete. Staged files remain at: $stage"
    throw
}
