$ErrorActionPreference = 'Stop'
$target = [IO.Path]::GetFullPath($env:TEMP)
$profile = Join-Path $target ('katran-cuda-retry-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $profile | Out-Null
try {
    $legacy = Join-Path $profile 'folder-ai-gpu-runtime'
    New-Item -ItemType Directory -Path $legacy | Out-Null
    $canary = Join-Path $legacy 'keep.txt'
    [IO.File]::WriteAllText($canary, 'original')
    $shim = Join-Path $profile 'fixture-python.exe'
    Add-Type -OutputAssembly $shim -OutputType ConsoleApplication -TypeDefinition @'
using System;
using System.IO;
using System.Reflection;
public static class FixturePython {
    public static int Main(string[] args) {
        if (args[1] == "venv") {
            string scripts = Path.Combine(args[2], "Scripts");
            Directory.CreateDirectory(scripts);
            if (File.Exists(Path.Combine(scripts, "python.exe"))) return 3;
            File.Copy(Assembly.GetExecutingAssembly().Location, Path.Combine(scripts, "python.exe"));
            return 0;
        }
        if (Array.IndexOf(args, "onnxruntime-gpu==1.23.2") >= 0) {
            string marker = Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "synthetic-partial.txt");
            if (!File.Exists(marker)) {
                File.WriteAllText(marker, "existing work");
                return 2;
            }
        }
        return 0;
    }
}
'@
    $setup = Join-Path $PSScriptRoot '../folder-semantic/setup-gpu.ps1'
    $runtime = Join-Path $profile 'folder-ai-gpu-runtime-owned'
    $manifest = Join-Path $runtime 'manifest.json'
    try {
        & $setup -Profile $profile -Python $shim
        throw 'Expected the first package installation to fail'
    } catch {
        if ($_.Exception.Message -notmatch 'Folder GPU package installation failed') { throw }
    }
    if (Test-Path -LiteralPath $manifest) { throw 'An interrupted install published its manifest' }
    & $setup -Profile $profile -Python $shim
    if (-not (Test-Path -LiteralPath $manifest -PathType Leaf)) { throw 'Retry did not publish its manifest' }
    if ([IO.File]::ReadAllText($canary) -ne 'original' -or @(Get-ChildItem -LiteralPath $legacy -Force).Count -ne 1) {
        throw 'Legacy partial installation was modified'
    }
    if ([IO.File]::ReadAllText((Join-Path $runtime '.venv/Scripts/synthetic-partial.txt')) -ne 'existing work') {
        throw 'Retry did not preserve the original environment'
    }
    Write-Output 'Folder CUDA retry passed: partial environment reused and legacy directory preserved.'
} finally {
    $resolved = [IO.Path]::GetFullPath($profile)
    if ((Split-Path -Parent $resolved) -ieq $target -and
        (Split-Path -Leaf $resolved) -match '^katran-cuda-retry-[a-f0-9]{32}$') {
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
