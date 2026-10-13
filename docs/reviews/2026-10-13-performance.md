# Performance and reliability review, 2026-10-13

## Build and environment

The baseline is commit `40326bb4bce3e883d0dcda91a66fc5c1680f7bb4`.
The candidate contains the fixes in this review.
The checks used Windows build 26200, PowerShell 7.6.5, Rust 1.98.1, and the pinned FFmpeg 9.0.2 runtime.
All state folders and media inputs were generated test data.

## Reproduced failures

Native Cancel during export stopped the baseline process with exit code `-1073740791`.
The error was `RefCell already borrowed` in the list's `WM_CANCELMODE` callback.
The Cancel action kept an immutable state borrow during `EnableWindow`, which sent this callback synchronously.
The fix releases the borrow before the native call.
The native suite now exercises Cancel and Ctrl+S before close cancellation.

A baseline drawing check found changes to the supplied font, text color, and background mode after button drawing.
The fix restores the GDI drawing context.
The native suite verifies these three properties.
Each file-picker `CoInitializeEx` call with `S_OK` or `S_FALSE` has one `CoUninitialize` call.

The descendant-pipe check took 7.62 seconds before the fix and 1.34 seconds after it.
The fix closes the Windows job before both pipe-reader joins.
Cancellation stops all job children.
Two subprocess unit tests examine this condition without the media runtime.

Metadata with a `1:8192` pixel aspect ratio produced a zero display dimension in the baseline parser.
The fix checks the display dimensions after aspect ratio and rotation.
It also rejects dimensions larger than the integer type before conversion.
The new metadata unit test passed.

A rotated clip with non-square pixels produced a 41-pixel picture width in its thumbnail.
Its correct thumbnail picture width was from 20 to 22 pixels.
Conversion also added unwanted bars before it removed pixel aspect ratio.
The fix sets `reset_sar=1` in the thumbnail and conversion scale filters.
The generated check verifies thumbnail pixels, decoded conversion frames, display dimensions, and source bytes.

A state-file reader blocked replacement after a row drag.
The app kept the previous JSON file and the newer partial file, but it did not try the save again.
If the app is not closing, the fix tries the state save again after one second.
The native lock check verifies the previous state and publication after lock removal.

## Performance measurements

The native benchmark used 1,000 missing-file entries and 100 selection changes.
It disabled redraw during the selection measurement to measure state work without screen work.
Three alternating baseline and candidate runs gave these median times:

| Measurement | Baseline | Candidate |
| --- | --- | --- |
| 100 selection changes with redraw disabled | 131.20 ms | 9.23 ms |
| Initial inspection of 1,000 missing entries | 215.25 ms | 201.78 ms |
| 500 forced list paints | 2204.28 ms | 2267.40 ms |

The state save now follows 250 milliseconds without a new state change.
Close tries to save the current state immediately.
Metadata results use one native update batch.
When most labels change together, the app rebuilds the list once.
The renderer no longer makes an array for every list entry during each paint.

The paint times were different between runs.
These results do not show a frame-rate improvement.
The live list area was 1225 by 1270 pixels at 96 DPI.
The GDI object count stayed at eight before and after 500 paints in each alternating run.

## Automated checks

Formatting, nine Rust unit tests, Clippy with warnings denied, and the release build passed.
Clippy included the native check feature.
Generated-media acceptance passed in `build/test-output/media-flow-7952/`.
It compared source bytes before and after exports and output failures.
Native UI acceptance passed in `build/test-output/native-ui-20261013-185040-668/`.
It included export protection, capture cancellation, state saves, lock removal, reopen, and legacy Stich migration.

The first native run could not acquire source locks inside the sandbox.
The media and native acceptance checks used approved execution outside the sandbox.
The production executable excludes the smoke harness and its capture helpers.
The native suite uses a different executable for internal checks and the production executable for external state checks.

Raw reproduction scripts, benchmark results, captures, and reports stay in `build/test-output/`.
The benchmark script is `build/test-output/review-bench.ps1`.
Baseline Cancel and drawing evidence is in `build/test-output/picker-review-baseline/`.

## Remaining acceptance

A different application took foreground activation during the sampled picker interval.
The baseline and candidate both restored the UI thread's active window and keyboard focus.
`SetForegroundWindow` returned 0 in both builds.
The native report keeps this evidence and records foreground return as unverified.

Physical foreground return, slow row dragging, and each visible picker frame still require the [interactive UI checks](../ui.md#interactive-checks).
These results apply only to generated videos.
No camera recordings were available for this review.

## Subsequent acceptance

The [acceptance record](2026-10-13-acceptance.md) gives later local recording checks and Windows Computer Use observations for commit `aa4ad84`.
