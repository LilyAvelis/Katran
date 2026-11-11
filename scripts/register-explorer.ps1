[CmdletBinding()]
param(
    [Parameter(Mandatory)] [ValidateSet('Plan', 'Install', 'Uninstall')] [string] $Mode,
    [string] $NilesoftRoot,
    [string] $Launcher = (Join-Path (Split-Path $PSScriptRoot -Parent) 'runtime\Katran.exe')
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$launcherFull = [IO.Path]::GetFullPath($Launcher)
if (-not [IO.Path]::IsPathRooted($launcherFull) -or $launcherFull.IndexOfAny([char[]]"`r`n`"'") -ge 0 -or $launcherFull.Contains('@')) {
    throw 'Launcher path cannot contain quotes, @, or newlines.'
}
$marker = '// Katran managed Explorer integration v1'
$importBlock = "`r`n// Katran managed import v1`r`nimport 'imports/katran.nss'`r`n// End Katran managed import v1`r`n"
$utf8 = [Text.UTF8Encoding]::new($false, $true)

function Get-BackupPath([string] $Path) {
    $stamp = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffffffZ')
    return "$Path.katran-backup-$stamp"
}
function Read-Config([string] $Path) {
    return $utf8.GetString([IO.File]::ReadAllBytes($Path))
}
function Write-Config([string] $Path, [string] $Text) {
    [IO.File]::WriteAllBytes($Path, $utf8.GetBytes($Text))
}
function Assert-Owned([string] $Path) {
    if ([IO.File]::Exists($Path)) {
        $current = Read-Config $Path
        if (-not $current.StartsWith($marker + "`r`n", [StringComparison]::Ordinal)) {
            throw "Refusing to modify unowned file: $Path"
        }
    }
}
if ($NilesoftRoot) {
    $root = [IO.Path]::GetFullPath($NilesoftRoot)
    $config = Join-Path $root 'shell.nss'
    $imports = Join-Path $root 'imports'
    $managed = Join-Path $imports 'katran.nss'
    if (-not [IO.File]::Exists($config) -or -not [IO.Directory]::Exists($imports)) { throw 'Nilesoft shell.nss or imports directory does not exist.' }
    $original = Read-Config $config
    Assert-Owned $managed
    $hasOwnedImport = $original.Contains($importBlock)
    $otherImports = $original.Replace($importBlock, '')
    if ($otherImports -match '(?im)^\s*import\s+[''"]imports[\\/]katran\.nss[''"]\s*(?://.*)?$') {
        throw 'Refusing to change an existing unowned katran.nss import.'
    }
    if ($Mode -eq 'Plan') {
        [pscustomobject]@{ Backend='Nilesoft'; Mode='Plan'; Launcher=$launcherFull; Config=$config; ManagedFile=$managed; Changes=@('Write managed import file', 'Add managed import if absent', 'Back up shell.nss before modification') }
        return
    }
    if ($Mode -eq 'Install') {
        $payload = @"
$marker
item(mode='single' type='dir' title='Открыть в Katran' image='$launcherFull' cmd='$launcherFull' args='--open-folder "@sel.path\."' window=hidden)
item(mode='single' type='back.dir' title='Открыть в Katran' image='$launcherFull' cmd='$launcherFull' args='--open-folder "@sel.dir\."' window=hidden)
"@ -replace "`r?`n", "`r`n"
        $payload += "`r`n"
        if (-not [IO.File]::Exists($managed) -or (Read-Config $managed) -ne $payload) { Write-Config $managed $payload }
        if (-not $hasOwnedImport) {
            $backup = Get-BackupPath $config
            [IO.File]::Copy($config, $backup, $false)
            Write-Config $config ($original + $importBlock)
        }
        [pscustomobject]@{ Backend='Nilesoft'; Mode='Install'; Config=$config; ManagedFile=$managed }
        return
    }
    if ($hasOwnedImport) {
        $backup = Get-BackupPath $config
        [IO.File]::Copy($config, $backup, $false)
        Write-Config $config ($original.Replace($importBlock, ''))
    }
    if ([IO.File]::Exists($managed)) { [IO.File]::Delete($managed) }
    [pscustomobject]@{ Backend='Nilesoft'; Mode='Uninstall'; Config=$config; ManagedFile=$managed }
    return
}

# Registry entries are opt-in and scoped to the current user. Never alter an
# existing entry with the same name unless it carries our ownership marker.
$locations = @(
    @{ Key='HKCU:\Software\Classes\Directory\shell\Katran.OpenFolder'; Arg='%1\.' },
    @{ Key='HKCU:\Software\Classes\Directory\Background\shell\Katran.OpenFolder'; Arg='%V\.' }
)
if ($Mode -eq 'Plan') {
    [pscustomobject]@{ Backend='HKCU'; Mode='Plan'; Launcher=$launcherFull; Keys=@($locations | ForEach-Object { $_.Key }) }
    return
}
foreach ($location in $locations) {
    if (Test-Path -LiteralPath $location.Key) {
        $existing = Get-ItemProperty -LiteralPath $location.Key -Name 'KatranManaged' -ErrorAction SilentlyContinue
        if ($null -eq $existing -or $existing.KatranManaged -ne 'v1') { throw "Refusing to modify unowned registry key: $($location.Key)" }
    }
}
foreach ($location in $locations) {
    if ($Mode -eq 'Uninstall') {
        if (Test-Path -LiteralPath $location.Key) { Remove-Item -LiteralPath $location.Key -Recurse -Force }
        continue
    }
    $key = New-Item -Path $location.Key -Force
    $key.SetValue('', 'Открыть в Katran')
    $key.SetValue('Icon', $launcherFull)
    $key.SetValue('KatranManaged', 'v1')
    $command = New-Item -Path (Join-Path $location.Key 'command') -Force
    $command.SetValue('', ('"{0}" --open-folder "{1}"' -f $launcherFull, $location.Arg))
}
[pscustomobject]@{ Backend='HKCU'; Mode=$Mode; Launcher=$launcherFull; Keys=@($locations | ForEach-Object { $_.Key }) }
