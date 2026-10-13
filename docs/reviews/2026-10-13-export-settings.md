# Export settings review, 2026-10-13

## Build and environment

The baseline is pushed commit `aa4ad8476cacf4a51a0da4a5e1497aafa3571bad`.
The candidate adds export settings, file size indications, H.265 encoding, and NVIDIA encoding with CPU recovery.
The production executable is `build/Stitch.exe`.
Its SHA-256 is `239420D968CFF7F1EFC7D8B69068C53DE9297BBB191C41CE7915059FD4727002`.

| Component | Checked environment |
| --- | --- |
| Windows | NT 10.0.26200.0 |
| PowerShell | 7.6.5 |
| Rust | 1.98.1 |
| FFmpeg | Pinned 9.0.2 runtime |
| GPU | NVIDIA GeForce RTX 4060 |
| NVIDIA driver | 591.44 |

The user selected source copying by default, separate from the Small, Medium, and Large file size targets.
The user also selected H.264 and H.265 encoding with automatic NVIDIA selection and CPU recovery.
The candidate keeps the previous source protection, cancellation, and output publication conditions.
The [media reference](../media.md#export-settings) owns the implemented settings and limits.

## Automated checks

The check commands ran from the project root.

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed. |
| `cargo test --locked` | Passed: 16 library tests and two UI tests. |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | Passed with no warnings. |

The deterministic checks passed for one CPU retry after NVIDIA failure and no retry after cancellation.
The bounded process checks passed for timeout, cancellation, and cleanup when child processes keep output pipes open.

Generated-media acceptance passed in `build/test-output/media-flow-20576/summary.json`.
CPU H.264 and H.265 output passed for all three size presets.
The checks passed for source bytes, packet order, decoded order, mixed geometry, audio, silence, and rotated non-square pixels.
Active cancellation, cancellation before export, destination protection, source aliases, and changed-source rejection passed.
The HDR and 10-bit conversion guard rejected forced encoding of the 10-bit inputs.
The target estimates increased from Small to Medium to Large.

## NVIDIA limit and CPU recovery

Both NVIDIA codec checks gave `false` on this computer.
The HEVC runtime diagnostic gave necessary NVENC API 13.1 and available API 13.0.
The runtime error also gave NVIDIA driver 610.00 or newer as necessary.
The diagnostic is `build/test-output/media-flow-20576/nvidia-hevc-diagnostic.txt`.
These results apply to the pinned runtime and installed driver in this record.

The acceptance check also supplied an available capability result to force an actual NVIDIA attempt.
That attempt failed with the installed driver.
The app then made and validated CPU output for both codecs and identified `CpuFallback`.
The retry kept the requested codec and target.
The checks did not include a successful NVIDIA encode.

## Native UI checks and observations

The final native suite passed in `build/test-output/native-ui-20261013-192236-608/`.
It checked the size presets, keyboard and numpad keys, codec and mode keys, and locked settings during export.
The state after restart contained Large and H.265.
The app used Source and H.264 for older state without export settings.
The existing selection, drag, cancellation, state, and migration checks also passed.

Two earlier native runs failed the foreground return check:

- `build/test-output/native-ui-20261013-191720-483/`
- `build/test-output/native-ui-20261013-191853-863/`.

Their Windows foreground activation result was 0.
The final trace recorded `picker_before_restore` as `[5636950, false]`.
Another application owned foreground activation before restoration.
The activation result remained 0, and the final report recorded foreground return as `null`.
The UI thread's active window and keyboard focus were correct.
The passed suite does not establish physical foreground return.

Earlier compact checks found 1274×695 pixels between timer frames after an immediate 480×360 resize check passed.
The final harness resizes, measures, and captures the window in one step.
The candidate also invalidates the full list after resize to repaint its row text.
The final controlled 480×360 captures showed the list and output summary within the window.
The inspected summary showed Medium, H.265, CPU encoding, and an approximate target of 110 KB.

## Remaining checks

These results apply to generated videos and native Windows messages.
The remaining checks are:

- Successful NVIDIA output with a driver compatible with the pinned runtime.
- Physical keyboard and mouse input.
- Foreground return after physical input.
- Motion display and each visible file-picker frame.
- Typical camera recordings with expected video order and audio.

The [interactive UI checks](../ui.md#interactive-checks) give the physical input procedure.
The [media acceptance checks](../media.md#failure-recovery-and-checks) give the source-recording procedure.
Raw outputs, diagnostics, and captures stay in ignored `build/test-output/` folders.
