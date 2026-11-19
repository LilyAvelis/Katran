[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$Request)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
# The launcher can inherit a newer shell's module path. This terminal and its
# helpers use the built-in Windows PowerShell modules, including Get-FileHash.
$env:PSModulePath = [IO.Path]::Combine($PSHOME, 'Modules')
$req = $null; $job = $null; $lock = $null; $transcript = $false
$state = 'failed'; $verified = $false; $code = 'installation_failed'; $message = 'Operation failed; see the terminal log.'
function Get-InstallerPath([string]$Path) {
    # Rust canonicalizes paths to Win32 extended form. The Windows PowerShell
    # 5.1 filesystem provider misinterprets that prefix as a drive name.
    if ($Path -match '^\\\\\?\\UNC\\') { $Path = '\\' + $Path.Substring(8) }
    elseif ($Path -match '^\\\\\?\\[a-z]:\\') { $Path = $Path.Substring(4) }
    if ($Path -notmatch '^(?:[a-z]:[\\/]|\\\\[^?.\\/][^\\/]*\\[^\\/]+)') {
        throw 'The installer requires an absolute drive or network share path.'
    }
    return [IO.Path]::GetFullPath($Path)
}
function Get-SetupPython([string]$Profile, [string]$Job) {
    $pythonLock = [IO.Path]::Combine($Profile, 'ai-installations', 'python-bootstrap.lock')
    $bootstrap = $null
    try {
        $bootstrap = [IO.File]::Open($pythonLock, [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
        & (Join-Path $scriptRoot 'ensure-python.ps1') -Profile $Profile -JobDir $Job
    } finally { if ($bootstrap) { $bootstrap.Dispose() } }
}
function Invoke-LoggedPython([string]$Executable, [string[]]$Arguments) {
    # A direct console child is visible but its native output is absent from
    # the PowerShell 5.1 transcript. Relay both streams through the host.
    $ErrorActionPreference = 'Continue'
    & $Executable @Arguments 2>&1 | ForEach-Object { Write-Host ([string]$_) }
    return $LASTEXITCODE
}
try {
    $scriptRoot = Get-InstallerPath $PSScriptRoot
    $requestPath = Get-InstallerPath $Request
    $req = Get-Content -LiteralPath $requestPath -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($req.id -notmatch '^[a-zA-Z0-9_-]{1,64}$' -or $req.component -notin @('emoji','folder','folder_gpu','translate','upscale') -or $req.action -notin @('install','verify')) { throw 'Invalid installation request.' }
    $profile = Get-InstallerPath $req.profile
    $bundle = Get-InstallerPath $req.bundle_root
    $candidateJob = [IO.Path]::Combine($profile, 'ai-installations', $req.id)
    $logPath = Get-InstallerPath $req.log_path
    if ($requestPath -ine [IO.Path]::Combine($candidateJob, 'request.json') -or (Get-InstallerPath $req.result_path) -ine [IO.Path]::Combine($candidateJob, 'result.json') -or $logPath -ine [IO.Path]::Combine($candidateJob, 'install.log')) { throw 'Installation paths do not match the request.' }
    $job = $candidateJob
    if (-not (Test-Path -LiteralPath $profile -PathType Container) -or -not (Test-Path -LiteralPath $bundle -PathType Container)) { throw 'Profile or application files are missing.' }
    $componentRoot = switch ($req.component) { 'emoji' { Join-Path $profile 'emoji-ai-runtime' } 'folder' { Join-Path $profile 'folder-ai-runtime' } 'folder_gpu' { Join-Path $profile 'folder-ai-gpu-runtime' } }
    Start-Transcript -LiteralPath $logPath -Append | Out-Null; $transcript = $true
    Write-Host "Katran AI: $($req.component) / $($req.action)"
    Write-Host 'This window shows the operation. You can share the local log with another assistant after reviewing it for personal paths.'
    $lockFile = Join-Path (Join-Path $profile 'ai-installations') ($req.component + '.lock')
    try { $lock = [IO.File]::Open($lockFile, [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None) }
    catch { $code = 'component_busy'; throw 'This component is already being installed or checked.' }
    if ($req.component -in @('translate','upscale')) {
        if ($req.action -eq 'install') {
            $pythonExe = Get-SetupPython $profile $job
        } elseif ($req.component -eq 'translate') {
            $pythonExe = $req.image.paddle_python
        } else {
            $pythonExe = $req.image.python
        }
        if (-not $pythonExe -or -not [IO.File]::Exists((Get-InstallerPath $pythonExe))) {
            $code = 'component_unavailable'
            throw 'Image runtime is missing. Use Install from Settings > AI.'
        }
        $code = if ($req.action -eq 'install') { 'installation_failed' } else { 'verification_failed' }
        $exitCode = Invoke-LoggedPython (Get-InstallerPath $pythonExe) @(
            '-I', (Join-Path $scriptRoot 'image-setup.py'), $requestPath, $profile, $job, $bundle
        )
        if ($exitCode -ne 0) { throw 'Image component setup or verification failed. See the terminal log for the failing step.' }
        $state = 'succeeded'; $verified = $true; $code = 'ok'; $message = 'Component passed its local functional check.'
        return
    }
    if ($req.action -eq 'install' -and -not (Test-Path -LiteralPath $componentRoot)) {
        $python = Get-SetupPython $profile $job
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
            & $installer @arguments | Out-Host
        } elseif ($req.component -eq 'folder') {
            $installer = Join-Path $bundle 'scripts/folder-semantic/setup.ps1'
            $arguments = @{ Profile=$profile; Python=$python }
            if ($reuse) { $arguments.ModelFile=$model; $arguments.TokenizerFile=$tokenizer }
            & $installer @arguments | Out-Host
        } else {
            & (Join-Path $bundle 'scripts/folder-semantic/setup-gpu.ps1') -Profile $profile -Python $python | Out-Host
        }
    } elseif ($req.action -eq 'install') { Write-Host 'Existing component detected. It will not be overwritten; checking it instead.' }
    $legacyFolder = $req.component -eq 'folder' -and -not (Test-Path -LiteralPath $componentRoot)
    if ($req.component -in @('folder','folder_gpu') -and -not $legacyFolder) {
        $manifestPath = Join-Path $componentRoot 'manifest.json'
        if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) { $code = 'component_unavailable'; throw 'Component manifest is missing; existing files were not changed.' }
        $manifest = Get-Content -LiteralPath $manifestPath -Raw -Encoding UTF8 | ConvertFrom-Json
        $backend = if ($req.component -eq 'folder') { 'onnxruntime-cpu' } else { 'onnxruntime-cuda' }
        if ($manifest.version -ne 1 -or $manifest.backend -cne $backend -or $manifest.onnxruntime -cne '1.23.2') { $code = 'component_unavailable'; throw 'Component manifest is incompatible; existing files were not changed.' }
    }
    $pythonExe = if ($req.component -eq 'emoji' -or $legacyFolder) {
        Join-Path $profile 'emoji-ai-runtime/local-experiments/emoji-ai/.venv/Scripts/python.exe'
    } else { Join-Path $componentRoot '.venv/Scripts/python.exe' }
    if (-not (Test-Path -LiteralPath $pythonExe -PathType Leaf)) { $code = 'component_unavailable'; throw 'Component is incomplete or missing. Existing files were not changed.' }
    $code = 'verification_failed'
    $exitCode = Invoke-LoggedPython $pythonExe @(
        (Join-Path $scriptRoot 'verify.py'), $req.component, '--profile', $profile, '--job', $job, '--bundle', $bundle
    )
    if ($exitCode -eq 2 -and $req.component -eq 'folder_gpu' -and $req.action -eq 'install') {
        $state = 'succeeded'; $verified = $false; $code = 'model_missing'; $message = 'GPU component installed; install Folder or Emoji AI to verify it.'
    } elseif ($exitCode -ne 0) { throw 'The functional verification failed. Inspect the log; existing files were not replaced.' }
    else { $state = 'succeeded'; $verified = $true; $code = 'ok'; $message = 'Component passed its local functional check.' }
} catch {
    Write-Host ("ERROR: " + $_.Exception.Message) -ForegroundColor Red
    $message = $_.Exception.Message
} finally {
    if ($lock) { $lock.Dispose() }
    if ($transcript) { try { Stop-Transcript | Out-Null } catch {} }
    if ($job) {
        $result = [ordered]@{id=[string]$req.id;component=[string]$req.component;action=[string]$req.action;state=$state;verified=$verified;code=$code;message=$message;log_path=$logPath;completed_at=[DateTimeOffset]::UtcNow.ToString('o')}
        $tempResult = Join-Path $job ('result-' + [guid]::NewGuid().ToString('N') + '.tmp')
        [IO.File]::WriteAllText($tempResult, (ConvertTo-Json -InputObject $result -Depth 5), [Text.UTF8Encoding]::new($false))
        [IO.File]::Move($tempResult, (Join-Path $job 'result.json'))
        Write-Host "Result: $state ($code). Log: $logPath"
        $quotedLog = $logPath.Replace("'", "''")
        Write-Host ("Read the log: Get-Content -Raw -LiteralPath '" + $quotedLog + "'")
        Write-Host 'Review the log before sharing it. This terminal can now be closed.'
    }
}
