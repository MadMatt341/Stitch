param(
    [Parameter(Mandatory)][string]$RuntimeDirectory,
    [string]$RuntimeManifest,
    [string]$Version = '0.1.0-beta',
    [string]$OutputDirectory
)
$ErrorActionPreference = 'Stop'
$stitchRoot = Split-Path -Parent $PSScriptRoot
if (-not $RuntimeManifest) { $RuntimeManifest = Join-Path $stitchRoot '.github\runtime\distribution.json' }
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $stitchRoot "build\releases\v$Version" }
if ($Version -notmatch '^\d+\.\d+\.\d+(?:-[a-z0-9.-]+)?$') { throw 'Invalid release version.' }

function Assert-StitchRelativePath([string]$Path) {
    if (-not $Path -or [IO.Path]::IsPathRooted($Path) -or $Path -match '(^|[\\/])\.\.([\\/]|$)|:') {
        throw "Unsafe package path: $Path"
    }
}
function Assert-StitchRecord([string]$Directory, $Record) {
    Assert-StitchRelativePath $Record.fileName
    if ($Record.fileName -match '[\\/]') { throw 'An artifact filename must not contain directories.' }
    $path = Join-Path $Directory $Record.fileName
    if ((Get-Item -LiteralPath $path).Length -ne $Record.sizeBytes -or
        (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $Record.sha256) {
        throw "Artifact hash or size mismatch: $($Record.fileName)"
    }
    return $path
}
function Get-StitchRecord([string]$Path) {
    return [ordered]@{
        fileName = [IO.Path]::GetFileName($Path)
        sizeBytes = (Get-Item -LiteralPath $Path).Length
        sha256 = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash
    }
}

Push-Location -LiteralPath $stitchRoot
try {
    if (-not (Test-Path -LiteralPath $RuntimeManifest)) { throw 'The verified runtime distribution manifest is missing.' }
    $record = Get-Content -LiteralPath $RuntimeManifest -Raw | ConvertFrom-Json
    $recipe = Get-Content -LiteralPath .github\runtime\build.json -Raw | ConvertFrom-Json
    if ($record.schemaVersion -ne 1 -or $record.status -ne 'verified' -or $record.target -ne 'win-x64' -or
        $record.ffmpegCommit -ne $recipe.ffmpeg.commit -or $record.nvcodecCommit -ne $recipe.nvcodec.commit) {
        throw 'The runtime manifest does not identify the verified selected runtime.'
    }
    if (-not $record.validationRunUrl -or -not $record.correspondingSource.url) {
        throw 'The runtime must have a successful validation record and a public corresponding source URL.'
    }
    $dirty = @(git status --porcelain)
    if ($LASTEXITCODE -ne 0 -or $dirty.Count -ne 0) { throw 'Commit all selected source changes before packaging.' }
    $commit = git rev-parse HEAD
    if ($LASTEXITCODE -ne 0) { throw 'Cannot identify the application source revision.' }
    $upstream = git rev-parse origin/main
    if ($LASTEXITCODE -ne 0 -or $commit -ne $upstream) { throw 'Push the intended application revision before packaging.' }
    $RuntimeDirectory = [IO.Path]::GetFullPath($RuntimeDirectory)
    $OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
    $allowedOutput = [IO.Path]::GetFullPath((Join-Path $stitchRoot 'build')) + [IO.Path]::DirectorySeparatorChar
    if (-not $OutputDirectory.StartsWith($allowedOutput, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Release output must stay under the project build folder.'
    }
    if (Test-Path -LiteralPath $OutputDirectory) {
        if (@(Get-ChildItem -LiteralPath $OutputDirectory -Force).Count -ne 0) { throw 'Use an empty release output directory.' }
    }
    $archive = Assert-StitchRecord $RuntimeDirectory $record.archive
    $sourceArchive = Assert-StitchRecord $RuntimeDirectory $record.correspondingSource
    $inventory = Assert-StitchRecord $RuntimeDirectory $record.resolvedSources
    $binaryNames = @($record.binaries.fileName | Sort-Object)
    if ($binaryNames.Count -ne 2 -or ($binaryNames -join '|') -ne 'ffmpeg.exe|ffprobe.exe') {
        throw 'The runtime must contain exactly FFmpeg and FFprobe.'
    }
    $rootFiles = @('ffmpeg.exe','ffprobe.exe','FFmpeg-LICENSE.txt','FFmpeg-README.txt','LICENSES.json','THIRD-PARTY-NOTICES.txt')
    $zip = [IO.Compression.ZipFile]::OpenRead($archive)
    try {
        $names = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
        foreach ($entry in $zip.Entries) {
            $name = $entry.FullName.Replace('\','/')
            if ($name.StartsWith('./')) { $name = $name.Substring(2) }
            Assert-StitchRelativePath $name
            if (-not $names.Add($name)) { throw "Duplicate Windows archive path: $name" }
            if ($name.EndsWith('/')) { continue }
            if ($name -notin $rootFiles -and -not $name.StartsWith('licenses/')) {
                throw "Unexpected runtime archive entry: $name"
            }
        }
        foreach ($name in $rootFiles) { if (-not $names.Contains($name)) { throw "Runtime file is missing: $name" } }
    } finally { $zip.Dispose() }

    $rustCompilerVersion = rustc --version
    if ($LASTEXITCODE -ne 0 -or $rustCompilerVersion -notmatch '^rustc 1\.98\.1\s') {
        throw 'Release packaging requires the specified Rust 1.98.1 toolchain.'
    }
    $rustSysroot = rustc --print sysroot
    if ($LASTEXITCODE -ne 0) { throw 'Cannot locate the Rust standard library notice.' }
    $rustLibraryNotice = Join-Path $rustSysroot 'share\doc\rust\COPYRIGHT-library.html'
    if (-not (Test-Path -LiteralPath $rustLibraryNotice -PathType Leaf)) {
        throw 'The Rust standard library copyright and license notice is missing.'
    }
    cargo build --release --locked --target-dir build/target
    if ($LASTEXITCODE -ne 0) { throw 'The release application build failed.' }
    $metadataText = cargo metadata --locked --offline --format-version 1
    if ($LASTEXITCODE -ne 0) { throw 'Cannot inspect locked Rust dependency licenses.' }
    $metadata = ($metadataText -join "`n") | ConvertFrom-Json
    $releaseName = "Stitch-v$Version-win-x64"
    $stage = Join-Path $OutputDirectory $releaseName
    $runtimeStage = Join-Path $stage 'runtime'
    New-Item -ItemType Directory -Force -Path $runtimeStage | Out-Null
    Expand-Archive -LiteralPath $archive -DestinationPath $runtimeStage
    foreach ($binary in $record.binaries) { Assert-StitchRecord $runtimeStage $binary | Out-Null }
    $licenses = @(Get-Content -LiteralPath (Join-Path $runtimeStage 'LICENSES.json') -Raw | ConvertFrom-Json)
    if ($licenses.Count -eq 0) { throw 'Runtime dependency license notices are missing.' }
    foreach ($license in $licenses) {
        Assert-StitchRelativePath $license.path
        $path = Join-Path (Join-Path $runtimeStage 'licenses') $license.path
        if ((Get-Item -LiteralPath $path).Length -ne $license.sizeBytes -or
            (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $license.sha256) {
            throw "Runtime license notice differs from its source record: $($license.path)"
        }
    }
    Copy-Item -LiteralPath build\target\release\stitch.exe -Destination (Join-Path $stage 'Stitch.exe')
    Copy-Item -LiteralPath LICENSE -Destination (Join-Path $stage 'LICENSE')
    $instructions = (Get-Content -LiteralPath docs\install.txt -Raw).Replace('@VERSION@', $Version)
    [IO.File]::WriteAllText((Join-Path $stage 'README.txt'), $instructions, [Text.UTF8Encoding]::new($false))
    $notices = [Collections.Generic.List[string]]::new()
    $notices.Add("Stitch $Version third-party notices")
    $notices.Add("Application source: https://github.com/MadMatt341/Stitch/tree/$commit")
    $notices.Add('The application uses MIT. The separate FFmpeg runtime uses GPL-3.0-or-later.')
    $notices.Add("Matching runtime source: $($record.correspondingSource.url)")
    $notices.Add('Runtime dependency notices are in runtime/licenses. Each dependency retains its license.')
    $notices.Add('Rust dependency notices follow. Their complete license files are in licenses/Rust.')
    $rustLibraryDirectory = Join-Path $stage 'licenses\Rust\standard-library'
    New-Item -ItemType Directory -Force -Path $rustLibraryDirectory | Out-Null
    Copy-Item -LiteralPath $rustLibraryNotice -Destination $rustLibraryDirectory
    $notices.Add('Rust standard library 1.98.1: licenses/Rust/standard-library/COPYRIGHT-library.html contains its copyright and license notices.')
    foreach ($package in $metadata.packages | Where-Object { $_.name -ne 'stitch' } | Sort-Object name,version) {
        $packageRoot = Split-Path -Parent $package.manifest_path
        $noticeFiles = @(Get-ChildItem -LiteralPath $packageRoot -File | Where-Object {
            $_.Name -match '^(LICEN[SC]E|COPYING|NOTICE|COPYRIGHT)([._-].*)?$'
        })
        if ($package.license_file) {
            Assert-StitchRelativePath $package.license_file
            $noticeFiles += Get-Item -LiteralPath (Join-Path $packageRoot $package.license_file)
        }
        $noticeFiles = @($noticeFiles | Sort-Object FullName -Unique)
        if ($noticeFiles.Count -eq 0) { throw "No license files found for $($package.name)." }
        $destination = Join-Path $stage "licenses\Rust\$($package.name)-$($package.version)"
        New-Item -ItemType Directory -Force -Path $destination | Out-Null
        foreach ($file in $noticeFiles) { Copy-Item -LiteralPath $file.FullName -Destination $destination }
        $notices.Add("$($package.name) $($package.version): $($package.license)")
    }
    [IO.File]::WriteAllLines((Join-Path $stage 'THIRD-PARTY-NOTICES.txt'), $notices, [Text.UTF8Encoding]::new($false))
    $portableZip = Join-Path $OutputDirectory "$releaseName.zip"
    [IO.Compression.ZipFile]::CreateFromDirectory($stage, $portableZip, [IO.Compression.CompressionLevel]::Optimal, $true)
    $applicationSource = Join-Path $OutputDirectory "Stitch-v$Version-source.zip"
    git archive --format=zip "--prefix=Stitch-v$Version-source/" "--output=$applicationSource" HEAD
    if ($LASTEXITCODE -ne 0) { throw 'Cannot make the application source archive.' }
    Copy-Item -LiteralPath $sourceArchive,$inventory -Destination $OutputDirectory
    $runtimeRecord = Join-Path $OutputDirectory 'Stitch-runtime.json'
    Copy-Item -LiteralPath $RuntimeManifest -Destination $runtimeRecord
    $assets = @($portableZip,$applicationSource,(Join-Path $OutputDirectory $record.correspondingSource.fileName),
        (Join-Path $OutputDirectory $record.resolvedSources.fileName),$runtimeRecord)
    $releaseRecord = [ordered]@{
        schemaVersion = 1
        applicationVersion = $Version
        applicationCommit = $commit
        applicationLicense = 'MIT'
        target = 'win-x64'
        applicationBinary = Get-StitchRecord (Join-Path $stage 'Stitch.exe')
        rustCompiler = $rustCompilerVersion
        rustStandardLibraryNotice = Get-StitchRecord (Join-Path $rustLibraryDirectory 'COPYRIGHT-library.html')
        runtime = $record
        assets = @($assets | ForEach-Object { Get-StitchRecord $_ })
    }
    $releaseManifest = Join-Path $OutputDirectory "Stitch-v$Version-manifest.json"
    [IO.File]::WriteAllText($releaseManifest, ($releaseRecord | ConvertTo-Json -Depth 12) + "`n", [Text.UTF8Encoding]::new($false))
    $assets += $releaseManifest
    $hashes = @($assets | ForEach-Object { "$( (Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant() )  $([IO.Path]::GetFileName($_))" })
    [IO.File]::WriteAllLines((Join-Path $OutputDirectory 'SHA256SUMS.txt'), $hashes, [Text.UTF8Encoding]::new($false))
    Write-Output $portableZip
} finally { Pop-Location }
