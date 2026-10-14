# 0.1.1 beta checks, 2026-10-14

## Build and scope

The release version is `0.1.1-beta` for Windows x64.
The release manifest identifies the packaged application source revision.
The executable SHA-256 is `36DDD53985EAFA0DCAEB00771DE7EBC6C6B33FB7D06B7E1F26D82F3B2589355F`.
The checks used Rust 1.98.1 and Windows build 26200.
The [distribution manifest](../../.github/runtime/distribution.json) identifies the verified public FFmpeg runtime.

The user selected these changes:

- Show the output size adjacent to the total length.
- Show an approximate size for quality conversion.
- Include Keep source in the + and - key sequence.
- Remove the E shortcut.
- Keep dark button backgrounds when H or Esc changes the Help view.

The [media contract](../media.md#file-size-estimates) owns the quality estimate calculation and its limits.
The [UI contract](../ui.md#output-summary-and-export-controls) owns the size controls and summary.

## Completed local checks

| Check | Result |
| --- | --- |
| Formatting | `cargo fmt --all -- --check` passed. |
| Unit tests | `cargo test --locked` passed all 26 tests. |
| Clippy | The check with all targets and features passed with no warnings. |
| Selected runtime | `scripts/check-runtime-candidate.ps1` passed with `build/runtime-candidate`. |
| Media acceptance | All three tests passed with the verified public runtime. |
| Native UI | `scripts/test-ui.ps1` passed with the final executable and runtime. |

The media checks passed CPU and NVIDIA H.264 and H.265 encoding, including Main10 and HDR output.
They also checked timing, audio, cancellation, source protection, and previous output protection.
The quality estimate was available without a fixed encoding bitrate.
Unit tests checked missing bitrate data, repeated clips, output dimensions, frame rate, and audio.

The native checks passed the four size choices, E removal, quality estimate display, and saved settings.
The button erasure check produced the dark application background for each checked control.
Earlier compact captures showed the quality estimate and complete Help text at 480 by 360 pixels.
Those captures used the same UI source before the application version change.

Raw media reports are in `build/test-output/media-flow-39652/`, `frame-flow-39652/`, and `color-flow-39652/`.
The final native reports are in `build/test-output/native-ui-20261014-094055-066/`.
Reports, captures, and generated videos stay in ignored output folders.
The [release procedure](../releases.md#publication-checks) gives separate package extraction and public download checks.

## Remaining beta limits

Physical +, -, H, and Esc input remains unverified.
The button erasure check does not show each compositor frame during Help changes.
File-picker foreground return and appearance remain interactive checks.
Typical camera recordings and quality estimate accuracy remain unverified.
HDR appearance on target displays and playback in target players or editors also remain unverified.

Quality encoding can make an output that differs from the estimate.
The estimate uses source metadata and does not enforce a file size target.
These limits do not replace the completed automated checks.
