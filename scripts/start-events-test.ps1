param([switch]$PrintExecutable)
$ErrorActionPreference = 'Stop'
$projectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$experiment = Join-Path $projectRoot 'runtime/experiments/clipboard-events'
$id = (Get-Content -LiteralPath (Join-Path $experiment 'current-version.txt') -Raw).Trim()
if ($id -notmatch '^[0-9a-f]{12}$') { throw 'Invalid experimental build pointer' }
$executable = Join-Path $experiment "$id/katran-ui.exe"
if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) { throw 'Build the events test first: scripts/build-events-test.ps1' }
if ($PrintExecutable) { return $executable }
# No second shelf/capture process: test the same data sequentially, not in parallel.
$running = @(Get-CimInstance Win32_Process -Filter "Name = 'katran-ui.exe'" | Where-Object {
    $_.ExecutablePath -and $_.ExecutablePath.StartsWith($projectRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)
})
if ($running.Count) { throw 'Close the running Katran window first, then launch EVENTS TEST again.' }
Start-Process -FilePath $executable -ArgumentList ('"{0}"' -f (Join-Path $projectRoot 'katran-data')) -WorkingDirectory $projectRoot -WindowStyle Normal
