Add-Type -AssemblyName System.Drawing
foreach ($dir in 'left', 'right') {
  $b = New-Object System.Drawing.Bitmap 144, 144
  $g = [System.Drawing.Graphics]::FromImage($b)
  $g.SmoothingMode = 'AntiAlias'
  $g.Clear([System.Drawing.Color]::Black)
  $pen = New-Object System.Drawing.Pen ([System.Drawing.Color]::White), 12
  $pen.StartCap = 'Round'; $pen.EndCap = 'Round'; $pen.LineJoin = 'Round'
  $s = if ($dir -eq 'left') { 1 } else { -1 }
  $cx = 72
  $g.DrawLine($pen, $cx + 34 * $s, 72, $cx - 34 * $s, 72)
  $g.DrawLines($pen, [System.Drawing.PointF[]]@(
    [System.Drawing.PointF]::new($cx - 6 * $s, 40), [System.Drawing.PointF]::new($cx - 38 * $s, 72), [System.Drawing.PointF]::new($cx - 6 * $s, 104)))
  $b.Save("$PSScriptRoot\ARROW_$($dir.ToUpper()).png", [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $b.Dispose()
}
