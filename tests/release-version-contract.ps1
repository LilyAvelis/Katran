$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '../scripts/release-version.ps1')
$app = [pscustomobject]@{name='katran';id='katran-id';version='3.1.1'}
$library = [pscustomobject]@{name='avilon-egui';id='avilon-id';version='0.1.0'}
foreach ($packages in @(@($library,$app),@($app,$library))) {
    $metadata = [pscustomobject]@{packages=$packages;workspace_members=@('katran-id','avilon-id')}
    if ((Get-KatranReleaseVersion $metadata) -cne '3.1.1') { throw 'Workspace order affected app version' }
}
foreach ($packages in @(@($library),@($app,$app))) {
    try { Get-KatranReleaseVersion ([pscustomobject]@{packages=$packages;workspace_members=@('katran-id','avilon-id')}); throw 'Expected rejection' }
    catch { if ($_.Exception.Message -eq 'Expected rejection') { throw } }
}
Write-Output 'Release version contract passed.'
