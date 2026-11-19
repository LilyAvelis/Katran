function Get-KatranModelCacheItem {
    param([string]$Path)
    $item = Get-Item -LiteralPath $Path -Force -ErrorAction SilentlyContinue
    if ($item -and ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw "Unexpected link in model cache: $Path"
    }
    return $item
}

function Test-KatranModelHash {
    param([string]$Path, [string]$Sha256)
    return ((Test-Path -LiteralPath $Path -PathType Leaf) -and
        (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash -eq $Sha256)
}

function Invoke-KatranModelCurl {
    param([string]$Source, [string]$Part, [bool]$Resume)
    $curlArgs = @('--disable', '--fail', '--location', '--proto', '=https',
        '--proto-redir', '=https', '--connect-timeout', '30', '--max-time', '600',
        '--max-filesize', '536870912', '--progress-bar', '--show-error',
        '--output', $Part, '--write-out', '%{http_code}')
    if ($Resume) { $curlArgs += @('--continue-at', '-') }
    $httpCode = & "$env:SystemRoot/System32/curl.exe" @curlArgs -- $Source
    return @{ ExitCode = $LASTEXITCODE; HttpCode = $httpCode }
}

function Get-KatranModelAsset {
    param(
        [Parameter(Mandatory)][string]$Profile,
        [Parameter(Mandatory)][ValidateSet('model.onnx','tokenizer.json')][string]$Name,
        [Parameter(Mandatory)][ValidatePattern('^https://')][string]$Source,
        [Parameter(Mandatory)][ValidatePattern('^[0-9a-f]{64}$')][string]$Sha256
    )
    $cache = [IO.Path]::GetFullPath($Profile)
    foreach ($segment in @('ai-model-cache', $Sha256.ToLowerInvariant())) {
        $cache = Join-Path $cache $segment
        $null = Get-KatranModelCacheItem $cache
        $null = [IO.Directory]::CreateDirectory($cache)
    }
    $ready = Join-Path $cache $Name
    $part = "$ready.part"
    $lockPath = "$ready.lock"
    $null = Get-KatranModelCacheItem $lockPath
    try {
        $lock = [IO.File]::Open($lockPath, 'OpenOrCreate', 'ReadWrite', 'None')
    } catch [IO.IOException] {
        throw "Cannot lock ${Name}. If another Katran installer is using it, wait for that run to finish before retrying."
    }
    try {
        if (Get-KatranModelCacheItem $ready) {
            if (!(Test-KatranModelHash $ready $Sha256)) { throw "Checksum mismatch in cached file: $ready" }
            Write-Host "Reusing verified $Name from the download cache."
            return $ready
        }

        $roots = @(
            @{ Path = (Join-Path $Profile 'folder-ai-runtime'); Model = 'model' }
            @{ Path = (Join-Path $Profile 'emoji-ai-runtime'); Model = 'local-experiments/emoji-ai/model/onnx' }
        )
        foreach ($stage in (Get-ChildItem -LiteralPath $Profile -Directory -Force |
                Where-Object Name -Match '^\.(folder|emoji)-ai-stage-[0-9a-f]{32}$' |
                Sort-Object LastWriteTime -Descending | Select-Object -First 32)) {
            $layout = if ($stage.Name.StartsWith('.folder-')) { 'model' } else { 'local-experiments/emoji-ai/model/onnx' }
            $roots += @{ Path = $stage.FullName; Model = $layout }
        }
        foreach ($root in $roots) {
            $candidate = Join-Path (Join-Path $root.Path $root.Model) $Name
            if (Test-KatranModelHash $candidate $Sha256) {
                Write-Host "Reusing verified $Name from an existing Katran installation or setup."
                return $candidate
            }
        }

        $partial = Get-KatranModelCacheItem $part
        if ($partial -and ($partial.PSIsContainer -or $partial.Length -gt 512MB)) {
            throw "Unexpected data in partial download: $part"
        }
        if ($partial -and (Test-KatranModelHash $part $Sha256)) {
            [IO.File]::Move($part, $ready)
            Write-Host "Reusing verified $Name from the previous download."
            return $ready
        }

        $resume = $partial -and $partial.Length -gt 0
        for ($attempt = 1; $attempt -le 3; $attempt++) {
            if ($resume) {
                $mib = (Get-Item -LiteralPath $part).Length / 1MB
                Write-Host ("Resuming {0} from {1:N1} MiB (attempt {2}/3) ..." -f $Name, $mib, $attempt)
            } else {
                Write-Host "Downloading $Name (attempt $attempt/3) ..."
            }
            $result = Invoke-KatranModelCurl $Source $part $resume
            $partial = Get-KatranModelCacheItem $part
            if ($partial -and $partial.Length -gt 512MB) { throw "Download exceeds the size limit: $Name" }
            if ($result.ExitCode -eq 0) {
                if (Test-KatranModelHash $part $Sha256) {
                    [IO.File]::Move($part, $ready)
                    Write-Host "Verified $Name (SHA-256)."
                    return $ready
                }
                [IO.File]::Delete($part)
                if (!$resume) { throw "Checksum mismatch after downloading $Name." }
                Write-Host "$Name did not pass verification after resuming. Retrying from the beginning."
            } elseif ($resume -and ($result.ExitCode -in @(33, 36) -or $result.HttpCode -eq '416')) {
                [IO.File]::Delete($part)
                Write-Host "The server did not accept resuming $Name. Retrying from the beginning."
            } else {
                Write-Host "Download interrupted (curl $($result.ExitCode)). Saved bytes will be reused."
            }
            $partial = Get-KatranModelCacheItem $part
            $resume = $partial -and $partial.Length -gt 0
        }
        throw "Could not verify $Name after three attempts. Run Install again to retry."
    } finally {
        $lock.Dispose()
    }
}
