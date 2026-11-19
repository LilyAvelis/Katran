$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$base = [IO.Path]::GetFullPath((Join-Path $repo ('target/ai contract тест ' + [guid]::NewGuid().ToString('N'))))
$expectedParent = [IO.Path]::GetFullPath((Join-Path $repo 'target'))
if ((Split-Path -Parent $base) -ine $expectedParent -or (Split-Path -Leaf $base) -notmatch '^ai contract тест [a-f0-9]{32}$') { throw 'Invalid synthetic test path' }
New-Item -ItemType Directory -Force -Path $base | Out-Null
try {
    $profile = Join-Path $base 'профиль space'
    $jobs = Join-Path $profile 'ai-installations'
    New-Item -ItemType Directory -Force -Path $jobs | Out-Null
    function New-Request([string]$Id, [string]$Action, [string]$Component = 'folder', [switch]$Extended) {
        $dir = Join-Path $jobs $Id
        New-Item -ItemType Directory -Force -Path $dir | Out-Null
        $request = @{ id=$Id; component=$Component; action=$Action; profile=$profile; bundle_root=$repo; log_path=(Join-Path $dir 'install.log'); result_path=(Join-Path $dir 'result.json'); image=@{python=$null;paddle_python=$null} }
        if ($Extended) {
            foreach ($key in 'profile','bundle_root','log_path','result_path') { $request[$key] = '\\?\' + $request[$key] }
        }
        [IO.File]::WriteAllText((Join-Path $dir 'request.json'), ($request | ConvertTo-Json), [Text.UTF8Encoding]::new($false))
        return $dir
    }
    $script = Join-Path $repo 'scripts/ai-components/terminal.ps1'
    $dir = New-Request 'verify-unavailable' 'verify' -Extended
    & powershell -NoProfile -ExecutionPolicy Bypass -File ('\\?\' + $script) -Request ('\\?\' + (Join-Path $dir 'request.json')) | Out-Null
    $result = Get-Content -LiteralPath (Join-Path $dir 'result.json') -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($result.state -ne 'failed' -or $result.verified -ne $false -or $result.code -ne 'component_unavailable' -or $result.id -ne 'verify-unavailable') { throw 'Missing component did not report an explicit failure' }
    if (-not (Test-Path -LiteralPath (Join-Path $dir 'install.log'))) { throw 'Missing log' }
    foreach ($component in 'translate','upscale') {
        $dir = New-Request "$component-unavailable" 'verify' $component -Extended
        & powershell -NoProfile -ExecutionPolicy Bypass -File $script -Request ('\\?\' + (Join-Path $dir 'request.json')) | Out-Null
        $result = Get-Content -LiteralPath (Join-Path $dir 'result.json') -Raw -Encoding UTF8 | ConvertFrom-Json
        if ($result.component -ne $component -or $result.code -ne 'component_unavailable' -or $result.verified) { throw "Missing $component runtime was accepted" }
    }
    $partialGpu = Join-Path $profile 'folder-ai-gpu-runtime'
    New-Item -ItemType Directory -Path $partialGpu | Out-Null
    [IO.File]::WriteAllText((Join-Path $partialGpu 'canary.txt'), 'keep')
    $dir = New-Request 'gpu-verify-partial' 'verify' 'folder_gpu'
    & powershell -NoProfile -ExecutionPolicy Bypass -File $script -Request (Join-Path $dir 'request.json') | Out-Null
    $result = Get-Content -LiteralPath (Join-Path $dir 'result.json') -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($result.code -ne 'component_unavailable' -or $result.verified) { throw 'Verify accepted a partial Folder CUDA installation' }
    if ((Test-Path -LiteralPath (Join-Path $jobs 'python-bootstrap.lock')) -or
        @(Get-ChildItem -LiteralPath $partialGpu -Force).Count -ne 1 -or
        [IO.File]::ReadAllText((Join-Path $partialGpu 'canary.txt')) -ne 'keep') {
        throw 'Verify unexpectedly started setup or touched the partial CUDA runtime'
    }
    $existing = Join-Path $profile 'folder-ai-runtime'
    New-Item -ItemType Directory -Path $existing | Out-Null
    [IO.File]::WriteAllText((Join-Path $existing 'canary.txt'), 'keep')
    $dir = New-Request 'install-existing' 'install'
    & powershell -NoProfile -ExecutionPolicy Bypass -File $script -Request (Join-Path $dir 'request.json') | Out-Null
    $result = Get-Content -LiteralPath (Join-Path $dir 'result.json') -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($result.state -ne 'failed' -or $result.code -ne 'component_unavailable' -or [IO.File]::ReadAllText((Join-Path $existing 'canary.txt')) -ne 'keep') { throw 'Existing incomplete runtime was modified or accepted' }
    $lock = [IO.File]::Open((Join-Path $jobs 'folder.lock'), [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
    try {
        $dir = New-Request 'lock-busy' 'verify'
        & powershell -NoProfile -ExecutionPolicy Bypass -File $script -Request (Join-Path $dir 'request.json') | Out-Null
        $result = Get-Content -LiteralPath (Join-Path $dir 'result.json') -Raw -Encoding UTF8 | ConvertFrom-Json
        if ($result.code -ne 'component_busy' -or $result.verified -ne $false) { throw 'Concurrent operation was not rejected' }
    } finally { $lock.Dispose() }
    $dir = New-Request 'bad-action' 'verify'
    $invalid = Get-Content -LiteralPath (Join-Path $dir 'request.json') -Raw | ConvertFrom-Json
    $invalid.action = 'run-arbitrary-code'
    [IO.File]::WriteAllText((Join-Path $dir 'request.json'), ($invalid | ConvertTo-Json), [Text.UTF8Encoding]::new($false))
    & powershell -NoProfile -ExecutionPolicy Bypass -File $script -Request (Join-Path $dir 'request.json') | Out-Null
    if (Test-Path -LiteralPath (Join-Path $dir 'result.json')) { throw 'Invalid request was accepted' }
    Write-Output 'AI terminal contract: synthetic paths, missing dependency, existing runtime, lock and invalid request passed.'
} finally {
    if ([IO.Path]::GetFullPath((Split-Path -Parent $base)) -ieq $expectedParent -and (Split-Path -Leaf $base) -match '^ai contract тест [a-f0-9]{32}$') { Remove-Item -LiteralPath $base -Recurse -Force }
}
