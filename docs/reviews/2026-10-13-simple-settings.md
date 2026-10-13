# Simplified settings review, 2026-10-13

## Candidate and selection

The baseline is pushed commit `09ca3dc6656504ae62e89ad32d5d1ca04f3697e4`.
The user selected Original, 30, and 60 for frame rate controls.
The user selected three size targets and a separate source preservation choice.
The user requested automatic format selection for modern computers.
The user also requested a divider, clip count, and total length above the settings.
The candidate uses Keep source, Small, Medium, and Large as the file size labels.

The app uses H.265 in MP4 for new encoding and keeps compatible source video through packet copying.
Existing original-timing CPU conditions and NVIDIA failure recovery still apply.
The [media contract](../media.md#export-settings) owns these conditions.
The [UI contract](../ui.md#output-summary-and-export-controls) owns the settings and summary placement.

The production executable is `build/Stitch.exe`.
Its SHA-256 is `23352D8ED660B1734B05B37CFB3E48E5EF003203726E0C743E1CA3309818A080`.
The environment is Windows NT 10.0.26200.0, PowerShell 7.6.5, Rust 1.98.1, and the pinned FFmpeg 9.0.2 runtime.
The GPU is an NVIDIA GeForce RTX 4060 with driver 591.44.

## Completed checks

| Check | Result |
| --- | --- |
| Formatting | `cargo fmt --all -- --check` passed. |
| Unit tests | `cargo test --locked` passed: 17 library tests and three UI tests. |
| Clippy | `cargo clippy --locked --all-targets --all-features -- -D warnings` passed. |
| Production build | `scripts/build.ps1 -SkipDownload` passed. |
| Media acceptance | Both tests in `cargo test --locked --test media_flow -- --ignored --nocapture` passed. |
| Native UI | `scripts/test-ui.ps1` passed. |

The media records are `build/test-output/media-flow-5100/summary.json` and `build/test-output/frame-flow-5100/summary.json`.
These checks used the final media source before the last UI text and total-length changes.
CPU H.264 and H.265 passed the size target and timing checks.
The source protection, cancellation, previous output preservation, and forced NVIDIA failure recovery checks also passed.
The library retains its earlier fixed rate values for compatibility checks.
The app exposes only the three selected rates.

The final UI records are in `build/test-output/native-ui-20261013-204820-810/`.
Native button messages selected each file size and frame rate choice.
Setting buttons stayed disabled during export, and setting commands could not change the active export.
Restart restored Large, H.265, and 30 fps.
Previous H.264 selections changed to H.265, and previous 23.976 and 59.94 fps selections changed to 30 and 60 fps.
State without export settings used Keep source and Original.

The inspected 480×360 captures showed the divider and total length above the two settings rows.
All setting labels and the bottom action buttons stayed inside the window.
The Help capture also stayed inside the window.
The count included repeated list entries.
The final total-length display uses a dash until all clip lengths are available.

## Remaining acceptance

Both NVIDIA capability checks gave `false` with the installed driver.
Forced NVIDIA attempts recovered to valid CPU output.
Successful NVIDIA output remains unverified.
The [export settings review](2026-10-13-export-settings.md#nvidia-limit-and-cpu-recovery) records the runtime and driver mismatch.

The native report recorded foreground return as `null` because Stitch did not own foreground activation during the sampled interval.
Physical keyboard input, mouse input, foreground return, and each displayed picker frame remain unverified.
Typical camera recordings and playback in target players or editors also remain unverified.
Raw media, state, and captures stay in ignored `build/test-output/` folders.
