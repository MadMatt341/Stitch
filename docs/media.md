# Media contract

`src/media.rs` owns inspection, thumbnails, export, and FFmpeg process lifetime.
`tests/media_flow.rs` owns the generated-video acceptance check.
The [development reference](development.md#checks) gives its command, prerequisites, output paths, and limits.

## Media selection

Stitch reads local video files and makes one MP4 output in list order.
The default video track has priority over other video tracks.
If no default video track is available, the app uses the first video track.
Attached pictures do not count as video tracks.
The default audio track has priority over other audio tracks.
If no default audio track is available, the app uses the first audio track.

Inspection records duration, display dimensions, frame rate, stream settings, file size, and modification time.
It also records the selected video bitrate when the stream metadata supplies a valid value.
It records bit depth, color primaries, transfer function, color matrix, range, and the Dolby Vision profile when present.
HDR detection uses PQ or HLG transfer metadata.
High bit depth alone does not mean HDR.

The app rejects missing files, invalid dimensions, and missing or invalid duration.
It also rejects a file that changes during inspection.
Display dimensions include rotation and pixel aspect ratio.
The video track dimensions and the display dimensions must be from 2 to 16384 pixels.

The app makes a 96×54 thumbnail from the selected video track.
The thumbnail keeps the picture proportions and uses padding as necessary.
The scale filter uses `reset_sar=1` to keep the display proportions with square pixels.
Thumbnail failure does not change the source file.
HDR thumbnails use tone mapping for the standard color UI.

## Copy and conversion

Copy mode keeps the H.264 or H.265 (HEVC) video packets.
Compatible stream layouts, codec settings, timing, and codec initialization data are necessary for all clips.
The source layout must contain one video track and only video or audio tracks.
Each audio track must use AAC.
Each MP4 or MOV track must have one sample description.
`parse_probe`, `single_mp4_descriptions`, and `can_copy` in `src/media.rs` own the full compatibility checks.

When copy mode includes audio, the app decodes each selected audio track and makes one AAC track.
It removes source padding and fills timing gaps at clip boundaries.
The output uses the first clip's audio sample rate and channel layout.
An unsupported audio channel layout causes failure.
If copied video fails processing or output validation, the app tries conversion unless cancellation is active.

App conversion makes H.265 video with an automatically selected color format and bit depth.
The first clip sets the output dimensions.
The app rounds dimensions up to even numbers.
It keeps the picture proportions and adds padding as necessary.
The scale filter uses `reset_sar=1` to include pixel aspect ratio when it makes square pixels.

It converts selected audio tracks to stereo AAC at 48 kHz.
If the list contains audio, clips without audio add silence for their duration.

If all clips have no audio, the output has no audio track.
The app can convert PQ HDR, HLG HDR, and standard color video with high bit depth.
Compatible HDR clips and clips with high bit depth can also use copy mode.
The Windows command-line limit applies to audio processing and conversion because each source stays a command-line input.

## Color conversion

The app selects one color format for the output timeline without a separate UI setting.
For standard color input, conversion uses BT.709 color.
H.265 conversion uses 10-bit output if any input has more than eight bits per component.
Other standard color conversion uses eight-bit output.
Input with more than 10 bits becomes 10-bit H.265 during conversion.
Copy mode keeps the original coded bit depth and color data.

If any input is HDR, H.265 conversion uses 10-bit BT.2020 output.
If all clips use HLG, the output keeps HLG.
Otherwise, HDR conversion uses PQ, including for mixed HDR and standard color clips.
The app converts each clip to the common output color format before it joins the frames.
It uses the CPU `zscale` filter with accurate transfer conversion and error diffusion dithering.
Standard color reference white is 100 cd/m²; HLG conversion uses the 1000 cd/m² reference display.

Conversion uses limited-range 4:2:0 pixels.
CPU H.265 uses `yuv420p10le` for 10-bit output, and NVIDIA uses `p010le` with the Main10 profile.
Both encoders receive the same output color tags.
The app removes source mastering display and dynamic HDR metadata during conversion because they do not describe the combined output.
Copy mode keeps compatible source metadata.

Missing HDR primaries and matrix metadata use BT.2020 defaults.
Missing standard color primaries and transfer metadata use BT.709 defaults.
Missing standard color YUV matrix metadata uses BT.601 below 1280×720 and BT.709 at higher dimensions.
RGB input without range metadata uses full range; other input uses limited range.
These defaults cannot identify untagged camera log video or recover missing color metadata.

The app can convert the PQ or HLG base layer of compatible Dolby Vision video.
Conversion does not make a new Dolby Vision output.
Dolby Vision profile 5 conversion remains unavailable because its base layer does not use standard HDR color.
Its error identifies the format and requests an HDR10 or HLG input.
The [UI reference](ui.md#output-summary-and-export-controls) owns error display.

The library's explicit H.264 output uses eight-bit BT.709 video.
HDR input uses Mobius tone mapping for this output and for UI thumbnails.
Tone mapping uses the full PQ signal peak or the HLG reference display peak.
It limits standard color luma to the display range after conversion.
The [FFmpeg filter reference](https://ffmpeg.org/ffmpeg-filters.html#zscale) describes the color conversion filters.

## Frame rates

Original is the default frame rate selection.
It applies to source mode and all three size presets.
Copy mode keeps the source video packets and their timing.
The existing copy compatibility conditions still apply to mixed-rate clips.

Conversion keeps each source frame and its relative presentation time with the original selection.
The app moves each clip's first frame to its position on the output timeline.
That position is the sum of the preceding clip durations.
Mixed source rates and variable source timing produce variable frame rate output.
The app does not convert the frames to the first clip's rate.

Conversion uses a microsecond filter time base and the `interleave` video filter to join the frames by presentation time.
It uses `-fps_mode:v passthrough` and `-enc_time_base:v filter` to keep that timing during encoding.
The video `concat` filter estimates the last frame duration from the average frame interval.
That estimate can move a clip boundary for variable-rate sources.
The explicit clip positions prevent this boundary change.

Conversion with original timing uses the CPU encoder.
Its private `fps` parameter supplies the highest inspected source rate as a nominal rate for encoder configuration.
This parameter does not change presentation times.
The pinned FFmpeg CLI cannot combine an explicit output `-r` with timestamp passthrough.
Without a valid nominal rate, the encoder can treat the microsecond time base as an excessive frame rate.
The CPU parameters prevent that condition without changing source timing.

The app has three frame rate selections: Original, 30, and 60 fps.
Original keeps fractional source timing as well as integer source rates.
A previous fixed selection below 45 fps becomes 30 fps when the app restores state.
A previous fixed selection of 45 fps or more becomes 60 fps.
The media library retains its previous fixed rate values for compatibility.

A fixed rate selection requires conversion, including in source mode.
The app applies one `fps` filter after it joins the video timeline.
This filter removes or repeats frames to make the selected rate.
It does not change playback speed.

The app repeats the final image as necessary and trims the result to the full clip timeline.
The final image padding has a finite frame count.
The trim uses the calculated output frame count, with a minimum of one frame for a short input.
Fixed-rate output can differ in video duration by one frame at the end.

Audio conversion keeps the source timeline and does not follow the video frame count.
It joins the trimmed audio segments with the audio `concat` filter.
Clips without audio supply silence when other clips contain audio.
The [UI reference](ui.md#output-summary-and-export-controls) gives the frame rate controls and indications.

## Export settings

Keep source is the default file size selection.
Its stored preset name is `source`.
If the copy compatibility checks pass, it copies the source video packets.
Otherwise, the app uses quality encoding to H.265.
CPU quality encoding uses `libx265`, CRF 18, and the `veryfast` encoder preset.

Fixed-rate quality encoding can use NVIDIA VBR with CQ 18 and no fixed bitrate target.
Copy failure recovery uses CPU quality encoding.

The app selects H.265 (HEVC) in MP4 for new video encoding.
The app has no format control.
It keeps the source video codec when packet copying is possible.
Restored codec selections do not change the app's encoding format.
The media library retains H.264 encoding support for its explicit export options and checks.

Small, Medium, and Large are the three file size presets.
In the app, each preset makes a new H.265 video track, including when the source clips permit copy mode.
These presets use average bitrate encoding in one pass.
Their base video targets apply to 1920×1080 pixels at 30 frames each second:

| Preset | Base video target |
| --- | --- |
| Small | 3 Mbps |
| Medium | 6 Mbps |
| Large | 10 Mbps |

The app calculates a scale factor from the conversion output dimensions and a frame rate for the bitrate target:

```text
scale = width * height * fps / (1920 * 1080 * 30)
target_bitrate = round(base_target * min(16, max(0.05, scale)) / 1000) * 1000
```

The output dimensions use even pixel counts from the first clip.
For fixed-rate output, `fps` is the selected output rate.
For original timing, `fps` is the duration-weighted average of the inspected source rates.
This average counts repeated list entries.
It sets the bitrate target and does not change frame timing.
The scale factor has a minimum of 0.05 and a maximum of 16.

H.264 and H.265 use the same targets.
Different codecs and encoders can give different picture quality at the same target bitrate.

These presets keep the conversion audio settings.
They make stereo AAC at 48 kHz with a target bitrate of 192 kbps when any clip contains audio.
The size presets use the same automatic color conversion as quality encoding.

## NVIDIA encoding and CPU recovery

The inspector worker checks NVIDIA availability in the background.
For each codec, it encodes two generated 256×256 frames with the NVIDIA encoder and a null output.
The H.265 check requires Main10 encoding because automatic output can need 10-bit color.

Each codec check has a five-second process timeout.
The process supervisor stops the job before it waits for the output readers after timeout or cancellation.
The app uses the event channel to send the result to the UI thread.
An encoder listing or a GPU name does not prove that an encode can succeed.

The app's fixed-rate conversions use `hevc_nvenc` when the NVIDIA HEVC check succeeds.
Otherwise, they use `libx265` with the CPU.
Conversion with original timing uses the CPU to keep correct encoder rate control with timestamp passthrough.
This includes quality encoding with Keep source selected and conversions with a size target.
The CPU retry keeps the selected bit depth and color format.

If NVIDIA encoding or its output validation fails, the app tries CPU encoding once.
The retry keeps the selected codec, target bitrate, input list, source locks, and temporary output folder.
The app reports the CPU retry through export progress and the result status.
Cancellation prevents this retry.
Source inspection failures and destination publication failures do not start a CPU retry.

## File size estimates

File size calculations use the available clip metadata.
They count each list entry, including repeated files.
They do not read source files or start media processes when export settings change.

The copy estimate adds each selected video bitrate multiplied by its clip duration.
If the clips contain audio, it adds the target bitrate for the rebuilt AAC track for the full list duration.
This audio target is 96 kbps for each channel, with a minimum of one channel and a maximum of eight.
A missing or invalid selected video bitrate prevents the copy estimate.
The app does not use the full source file size as the selected video size.

For encoding presets, the calculation multiplies the target video bitrate by the list duration.
If any clip contains audio, it includes 192 kbps for the full duration.
The calculations divide the total number of bits by eight to give bytes.
They do not include MP4 container data.
If all clips have no audio track, the calculation has no audio contribution.

These values are approximate.
The output file size can be smaller or larger than the target.
Short clips or simple picture content can give much smaller files.
The [x265 quality reference](https://x265.readthedocs.io/en/master/cli.html#cmdoption-crf) describes how picture content changes CRF output size.

For quality conversion, the estimate uses each selected source video bitrate as a reference.
It adjusts each bitrate for the output pixel count and frame rate.
If a bitrate is missing or invalid, it uses the Medium reference rate for that clip.
This reference rate uses the output dimensions and the clip's output frame rate.
The calculation includes 192 kbps for the full duration when any clip contains audio.
The estimate does not set the encoding bitrate.

Quality encoding can differ from its estimate because picture content, source compression, and encoder choice change its bitrate.
If source copying changes to conversion, the summary uses the quality estimate.
The [UI reference](ui.md#output-summary-and-export-controls) gives the file size labels and controls.

## Output protection

The output filename must have the `.mp4` extension.
Its parent folder must exist.
The app holds each source file for read access during export.
These file handles prevent concurrent write or delete access.
The app compares file identity to reject a source as output, also through a hard-link alias.
It inspects the source again and rejects changes since the source entered the list.

The app makes a separate temporary folder beside the destination.
This folder holds the partial output, input manifest, and filter scripts.
The app validates duration, video codec for conversion, display dimensions, and whether the output contains audio.
For conversion, it also validates output bit depth, color primaries, transfer function, matrix, and range.
For a fixed frame rate selection, it also validates the output frame rate.
App conversion must have an H.265 video track.
Library conversion must have a video track with the requested codec.

It flushes the completed output before replacement.
The app replaces the destination through `MoveFileExW` only after validation passes.

Failure or cancellation keeps the previous destination.
The app removes its own temporary export folder after success, failure, or cancellation.
It does not remove unrelated files from the destination folder.
If a destination is in use, replacement fails and the previous file stays unchanged.
After you close the application that holds the destination, start the export again.

FFmpeg and FFprobe run without a console window on the screen.
The app enables only local file and pipe protocols for their inputs.
Windows job objects supervise child processes and end them when their job handles close.

Cancellation closes the active process job and stops its child processes.
The app closes the job before it waits for both output readers to stop.
Child processes can keep pipe handles open after their parent process stops.
The UI keeps its state in memory until its workers stop.

## Failure recovery and checks

If a source changed, remove its list entry.
Add the source again after recording or other writes stop.
If the runtime is missing, run `scripts/build.ps1` from the project root.
If a Dolby Vision profile 5 clip needs conversion, supply an HDR10 or HLG version.

Unit tests examine metadata selection, copy compatibility, conversion arguments, and filename handling.
Unit tests use `cmd.exe` and Windows PowerShell to examine shutdown when child processes keep pipe handles open.

Export settings checks must examine bitrate metadata, target calculations, codec arguments, and source mode quality settings.
Frame rate checks must examine original timing defaults, the three app selections, and previous selection migration.
They must also examine fixed-rate conversion with Keep source selected.
They must examine one CPU retry after NVIDIA failure and no retry after cancellation.
Timeout checks must examine process shutdown when a child process keeps pipe handles open.
These retry checks cannot depend on the absence of a working GPU.

The generated-video check examines packet order, decoded frame order, mixed dimensions and rates, audio, and silence.
It includes H.264 copy, 10-bit HEVC copy, active cancellation, destination locks, source aliases, and changed sources.
It examines rotated video with non-square pixels through thumbnail pixels and decoded output frames.

Color checks examine standard color 10-bit video, PQ, HLG, mixed color formats, and input order changes.
They compare decoded grayscale luminance against the source signals and check output color tags and bit depth.
They also examine decoded HDR red and blue color components against known signal values.
They exercise each app frame rate and size choice, HDR packet copying, tone-mapped thumbnails, audio, and silence.
They also examine NVIDIA quality encoding, size targets, and CPU recovery with 10-bit HDR output.

It makes FFmpeg outputs and verifies temporary-folder cleanup.
It compares source file bytes before and after exports, including output failure checks.

For encoding changes, the generated-video check must examine H.264 and H.265 CPU output.
Frame timing checks must compare decoded presentation times and frame counts for mixed and variable-rate inputs.
They must include reordered clips, clips without audio, all fixed selections, and audio timing at clip boundaries.
It must examine NVIDIA output for each codec when its capability check succeeds.
An unavailable NVIDIA encoder must have a recorded check limit.
The checks must include requested codec validation, repeated clips, audio, and previous-output protection.

These generated inputs do not prove compatibility with each camera or recording program.
For changes to media behavior, also use typical source recordings with their expected order and audio.
Keep source hashes before and after the check to examine source protection.
Record the build, runtime, input properties, output result, and remaining checks.
