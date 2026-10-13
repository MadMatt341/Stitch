# Acceptance checks, 2026-10-13

## Build and scope

These checks used commit `aa4ad8476cacf4a51a0da4a5e1497aafa3571bad`.
The [performance review](2026-10-13-performance.md) records its fixes and automated checks.
This record adds local recording checks and Windows Computer Use observations.
It does not verify subsequent export settings changes.

The preserved executable is `build/Stitch-acceptance-aa4ad84.exe`.
Its SHA-256 is `8E2CA8861662C05E12D19BA01F63B1C8E18651DE14C947090C8E1558999B077F`.
The environment used Windows build 26200 and the pinned FFmpeg 9.0.2 runtime.
The UI used a separate state folder.
The checks did not change the user's default state.

M selected local recordings from `D:\Videos` with a maximum file size of 100 MB.
All five selected recordings were smaller than 20 MB each.
Private filenames, source hashes, generated outputs, and raw results stay in ignored output folders.

## Local recording results

| Check | Inputs | Output | Result |
| --- | --- | --- | --- |
| H.264 copy | Two 1280×720, 60 fps MP4 recordings | 1280×720, 60 fps; 117.866667 seconds | Passed |
| H.264 conversion | A 1920×1080, 30 fps MOV, then a 2560×1440, 60 fps MP4 | 1920×1080, 30 fps; 56.8 seconds | Passed |
| 10-bit HEVC copy | One 2560×1440, 60 fps MP4 recording used twice | 2560×1440, 60 fps; 168.9 seconds | Passed |

The copy export preserved all 7,072 video packet hashes in input order.
Its video decode timestamps increased throughout the output.
The conversion output used the first clip's dimensions and frame rate.
Its expected duration was 56.766666 seconds.
Six decoded frame samples matched their expected clips with SSIM values of at least 0.997630.
Samples from the incorrect clip had SSIM values no larger than 0.461367.

The repeated HEVC export preserved all 10,134 video packet hashes in input order.
It kept the `Main 10` profile, `yuv420p10le` pixels, and `bt709` color metadata.
Its video decode timestamps increased throughout the output.
This source contains 10-bit SDR video.
It does not contain PQ or HLG transfer metadata for an HDR check.

All three outputs contained stereo AAC at 48 kHz.
Complete video and audio decoding reported no errors.
Audio comparisons included both stereo channels and samples near the clip boundary.
The checks distinguished silence from measurable audio before they compared waveform correlation.
Silent decoded samples matched exactly.

Measurable H.264 copy audio had correlation greater than 0.999997 with no sample offset at 16 kHz.
Measurable HEVC copy audio had correlation greater than 0.999954 with no sample offset at 16 kHz.
The quiet conversion audio samples had correlation of at least 0.975060 with no sample offset at 16 kHz.

All five source SHA-256 values stayed unchanged.
The exports removed their temporary folders.
The final reports are in `build/test-output/input-acceptance-20261013-190026/real-media/`.

## Windows Computer Use observations

The checks used pointer and keyboard input through Windows Computer Use.
These observations are separate from the native-message suite.

| Check | Observed result |
| --- | --- |
| Empty view click | No picker opened. |
| A key | The modern Add videos picker opened. |
| Picker cancellation | Focus returned to the empty window after the dialog closed. H opened Help. |
| Multiple file selection | Both clips entered the list. Focus returned to the list after inspection started. |
| Inspection display | Both colored thumbnails and durations appeared. |
| Row click and Up key | Both inputs used the same selection indication. Focus stayed on the list. |
| Row drag and release | The order changed from red/blue to blue/red. Selection stayed on the red entry. |
| Ctrl+Q and reopen | The app closed with exit code 0. Order, selection, thumbnails, and list focus returned. |

The first returned picker capture used dark shell colors.
This capture does not show each compositor frame during picker creation.
The drag API completes one gesture before the next capture.
Before and after captures do not show continuous slow drag motion.

Lower-right and right-edge drag gestures left the window at 1274×1394 pixels.
These gestures did not establish interactive resize acceptance.

## Live confirmation and limits

M replied "looks good" to the grouped live acceptance checks.
This confirmation included full row motion without flicker, Esc cancellation during a held drag, and picker opening without an observed white flash.
It also included edge resizing and adding a video through file dropping.
The native suite already passed drag cancellation, file dropping, and state restoration through Windows messages.

No required acceptance check remains for this build.
These results apply to this Windows environment and the selected local recordings.
They do not prove compatibility with each recording program or behavior in each compositor frame on other systems.
Frame and waveform comparisons examined selected samples.
No listening check was performed.

The raw Computer Use observations are in `build/test-output/input-acceptance-20261013-190026/computer-use.json`.
