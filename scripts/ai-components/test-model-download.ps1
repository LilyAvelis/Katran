$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'model-download.ps1')

$fixture = Join-Path ([IO.Path]::GetTempPath()) ('katran-model-download-' + [guid]::NewGuid().ToString('N'))
$null = [IO.Directory]::CreateDirectory($fixture)
$profileDir = Join-Path $fixture 'profile'
$model = 'verified model fixture'
$tokenizer = 'verified tokenizer fixture'
$source = 'https://example.invalid/model'

function Invoke-KatranModelCurl {
    param([string]$Source, [string]$Part, [bool]$Resume)
    $script:resumes.Add($Resume)
    switch ($script:mode) {
        'interrupt' {
            if (!$Resume) { [IO.File]::WriteAllText($Part, 'verified ') }
            return @{ ExitCode = 18; HttpCode = '206' }
        }
        'finish' {
            if (!$Resume -or [IO.File]::ReadAllText($Part) -cne 'verified ') { throw 'Saved bytes were not reused.' }
            [IO.File]::AppendAllText($Part, 'model fixture')
            return @{ ExitCode = 0; HttpCode = '206' }
        }
        'no-range' {
            if ($Resume) { return @{ ExitCode = 22; HttpCode = '416' } }
            [IO.File]::WriteAllText($Part, $tokenizer)
            return @{ ExitCode = 0; HttpCode = '200' }
        }
        default { throw 'Unexpected download.' }
    }
}

try {
    $reference = Join-Path $fixture 'reference'
    [IO.File]::WriteAllText($reference, $model)
    $modelHash = (Get-FileHash -LiteralPath $reference).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText($reference, $tokenizer)
    $tokenizerHash = (Get-FileHash -LiteralPath $reference).Hash.ToLowerInvariant()
    $script:resumes = [Collections.Generic.List[bool]]::new()
    $script:mode = 'interrupt'
    try {
        $null = Get-KatranModelAsset $profileDir 'model.onnx' $source $modelHash
        throw 'Incomplete model accepted.'
    } catch {
        if ($_ -notlike '*after three attempts*') { throw }
    }
    if (($script:resumes -join ',') -ne 'False,True,True') { throw 'Retry did not resume.' }

    $script:mode = 'finish'
    $ready = Get-KatranModelAsset $profileDir 'model.onnx' $source $modelHash
    if ([IO.File]::ReadAllText($ready) -cne $model) { throw 'Wrong downloaded bytes.' }
    $script:mode = 'offline'
    if ((Get-KatranModelAsset $profileDir 'model.onnx' $source $modelHash) -cne $ready) { throw 'Cache was not used.' }

    $stage = Join-Path $profileDir ('.emoji-ai-stage-' + 'a' * 32 + '/local-experiments/emoji-ai/model/onnx')
    $null = [IO.Directory]::CreateDirectory($stage)
    $old = Join-Path $stage 'tokenizer.json'
    [IO.File]::WriteAllText($old, $tokenizer)
    if ((Get-KatranModelAsset $profileDir 'tokenizer.json' $source $tokenizerHash) -cne $old) { throw 'Old stage was not used.' }
    [IO.File]::WriteAllText($old, 'invalid')
    $tokenizerCache = Join-Path $profileDir "ai-model-cache/$tokenizerHash"
    $partial = Join-Path $tokenizerCache 'tokenizer.json.part'
    [IO.File]::WriteAllText($partial, 'verified ')
    $script:resumes.Clear()
    $script:mode = 'no-range'
    $ready = Get-KatranModelAsset $profileDir 'tokenizer.json' $source $tokenizerHash
    if (($script:resumes -join ',') -ne 'True,False' -or [IO.File]::ReadAllText($ready) -cne $tokenizer) {
        throw 'Server without range support was not handled.'
    }
    [IO.File]::WriteAllText($ready, 'corrupt')
    $script:mode = 'offline'
    try {
        $null = Get-KatranModelAsset $profileDir 'tokenizer.json' $source $tokenizerHash
        throw 'Corrupt cache accepted.'
    } catch {
        if ($_ -notlike '*Checksum mismatch*' -or [IO.File]::ReadAllText($ready) -cne 'corrupt') { throw }
    }
    Write-Host 'Model download retry and reuse passed.'
} finally {
    $resolved = (Resolve-Path -LiteralPath $fixture).ProviderPath
    if ($resolved -eq [IO.Path]::GetFullPath($fixture)) {
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
