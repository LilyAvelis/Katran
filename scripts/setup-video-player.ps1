param(
    [string]$OutputDirectory = (Join-Path $PSScriptRoot '../runtime/components/mpv-20261107'),
    [string]$Archive
)
$ErrorActionPreference = 'Stop'
# Same pinned Windows x64 runtime as Afio. No rolling daily-build dependency.
$download = 'https://github.com/shinchiro/mpv-winbuild-cmake/releases/download/20261107/mpv-dev-x86_64-20261107-git-9028187538.7z'
$archiveHash = 'DF7C433A2D481FDAF6163B201F26ABBB0EFF0C215C51368397088C6D8F7A001B'
$dllHash = 'C91949577AE9CC2D500208DA51F2C4A24CFA7A02D0CE6E130BF67EC738B450BB'
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$destination = Join-Path $OutputDirectory 'libmpv-2.dll'
if ((Test-Path -LiteralPath $destination) -and
    (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash -ceq $dllHash) {
    Write-Output "Video player already verified: $destination"
    return
}
if (-not (Get-Command tar -ErrorAction SilentlyContinue)) { throw 'Windows tar is required to unpack the video player' }
$scratch = Join-Path $OutputDirectory ('.setup-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $scratch | Out-Null
# Keep a failed setup directory for diagnosis. Never delete a caller's archive.
if ($Archive) {
    $Archive = [IO.Path]::GetFullPath($Archive)
} else {
    $Archive = Join-Path $scratch 'mpv.7z'
    Write-Output 'Downloading the pinned video player (about 30 MiB)...'
    Invoke-WebRequest -Uri $download -OutFile $Archive
}
if ((Get-FileHash -LiteralPath $Archive -Algorithm SHA256).Hash -cne $archiveHash) { throw 'Video player archive SHA256 mismatch' }
& tar -xf $Archive -C $scratch libmpv-2.dll
if ($LASTEXITCODE -ne 0) { throw 'Cannot unpack video player' }
$candidate = Join-Path $scratch 'libmpv-2.dll'
if ((Get-FileHash -LiteralPath $candidate -Algorithm SHA256).Hash -cne $dllHash) { throw 'Video player DLL SHA256 mismatch' }
Move-Item -LiteralPath $candidate -Destination $destination -Force
if ((Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash -cne $dllHash) { throw 'Installed video player SHA256 mismatch' }
$receipt = [ordered]@{component='libmpv'; build='20261107-git-9028187538'; source=$download; archive_sha256=$archiveHash.ToLowerInvariant(); dll_sha256=$dllHash.ToLowerInvariant(); platform='windows-x64'; installed_utc=[DateTime]::UtcNow.ToString('o')}
$receipt | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $OutputDirectory 'video-player-runtime.json') -Encoding utf8
@'
Katran optional video player: libmpv Windows x64 build 20261107-git-9028187538.
Provided by shinchiro/mpv-winbuild-cmake, independently of Katran.
Binary release: https://github.com/shinchiro/mpv-winbuild-cmake/releases/tag/20261107
Build recipes/source pins: https://github.com/shinchiro/mpv-winbuild-cmake
mpv source: https://github.com/mpv-player/mpv
mpv Copyright/license: https://github.com/mpv-player/mpv/blob/master/Copyright
mpv GPL license: https://github.com/mpv-player/mpv/blob/master/LICENSE.GPL
mpv LGPL license: https://github.com/mpv-player/mpv/blob/master/LICENSE.LGPL
FFmpeg source/license: https://ffmpeg.org/legal.html
The DLL is not covered by Katran's license. Do not assume an LGPL-only build.
This setup downloads the upstream runtime separately. Public redistribution
requires the exact build's component/license/source compliance review.
Afio Rust/egui integration informed this adapter; Afio project code is MIT.
'@ | Set-Content -LiteralPath (Join-Path $OutputDirectory 'LIBMPV-NOTICE.txt') -Encoding utf8
Write-Output "Video player installed and hash-verified: $destination"
Write-Output 'Restart Katran to use the installed video player.'
