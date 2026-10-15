param([switch]$SkipDownload)
$ErrorActionPreference = 'Stop'
$stitchRoot = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $stitchRoot
try {
    $manifestPath = Join-Path $stitchRoot '.github\runtime\distribution.json'
    if (-not (Test-Path -LiteralPath $manifestPath)) { throw 'The verified runtime distribution manifest is missing.' }
    $record = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    $recipe = Get-Content -LiteralPath .github/runtime/build.json -Raw | ConvertFrom-Json
    if ($record.schemaVersion -ne 1 -or $record.status -ne 'verified' -or $record.target -ne 'win-x64' -or
        $record.ffmpegCommit -ne $recipe.ffmpeg.commit -or $record.nvcodecCommit -ne $recipe.nvcodec.commit -or
        -not $record.validationRunUrl -or -not $record.correspondingSource.url) {
        throw 'The runtime distribution manifest is not verified for Windows x64.'
    }
    $binaryNames = @($record.binaries.fileName | Sort-Object)
    if ($binaryNames.Count -ne 2 -or ($binaryNames -join '|') -ne 'ffmpeg.exe|ffprobe.exe') {
        throw 'The verified runtime must identify exactly FFmpeg and FFprobe.'
    }
    if ($record.archive.fileName -notmatch '^[A-Za-z0-9_.-]+\.zip$' -or
        $record.archive.sha256 -notmatch '^[A-Fa-f0-9]{64}$' -or $record.archive.sizeBytes -le 0 -or
        $record.archive.url -notmatch '^https://github\.com/MadMatt341/Stitch-Releases/releases/download/') {
        throw 'The verified runtime archive record is invalid.'
    }
    $cacheRoot = Join-Path $stitchRoot ".reader-data\runtime-$($record.archive.sha256.ToLowerInvariant())"
    New-Item -ItemType Directory -Force -Path $cacheRoot | Out-Null
    $archive = Join-Path $cacheRoot $record.archive.fileName
    if (-not (Test-Path -LiteralPath $archive)) {
        if ($SkipDownload) { throw 'The pinned FFmpeg archive is missing.' }
        Invoke-WebRequest -Uri $record.archive.url -OutFile $archive
    }
    if ((Get-Item -LiteralPath $archive).Length -ne $record.archive.sizeBytes -or
        (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $record.archive.sha256) {
        throw 'FFmpeg archive hash or size mismatch.'
    }
    $zip = [IO.Compression.ZipFile]::OpenRead($archive)
    try {
        $names = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
        $rootFiles = @('ffmpeg.exe','ffprobe.exe','FFmpeg-LICENSE.txt','FFmpeg-README.txt','LICENSES.json','THIRD-PARTY-NOTICES.txt')
        foreach ($entry in $zip.Entries) {
            $name = $entry.FullName.Replace('\','/')
            if ($name.StartsWith('./')) { $name = $name.Substring(2) }
            if (-not $name -or [IO.Path]::IsPathRooted($name) -or $name -match '(^|/)\.\.(/|$)|:') {
                throw 'Unsafe runtime archive path.'
            }
            if (-not $names.Add($name)) { throw "Duplicate runtime archive path: $name" }
            if (-not $name.EndsWith('/') -and $name -notin $rootFiles -and -not $name.StartsWith('licenses/')) {
                throw "Unexpected runtime archive entry: $name"
            }
        }
        foreach ($name in $rootFiles) { if (-not $names.Contains($name)) { throw "Runtime file is missing: $name" } }
    } finally { $zip.Dispose() }
    $runtime = Join-Path $cacheRoot "extracted-$([Guid]::NewGuid().ToString('N'))"
    Expand-Archive -LiteralPath $archive -DestinationPath $runtime
    foreach ($binary in $record.binaries) {
        $path = Join-Path $runtime $binary.fileName
        if ((Get-Item -LiteralPath $path).Length -ne $binary.sizeBytes -or
            (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $binary.sha256) {
            throw "Runtime binary hash or size mismatch: $($binary.fileName)"
        }
    }
    $licenses = @(Get-Content -LiteralPath (Join-Path $runtime 'LICENSES.json') -Raw | ConvertFrom-Json)
    if ($licenses.Count -eq 0) { throw 'Runtime dependency license notices are missing.' }
    foreach ($license in $licenses) {
        if (-not $license.path -or [IO.Path]::IsPathRooted($license.path) -or
            $license.path -match '(^|[\\/])\.\.([\\/]|$)|:') { throw 'Invalid runtime license path.' }
        $path = Join-Path (Join-Path $runtime 'licenses') $license.path
        if ((Get-Item -LiteralPath $path).Length -ne $license.sizeBytes -or
            (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $license.sha256) {
            throw "Runtime license hash or size mismatch: $($license.path)"
        }
    }
    cargo build --release --locked --target-dir build/target
    if ($LASTEXITCODE -ne 0) { throw 'Stitch build failed.' }
    $destination = Join-Path $stitchRoot 'build'
    $runtimeDestination = Join-Path $destination 'runtime'
    New-Item -ItemType Directory -Force -Path $runtimeDestination | Out-Null
    Copy-Item -LiteralPath (Join-Path $stitchRoot 'build\target\release\stitch.exe') -Destination (Join-Path $destination 'Stitch.exe')
    Get-ChildItem -LiteralPath $runtime -Force | Copy-Item -Destination $runtimeDestination -Recurse -Force
    Write-Output (Join-Path $destination 'Stitch.exe')
} finally { Pop-Location }
