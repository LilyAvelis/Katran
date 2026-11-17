[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$Request)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$req = $null; $job = $null; $lock = $null; $transcript = $false
$state = 'failed'; $verified = $false; $code = 'installation_failed'; $message = 'Operation failed; see the terminal log.'
try {
    $requestPath = [IO.Path]::GetFullPath($Request)
    if (-not [IO.Path]::IsPathRooted($Request)) { throw 'Request path must be absolute.' }
    $req = Get-Content -LiteralPath $requestPath -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($req.id -notmatch '^[a-zA-Z0-9_-]{1,64}$' -or $req.component -notin @('emoji','folder','folder_gpu') -or $req.action -notin @('install','verify')) { throw 'Invalid installation request.' }
    foreach ($p in @($req.profile,$req.bundle_root,$req.log_path,$req.result_path)) { if (-not [IO.Path]::IsPathRooted([string]$p)) { throw 'All installation paths must be absolute.' } }
    $profile = [IO.Path]::GetFullPath([string]$req.profile)
    $bundle = [IO.Path]::GetFullPath([string]$req.bundle_root)
    $job = [IO.Path]::GetFullPath((Join-Path (Join-Path $profile 'ai-installations') $req.id))
    if ($requestPath -ine (Join-Path $job 'request.json') -or [IO.Path]::GetFullPath([string]$req.result_path) -ine (Join-Path $job 'result.json') -or [IO.Path]::GetFullPath([string]$req.log_path) -ine (Join-Path $job 'install.log')) { throw 'Installation paths do not match the request.' }
    if (-not (Test-Path -LiteralPath $profile -PathType Container) -or -not (Test-Path -LiteralPath $bundle -PathType Container)) { throw 'Profile or application files are missing.' }
    $componentRoot = switch ($req.component) { 'emoji' { Join-Path $profile 'emoji-ai-runtime' } 'folder' { Join-Path $profile 'folder-ai-runtime' } 'folder_gpu' { Join-Path $profile 'folder-ai-gpu-runtime' } }
    Start-Transcript -LiteralPath $req.log_path -Append | Out-Null; $transcript = $true
    Write-Host "Katran AI: $($req.component) / $($req.action)"
    Write-Host 'This window shows the operation. You can share the local log with another assistant after reviewing it for personal paths.'
    $lockFile = Join-Path (Join-Path $profile 'ai-installations') ($req.component + '.lock')
    try { $lock = [IO.File]::Open($lockFile, [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None) }
    catch { $code = 'component_busy'; throw 'This component is already being installed or checked.' }
    if ($req.action -eq 'install' -and -not (Test-Path -LiteralPath $componentRoot)) {
        $python = & (Join-Path $PSScriptRoot 'ensure-python.ps1') -Profile $profile -JobDir $job
        if (-not $python -or -not (Test-Path -LiteralPath $python -PathType Leaf)) { throw 'Python 3.10 x64 could not be prepared.' }
        $legacy = Join-Path $profile 'emoji-ai-runtime/local-experiments/emoji-ai/model/onnx'
        $folderModel = Join-Path $profile 'folder-ai-runtime/model'
        $candidate = if ($req.component -eq 'emoji') { $folderModel } else { $legacy }
        $model = Join-Path $candidate 'model.onnx'; $tokenizer = Join-Path $candidate 'tokenizer.json'
        $reuse = (Test-Path -LiteralPath $model -PathType Leaf) -and (Test-Path -LiteralPath $tokenizer -PathType Leaf)
        if ($reuse) {
            $reuse = ((Get-FileHash -LiteralPath $model -Algorithm SHA256).Hash.ToLowerInvariant() -ceq 'ca456c06b3a9505ddfd9131408916dd79290368331e7d76bb621f1cba6bc8665') -and ((Get-FileHash -LiteralPath $tokenizer -Algorithm SHA256).Hash.ToLowerInvariant() -ceq '0b44a9d7b51c3c62626640cda0e2c2f70fdacdc25bbbd68038369d14ebdf4c39')
        }
        if ($req.component -eq 'emoji') {
            $installer = Join-Path $bundle 'scripts/install-emoji-ai-model.ps1'
            $arguments = @{ Profile=$profile; Python=$python }
            if ($reuse) { $arguments.ModelFile=$model; $arguments.TokenizerFile=$tokenizer }
            & $installer @arguments
        } elseif ($req.component -eq 'folder') {
            $installer = Join-Path $bundle 'scripts/folder-semantic/setup.ps1'
            $arguments = @{ Profile=$profile; Python=$python }
            if ($reuse) { $arguments.ModelFile=$model; $arguments.TokenizerFile=$tokenizer }
            & $installer @arguments
        } else {
            & (Join-Path $bundle 'scripts/folder-semantic/setup-gpu.ps1') -Profile $profile -Python $python
        }
    } elseif ($req.action -eq 'install') { Write-Host 'Existing component detected. It will not be overwritten; checking it instead.' }
    if ($req.component -in @('folder','folder_gpu')) {
        $manifestPath = Join-Path $componentRoot 'manifest.json'
        if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) { $code = 'component_unavailable'; throw 'Component manifest is missing; existing files were not changed.' }
        $manifest = Get-Content -LiteralPath $manifestPath -Raw -Encoding UTF8 | ConvertFrom-Json
        $backend = if ($req.component -eq 'folder') { 'onnxruntime-cpu' } else { 'onnxruntime-cuda' }
        if ($manifest.version -ne 1 -or $manifest.backend -cne $backend -or $manifest.onnxruntime -cne '1.23.2') { $code = 'component_unavailable'; throw 'Component manifest is incompatible; existing files were not changed.' }
    }
    $pythonExe = if ($req.component -eq 'emoji') { Join-Path $componentRoot 'local-experiments/emoji-ai/.venv/Scripts/python.exe' } else { Join-Path $componentRoot '.venv/Scripts/python.exe' }
    if (-not (Test-Path -LiteralPath $pythonExe -PathType Leaf)) { $code = 'component_unavailable'; throw 'Component is incomplete or missing. Existing files were not changed.' }
    $code = 'verification_failed'
    & $pythonExe (Join-Path $PSScriptRoot 'verify.py') $req.component --profile $profile --job $job --bundle $bundle
    if ($LASTEXITCODE -eq 2 -and $req.component -eq 'folder_gpu' -and $req.action -eq 'install') {
        $state = 'succeeded'; $verified = $false; $code = 'model_missing'; $message = 'GPU component installed; install Folder or Emoji AI to verify it.'
    } elseif ($LASTEXITCODE -ne 0) { throw 'The functional verification failed. Inspect the log; existing files were not replaced.' }
    else { $state = 'succeeded'; $verified = $true; $code = 'ok'; $message = 'Component passed its local functional check.' }
} catch {
    Write-Host ("ERROR: " + $_.Exception.Message) -ForegroundColor Red
    $message = $_.Exception.Message
} finally {
    if ($lock) { $lock.Dispose() }
    if ($transcript) { try { Stop-Transcript | Out-Null } catch {} }
    if ($req -and $job -and $req.id -match '^[a-zA-Z0-9_-]{1,64}$' -and [IO.Path]::GetFullPath([string]$req.result_path) -ieq (Join-Path $job 'result.json')) {
        $result = [ordered]@{id=[string]$req.id;component=[string]$req.component;action=[string]$req.action;state=$state;verified=$verified;code=$code;message=$message;log_path=[string]$req.log_path;completed_at=[DateTimeOffset]::UtcNow.ToString('o')}
        $tempResult = Join-Path $job ('result-' + [guid]::NewGuid().ToString('N') + '.tmp')
        [IO.File]::WriteAllText($tempResult, (ConvertTo-Json -InputObject $result -Depth 5), [Text.UTF8Encoding]::new($false))
        [IO.File]::Move($tempResult, (Join-Path $job 'result.json'))
        Write-Host "Result: $state ($code). Log: $($req.log_path)"
        $quotedLog = ([string]$req.log_path).Replace("'", "''")
        Write-Host ("Read the log: Get-Content -Raw -LiteralPath '" + $quotedLog + "'")
        Write-Host 'Review the log before sharing it. This terminal can now be closed.'
    }
}
