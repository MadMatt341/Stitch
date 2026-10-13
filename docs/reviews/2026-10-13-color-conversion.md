# Color conversion review, 2026-10-13

## Candidate and selection

The baseline is pushed commit `c4f1f5681a45220f85912c037726f7b90679ca67`.
The user requested HDR and 10-bit conversion instead of instructions to avoid conversion.
The user then requested one automatic export policy that gives priority to speed and quality.
The candidate keeps compatible source video and otherwise makes H.265 output with automatic color selection.
The existing file size and frame rate controls stay independent.
The candidate needs no additional color or format control.

The [media contract](../media.md#color-conversion) owns the selected output formats and conditions.
The [UI contract](../ui.md#output-summary-and-export-controls) owns settings and error display.
These checks apply to the color conversion changes after the baseline commit.

The production executable is `build/Stitch.exe`.
Its SHA-256 is `2589A8F7E05C7D6C9E3908363E95034E512CEF7E0F03089E850BD08A8D7CDFBB`.
The environment is Windows NT 10.0.26200.0, PowerShell 7.6.5, Rust 1.98.1, and the pinned FFmpeg 9.0.2 runtime.
The GPU is an NVIDIA GeForce RTX 4060 with driver 591.44.

## Completed checks

| Check | Result |
| --- | --- |
| Formatting | `cargo fmt --all -- --check` passed. |
| Unit tests | `cargo test --locked` passed: 20 library tests and four UI tests. |
| Clippy | `cargo clippy --locked --all-targets --all-features -- -D warnings` passed. |
| Production build | `scripts/build.ps1 -SkipDownload` passed. |
| Media acceptance | All three tests in `cargo test --locked --test media_flow -- --ignored --nocapture` passed. |
| Native UI | `scripts/test-ui.ps1` passed. |

The final generated media records use process ID `15132` under `build/test-output/`.
The folders are `media-flow-15132`, `frame-flow-15132`, and `color-flow-15132`.
The checks included standard color 10-bit, PQ, HLG, mixed HDR and standard color, and changes to input order.
Original, 30, and 60 fps passed with all four file size choices.
The output kept the expected bit depth, color tags, timing, audio positions, and silence.

Grayscale checks compared decoded luminance with source signal values, including 1000 cd/m² highlights.
HDR red and blue checks compared decoded RGB components with known signal values.
The comparison limits were six percent, with small absolute tolerances for dark values.
HDR packet copy, tone-mapped thumbnails, explicit library H.264 tone mapping, source hashes, and previous output preservation also passed.
CPU recovery after forced NVIDIA failure passed for quality encoding and a size target with Main10 HDR output.

The final UI records are in `build/test-output/native-ui-20261013-212057-284/`.
The production app enabled export for mixed eight-bit and 10-bit standard color input.
It also enabled export for mixed standard color and PQ HDR input.
Both cases passed with each file size and frame rate choice.
The existing setting buttons, saved state, cancellation, scrolling, and picker checks also passed.

## Recording acceptance

Inspection of the four recordings in the user's screenshot found one eight-bit H.264 clip and three 10-bit H.265 clips.
All four use standard BT.709 color at 60 fps.
The previous check incorrectly classified the three 10-bit clips as HDR and blocked conversion.

The recording check used two-second packet-copy excerpts from each original file in the displayed order.
It made one 8.1-second, 1280×720, 60 fps, 10-bit H.265 output with audio.
The output dimensions followed the first clip.
The source hashes before and after the check were identical.
The records are in `build/test-output/recording-color-20261013-211453/`.
This check does not establish acceptance for the full recording lengths.

## Remaining acceptance

The installed driver did not pass the NVIDIA H.264 or Main10 H.265 capability checks.
Successful NVIDIA output remains unverified.
The [export settings review](2026-10-13-export-settings.md#nvidia-limit-and-cpu-recovery) records the runtime and driver mismatch.
Conversion with original timing still uses CPU encoding under the pinned runtime.

Physical keyboard input, mouse input, foreground return, and each displayed picker frame remain unverified.
HDR appearance on target displays and playback in target players or editors also remain unverified.
Dolby Vision profile 5 conversion remains unavailable; compatible PQ or HLG base layers use standard HDR output.
Converted output does not retain source Dolby Vision or dynamic HDR metadata.
Raw media, state, captures, and logs stay in ignored `build/test-output/` folders.
