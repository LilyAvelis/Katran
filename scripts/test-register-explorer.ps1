$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$script = Join-Path $PSScriptRoot 'register-explorer.ps1'
$tempRoot = Join-Path ([IO.Path]::GetTempPath()) ('katran-explorer-test-' + [guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($tempRoot) | Out-Null
function Assert-True($condition, [string] $message) { if (-not $condition) { throw $message } }
try {
    foreach ($bom in @($false, $true)) {
        $root = Join-Path $tempRoot $(if ($bom) {'bom'} else {'plain'})
        [IO.Directory]::CreateDirectory((Join-Path $root 'imports')) | Out-Null
        $config = Join-Path $root 'shell.nss'
        $encoding = [Text.UTF8Encoding]::new($bom)
        $body = "// Фикстура Лилии`r`nimport 'imports/custom.nss'`r`n// нет завершающего переноса"
        $initial = $encoding.GetPreamble() + $encoding.GetBytes($body)
        [IO.File]::WriteAllBytes($config, $initial)
        $launcher = Join-Path $tempRoot 'Программа test\runtime\Katran.exe'
        $plan = & $script -Mode Plan -NilesoftRoot $root -Launcher $launcher
        Assert-True ($plan.Backend -eq 'Nilesoft') 'Plan backend'
        Assert-True (-not [IO.File]::Exists((Join-Path $root 'imports\katran.nss'))) 'Plan must not write'
        & $script -Mode Install -NilesoftRoot $root -Launcher $launcher | Out-Null
        $first = [IO.File]::ReadAllBytes($config)
        $nss = [IO.File]::ReadAllText((Join-Path $root 'imports\katran.nss'))
        Assert-True ($nss.Contains("type='dir'")) 'Selected folder entry missing'
        Assert-True ($nss.Contains("type='back.dir'")) 'Folder background entry missing'
        Assert-True ($nss.Contains('"@sel.path\."')) 'Selected path quoting missing'
        Assert-True ($nss.Contains('"@sel.dir\."')) 'Background path quoting missing'
        Assert-True ($nss.Contains("cmd='$launcher'")) 'Launcher path missing'
        $backupCount = @(Get-ChildItem -LiteralPath $root -Filter 'shell.nss.katran-backup-*').Count
        Assert-True ($backupCount -eq 1) 'First install must create one backup'
        & $script -Mode Install -NilesoftRoot $root -Launcher $launcher | Out-Null
        Assert-True ([Convert]::ToBase64String($first) -eq [Convert]::ToBase64String([IO.File]::ReadAllBytes($config))) 'Repeated install changed config'
        Assert-True (@(Get-ChildItem -LiteralPath $root -Filter 'shell.nss.katran-backup-*').Count -eq 1) 'Repeat install made backup'
        & $script -Mode Uninstall -NilesoftRoot $root -Launcher $launcher | Out-Null
        Assert-True ([Convert]::ToBase64String($initial) -eq [Convert]::ToBase64String([IO.File]::ReadAllBytes($config))) 'Original bytes were not restored'
        Assert-True (-not [IO.File]::Exists((Join-Path $root 'imports\katran.nss'))) 'Managed file survived uninstall'
        Assert-True (@(Get-ChildItem -LiteralPath $root -Filter 'shell.nss.katran-backup-*').Count -eq 2) 'Uninstall must back up modified config'
        $foreign = Join-Path $root 'imports\katran.nss'
        [IO.File]::WriteAllText($foreign, '// user file')
        try { & $script -Mode Install -NilesoftRoot $root -Launcher $launcher | Out-Null; throw 'Expected refusal' }
        catch { if ($_.Exception.Message -eq 'Expected refusal') { throw } }
        Assert-True ([IO.File]::ReadAllText($foreign) -eq '// user file') 'Unowned file changed'
    }
    'Explorer registration synthetic tests passed.'
} finally {
    if ([IO.Path]::GetFileName($tempRoot) -notmatch '^katran-explorer-test-[0-9a-f]{32}$') { throw 'Unexpected synthetic test path' }
    if (Test-Path -LiteralPath $tempRoot) { Remove-Item -LiteralPath $tempRoot -Recurse -Force }
}
