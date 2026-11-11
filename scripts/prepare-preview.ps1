param([Parameter(Mandatory=$true)][ValidateSet('snapshot','package')][string]$Mode)
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
Set-Location $root
$state = Join-Path $root 'target/preview-inputs.json'
function Inputs {
    $files = @(Get-ChildItem src,assets -Recurse -File) + @(Get-Item Cargo.toml,Cargo.lock,build.rs)
    @($files | Sort-Object FullName | ForEach-Object { [ordered]@{ path = $_.FullName.Substring($root.Length + 1); sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash } })
}
if ($Mode -eq 'snapshot') {
    $record = [ordered]@{ source_commit = (& git rev-parse HEAD | Out-String).Trim(); os_arch = 'windows-x86_64'; rustc = (& rustc -vV | Out-String).Trim(); cargo = (& cargo --version | Out-String).Trim(); features = 'ui,clipboard-events'; profile = 'release'; inputs = @(Inputs) }
    $record | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $state -Encoding UTF8
    Write-Output "Input snapshot: $state. Build immediately; keep inputs unchanged."
    exit
}
if (!(Test-Path $state)) { throw 'Run snapshot before the build.' }
$before = Get-Content -LiteralPath $state -Raw | ConvertFrom-Json
if (($before.inputs | ConvertTo-Json -Depth 6 -Compress) -ne (@(Inputs) | ConvertTo-Json -Depth 6 -Compress)) { throw 'Inputs changed since snapshot; do not package this binary.' }
if ($before.rustc -ne ((& rustc -vV | Out-String).Trim()) -or $before.cargo -ne ((& cargo --version | Out-String).Trim())) { throw 'Toolchain changed since snapshot.' }
$exe = Join-Path $root 'target/build/release/katran-ui.exe'
if (!(Test-Path $exe) -or (Get-Item $exe).LastWriteTimeUtc -lt (Get-Item $state).LastWriteTimeUtc) { throw 'No binary newer than snapshot; build first.' }
$sha = (Get-FileHash $exe -Algorithm SHA256).Hash.ToLowerInvariant()
$dest = Join-Path $root "runtime/preview/$sha"
if (Test-Path $dest) { throw "Preview already exists: $dest" }
New-Item -ItemType Directory -Path $dest | Out-Null
Copy-Item $exe (Join-Path $dest 'katran-ui.exe')
Copy-Item assets/everything/es.exe,assets/everything/LICENSE,assets/everything/NOTICE.txt,AFIO-LICENSE $dest
Copy-Item assets/emoji-flags/LICENSE-GRAPHICS (Join-Path $dest 'LICENSE-EMOJI-FLAGS.txt')
Copy-Item assets/emoji-flags/ATTRIBUTION.md (Join-Path $dest 'ATTRIBUTION-EMOJI-FLAGS.md')
[ordered]@{ kind='unaccepted preview'; executable_sha256=$sha; build_inputs=$before; es_sha256=(Get-FileHash assets/everything/es.exe).Hash; note='Snapshot before and after build; build invocation must be performed as documented.' } | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $dest 'build-manifest.json') -Encoding UTF8
Write-Output "Preview: $dest"
