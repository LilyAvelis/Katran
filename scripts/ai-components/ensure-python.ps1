param([Parameter(Mandatory=$true)][string]$Profile, [Parameter(Mandatory=$true)][string]$JobDir)
$ErrorActionPreference = 'Stop'
function Test-Python([string]$Candidate) {
    if (-not $Candidate -or -not (Test-Path -LiteralPath $Candidate -PathType Leaf)) { return $false }
    try {
        # Windows PowerShell strips nested double quotes from native arguments.
        $value = & $Candidate -I -c 'import sys, venv, ensurepip, ssl;print(sys.version_info[:2], sys.maxsize.bit_length() + 1)' 2>$null
        return ($LASTEXITCODE -eq 0 -and $value -ceq '(3, 10) 64')
    } catch { return $false }
}
$tools = Join-Path $Profile 'ai-tools'
if (Test-Path -LiteralPath $tools) {
    $entry = Get-Item -LiteralPath $tools -Force
    if (-not $entry.PSIsContainer -or ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw 'The private Python tools directory is not a regular profile directory.'
    }
}
$private = Join-Path $tools 'python310/python.exe'
if (Test-Python $private) { Write-Output $private; return }
$managed = Join-Path $tools 'python310-managed/python.exe'
if (Test-Python $managed) { Write-Output $managed; return }
$launcher = Get-Command py.exe -ErrorAction SilentlyContinue
if ($launcher) {
    try {
        $candidate = & $launcher.Source -3.10-64 -c 'import sys;print(sys.executable)' 2>$null
        if ($LASTEXITCODE -eq 0 -and (Test-Python $candidate)) { Write-Output $candidate; return }
    } catch { }
}
$system = Get-Command python.exe -ErrorAction SilentlyContinue
if ($system -and (Test-Python $system.Source)) { Write-Output $system.Source; return }
foreach ($location in @(
    [IO.Path]::Combine([Environment]::GetFolderPath('LocalApplicationData'), 'Programs/Python/Python310/python.exe'),
    [IO.Path]::Combine($env:USERPROFILE, 'AppData/Local/Programs/Python/Python310/python.exe'),
    [IO.Path]::Combine([Environment]::GetFolderPath('ProgramFiles'), 'Python310/python.exe')
)) {
    if (Test-Python $location) { Write-Output $location; return }
}
# terminal.ps1 holds the profile-wide python-bootstrap.lock across this helper.
# A separate owned directory lets an interrupted legacy installation remain intact.
$target = Join-Path $tools 'python310-managed'
$marker = Join-Path $target '.katran-python-bootstrap'
$markerValue = 'Katran managed Python 3.10 bootstrap v1'
if (Test-Path -LiteralPath $target) {
    $item = Get-Item -LiteralPath $target -Force
    if (-not $item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
        -not (Test-Path -LiteralPath $marker -PathType Leaf) -or
        ((Get-Item -LiteralPath $marker -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -or
        (Get-Content -LiteralPath $marker -Raw) -cne $markerValue) {
        throw 'The managed Python directory is not recognized; its contents were not changed.'
    }
} else {
    New-Item -ItemType Directory -Path $target -Force | Out-Null
    [IO.File]::WriteAllText($marker, $markerValue)
}
# Publish a fully verified installer under the profile lock. A retry uses a
# fresh job directory, so a job-local download alone cannot be reused.
$installerName = 'python-3.10.11-amd64.exe'
$installer = Join-Path $tools $installerName
$cached = Test-Path -LiteralPath $installer
if ($cached) {
    $candidate = $installer
    Write-Host 'Checking the cached Python installer...'
} else {
    $candidate = Join-Path $JobDir $installerName
    if (-not (Test-Path -LiteralPath $candidate)) {
        Write-Host 'Downloading official 64-bit Python 3.10.11 into the installation job...'
        Invoke-WebRequest -Uri 'https://www.python.org/ftp/python/3.10.11/python-3.10.11-amd64.exe' -OutFile $candidate -UseBasicParsing
    }
}
$entry = Get-Item -LiteralPath $candidate -Force
if ($entry.PSIsContainer -or ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
    throw 'The Python installer cache is not a regular file.'
}
# Use the Windows PowerShell module even when the parent supplies PS 7 modules.
Import-Module (Join-Path $PSHOME 'Modules/Microsoft.PowerShell.Security/Microsoft.PowerShell.Security.psd1')
$signature = Get-AuthenticodeSignature -LiteralPath $candidate
if ($signature.Status -ne 'Valid' -or $null -eq $signature.SignerCertificate -or $signature.SignerCertificate.Subject -notmatch '(?i)(^|,)\s*CN\s*=\s*Python Software Foundation\s*(,|$)') {
    throw 'Official Python installer signature could not be verified. The file was left intact.'
}
if (-not $cached) { [IO.File]::Move($candidate, $installer) }
Write-Host 'Installing verified Python privately in the Katran profile (no PATH change)...'
$arguments = @('/quiet','InstallAllUsers=0','PrependPath=0','Include_launcher=0','Include_test=0', ('TargetDir="{0}"' -f $target))
$process = Start-Process -FilePath $installer -ArgumentList $arguments -Wait -PassThru -WindowStyle Hidden
if ($process.ExitCode -ne 0 -or -not (Test-Python $managed)) { throw "Private Python setup failed ($($process.ExitCode)). Install Python 3.10 x64 from python.org and retry." }
Write-Output $managed
