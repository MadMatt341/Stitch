# Frame rate review, 2026-10-13

## Candidate and selection

The baseline is pushed commit `2b71ba4f2a73c6e9e71821d7d9a9881b1c83238c`.
The user accepted original frame timing by default and a separate fixed frame rate selection.
The candidate keeps this selection independent of the file size preset and codec.
The [media contract](../media.md#frame-rates) owns the implemented behavior and encoder conditions.

The production executable is `build/Stitch.exe`.
Its SHA-256 is `56BB6BAA28D1836263FA68BBFE21E11E91BD489EFD94EAEFC312815F47F9A2C9`.
The environment is Windows NT 10.0.26200.0, PowerShell 7.6.5, Rust 1.98.1, and the pinned FFmpeg 9.0.2 runtime.
The GPU is an NVIDIA GeForce RTX 4060 with driver 591.44.

## Implementation observations

The first timestamp check found a shifted clip boundary after a variable-rate clip entered the first position.
The video `concat` filter estimated its final frame duration from the average frame interval.
The candidate now places video frames at explicit clip positions and joins them with `interleave`.
The positions use the inspected durations of preceding clips.

A subsequent picture check found reduced color detail with low target bitrates.
Without a valid nominal frame rate, the encoder used the microsecond time base as an excessive rate.
The pinned CLI rejected an explicit output `-r` with timestamp passthrough.
CPU encoder parameters now supply a nominal rate without changing frame timestamps.
Conversion with original timing uses the CPU.
Fixed-rate presets can select NVIDIA when its codec check succeeds.

Fixed-rate output also needs the full final clip interval after its last original frame.
The candidate repeats the final image and trims the output to the known timeline duration.
This prevents a short final clip when the selected output rate exceeds the final source rate.

A single-frame check then found that a duration-based trim rounded to zero at the output time base.
A candidate with indefinite padding did not stop, and its test process was stopped.
The final candidate uses finite padding and an output frame count trim with a minimum of one frame.
The short-clip checks use a five-second cancellation guard.

## Completed checks

| Check | Result |
| --- | --- |
| Formatting | `cargo fmt --all -- --check` passed. |
| Unit tests | `cargo test --locked` passed: 17 library tests and three UI tests. |
| Clippy | `cargo clippy --locked --all-targets --all-features -- -D warnings` passed. |
| Production build | `scripts/build.ps1 -SkipDownload` passed. |
| Media acceptance | Both tests in `cargo test --locked --test media_flow -- --ignored --nocapture` passed. |
| Native UI | `scripts/test-ui.ps1` passed. |

The final media records are `build/test-output/media-flow-12316/summary.json` and `build/test-output/frame-flow-12316/summary.json`.
CPU H.264 and H.265 passed for original timing and all eight fixed rate selections.
Original output kept the decoded frame count and presentation times within 20 microseconds in the checked inputs.
The checks included 24, 29.97, and 60 fps clips, irregular timing, reordered clips, repeated entries, audio, and silence.
Fixed output used the requested rational frame intervals within 10 microseconds.
The audio length and sound positions passed the boundary checks.

Single-frame input passed at 24 and 60 fps with both CPU codecs.
The existing picture, source protection, cancellation, and destination protection checks also passed.

The final UI records are in `build/test-output/native-ui-20261013-201731-635/`.
F and Shift+F passed rate cycling and source-mode conversion checks.
Export controls stayed locked during export.
The saved state after restart contained Large, H.265, and 23.976 fps.
Older state used original timing.
The inspected 480×360 captures showed the rate line, controls, and Help within the window.

## Remaining acceptance

Both NVIDIA capability checks gave `false` with the installed driver.
Forced NVIDIA attempts failed and recovered to valid CPU output for all fixed selections and both codecs.
Successful NVIDIA output remains unverified.
The [export settings review](2026-10-13-export-settings.md#nvidia-limit-and-cpu-recovery) records the runtime and driver mismatch.

The native report recorded foreground return as `null` because another application owned foreground activation before restoration.
Physical keyboard input, mouse input, foreground return, and each displayed picker frame remain unverified.
Typical camera recordings and playback in target players or editors also remain unverified.
Raw media, state, and captures stay in ignored `build/test-output/` folders.
