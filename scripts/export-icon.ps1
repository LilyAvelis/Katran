param([string]$Root = (Join-Path $PSScriptRoot '..'))
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$branding = Join-Path ([IO.Path]::GetFullPath($Root)) 'assets/branding'
$source = [Drawing.Image]::FromFile((Join-Path $branding 'katran-logo.png'))
try {
    if ($source.Width -ne $source.Height) { throw 'Katran logo must be square' }
    function Export-Png([int]$edge) {
        $bitmap = [Drawing.Bitmap]::new($edge, $edge, [Drawing.Imaging.PixelFormat]::Format32bppArgb)
        try {
            $graphics = [Drawing.Graphics]::FromImage($bitmap)
            try {
                # The supplied artwork fills its canvas; use its edge color so
                # high-quality downscaling cannot introduce translucent corners.
                $graphics.Clear($source.GetPixel(0, 0))
                $graphics.InterpolationMode = [Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
                $graphics.PixelOffsetMode = [Drawing.Drawing2D.PixelOffsetMode]::HighQuality
                $graphics.CompositingQuality = [Drawing.Drawing2D.CompositingQuality]::HighQuality
                $graphics.DrawImage($source, 0, 0, $edge, $edge)
            } finally { $graphics.Dispose() }
            $stream = [IO.MemoryStream]::new()
            try {
                $bitmap.Save($stream, [Drawing.Imaging.ImageFormat]::Png)
                return ,$stream.ToArray()
            } finally { $stream.Dispose() }
        } finally { $bitmap.Dispose() }
    }
    $edges = @(16,20,24,32,40,48,64,96,128,256)
    $images = @($edges | ForEach-Object { Export-Png $_ })
    [IO.File]::WriteAllBytes((Join-Path $branding 'katran-icon.png'), $images[-1])
    $stream = [IO.MemoryStream]::new()
    $writer = [IO.BinaryWriter]::new($stream)
    try {
        $writer.Write([uint16]0)
        $writer.Write([uint16]1)
        $writer.Write([uint16]$edges.Count)
        $offset = 6 + 16 * $edges.Count
        for ($index = 0; $index -lt $edges.Count; $index++) {
            $edge = $edges[$index]
            $data = $images[$index]
            $writer.Write([byte]($edge % 256))
            $writer.Write([byte]($edge % 256))
            $writer.Write([byte]0)
            $writer.Write([byte]0)
            $writer.Write([uint16]1)
            $writer.Write([uint16]32)
            $writer.Write([uint32]$data.Length)
            $writer.Write([uint32]$offset)
            $offset += $data.Length
        }
        foreach ($data in $images) { $writer.Write([byte[]]$data) }
        [IO.File]::WriteAllBytes((Join-Path $branding 'katran.ico'), $stream.ToArray())
    } finally { $writer.Dispose(); $stream.Dispose() }
} finally { $source.Dispose() }
Write-Output 'Exported Katran PNG and Windows ICO from katran-logo.png'
