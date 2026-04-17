Add-Type -AssemblyName System.Drawing

function New-RoundedRectPath {
    param(
        [float]$X,
        [float]$Y,
        [float]$Width,
        [float]$Height,
        [float]$Radius
    )

    $path = New-Object System.Drawing.Drawing2D.GraphicsPath
    $diameter = $Radius * 2

    $path.AddArc($X, $Y, $diameter, $diameter, 180, 90)
    $path.AddArc($X + $Width - $diameter, $Y, $diameter, $diameter, 270, 90)
    $path.AddArc($X + $Width - $diameter, $Y + $Height - $diameter, $diameter, $diameter, 0, 90)
    $path.AddArc($X, $Y + $Height - $diameter, $diameter, $diameter, 90, 90)
    $path.CloseFigure()
    return $path
}

function Draw-BrandIcon {
    param(
        [int]$Size,
        [string]$OutputPath
    )

    $bmp = New-Object System.Drawing.Bitmap $Size, $Size, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $g.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
    $g.Clear([System.Drawing.Color]::Transparent)

    $pad = [float]($Size * 0.08)
    $cardRadius = [float]($Size * 0.24)
    $card = New-RoundedRectPath $pad $pad ($Size - $pad * 2) ($Size - $pad * 2) $cardRadius

    $gradRect = New-Object System.Drawing.RectangleF $pad, $pad, ($Size - $pad * 2), ($Size - $pad * 2)
    $grad = New-Object System.Drawing.Drawing2D.LinearGradientBrush `
        (New-Object System.Drawing.PointF ($pad), ($pad)), `
        (New-Object System.Drawing.PointF ($Size - $pad), ($Size - $pad)), `
        ([System.Drawing.Color]::FromArgb(255, 255, 165, 76)), `
        ([System.Drawing.Color]::FromArgb(255, 255, 94, 58))
    $g.FillPath($grad, $card)

    $highlight = New-Object System.Drawing.Drawing2D.PathGradientBrush @($card)
    $highlight.CenterColor = [System.Drawing.Color]::FromArgb(92, 255, 241, 220)
    $highlight.SurroundColors = @([System.Drawing.Color]::FromArgb(0, 255, 241, 220))
    $g.FillPath($highlight, $card)

    $cardPen = New-Object System.Drawing.Pen ([System.Drawing.Color]::FromArgb(96, 255, 247, 237)), ([float]($Size * 0.016))
    $g.DrawPath($cardPen, $card)

    $capsuleX = [float]($Size * 0.19)
    $capsuleY = [float]($Size * 0.31)
    $capsuleW = [float]($Size * 0.62)
    $capsuleH = [float]($Size * 0.38)
    $capsule = New-RoundedRectPath $capsuleX $capsuleY $capsuleW $capsuleH ([float]($capsuleH * 0.52))
    $capsuleBrush = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 34, 25, 19))
    $capsulePen = New-Object System.Drawing.Pen ([System.Drawing.Color]::FromArgb(88, 255, 232, 213)), ([float]($Size * 0.01))
    $g.FillPath($capsuleBrush, $capsule)
    $g.DrawPath($capsulePen, $capsule)

    $statusSize = [float]($Size * 0.12)
    $statusX = [float]($Size * 0.70)
    $statusY = [float]($Size * 0.20)
    $statusBrush = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 77, 217, 255))
    $statusGlow = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(90, 77, 217, 255))
    $g.FillEllipse($statusGlow, $statusX - ($statusSize * 0.18), $statusY - ($statusSize * 0.18), $statusSize * 1.36, $statusSize * 1.36)
    $g.FillEllipse($statusBrush, $statusX, $statusY, $statusSize, $statusSize)

    $bodyBrush = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 255, 233, 212))
    $bodyPen = New-Object System.Drawing.Pen ([System.Drawing.Color]::FromArgb(255, 255, 233, 212)), ([float]($Size * 0.034))
    $bodyPen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
    $bodyPen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
    $bodyPen.LineJoin = [System.Drawing.Drawing2D.LineJoin]::Round

    $cx = [float]($Size * 0.50)
    $cy = [float]($Size * 0.50)
    $bodyW = [float]($Size * 0.12)
    $bodyH = [float]($Size * 0.17)
    $g.FillEllipse($bodyBrush, $cx - ($bodyW / 2), $cy - ($bodyH / 2), $bodyW, $bodyH)

    $g.DrawLine($bodyPen, ($cx - ($Size * 0.028)), ($cy + ($Size * 0.09)), $cx, ($cy + ($Size * 0.135)))
    $g.DrawLine($bodyPen, $cx, ($cy + ($Size * 0.135)), ($cx + ($Size * 0.028)), ($cy + ($Size * 0.09)))

    $g.DrawLine($bodyPen, $cx, ($cy - ($Size * 0.12)), ($cx - ($Size * 0.045)), ($cy - ($Size * 0.19)))
    $g.DrawLine($bodyPen, $cx, ($cy - ($Size * 0.12)), ($cx + ($Size * 0.045)), ($cy - ($Size * 0.19)))
    $g.FillEllipse($bodyBrush, $cx - ($Size * 0.055), ($cy - ($Size * 0.155)), ($Size * 0.02), ($Size * 0.02))
    $g.FillEllipse($bodyBrush, $cx + ($Size * 0.035), ($cy - ($Size * 0.155)), ($Size * 0.02), ($Size * 0.02))

    $g.DrawBezier(
        $bodyPen,
        ($cx - ($Size * 0.05)), ($cy - ($Size * 0.01)),
        ($cx - ($Size * 0.12)), ($cy - ($Size * 0.05)),
        ($cx - ($Size * 0.17)), ($cy - ($Size * 0.09)),
        ($cx - ($Size * 0.18)), ($cy - ($Size * 0.14))
    )
    $g.DrawBezier(
        $bodyPen,
        ($cx + ($Size * 0.05)), ($cy - ($Size * 0.01)),
        ($cx + ($Size * 0.12)), ($cy - ($Size * 0.05)),
        ($cx + ($Size * 0.17)), ($cy - ($Size * 0.09)),
        ($cx + ($Size * 0.18)), ($cy - ($Size * 0.14))
    )

    $clawPen = New-Object System.Drawing.Pen ([System.Drawing.Color]::FromArgb(255, 255, 233, 212)), ([float]($Size * 0.026))
    $clawPen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
    $clawPen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
    $clawPen.LineJoin = [System.Drawing.Drawing2D.LineJoin]::Round

    $g.DrawArc($clawPen, ($cx - ($Size * 0.245)), ($cy - ($Size * 0.18)), ($Size * 0.10), ($Size * 0.10), 320, 190)
    $g.DrawArc($clawPen, ($cx - ($Size * 0.22)), ($cy - ($Size * 0.14)), ($Size * 0.09), ($Size * 0.09), 210, 145)
    $g.DrawArc($clawPen, ($cx + ($Size * 0.145)), ($cy - ($Size * 0.18)), ($Size * 0.10), ($Size * 0.10), 30, 190)
    $g.DrawArc($clawPen, ($cx + ($Size * 0.13)), ($cy - ($Size * 0.14)), ($Size * 0.09), ($Size * 0.09), 185, 145)

    foreach ($offset in -0.05, 0.0, 0.05) {
        $g.DrawLine(
            $bodyPen,
            ($cx - ($Size * 0.06)),
            ($cy + ($Size * $offset)),
            ($cx + ($Size * 0.06)),
            ($cy + ($Size * $offset))
        )
    }

    $bmp.Save($OutputPath, [System.Drawing.Imaging.ImageFormat]::Png)

    $clawPen.Dispose()
    $bodyPen.Dispose()
    $bodyBrush.Dispose()
    $statusBrush.Dispose()
    $statusGlow.Dispose()
    $capsulePen.Dispose()
    $capsuleBrush.Dispose()
    $cardPen.Dispose()
    $highlight.Dispose()
    $grad.Dispose()
    $card.Dispose()
    $capsule.Dispose()
    $g.Dispose()
    $bmp.Dispose()
}

$root = Split-Path -Parent $PSScriptRoot
$iconsDir = Join-Path $root "src-tauri\icons"
$docsDir = Join-Path $root "docs"
$tmpDir = Join-Path $root ".tmp-brand"

New-Item -ItemType Directory -Force -Path $iconsDir, $docsDir, $tmpDir | Out-Null

$targets = @(
    @{ Size = 1024; Path = (Join-Path $iconsDir "icon.png") },
    @{ Size = 256; Path = (Join-Path $iconsDir "128x128@2x.png") },
    @{ Size = 128; Path = (Join-Path $iconsDir "128x128.png") },
    @{ Size = 64; Path = (Join-Path $iconsDir "64x64.png") },
    @{ Size = 32; Path = (Join-Path $iconsDir "32x32.png") },
    @{ Size = 310; Path = (Join-Path $iconsDir "Square310x310Logo.png") },
    @{ Size = 284; Path = (Join-Path $iconsDir "Square284x284Logo.png") },
    @{ Size = 150; Path = (Join-Path $iconsDir "Square150x150Logo.png") },
    @{ Size = 142; Path = (Join-Path $iconsDir "Square142x142Logo.png") },
    @{ Size = 107; Path = (Join-Path $iconsDir "Square107x107Logo.png") },
    @{ Size = 89; Path = (Join-Path $iconsDir "Square89x89Logo.png") },
    @{ Size = 71; Path = (Join-Path $iconsDir "Square71x71Logo.png") },
    @{ Size = 44; Path = (Join-Path $iconsDir "Square44x44Logo.png") },
    @{ Size = 30; Path = (Join-Path $iconsDir "Square30x30Logo.png") },
    @{ Size = 50; Path = (Join-Path $iconsDir "StoreLogo.png") },
    @{ Size = 256; Path = (Join-Path $docsDir "brand-logo.png") }
)

foreach ($target in $targets) {
    Draw-BrandIcon -Size $target.Size -OutputPath $target.Path
}

$icoSource = Join-Path $iconsDir "128x128@2x.png"
$icoPath = Join-Path $iconsDir "icon.ico"
ffmpeg -y -i $icoSource $icoPath | Out-Null

$npxCommand = Get-Command npx -ErrorAction SilentlyContinue
if ($null -eq $npxCommand) {
    Write-Warning "找不到 npx，已更新 PNG / ICO / docs logo，但未重生 ICNS、iOS、Android 圖示。"
} else {
    & $npxCommand.Source "-y" "@tauri-apps/cli" "icon" (Join-Path $iconsDir "icon.png") "-o" $iconsDir "--ios-color" "#fff4ea"
    if ($LASTEXITCODE -ne 0) {
        throw "Tauri icon generator 執行失敗，請確認 Node.js / npx 可正常使用。"
    }
}

Remove-Item -Recurse -Force $tmpDir -ErrorAction SilentlyContinue
