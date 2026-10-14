# 0.1.2 beta checks, 2026-10-14

## Build and scope

The release version is `0.1.2-beta` for Windows x64.
The release manifest identifies the packaged application source revision.
The executable SHA-256 is `AF1C1BA176293669C020F7975598FF735D68F8F71623F423BBC16977DFE44473`.
The local checks used Rust 1.98.1 and the verified public FFmpeg 9.0.2 runtime.
The [distribution manifest](../../.github/runtime/distribution.json) identifies its binary archives and matching source.
The runtime inputs and dependency pins did not change.
GitHub Actions stayed disabled in the public source repository.

The user selected the combined changes from two chats:

- Copy compatible video and AAC-LC audio with correct packet timing.
- Encode the necessary tracks automatically when copying cannot keep correct output.
- Show both track operations and give encoding causes through F1.
- Include more local input containers.
- Correct MKV, MPEG, Theora, and MP4 edit-list inspection.
- Increase CPU and NVIDIA encoding capacity with the existing quality settings.

The [track-copy record](2026-10-14-track-copy.md) gives the copied-packet, audio-sample, retry, and recording results.
The [stitching record](2026-10-14-stitching.md) gives the input inventory, format, and speed results from the other chat.
Those records identify their checked builds and conditions.
Trimming stays in TVC.

## Completed local checks

| Check | Result |
| --- | --- |
| Formatting | `cargo fmt --all -- --check` passed. |
| Unit tests | `cargo test --locked` passed all 34 tests. |
| Clippy | All targets and features passed with no warnings. |
| Selected runtime | `scripts/check-runtime-candidate.ps1` passed with `build/runtime-candidate`. |
| Media acceptance | All five `media_flow` tests passed. |
| Track copy | The `copy_flow` test passed. |
| Native UI | `scripts/test-ui.ps1` passed with the 0.1.2 executable. |

The media checks included CPU and NVIDIA encoding, H.264, H.265, Main10, HDR, frame timing, and source protection.
The track-copy check compared compressed packets and decoded audio samples.
It also checked automatic audio recovery, cancellation, and previous destination protection.

Raw reports stay in `build/test-output/media-flow-22660/`, `frame-flow-22660/`, `color-flow-22660/`, `edit-flow-22660/`, and `container-flow-22660/`.
The track-copy reports stay in `build/test-output/copy-flow-19264/`.
The native reports stay in `build/test-output/native-ui-20261014-134255-912/`.

## Publication checks

Package extraction, application start, media export, and public download checks remain necessary before publication.
The [release procedure](../releases.md#publication-checks) gives these checks.

## Remaining beta limits

Physical keyboard and mouse checks remain incomplete.
Native Windows messages do not prove physical F1 input or each displayed file-picker frame.
The full 192-file export remains incomplete.
Generated format cases and the recording pair do not show compatibility with each camera, OBS configuration, player, or editor.
HDR appearance on target displays remains unverified.
Converted output does not keep dynamic HDR or source Dolby Vision metadata.
Dolby Vision profile 5 conversion is unavailable.
HEVC playback can require an additional codec in Windows players.
