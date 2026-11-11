function Get-KatranReleaseVersion {
    param([Parameter(Mandatory)]$Metadata)
    $packages = @($Metadata.packages | Where-Object { $_.name -ceq 'katran' -and $_.id -cin $Metadata.workspace_members })
    if ($packages.Count -ne 1) { throw 'Expected exactly one Katran workspace package' }
    $version = $packages[0].version
    if ($version -cnotmatch '^\d+\.\d+\.\d+$') { throw 'Expected a stable Katran release version' }
    return $version
}
