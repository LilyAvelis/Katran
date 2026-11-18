function Get-KatranSources([string]$Root) {
    # This Python worker is embedded in the Rust binary with include_str!.
    $folderSemanticWorker = Join-Path $Root 'scripts/folder-semantic/worker.py'
    $imageUpscaleWorker = Join-Path $Root 'scripts/image-upscale/worker.py'
    $files = @((Get-ChildItem -LiteralPath (Join-Path $Root 'src') -Recurse -File).FullName) + @((Get-ChildItem -LiteralPath (Join-Path $Root 'assets/branding') -Recurse -File).FullName) + @('Cargo.toml','Cargo.lock','build.rs','scripts/build-working.ps1','scripts/install-build.ps1' | ForEach-Object { Join-Path $Root $_ })
    if (Test-Path -LiteralPath (Join-Path $Root 'assets/emoji-flags')) { $files += @((Get-ChildItem -LiteralPath (Join-Path $Root 'assets/emoji-flags') -Recurse -File).FullName) }
    foreach ($assetDir in @('assets/themes/lilies','assets/everything')) {
        $files += @((Get-ChildItem -LiteralPath (Join-Path $Root $assetDir) -Recurse -File).FullName)
    }
    if ($env:KATRAN_BUNDLE_APPLE -eq '1') { $files += (Get-Item -LiteralPath (Join-Path $Root 'assets/emoji-bundle/apple.zip')).FullName }
    if (Test-Path -LiteralPath (Join-Path $Root 'assets/unicode')) { $files += @((Get-ChildItem -LiteralPath (Join-Path $Root 'assets/unicode') -Recurse -File).FullName) }
    foreach ($crate in @('avilon-actions','avilon-egui','avilon-media','avilon-windows')) {
        $crateRoot = Join-Path $Root "crates/$crate"
        $files += @((Get-Item -LiteralPath (Join-Path $crateRoot 'Cargo.toml'),(Join-Path $crateRoot 'LICENSE')).FullName)
        $files += @(Get-ChildItem -LiteralPath (Join-Path $crateRoot 'src') -Recurse -File -Filter '*.rs' | Where-Object { $_.FullName.Substring($crateRoot.Length + 1) -notmatch '(^|[\\/])(target|build)[\\/]' } | ForEach-Object FullName)
    }
    if (Test-Path -LiteralPath $folderSemanticWorker) { $files += $folderSemanticWorker }
    if (Test-Path -LiteralPath $imageUpscaleWorker) { $files += $imageUpscaleWorker }
    $translationScripts = Join-Path $Root 'scripts/image-translate'
    if (Test-Path -LiteralPath $translationScripts) {
        $files += @((Get-ChildItem -LiteralPath $translationScripts -File -Filter '*.py').FullName)
    }
    # Optional AI installer resources are embedded, so their bytes are build inputs.
    foreach ($inputDir in @('scripts/ai-components','scripts/emoji-ai','assets/emoji-ai/cldr')) {
        $files += @((Get-ChildItem -LiteralPath (Join-Path $Root $inputDir) -File).FullName)
    }
    foreach ($inputFile in @('scripts/folder-semantic/setup.ps1','scripts/folder-semantic/setup-gpu.ps1','scripts/install-emoji-ai-model.ps1','scripts/create-emoji-ai-runtime-manifest.py')) { $files += (Join-Path $Root $inputFile) }
    return @($files | Sort-Object -Unique | ForEach-Object { [ordered]@{ path = $_.Substring($Root.Length + 1); sha256 = (Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant() } })
}
function Invoke-KatranInstall {
    param([Parameter(Mandatory)][string]$Root, [Parameter(Mandatory)][scriptblock]$Build, [Parameter(Mandatory)][scriptblock]$Register)
    $Root = [IO.Path]::GetFullPath($Root).TrimEnd([IO.Path]::DirectorySeparatorChar)
    $rustc = (& rustc -vV | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $rustc -notmatch '(?m)^host: (\S+)\r?$') { throw 'Cannot determine Rust toolchain and host target' }
    $targetOverride = $env:CARGO_BUILD_TARGET
    $target = if ($targetOverride) { $targetOverride } else { $Matches[1] }
    $cargoVersion = (& cargo --version | Out-String).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'Cannot determine Cargo version' }
    $appleBundle = ($env:KATRAN_BUNDLE_APPLE -eq '1')
    $before = @(Get-KatranSources $Root)
    $beforeJson = ConvertTo-Json -InputObject $before -Depth 5 -Compress
    $cargo = Get-Content -LiteralPath (Join-Path $Root 'Cargo.toml') -Raw
    if ($cargo -notmatch '(?ms)^\[package\]\s*\r?\n(?:(?!^\[).)*?^version\s*=\s*"([^"]+)"') { throw 'Cannot parse package version from Cargo.toml' }
    $version = $Matches[1]
    & $Build
    if ((ConvertTo-Json -InputObject @(Get-KatranSources $Root) -Depth 5 -Compress) -cne $beforeJson) { throw 'Sources changed during build; refusing installation' }
    if ((& rustc -vV | Out-String).Trim() -cne $rustc -or (& cargo --version | Out-String).Trim() -cne $cargoVersion -or ($env:KATRAN_BUNDLE_APPLE -eq '1') -ne $appleBundle -or ($env:CARGO_BUILD_TARGET -cne $targetOverride)) { throw 'Build configuration changed during build; refusing installation' }
    $built = Join-Path $Root 'target/build/release/katran-ui.exe'
    $hash = (Get-FileHash -LiteralPath $built -Algorithm SHA256).Hash.ToLowerInvariant()
    $id = $hash.Substring(0,12)
    $versions = Join-Path $Root 'runtime/versions'
    New-Item -ItemType Directory -Path $versions -Force | Out-Null
    $stage = Join-Path $versions ('.stage-' + [guid]::NewGuid().ToString('N'))
    $destination = Join-Path $versions $id
    $pointer = Join-Path $Root 'runtime/current-version.txt'
    $pointerTemp = Join-Path $Root ('runtime/.pointer-' + [guid]::NewGuid().ToString('N'))
    $pointerBackup = $pointerTemp + '.bak'
    try {
        New-Item -ItemType Directory -Path $stage | Out-Null
        Copy-Item -LiteralPath $built -Destination (Join-Path $stage 'katran-ui.exe')
        if (Test-Path -LiteralPath (Join-Path $Root 'assets/unicode/LICENSE.txt')) {
            Copy-Item -LiteralPath (Join-Path $Root 'assets/unicode/LICENSE.txt') -Destination (Join-Path $stage 'LICENSE-UNICODE.txt')
            Copy-Item -LiteralPath (Join-Path $Root 'assets/unicode/SOURCE.md') -Destination (Join-Path $stage 'UNICODE-SOURCES.md')
        }
        if (Test-Path -LiteralPath (Join-Path $Root 'assets/emoji-flags/LICENSE-GRAPHICS')) {
            Copy-Item -LiteralPath (Join-Path $Root 'assets/emoji-flags/LICENSE-GRAPHICS') -Destination (Join-Path $stage 'LICENSE-EMOJI-FLAGS.txt')
            Copy-Item -LiteralPath (Join-Path $Root 'assets/emoji-flags/ATTRIBUTION.md') -Destination (Join-Path $stage 'ATTRIBUTION-EMOJI-FLAGS.md')
        }
        if ((Get-FileHash -LiteralPath (Join-Path $stage 'katran-ui.exe') -Algorithm SHA256).Hash.ToLowerInvariant() -cne $hash) { throw 'Copied executable hash mismatch' }
        $manifest = [ordered]@{ profile = 'release'; rustc = $rustc; cargo = $cargoVersion; target = $target; features = @('ui','clipboard-events'); default_features = $true; bundled_apple = $appleBundle; version = $version; update_format = 1; minimum_launcher_protocol = 1; capture_backend = 'windows-events'; shell_visual_protocol = 1; executable_sha256 = $hash; sources = $before }
        $manifestJson = ConvertTo-Json -InputObject $manifest -Depth 5
        Set-Content -LiteralPath (Join-Path $stage 'build-manifest.json') -Value $manifestJson -Encoding utf8
        if (Test-Path -LiteralPath $destination) {
            if ((Get-FileHash -LiteralPath (Join-Path $destination 'katran-ui.exe') -Algorithm SHA256).Hash.ToLowerInvariant() -cne $hash) { throw 'Existing version executable hash mismatch' }
            $existing = Get-Content -LiteralPath (Join-Path $destination 'build-manifest.json') -Raw | ConvertFrom-Json
            if ((ConvertTo-Json -InputObject $existing -Depth 5 -Compress) -cne (ConvertTo-Json -InputObject ($manifestJson | ConvertFrom-Json) -Depth 5 -Compress)) { throw 'Existing version manifest differs; refusing to overwrite immutable version' }
        } else { Move-Item -LiteralPath $stage -Destination $destination }
        if ((ConvertTo-Json -InputObject @(Get-KatranSources $Root) -Depth 5 -Compress) -cne $beforeJson) { throw 'Sources changed before installation; refusing pointer update' }
        $installed = Join-Path $destination 'katran-ui.exe'
        if ((Get-FileHash -LiteralPath $installed -Algorithm SHA256).Hash.ToLowerInvariant() -cne $hash) { throw 'Installed executable hash mismatch' }
        $installedManifest = Get-Content -LiteralPath (Join-Path $destination 'build-manifest.json') -Raw | ConvertFrom-Json
        if ((ConvertTo-Json -InputObject $installedManifest -Depth 5 -Compress) -cne (ConvertTo-Json -InputObject ($manifestJson | ConvertFrom-Json) -Depth 5 -Compress)) { throw 'Installed manifest mismatch' }
        $launcher = Join-Path $Root 'runtime/Katran.exe'
        Copy-Item -LiteralPath (Join-Path $Root 'target/build/release/katran-launcher.exe') -Destination $launcher -Force
        & $Register $launcher
        # Check again after the external registration step, immediately before commit.
        if ((ConvertTo-Json -InputObject @(Get-KatranSources $Root) -Depth 5 -Compress) -cne $beforeJson) { throw 'Sources changed before pointer update' }
        if ((Get-FileHash -LiteralPath $installed -Algorithm SHA256).Hash.ToLowerInvariant() -cne $hash) { throw 'Installed executable changed before pointer update' }
        $finalManifest = Get-Content -LiteralPath (Join-Path $destination 'build-manifest.json') -Raw | ConvertFrom-Json
        if ((ConvertTo-Json -InputObject $finalManifest -Depth 5 -Compress) -cne (ConvertTo-Json -InputObject ($manifestJson | ConvertFrom-Json) -Depth 5 -Compress)) { throw 'Installed manifest changed before pointer update' }
        [IO.File]::WriteAllText($pointerTemp, $id + "`r`n", [Text.Encoding]::ASCII)
        if (Test-Path -LiteralPath $pointer) { [IO.File]::Replace($pointerTemp, $pointer, $pointerBackup) } else { [IO.File]::Move($pointerTemp, $pointer) }
        Write-Output "Prepared Katran ${version}: $id"
    } finally {
        # Both cleanup targets are generated direct children of the controlled runtime tree.
        if ((Split-Path -Parent $stage) -ceq $versions -and (Split-Path -Leaf $stage) -match '^\.stage-[0-9a-f]{32}$' -and (Test-Path -LiteralPath $stage)) { Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction SilentlyContinue }
        if ((Split-Path -Parent $pointerBackup) -ceq (Join-Path $Root 'runtime') -and (Split-Path -Leaf $pointerBackup) -match '^\.pointer-[0-9a-f]{32}\.bak$' -and (Test-Path -LiteralPath $pointerBackup)) { Remove-Item -LiteralPath $pointerBackup -Force -ErrorAction SilentlyContinue }
        if ((Split-Path -Parent $pointerTemp) -ceq (Join-Path $Root 'runtime') -and (Split-Path -Leaf $pointerTemp) -match '^\.pointer-[0-9a-f]{32}$' -and (Test-Path -LiteralPath $pointerTemp)) { Remove-Item -LiteralPath $pointerTemp -Force -ErrorAction SilentlyContinue }
    }
}
