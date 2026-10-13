# Public downloads

The application name is Stitch.
The executable name is `Stitch.exe`.
The public source repository is `MadMatt341/Stitch`.
The public download repository is `MadMatt341/Stitch-Releases`.

The public source history starts with the first release preparation snapshot.
The user selected this boundary to keep personal author metadata from the previous nine commits private.
The previous development history stays in the private `MadMatt341/Stitch-Development` repository.
Subsequent application source changes use the public repository.

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

The build makes `ffmpeg.exe` and `ffprobe.exe` for generic Windows x64 processors.
Its feature configuration comes from those build scripts.
It does not establish compatibility by comparison with the Gyan archive.
GNU toolchain archives use the specified HTTPS mirror with the original checksum checks.

The build keeps resolved sources, source changes, build scripts, bootstrap inputs, commands, and configuration.
The runtime archive and source archive have SHA-256 records.
The runtime includes unchanged license notices from the captured source trees.
Its `LICENSES.json` file records their source paths and hashes.
This collection includes build tools and unused source packages.

The Windows job checks those records and runs the media acceptance check with the candidate.
A successful build remains a candidate until its source record and final application checks pass.

## Prepare the runtime

From the project root, start the manual GitHub workflow:

```powershell
gh workflow run build-runtime.yml --repo MadMatt341/Stitch --ref main
```

This command starts a separate runtime build on GitHub.
It does not publish a download or change the development runtime.
The workflow can require up to six hours for compilation.
Its `runtime-candidate-<revision>` artifact contains binaries and matching source together.
Its separate Windows artifact contains the generated media-check results.

If the build fails, examine the workflow logs and its failure artifact.
Correct the reported problem before you start another build.
Do not use a failed or incomplete build for publication.

## Publication checks

After runtime validation, record the selected runtime in `.github/runtime/distribution.json`.
This manifest must have `schemaVersion: 1`, `status: verified`, and `target: win-x64`.
It records the FFmpeg and NVIDIA header commits, binary hashes, source archive, source inventory, and public asset URLs.
Its `validationRunUrl` identifies the successful Windows check.
Do not create this verified record before the source audit and applicable runtime checks pass.

From the project root, prepare the release files:

```powershell
./scripts/package-release.ps1 -RuntimeDirectory build/runtime-candidate -Version 0.1.0-beta
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
