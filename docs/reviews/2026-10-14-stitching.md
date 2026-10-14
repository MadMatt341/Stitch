# Stitching checks, 2026-10-14

The user selected all videos directly in `C:\Users\macie\Videos`, in filename order.
The user also selected more input formats with MP4 output.
The checks used local builds and kept the source recordings unchanged.

## Build and environment

The source baseline was `c9994703e0200512205d4b922e2a93037a2d5952`.
The changed build used Rust 1.98.1 and the source-captured FFmpeg 9.0.2 runtime.
The computer had a Ryzen 9 9900X, 24 logical processors, and an RTX 5070.
NVIDIA H.264 and H.265 Main10 capability checks completed without errors.

| File | SHA-256 |
| --- | --- |
| Verified `Stitch.exe` | `afdf2d29b731d67d9084ece574bc9736cf9353fceefa95b9c5236f07f41bd9f5` |
| `build/runtime/ffmpeg.exe` | `6b55b32364c2ea7e23ff902a6eb1a76613fec81bd01da2a8fa5443f67431dbb4` |
| `build/runtime/ffprobe.exe` | `a4876a4c9922526eb3451fb02cde986b37cb93328f3007a7cfb21216f9675f65` |

Raw records are in `build/test-output/size-investigation-20261014/`.
Its `summary.json` contains the inventory, measured export results, source checks, and check limits.

The chat `Explore saved export settings` started separate copy changes during the final checks.
The final verification used an isolated copy of the baseline and this investigation's changes.
Its output is in `build/test-output/size-investigation-20261014/verified-source/build/`.

## Source sizes and copy behavior

Inspection found no errors in all 192 direct video files.
All used the MP4 container.
Their total size was 91,763,670,786 bytes (91.76 GB).
Their total video duration was 50,248.516 seconds, approximately 13 hours and 57 minutes.
Only 68 clips can use copy mode individually with the verified build.
Conversion was necessary for the full list because its stream settings were not all compatible.

The real recording pair was `2026-09-30 14-06-18.mp4` and `temporal_pickup and selector.mp4`.
The pair contained 68.2 seconds of video.
MP4 edit lists kept two coded reference pictures out of the displayed frames.
The changed build used conversion to prevent more displayed frames at the join.

MKV fixtures contained the same compressed video packets without the MP4 edit lists.
The changed build read their Matroska duration tags and used copy mode.

| Inputs | Total input bytes | Output bytes | Export time | Result |
| --- | --- | --- | --- | --- |
| Original MP4 pair | 80,561,081 | 34,649,755 | 22.60 s | H.265 conversion, 4,090 displayed frames |
| MKV fixtures | 80,508,639 | 80,572,966 | 1.26 s | Copied video, 4,092 displayed frames |

Both completed outputs had correct displayed frame counts.
Each frame timestamp was after the previous timestamp.
The MKV copy output kept all compressed video packets in source order.
These results show why copy size can be approximately the source sum and conversion size can differ.

## Preset speed

Each preset check used the same 68.2-second recording pair.
The changes increased CPU encoder capacity and NVIDIA input decoder capacity.
They kept the existing quality settings and bitrate targets.

| Preset | Original CPU, before | Original CPU, after | 60 fps NVIDIA, before | 60 fps NVIDIA, after |
| --- | --- | --- | --- | --- |
| Small | 43.67 s | 25.47 s | 12.37 s | 8.26 s |
| Medium | 51.95 s | 26.92 s | 12.13 s | 8.13 s |
| Large | 60.14 s | 30.40 s | 12.14 s | 8.19 s |

The CPU checks used approximately 42 to 49 percent less time.
The NVIDIA checks used approximately 33 percent less time.
Encoding used most of the export time.
Approximately 0.02 to 0.06 seconds were necessary for destination publication.
Original conversion still used the CPU to keep source frame timing.

## Format and protection checks

The input extension list now contains 20 extensions.
All 22 generated format and codec cases completed without errors, including AV1 in WebM and ProRes in MOV.
A mixed list with six container types also completed without errors.
These format checks preceded the last inspection corrections and used the same runtime and conversion settings.
Checks examined decoded image order, frame counts, audio, duration, and source hashes.
In all 22 individual format outputs, each frame timestamp was after the previous timestamp.

The checks found incomplete MPEG duration estimates and missing Theora repeat frames.
The changed implementation kept the full video timeline for these inputs.
Five media acceptance cases, 27 unit tests, formatting, Clippy, and the native UI check completed without errors.
The native UI check preceded the last inspection corrections and used the same file-picker changes.
The native UI records are in `build/test-output/native-ui-20261014-105504-494/`.

All 192 original file sizes and modification times stayed unchanged.
The three tested real recordings also kept their SHA-256 hashes.
Each of these recordings gave the same metadata through eight more inspections with the verified build.
The app removed its temporary export folders after the checks.

Repeated checks also found MP4 and MPEG seeks that returned no tail data without an error.
This failure changed the copy decision between inspections and caused an incorrect source-change error.
The correction examines packet timestamps to make sure that the sample reaches the video end.
If it does not, inspection reads all packet flags without a seek.
MPEG duration inspection also reads without a seek if the tail read supplies no frames.

## Remaining checks

The full 192-file export did not complete during this investigation.
The speed checks for the full list stopped after approximately 60 seconds and examined cancellation cleanup.
They show initial processing speed, not completed output size or acceptance of the full list.
These complete-list speed checks preceded the last inspection corrections.

Native Windows messages did not show physical input or file-picker foreground return.
The sampled picker images did not show every displayed frame.
The [interactive checks](../ui.md#interactive-checks) are still necessary for these conditions.
Generated format cases do not show compatibility with every recording program or camera.
