$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$sandbox = Join-Path ([IO.Path]::GetTempPath()) ('katran-release-test-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path (Join-Path $sandbox 'src'),(Join-Path $sandbox 'binaries'),(Join-Path $sandbox 'katran-data'),(Join-Path $sandbox 'runtime/old'),(Join-Path $sandbox 'assets/branding'),(Join-Path $sandbox 'assets/themes/lilies'),(Join-Path $sandbox 'assets/everything'),(Join-Path $sandbox 'assets/emoji-flags'),(Join-Path $sandbox 'assets/unicode'),(Join-Path $sandbox 'assets/emoji-ai/cldr'),(Join-Path $sandbox 'scripts/emoji-ai'),(Join-Path $sandbox 'scripts/ai-components'),(Join-Path $sandbox 'scripts/folder-semantic') | Out-Null
try {
    foreach ($crate in @('avilon-actions','avilon-egui','avilon-media','avilon-windows')) {
        foreach ($dir in @("crates/$crate/src/nested", "crates/$crate/src/target", "crates/$crate/target")) { New-Item -ItemType Directory -Path (Join-Path $sandbox $dir) -Force | Out-Null }
        foreach ($file in @('Cargo.toml','LICENSE','src/lib.rs','src/nested/module.rs','src/target/private.rs','target/privatefile','Cargo.lock')) { [IO.File]::WriteAllText((Join-Path $sandbox "crates/$crate/$file"), "synthetic-$file") }
    }
    foreach ($file in @('katran-ui.exe','katran-launcher.exe','katran.exe')) { [IO.File]::WriteAllText((Join-Path $sandbox "binaries/$file"), "synthetic-$file") }
    foreach ($file in @('Cargo.toml','Cargo.lock','build.rs','AFIO-LICENSE','THIRD-PARTY-NOTICES.txt','src/lib.rs','.env.local','katran-data/private.txt','runtime/old/private.exe','binaries/private.pdb')) { [IO.File]::WriteAllText((Join-Path $sandbox $file), 'synthetic-private-canary') }
    [IO.File]::WriteAllText((Join-Path $sandbox 'src/catalogue_schema.rs'), 'pub(crate) const VERSION: i64 = 12;')
    foreach ($file in @('assets/everything/es.exe','assets/everything/LICENSE','assets/everything/NOTICE.txt','assets/emoji-flags/LICENSE-GRAPHICS','assets/emoji-flags/ATTRIBUTION.md','assets/unicode/LICENSE.txt','assets/unicode/SOURCE.md')) { [IO.File]::WriteAllText((Join-Path $sandbox $file), "synthetic-$file") }
    $emojiInputs = @('scripts/install-emoji-ai-model.ps1','scripts/create-emoji-ai-runtime-manifest.py','scripts/emoji-ai/worker.py','scripts/emoji-ai/semantic_search.py','assets/unicode/emoji-test.txt','assets/emoji-ai/cldr/LICENSE','assets/emoji-ai/cldr/cldr-annotations-full-en.json','assets/emoji-ai/cldr/cldr-annotations-derived-full-en.json','assets/emoji-ai/cldr/cldr-annotations-full-ru.json','assets/emoji-ai/cldr/cldr-annotations-derived-full-ru.json')
    $emojiInputs += @('scripts/ai-components/terminal.ps1','scripts/ai-components/ensure-python.ps1','scripts/ai-components/verify.py','scripts/folder-semantic/setup.ps1','scripts/folder-semantic/setup-gpu.ps1','scripts/folder-semantic/worker.py')
    foreach ($file in $emojiInputs) { [IO.File]::WriteAllText((Join-Path $sandbox $file), "synthetic-$file") }
    $additionalInputs = @('crates/baloh-collection/Cargo.toml','crates/baloh-collection/src/codec.rs','scripts/image-upscale/worker.py','scripts/image-translate/translate_page.py','src/google_lens_handoff.html','scripts/setup-video-player.ps1')
    foreach ($file in $additionalInputs) {
        $path = Join-Path $sandbox $file
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $path) | Out-Null
        [IO.File]::WriteAllText($path, "synthetic-$file")
    }
    $out = Join-Path $sandbox 'output'
    & (Join-Path $root 'scripts/package-release.ps1') -Root $sandbox -BinaryDirectory 'binaries' -OutputDirectory $out -Version '2.0.0' -DependencyNoticesPath (Join-Path $sandbox 'THIRD-PARTY-NOTICES.txt')
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead((Join-Path $out 'Katran-2.0.0-windows-x64-portable.zip'))
    try {
        $names = @($zip.Entries.FullName)
        $hash = (Get-FileHash -LiteralPath (Join-Path $sandbox 'binaries/katran-ui.exe') -Algorithm SHA256).Hash.ToLowerInvariant()
        $expected = @('AFIO-LICENSE','THIRD-PARTY-NOTICES.txt','Katran.exe','katran-cli.exe','portable.flag','runtime/Katran.exe','runtime/current-version.txt',"runtime/versions/$($hash.Substring(0,12))/katran-ui.exe","runtime/versions/$($hash.Substring(0,12))/build-manifest.json","runtime/versions/$($hash.Substring(0,12))/THIRD-PARTY-NOTICES.txt","runtime/versions/$($hash.Substring(0,12))/es.exe","runtime/versions/$($hash.Substring(0,12))/Everything-ES-LICENSE.txt","runtime/versions/$($hash.Substring(0,12))/Everything-ES-NOTICE.txt") + $emojiInputs
        $expected += 'scripts/setup-video-player.ps1'
        if (@(Compare-Object ($names | Sort-Object) ($expected | Sort-Object)).Count) { throw 'Portable whitelist differs' }
    } finally { $zip.Dispose() }
    $release = Get-Content -LiteralPath (Join-Path $out 'release.json') -Raw | ConvertFrom-Json
    if ($release.data_schema_version -ne 12) { throw 'Release data schema version differs' }
    $update = Join-Path $out 'Katran-2.0.0-update.zip'
    if ($release.update.sha256 -cne (Get-FileHash -LiteralPath $update -Algorithm SHA256).Hash.ToLowerInvariant() -or $release.update.size_bytes -ne (Get-Item -LiteralPath $update).Length) { throw 'Update metadata does not match ZIP' }
    $zip = [IO.Compression.ZipFile]::OpenRead($update)
    try {
        if (@(Compare-Object @($zip.Entries.FullName | Sort-Object) @('build-manifest.json','katran-ui.exe','THIRD-PARTY-NOTICES.txt' | Sort-Object)).Count) { throw 'Update whitelist differs' }
        $reader = New-Object IO.StreamReader($zip.GetEntry('build-manifest.json').Open())
        try { $manifest = $reader.ReadToEnd() | ConvertFrom-Json } finally { $reader.Dispose() }
        $sourcePaths = @($manifest.sources | ForEach-Object { $_.path.Replace('\','/') })
        foreach ($file in ($emojiInputs + $additionalInputs)) {
            if ($file -eq 'assets/unicode/emoji-test.txt') { continue }
            $entry = @($manifest.sources | Where-Object { $_.path.Replace('\','/') -eq $file })
            if ($entry.Count -ne 1 -or $entry[0].sha256 -cne (Get-FileHash -LiteralPath (Join-Path $sandbox $file) -Algorithm SHA256).Hash.ToLowerInvariant()) { throw "Missing or incorrect emoji input hash: $file" }
        }
        foreach ($crate in @('avilon-actions','avilon-egui','avilon-media','avilon-windows')) {
            foreach ($file in @('Cargo.toml','LICENSE','src/lib.rs','src/nested/module.rs')) {
                $path = "crates/$crate/$file"
                $entry = @($manifest.sources | Where-Object { $_.path.Replace('\','/') -eq $path })
                if ($entry.Count -ne 1 -or $entry[0].sha256 -cne (Get-FileHash -LiteralPath (Join-Path $sandbox $path) -Algorithm SHA256).Hash.ToLowerInvariant()) { throw "Missing or incorrect source hash: $path" }
            }
            foreach ($file in @('src/target/private.rs','target/privatefile','Cargo.lock')) { if ("crates/$crate/$file" -in $sourcePaths) { throw "Unexpected crate input: $file" } }
        }
        $noticesEntry = $zip.GetEntry('THIRD-PARTY-NOTICES.txt')
        $noticesStream = $noticesEntry.Open()
        try { $noticesHash = [BitConverter]::ToString([Security.Cryptography.SHA256]::Create().ComputeHash($noticesStream)).Replace('-','').ToLowerInvariant() } finally { $noticesStream.Dispose() }
        if ($manifest.third_party_notices_sha256 -cne $noticesHash) { throw 'Combined notice digest differs' }
        $reader = New-Object IO.StreamReader($noticesEntry.Open())
        try { $notices = $reader.ReadToEnd() } finally { $reader.Dispose() }
        foreach ($file in @('assets/everything/NOTICE.txt','assets/everything/LICENSE','assets/emoji-flags/LICENSE-GRAPHICS','assets/emoji-flags/ATTRIBUTION.md','assets/unicode/LICENSE.txt','assets/unicode/SOURCE.md')) { if (!$notices.Contains("synthetic-$file")) { throw "Missing combined notice: $file" } }
        if (!$notices.Contains('synthetic-assets/emoji-ai/cldr/LICENSE')) { throw 'Missing CLDR license notice' }
        if (!$notices.Contains('synthetic-private-canary')) { throw 'Missing dependency notices' }
    } finally { $zip.Dispose() }
    $second = Join-Path $sandbox 'second'
    & (Join-Path $root 'scripts/package-release.ps1') -Root $sandbox -BinaryDirectory 'binaries' -OutputDirectory $second -Version '2.0.0' -DependencyNoticesPath (Join-Path $sandbox 'THIRD-PARTY-NOTICES.txt')
    if ((Get-FileHash -LiteralPath $update).Hash -cne (Get-FileHash -LiteralPath (Join-Path $second 'Katran-2.0.0-update.zip')).Hash) { throw 'ZIP bytes are not deterministic' }
    if ((Get-FileHash -LiteralPath (Join-Path $out 'Katran-2.0.0-windows-x64-portable.zip')).Hash -cne (Get-FileHash -LiteralPath (Join-Path $second 'Katran-2.0.0-windows-x64-portable.zip')).Hash) { throw 'Portable ZIP bytes are not deterministic' }
    $refused = $false
    try { & (Join-Path $root 'scripts/package-release.ps1') -Root $sandbox -BinaryDirectory 'binaries' -OutputDirectory $out -Version '2.0.0' -DependencyNoticesPath (Join-Path $sandbox 'THIRD-PARTY-NOTICES.txt') } catch { $refused = $true }
    if (!$refused) { throw 'Packaging overwrote existing output' }
    Write-Output 'Release package contract passed (synthetic files only; no application launched).'
} finally {
    $resolved = [IO.Path]::GetFullPath($sandbox)
    if ((Split-Path -Parent $resolved) -eq [IO.Path]::GetTempPath().TrimEnd('\') -and (Split-Path -Leaf $resolved) -match '^katran-release-test-[a-f0-9]{32}$') { Remove-Item -LiteralPath $resolved -Recurse -Force }
}
