param([string]$RuntimeDirectory = 'build/runtime-candidate')
$ErrorActionPreference = 'Stop'
$stitchRoot = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $stitchRoot
try {
    $RuntimeDirectory = [IO.Path]::GetFullPath($RuntimeDirectory)
    $record = Get-Content -LiteralPath (Join-Path $RuntimeDirectory 'runtime-build-result.json') -Raw | ConvertFrom-Json
    $recipe = Get-Content -LiteralPath .github/runtime/build.json -Raw | ConvertFrom-Json
    if ($record.schemaVersion -ne 1 -or $record.status -ne 'captured-awaiting-windows-validation' -or $record.target -ne 'win-x64' -or
        $record.ffmpegCommit -ne $recipe.ffmpeg.commit -or $record.nvcodecCommit -ne $recipe.nvcodec.commit -or
        $record.buildScriptsCommit -ne $recipe.buildScripts.commit) {
        throw 'The candidate has not passed its source audit for the selected Windows x64 runtime.'
    }
    foreach ($file in @($record.archive, $record.correspondingSource, $record.resolvedSources)) {
        if ($file.fileName -notmatch '^[A-Za-z0-9_.-]+$' -or $file.sha256 -notmatch '^[A-Fa-f0-9]{64}$') {
            throw 'Invalid runtime candidate artifact record.'
        }
        $path = Join-Path $RuntimeDirectory $file.fileName
        if ((Get-Item -LiteralPath $path).Length -ne $file.sizeBytes -or (Get-FileHash -LiteralPath $path).Hash -ne $file.sha256) {
            throw "Runtime candidate hash or size mismatch: $($file.fileName)"
        }
    }
    $binaryNames = @($record.binaries.fileName | Sort-Object)
    if ($binaryNames.Count -ne 2 -or ($binaryNames -join '|') -ne 'ffmpeg.exe|ffprobe.exe') {
        throw 'The runtime candidate must identify exactly FFmpeg and FFprobe.'
    }
    cargo build --release --locked --target-dir build/target
    if ($LASTEXITCODE -ne 0) { throw 'The candidate application build failed.' }
    New-Item -ItemType Directory -Force -Path build/runtime,build/test-output | Out-Null
    Copy-Item -LiteralPath build/target/release/stitch.exe -Destination build/Stitch.exe
    Expand-Archive -LiteralPath (Join-Path $RuntimeDirectory $record.archive.fileName) -DestinationPath build/runtime -Force
    foreach ($binary in $record.binaries) {
        $path = Join-Path build/runtime $binary.fileName
        if ((Get-Item -LiteralPath $path).Length -ne $binary.sizeBytes -or (Get-FileHash -LiteralPath $path).Hash -ne $binary.sha256) {
            throw "Runtime binary hash or size mismatch: $($binary.fileName)"
        }
        & $path -version
        if ($LASTEXITCODE -ne 0) { throw "Runtime tool failed: $($binary.fileName)" }
    }
    $filters = & build/runtime/ffmpeg.exe -hide_banner -filters 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0) { throw 'Cannot inspect runtime filters.' }
    foreach ($name in @('zscale', 'tonemap', 'limiter', 'interleave')) {
        if ($filters -notmatch "\b$name\b") { throw "Required filter is missing: $name" }
    }
    $encoders = & build/runtime/ffmpeg.exe -hide_banner -encoders 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0) { throw 'Cannot inspect runtime encoders.' }
    foreach ($name in @('libx264', 'libx265', 'h264_nvenc', 'hevc_nvenc', 'aac')) {
        if ($encoders -notmatch "\b$name\b") { throw "Required encoder is missing: $name" }
    }
    cargo test --locked --test media_flow -- --ignored
    if ($LASTEXITCODE -ne 0) { throw 'The candidate media acceptance check failed.' }
} finally { Pop-Location }
