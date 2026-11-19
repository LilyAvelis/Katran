$ErrorActionPreference = 'Stop'
$root = Join-Path $env:TEMP ('katran-bootstrap-test-' + [guid]::NewGuid().ToString('N'))
$scriptPath = Join-Path $PSScriptRoot 'ensure-python.ps1'
New-Item -ItemType Directory -Path $root | Out-Null
$profile = Join-Path $root 'profile'; $job = Join-Path $root 'job'
New-Item -ItemType Directory -Path $profile,$job | Out-Null
$legacy = Join-Path $profile 'ai-tools/python310'
New-Item -ItemType Directory -Path $legacy -Force | Out-Null
Set-Content -LiteralPath (Join-Path $legacy 'keep.txt') -Value 'legacy'
$fixturePython = Join-Path $root 'fixture-python.exe'
Add-Type -OutputAssembly $fixturePython -OutputType ConsoleApplication -TypeDefinition @'
public static class FixturePython {
    public static int Main(string[] args) {
        System.Console.WriteLine("(3, 10) 64");
        return 0;
    }
}
'@
$global:katranTestDownloads = 0; $global:katranTestStarts = 0
function Test-Path { param([string]$LiteralPath, [string]$PathType) if ($LiteralPath -like '*python.exe' -and -not $LiteralPath.StartsWith($root, [StringComparison]::OrdinalIgnoreCase)) { return $false }; if ($PathType) { Microsoft.PowerShell.Management\Test-Path -LiteralPath $LiteralPath -PathType $PathType } else { Microsoft.PowerShell.Management\Test-Path -LiteralPath $LiteralPath } }
function Get-Command { param($Name, $ErrorAction) return $null }
function Import-Module { param($Name) }
function Invoke-WebRequest { param($Uri,$OutFile,[switch]$UseBasicParsing) $global:katranTestDownloads++; [IO.File]::WriteAllText($OutFile,'fixture') }
function Get-AuthenticodeSignature { param($LiteralPath) [pscustomobject]@{Status='Valid';SignerCertificate=[pscustomobject]@{Subject='CN=Python Software Foundation'}} }
function Start-Process {
    param($FilePath,$ArgumentList,[switch]$Wait,[switch]$PassThru,$WindowStyle)
    $global:katranTestStarts++
    if ($global:katranTestStarts -eq 1) { return [pscustomobject]@{ExitCode=1} }
    $installTarget = ($ArgumentList | Where-Object { $_ -like 'TargetDir=*' }).Substring(10).Trim('"')
    [IO.File]::Copy($fixturePython, (Join-Path $installTarget 'python.exe'))
    return [pscustomobject]@{ExitCode=0}
}
try {
    $firstJob = Join-Path $job 'attempt-1'
    $retryJob = Join-Path $job 'attempt-2'
    New-Item -ItemType Directory -Path $firstJob,$retryJob | Out-Null
    try { & $scriptPath -Profile $profile -JobDir $firstJob; throw 'Expected failure' }
    catch { if ($_.Exception.Message -notmatch 'Private Python setup failed') { throw } }
    $installed = & $scriptPath -Profile $profile -JobDir $retryJob
    $expected = Join-Path $profile 'ai-tools/python310-managed/python.exe'
    if ($installed -cne $expected -or (& $scriptPath -Profile $profile -JobDir $retryJob) -cne $expected) {
        throw 'Retry did not return and reuse the ready private Python'
    }
    if ($global:katranTestDownloads -ne 1 -or $global:katranTestStarts -ne 2) { throw 'Installer was not reused on retry.' }
    if ((Get-Content -LiteralPath (Join-Path $legacy 'keep.txt')) -ne 'legacy') { throw 'Legacy data changed.' }
    $marker = Join-Path $profile 'ai-tools/python310-managed/.katran-python-bootstrap'
    Remove-Item -LiteralPath $installed
    Set-Content -LiteralPath $marker -Value 'foreign'
    try { & $scriptPath -Profile $profile -JobDir $job; throw 'Expected refusal' } catch { if ($_.Exception.Message -notmatch 'not recognized') { throw } }
    if ($global:katranTestStarts -ne 2) { throw 'Unowned directory was used.' }
    Write-Output 'PASS: interrupted managed retry, installer reuse, legacy preservation, unowned refusal'
} finally {
    $resolved = [IO.Path]::GetFullPath($root)
    if ((Split-Path -Parent $resolved) -ieq [IO.Path]::GetFullPath($env:TEMP) -and
        (Split-Path -Leaf $resolved) -match '^katran-bootstrap-test-[a-f0-9]{32}$') {
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
