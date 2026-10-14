# Public downloads

The application name is Stitch.
The executable name is `Stitch.exe`.
The public source repository is `MadMatt341/Stitch`.
The public download repository is `MadMatt341/Stitch-Releases`.

The public source history starts with the first release preparation snapshot.
The user selected this boundary to keep personal author metadata from the previous nine commits private.
The previous development history stays in the private `MadMatt341/Stitch-Development` repository.
Subsequent application source changes use the public repository.

## Execution policy

Run builds, checks, runtime compilation, packaging, and release uploads locally.
The user requires this project to operate without GitHub Actions.
Actions are disabled in the active `MadMatt341/Stitch` source repository.
GitHub remains the host for Git source and public download files.
Historical Actions links identify completed checks for earlier releases.

## Licenses

The user selected MIT for the application code and its documentation.
The root `LICENSE` file gives these terms.
MIT permits closed-source changes and commercial distribution with the required license notice.
The selected policy gives priority to broad reuse.

The application starts the FFmpeg tools as separate processes.
It does not link FFmpeg libraries into the application.
The bundled FFmpeg runtime keeps GPL-3.0-or-later and each dependency's license.
Its corresponding source archive must accompany the binary download.
The source capture scripts in `.github/runtime/` keep GPL-2.0-or-later from the TVC procedure.
Their headers and `.github/runtime/COPYING` give these terms.

The portable package also includes the Rust dependency license files.
It keeps the unchanged standard library notice from the specified Rust toolchain in `licenses/Rust/standard-library/COPYRIGHT-library.html`.
The release manifest records the compiler version and that notice's hash.

## Selected release scope

The user selected a complete portable ZIP for Windows x64.
The ZIP must contain the executable, its runtime, licenses, notices, and instructions.
Users extract the complete folder before they start `Stitch.exe`.
The executable alone does not contain the required FFmpeg tools.

The final release must use the completed, checked, committed, and pushed application revision.
The first release is a beta with recorded acceptance limits.
Keep generated release output under `build/`.
Do not copy the complete build folder into a release.

## Runtime sources

The current development runtime comes from Gyan's FFmpeg 9.0.2 archive.
Its notices identify the FFmpeg revision.
They do not establish the complete matching source for its external libraries.
This runtime is not an approved input for a public bundled release.
The [development reference](development.md#runtime-provenance) owns the current development pin.

The user selected a verified runtime before publication.
`.github/runtime/build.json` owns the candidate build inputs.
The candidate keeps the FFmpeg 9.0.2 source revision.
It pins NVIDIA API 13.0 headers to permit encoding with the selected driver baseline.
It uses TVC's specified build-script revision and a specified upstream build container.
The manifest also pins the remaining native Git dependencies from TVC's verified source inventory.

Six source records include local patch commits.
The manifest pins their upstream parents and expected patched trees.
The build keeps the specified recipe patches and verifies the resulting trees for compiled sources.
Each selected recipe that uses `git am` must have an expected patched tree before compilation.
Unused sources stay at their upstream parents.

After cleanup, source checks compare Git revisions and trees directly.
These checks do not depend on temporary build markers.
The build makes the complete binary and source records before this final audit.
If this audit fails, the build keeps those records for examination with status `captured-awaiting-source-audit`.
Only a passing audit permits status `captured-awaiting-windows-validation` and the Windows media check.

Fontconfig uses its official GitHub mirror at the same source revision.
The build script downloads all specified Git sources before it verifies their revisions.
This includes compiler sources and optional packages excluded from the upstream download group.
Fixed source commits replace the upstream branch reset rules.

The build makes `ffmpeg.exe` and `ffprobe.exe` for generic Windows x64 processors.
Its feature configuration comes from those build scripts.
It does not establish compatibility by comparison with the Gyan archive.
GNU toolchain archives use the specified HTTPS mirror with the original checksum checks.

The build keeps resolved sources, source changes, build scripts, bootstrap inputs, commands, and configuration.
The runtime archive and source archive have SHA-256 records.
The runtime includes unchanged license notices from the captured source trees.
Its `LICENSES.json` file records their source paths and hashes.
This collection includes build tools and unused source packages.

The local Windows check verifies those records and runs the media acceptance check with the candidate.
A successful build remains a candidate until its source record and final application checks pass.

### Selected public runtime

The [distribution manifest](../.github/runtime/distribution.json) records the checked runtime for `v0.1.0-beta`.
The native build passed its source audit.
The [Windows validation](https://github.com/MadMatt341/Stitch/actions/runs/38512516251) passed with the selected application source.
Local media checks also passed CPU and NVIDIA encoding with this runtime.
The [release check record](reviews/2026-10-14-public-release.md) gives the results and remaining beta limits.

The public release has separate application, runtime, and source downloads.
The portable application ZIP contains the runtime and license notices.
The large FFmpeg source archive stays separate from that ZIP.
Keep the matching source available with each runtime distribution.

## Prepare the runtime

Use the checked public runtime for a release that does not change runtime inputs.
Keep its binary archive, matching source archive, and source inventory together under `build/runtime-candidate/`.
The [distribution manifest](../.github/runtime/distribution.json) gives their public URLs and hashes.

For a new runtime build, use a local Linux container engine.
The engine must support the image in `.github/runtime/build.json`.
The build also needs network access and sufficient local disk space for compilation and source capture.

From the project root, run the build in that container:

```powershell
$stitchRuntimeRecipe = Get-Content -Raw .github/runtime/build.json | ConvertFrom-Json
$stitchRuntimeRoot = (Get-Location).Path
docker run --rm --mount "type=bind,source=$stitchRuntimeRoot,target=/workspace" --workdir /workspace $stitchRuntimeRecipe.environment.container bash .github/runtime/build-runtime.sh /workspace
```

This command runs the specified build script locally.
It does not start GitHub Actions or publish files.
The script requires absent `build/runtime-work/` and `build/runtime-output/` directories.
It keeps binaries, matching source, source inventory, hashes, and the build record under `build/runtime-output/`.
Compilation can take several hours.

If the build fails, examine its local output and build logs.
Correct the reported problem before you make another build.
Do not use a failed or incomplete build for publication.
Keep source audit requirements and the specified dependency pins.

After compilation and source audit pass, check the candidate locally on Windows.
From the project root, run:

```powershell
./scripts/check-runtime-candidate.ps1 -RuntimeDirectory build/runtime-output
```

This script verifies source pins, archive hashes, binary hashes, required encoders, and required filters.
It then runs the three media acceptance checks locally with the current application source.
If validation fails, examine the local logs and reports before you correct the reported cause.
Record the checked source revision, runtime hashes, environment, results, and remaining limits in a committed check record.

## Publication checks

After runtime validation, record the selected runtime in `.github/runtime/distribution.json`.
This manifest must have `schemaVersion: 1`, `status: verified`, and `target: win-x64`.
It records the FFmpeg and NVIDIA header commits, binary hashes, source archive, source inventory, and public asset URLs.
Its `validationRunUrl` identifies the successful Windows check record.
For a new local runtime build, use the public URL of its committed local check record.
Existing Actions URLs remain historical evidence for the first beta.
Do not create this verified record before the source audit and applicable runtime checks pass.

From the project root, prepare the release files:

```powershell
./scripts/package-release.ps1 -RuntimeDirectory build/runtime-candidate -Version 0.1.1-beta
```

The script requires clean, committed source at `origin/main` and the verified runtime manifest.
It verifies the runtime and source hashes before extraction.
It includes only the application, selected runtime files, instructions, and license notices in the portable folder.
It also makes source archives, a release manifest, and `SHA256SUMS.txt` under `build/releases/`.

If packaging fails, correct the reported input or source problem.
Use an empty output directory for the next attempt.
Do not replace a verified record with hashes from an unrelated runtime.

Before publication, complete these checks:

1. Make sure that all selected application work is complete.
2. Examine the runtime source inventory and captured changes.
3. Record the verified runtime archive, binaries, source archive, and hashes at their owner.
4. Run the [development checks](development.md#checks) with the selected runtime and final application revision.
5. Record the applicable [interactive UI results](ui.md#interactive-checks).
6. Make the ZIP from an explicit list of release files.
7. Extract the ZIP into an empty folder.
8. Check application start and media export from that folder.
9. Make sure that Git contains all selected source, build, and documentation changes.
10. Make sure that the destination branch contains the intended commit.
11. Examine tracked files, history, and author metadata before you make the source repository public.
12. Upload the runtime source archive, notices, and checksums with the public application ZIP.
13. Verify the public download without GitHub authentication.

Record incomplete checks and their limits before a release decision.
Keep dated results and raw reports separate from this procedure.
The [2026-10-14 preparation record](reviews/2026-10-14-public-release-preparation.md) gives the public source boundary and build corrections.

The [0.1.1 beta check record](reviews/2026-10-14-quality-release.md) gives local results and remaining limits for that application release.
