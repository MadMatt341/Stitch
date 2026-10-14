# Public release preparation, 2026-10-14

## Selected scope

The user selected a full public source repository under MIT.
The user then selected a new public history to keep the previous commit author metadata private.
The user requested continued overnight work, with all release changes committed, pushed, and public after verification.
The [release reference](../releases.md) owns the selected license boundaries, runtime source requirements, and package procedure.

The public source repository is [MadMatt341/Stitch](https://github.com/MadMatt341/Stitch).
Its first commit is `20fb7bf3d877a8b6cf85f2cedcdcf8a917e72372`.
An anonymous GitHub API check confirmed public access, MIT recognition, one initial commit, and only the GitHub noreply author address.
The previous development history stays in private `MadMatt341/Stitch-Development`.
The local `development` remote and `private-history` branch retain that history.

The history examination covered nine previous commits, 27 unique paths, and 97 file versions.
It found no credential patterns or tracked private media.
It found a personal author address in commit metadata, which caused the selected new-history boundary.
No previous commit objects were pushed to the new public repository.

## Application and package checks

The [color conversion review](2026-10-13-color-conversion.md) records the completed HDR, 10-bit, recording, media, and native UI checks.
The first public source snapshot passed [Windows CI](https://github.com/MadMatt341/Stitch/actions/runs/38473339122).
This check used the development runtime.
It does not approve that runtime for public binary distribution.

The package script passed PowerShell syntax checks.
It correctly refused packaging without a verified runtime manifest.
All 24 locked Rust dependencies had package license notices, including the separate Unicode license.
Actual release packaging and extracted-package acceptance remain pending the runtime build.

The installed Rust 1.98.1 toolchain has a separate standard library copyright and license notice.
Cargo dependency metadata does not include this file.
The package procedure now keeps that unchanged notice and records its hash and the compiler version.
The notice-copy check retained all bytes, and PowerShell syntax passed.

The runtime scripts passed Bash and embedded Python syntax checks.
The bootstrap patch applied to the specified upstream source revision.
The license collector retained a nested notice and excluded Git metadata in its check.
The configured upstream recipes retained their original archive checksums after the mirror change.
These records are in ignored `build/test-output/`.

## Build corrections

The first runtime [run](https://github.com/MadMatt341/Stitch/actions/runs/38473383028) failed before compilation because the previous container digest was unavailable.
An anonymous registry check verified the replacement Linux x64 image and its immutable digest.
The build manifest owns that digest.
The current candidate pins NVIDIA API 13.0 headers at `e844e5b26f46bb77479f063029595293aa8f812d`.
The specified FFmpeg 9.0.2 source accepts those headers.

TVC reported repeated GNU archive download timeouts from its runner.
Stitch now uses the same HTTPS mirror with the original SHA-256 and SHA-512 values.
The source capture procedure also records checksums for downloaded archive files beside the source folders.
Superseded runtime runs were canceled before a new candidate was started.

The [runtime run](https://github.com/MadMatt341/Stitch/actions/runs/38475195325) used source revision `ef3b29414e578f5f5cf56158800e89bb892045b4`.
The subsequent dependency pin change replaces that candidate.
Successful local NVIDIA encoding with this runtime remains pending.
No public portable application ZIP exists at this checkpoint.

TVC's later build found an incompatibility between current MinGW headers and the current Intel VPL source.
Stitch now pins 85 additional Git sources from the previous verified TVC inventory.
This includes MinGW headers, VPL, x264, x265, and unused sources downloaded by the shared build system.
FFmpeg 9.0.2, NVIDIA API 13.0, and the Windows metadata pin stay as selected.
The build verifies each downloaded Git revision before compilation.

The first all-source pin attempt failed because LuaJIT's recorded revision included a local recipe patch.
Five records now pin verified upstream parents and retain the original recipe patches.
The build compares compiled source trees with the original inventory after patching.
Filtered clones of four network libraries no longer use a depth limit, which permits their specified older revisions.
Fontconfig's official mirror had the same upstream commit and source tree when the GitLab server failed.

The next [runtime run](https://github.com/MadMatt341/Stitch/actions/runs/38481085139) stopped at the source check because the upstream download group excluded libsixel.
The workflow now downloads all specified Git targets before the revision checks.
It also removes branch reset rules from fixed-commit recipes to retain the selected sources through cleanup.
The application source check passed for `b511308b6ed10cd0eed4dc149020dac1e18d4122`.

The subsequent [runtime run](https://github.com/MadMatt341/Stitch/actions/runs/38482019166) compiled and installed FFmpeg successfully.
Its source check then expected unpatched Fontconfig because cleanup had removed the patch marker.
The source check now compares all specified revisions and patched trees directly, without temporary build markers.
Windows media validation did not start for this failed run.
An isolated Git check reproduced Fontconfig's expected patched tree and its unused upstream state without build markers.
Bash syntax and all seven embedded Python blocks passed after the correction.

The final source audit now follows complete binary and source capture.
The workflow keeps these artifacts if that audit fails, with status `captured-awaiting-source-audit`.
The active runtime run stays in use.
Publication still requires source validation.
Bash syntax, eight embedded Python blocks, output ordering, and the status change passed their checks.

The [runtime run](https://github.com/MadMatt341/Stitch/actions/runs/38490670264) compiled FFmpeg but found a local OpenSSL patch in the final source check.
Its upstream parent stays at `d8bf6cdd4849925c30e4f1911c7acb49cb34b702`.
An isolated Git check applied the specified recipe patch and produced tree `e03bc9ea8f9205da805c4631626047faff3c3287`.
All 85 selected recipes were examined for commands that change Git commits.
OpenSSL was the only selected `git am` recipe without an expected patched tree.
The build now requires such a tree record before compilation.

The [runtime run](https://github.com/MadMatt341/Stitch/actions/runs/38500425805) completed native compilation, source capture, and the final source audit.
Its Windows job verified tool startup, required encoders, and required filters.
The three media checks then stopped before export because the fresh runner had no parent test-output folder.
Each media fixture now creates that parent while it keeps the separate process-ID folder check.
The Windows-only validation workflow can check the retained candidate with the corrected application source.
All 24 unit tests, formatting, Clippy with all features, and the new PowerShell syntax checks passed.

## Process check correction

The later source [CI run](https://github.com/MadMatt341/Stitch/actions/runs/38475193025) failed two inherited-pipe timing assertions.
Their elapsed time included PowerShell startup, which took longer on that runner.
The new helpers report readiness after they start the child that retains inherited pipes.
The checks measure cleanup from that point and retain the four-second limit.
The helper child otherwise sleeps for 30 seconds.

Local cleanup took approximately 228 milliseconds after normal parent exit and 21 milliseconds after cancellation.
All 24 unit tests, formatting, and Clippy with all features passed after this correction.
Commit `2e5f4053d45caa077fffd103edaad48a8cb6cb5c` contains the correction.
The production process supervisor did not change.

## Preparation checkpoint

At this checkpoint, the runtime source audit, final runtime pin, local checks, and package acceptance remained pending.
Public download publication and anonymous download verification also remained pending.
The [release check record](2026-10-14-public-release.md) gives the subsequent results and remaining beta limits.
