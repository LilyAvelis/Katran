param([Parameter(Mandatory=$true)][string]$Profile, [Parameter(Mandatory=$true)][string]$JobDir)
$ErrorActionPreference = 'Stop'
function Test-Python([string]$Candidate) {
    if (-not $Candidate -or -not (Test-Path -LiteralPath $Candidate -PathType Leaf)) { return $false }
    try {
        # Windows PowerShell strips nested double quotes from native arguments.
        $value = & $Candidate -I -c 'import sys;print(sys.version_info[:2], sys.maxsize.bit_length() + 1)' 2>$null
        return ($LASTEXITCODE -eq 0 -and $value -ceq '(3, 10) 64')
    } catch { return $false }
}
$private = Join-Path $Profile 'ai-tools/python310/python.exe'
if (Test-Python $private) { Write-Output $private; return }
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
$target = Join-Path $Profile 'ai-tools/python310'
if (Test-Path -LiteralPath $target) { throw 'A private Python directory exists but is unusable. Inspect it before retrying; it was not overwritten.' }
$installer = Join-Path $JobDir 'python-3.10.11-amd64.exe'
if (Test-Path -LiteralPath $installer) { throw 'Python bootstrap file already exists in this job. Start a fresh installation.' }
Write-Host 'Downloading official 64-bit Python 3.10.11 into the installation job...'
Invoke-WebRequest -Uri 'https://www.python.org/ftp/python/3.10.11/python-3.10.11-amd64.exe' -OutFile $installer -UseBasicParsing
# Load the Windows PowerShell module even when the parent supplies a PowerShell 7 module path.
Import-Module (Join-Path $PSHOME 'Modules/Microsoft.PowerShell.Security/Microsoft.PowerShell.Security.psd1')
$signature = Get-AuthenticodeSignature -LiteralPath $installer
if ($signature.Status -ne 'Valid' -or $null -eq $signature.SignerCertificate -or $signature.SignerCertificate.Subject -notmatch '(?i)(^|,)\s*CN\s*=\s*Python Software Foundation\s*(,|$)') {
    throw 'Official Python installer signature could not be verified. Install Python 3.10 x64 from python.org and retry.'
}
Write-Host 'Installing verified Python privately in the Katran profile (no PATH change)...'
$arguments = @('/quiet','InstallAllUsers=0','PrependPath=0','Include_launcher=0','Include_test=0', ('TargetDir="{0}"' -f $target))
$process = Start-Process -FilePath $installer -ArgumentList $arguments -Wait -PassThru -WindowStyle Normal
if ($process.ExitCode -ne 0 -or -not (Test-Python $private)) { throw "Private Python setup failed ($($process.ExitCode)). Install Python 3.10 x64 from python.org and retry." }
Write-Output $private
