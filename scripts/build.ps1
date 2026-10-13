param([switch]$SkipDownload)
$ErrorActionPreference = 'Stop'
$stitchRoot = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $stitchRoot
try {
    $archive = Join-Path $stitchRoot '.reader-data\ffmpeg-9.0.2.7z'
    $runtimeRoot = Join-Path $stitchRoot '.reader-data\runtime'
    $runtime = Join-Path $runtimeRoot 'ffmpeg-9.0.2-essentials_build'
    $expected = '4705843CCAAF54257C16AD90F3E952ECE33C17DF964ECF7BFDBB0F49C7171077'
    New-Item -ItemType Directory -Force -Path $runtimeRoot | Out-Null
    if (-not (Test-Path -LiteralPath $archive)) {
        if ($SkipDownload) { throw 'The pinned FFmpeg archive is missing.' }
        Invoke-WebRequest -Uri 'https://www.gyan.dev/ffmpeg/builds/packages/ffmpeg-9.0.2-essentials_build.7z' -OutFile $archive
    }
    if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $expected) { throw 'FFmpeg archive hash mismatch.' }
    # Extract the verified archive on each build. Do not trust a previous extraction.
    tar -xf $archive -C $runtimeRoot
    if ($LASTEXITCODE -ne 0) { throw 'Cannot extract FFmpeg.' }
    cargo build --release --locked --target-dir build/target
    if ($LASTEXITCODE -ne 0) { throw 'Stitch build failed.' }
    $destination = Join-Path $stitchRoot 'build'
    New-Item -ItemType Directory -Force -Path (Join-Path $destination 'runtime') | Out-Null
    Copy-Item -LiteralPath (Join-Path $stitchRoot 'build\target\release\stitch.exe') -Destination (Join-Path $destination 'Stitch.exe')
    foreach ($name in @('ffmpeg.exe','ffprobe.exe')) {
        Copy-Item -LiteralPath (Join-Path $runtime "bin\$name") -Destination (Join-Path $destination "runtime\$name")
    }
    Copy-Item -LiteralPath (Join-Path $runtime 'LICENSE') -Destination (Join-Path $destination 'runtime\FFmpeg-LICENSE.txt')
    Copy-Item -LiteralPath (Join-Path $runtime 'README.txt') -Destination (Join-Path $destination 'runtime\FFmpeg-README.txt')
    Write-Output (Join-Path $destination 'Stitch.exe')
} finally { Pop-Location }
