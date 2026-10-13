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

The active [runtime run](https://github.com/MadMatt341/Stitch/actions/runs/38475195325) uses source revision `ef3b29414e578f5f5cf56158800e89bb892045b4`.
Its result remains a candidate until source review and Windows media checks pass.
Successful local NVIDIA encoding with this runtime remains pending.
No public portable application ZIP exists at this checkpoint.

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

## Remaining work

The runtime source audit, final runtime pin, local media and NVIDIA checks, native UI checks, and package acceptance remain pending.
Public download publication and anonymous download verification also remain pending.
Physical input and target-display observations remain separate beta limits.
The release continuation must retain these limits and cannot report them as completed observations.
