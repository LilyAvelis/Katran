param(
    [switch]$Apply,
    [int]$BudgetGiB = 32,
    [int]$IncrementalGiB = 8
)
$ErrorActionPreference = 'Stop'
$projectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$targetRoot = Join-Path $projectRoot 'target'
if (-not (Test-Path -LiteralPath $targetRoot)) { return }
$planJson = & python -X utf8 (Join-Path $PSScriptRoot 'build-cache-plan.py') --root $targetRoot --budget-gib $BudgetGiB --incremental-gib $IncrementalGiB
if ($LASTEXITCODE -ne 0) { throw 'Build-cache inventory failed; nothing deleted' }
$plan = $planJson | ConvertFrom-Json
$summary = 'Build outputs: {0:N2} GiB -> {1:N2} GiB planned ({2} entries)' -f ($plan.before_bytes / 1GB), ($plan.planned_after_bytes / 1GB), @($plan.items).Count
Write-Output $summary
if (-not $Apply -or @($plan.items).Count -eq 0) { return }

# The installed artifact is independent of Cargo outputs. Fail closed if that
# fact cannot be verified, or process enumeration is unavailable.
$runtime = Join-Path $projectRoot 'runtime'
$version = (Get-Content -LiteralPath (Join-Path $runtime 'current-version.txt') -Raw).Trim()
if ($version -notmatch '^[0-9a-f]{12}$') { throw 'Invalid installed runtime pointer' }
$installedRoot = Join-Path $runtime "versions/$version"
$installed = Join-Path $installedRoot 'katran-ui.exe'
$manifest = Get-Content -LiteralPath (Join-Path $installedRoot 'build-manifest.json') -Raw | ConvertFrom-Json
$installedHash = (Get-FileHash -LiteralPath $installed -Algorithm SHA256).Hash.ToLowerInvariant()
if ($installedHash -cne $manifest.executable_sha256 -or -not $installedHash.StartsWith($version)) { throw 'Installed artifact verification failed' }
$processes = @(Get-CimInstance Win32_Process)
if ($processes | Where-Object { $_.Name -match '^(cargo|rustc|rustdoc|cl|link|lld-link)\.exe$' }) { throw 'Build is active; cleanup deferred' }
$prefix = $targetRoot.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
$validated = [Collections.Generic.List[object]]::new()
foreach ($item in $plan.items) {
    $relative = $item.relative.Replace('/', '\')
    if ($relative -notmatch '^(debug|build\\debug\\incremental\\[^\\]+|build\\debug\\deps\\\w+-[0-9a-f]{16}\.(exe|pdb))$') { throw 'Unexpected cleanup category' }
    $absolute = [IO.Path]::GetFullPath((Join-Path $targetRoot $item.relative))
    if (-not $absolute.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) { throw 'Cleanup escaped target' }
    $ancestor = $absolute
    while ($ancestor.Length -ge $targetRoot.Length) {
        if ((Get-Item -LiteralPath $ancestor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Cleanup path contains a reparse point' }
        if ($ancestor -ceq $targetRoot) { break }
        $ancestor = [IO.Path]::GetDirectoryName($ancestor)
    }
    if ($processes | Where-Object {
        $_.ExecutablePath -and ($_.ExecutablePath.Equals($absolute, [StringComparison]::OrdinalIgnoreCase) -or $_.ExecutablePath.StartsWith($absolute + '\', [StringComparison]::OrdinalIgnoreCase))
    }) { throw "A planned build output is running: $absolute" }
    $validated.Add([pscustomobject]@{path=$absolute; bytes=$item.bytes; reason=$item.reason})
}
$removedBytes = [long]0
$failures = @()
foreach ($item in $validated) {
    try {
        Remove-Item -LiteralPath $item.path -Recurse -Force
        $removedBytes += $item.bytes
    } catch {
        $failures += [pscustomobject]@{path=$item.path; error=$_.Exception.Message}
    }
}
$receipt = [ordered]@{utc=[DateTime]::UtcNow.ToString('o'); runtime=$version; installed_sha256=$installedHash; before_bytes=$plan.before_bytes; removed_bytes=$removedBytes; estimated_after_bytes=($plan.before_bytes-$removedBytes); budget_bytes=$plan.budget_bytes; failures=$failures; planned_entries=@($validated.ToArray())}
$receipt | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $targetRoot 'build-cache-last.json') -Encoding utf8
if ((Get-FileHash -LiteralPath $installed -Algorithm SHA256).Hash.ToLowerInvariant() -cne $installedHash) { throw 'Installed artifact changed during maintenance' }
Write-Output ('Removed {0:N2} GiB; {1} failed entries. Runtime unchanged.' -f ($removedBytes / 1GB), $failures.Count)
if ($failures.Count -gt 0 -or ($plan.before_bytes-$removedBytes) -gt $plan.budget_bytes) { Write-Warning 'Build outputs remain above budget or locked; inspect target/build-cache-last.json. Dependency libraries and release outputs are intentionally preserved.' }
