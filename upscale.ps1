<#
  Upscales every texture Vita3K has exported that doesn't have a good upscaled copy yet, with Upscayl's
  own upscaler (upscayl-bin, the engine behind the Upscayl app), into the import folder Vita3K loads
  replacements from.

    export\                                textures Vita3K dumps as you play (and your batch folders)
    import\upscayl_png_<model>_<scale>x\   the upscaled copies, same file names (the texture's hash)

  Run it once:            powershell -ExecutionPolicy Bypass -File upscale.ps1
  Keep it running while you play, upscaling each new texture as Vita3K dumps it:
                          powershell -ExecutionPolicy Bypass -File upscale.ps1 -Watch
  Another model:          ... -Model ultrasharp-4x   (any model in Upscayl's resources\models)

  Each texture is upscaled on its own (doing a whole folder at once can lose the GPU and mix up the
  results) and checked against the original: four times the size, with about the same colours and
  transparency. When the model fails, which happens with very small textures (a few pixels tall), the
  texture is resized smoothly instead. Empty or broken files already in the import folder are redone.

  Vita3K needs "Import textures" on for the game (and "Export textures" to keep collecting new ones).
  It reads the import folder when the game starts, so new upscales show up the next time you start it.
#>
param(
    [switch]$Watch,
    [string]$Model = 'digital-art-4x',
    [ValidateSet(2, 3, 4)][int]$Scale = 4,
    [int]$Every = 15
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$root = $PSScriptRoot
$export = Join-Path $root 'export'
$import = Join-Path $root 'import'
$out = Join-Path $import "upscayl_png_${Model}_${Scale}x"
$upscaylRoot = @("$env:ProgramFiles\Upscayl\resources", "$env:LOCALAPPDATA\Programs\Upscayl\resources") | Where-Object { Test-Path "$_\bin\upscayl-bin.exe" } | Select-Object -First 1
if (-not $upscaylRoot) { throw 'Upscayl was not found (looked in Program Files and AppData\Local\Programs).' }
$bin = Join-Path $upscaylRoot 'bin\upscayl-bin.exe'
$models = Join-Path $upscaylRoot 'models'
if (-not (Test-Path (Join-Path $models "$Model.param"))) { throw "Upscayl has no model called $Model (see $models)." }
New-Item -ItemType Directory -Force $out | Out-Null

# Size, average brightness and average opacity of an image, from a grid of samples; $null if it can't be read.
function Get-Look([string]$path) {
    try { $b = [System.Drawing.Bitmap]::FromFile($path) } catch { return $null }
    try {
        $n = 0; $rgb = 0.0; $alpha = 0.0
        $sx = [Math]::Max(1, [int]($b.Width / 24)); $sy = [Math]::Max(1, [int]($b.Height / 24))
        for ($y = 0; $y -lt $b.Height; $y += $sy) {
            for ($x = 0; $x -lt $b.Width; $x += $sx) {
                $c = $b.GetPixel($x, $y); $rgb += ($c.R + $c.G + $c.B) / 3; $alpha += $c.A; $n++
            }
        }
        return [pscustomobject]@{ W = $b.Width; H = $b.Height; Rgb = $rgb / $n; Alpha = $alpha / $n }
    } finally { $b.Dispose() }
}

# Whether an upscale looks like its original: the right size, similar colours and transparency.
function Test-Upscale([string]$upscaled, [string]$original) {
    $o = Get-Look $upscaled; $s = Get-Look $original
    if (-not $o -or -not $s) { return $false }
    return ($o.W -eq $s.W * $Scale) -and ($o.H -eq $s.H * $Scale) -and ([Math]::Abs($o.Alpha - $s.Alpha) -lt 25) -and ([Math]::Abs($o.Rgb - $s.Rgb) -lt 30)
}

# A plain, smooth resize (keeps transparency), for textures the model can't handle.
function Resize-Smooth([string]$from, [string]$to) {
    $src = [System.Drawing.Bitmap]::FromFile($from)
    try {
        $dst = New-Object System.Drawing.Bitmap ($src.Width * $Scale), ($src.Height * $Scale), ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
        $g = [System.Drawing.Graphics]::FromImage($dst)
        $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
        $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
        $g.CompositingMode = [System.Drawing.Drawing2D.CompositingMode]::SourceCopy
        $wrap = New-Object System.Drawing.Imaging.ImageAttributes
        $wrap.SetWrapMode([System.Drawing.Drawing2D.WrapMode]::TileFlipXY)
        $g.DrawImage($src, (New-Object System.Drawing.Rectangle 0, 0, $dst.Width, $dst.Height), 0, 0, $src.Width, $src.Height, [System.Drawing.GraphicsUnit]::Pixel, $wrap)
        $g.Dispose()
        $dst.Save($to, [System.Drawing.Imaging.ImageFormat]::Png)
        $dst.Dispose()
    } finally { $src.Dispose() }
}

# A finished upscale: a real PNG (not an empty or zeroed file left by a failed run).
function Test-Done([System.IO.FileInfo]$f) {
    if ($f.Length -lt 67) { return $false }
    $head = [byte[]]::new(8)
    $s = [System.IO.File]::OpenRead($f.FullName); try { [void]$s.Read($head, 0, 8) } finally { $s.Dispose() }
    return [BitConverter]::ToString($head) -eq '89-50-4E-47-0D-0A-1A-0A'
}

function Invoke-Pass {
    $done = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    Get-ChildItem $import -Recurse -File -Filter *.png | Where-Object { Test-Done $_ } | ForEach-Object { [void]$done.Add($_.BaseName) }
    $todo = [ordered]@{}
    Get-ChildItem $export -Recurse -File -Filter *.png | Where-Object { -not $done.Contains($_.BaseName) } | ForEach-Object {
        if (-not $todo.Contains($_.BaseName)) { $todo[$_.BaseName] = $_.FullName }
    }
    if ($todo.Count -eq 0) { return 0 }
    Write-Host ("{0:HH:mm:ss}  {1} texture(s) to upscale with {2} ({3}x)" -f (Get-Date), $todo.Count, $Model, $Scale)
    $ai = 0; $smooth = 0; $failed = 0; $i = 0
    foreach ($hash in $todo.Keys) {
        $i++
        $src = $todo[$hash]
        $dst = Join-Path $out "$hash.png"
        $tmp = Join-Path $env:TEMP "vita3k-upscale-$hash.png"
        Remove-Item $tmp -Force -ErrorAction SilentlyContinue
        # upscayl-bin writes its progress to the error stream; that isn't a failure.
        $ErrorActionPreference = 'Continue'
        & $bin -i $src -o $tmp -s $Scale -m $models -n $Model -f png -j 1:1:1 2>&1 | Out-Null
        $ErrorActionPreference = 'Stop'
        if ((Test-Path $tmp) -and (Test-Upscale $tmp $src)) {
            Move-Item $tmp $dst -Force; $ai++
        } else {
            Remove-Item $tmp -Force -ErrorAction SilentlyContinue
            try { Resize-Smooth $src $dst; $smooth++ } catch { $failed++; Write-Warning "Couldn't upscale $hash ($_)" }
        }
        if ($i % 25 -eq 0) { Write-Host ("{0:HH:mm:ss}  {1} of {2}" -f (Get-Date), $i, $todo.Count) }
    }
    Write-Host ("{0:HH:mm:ss}  Done: {1} with {2}, {3} resized smoothly (too small for the model), {4} failed" -f (Get-Date), $ai, $Model, $smooth, $failed)
    return $ai + $smooth
}

if ($Watch) {
    Write-Host "Watching $export for new textures (every $Every s). Ctrl+C to stop."
    while ($true) { [void](Invoke-Pass); Start-Sleep -Seconds $Every }
} else {
    $n = Invoke-Pass
    if ($n -eq 0) { Write-Host 'Every exported texture already has an upscaled copy.' }
}
