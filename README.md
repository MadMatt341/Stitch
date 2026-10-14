# Stitch

Stitch makes one MP4 video from local video files in a list.
The list sets the order of the video clips.
The app does not change the source files.

The application source is public under the [MIT license](LICENSE).
The FFmpeg runtime keeps its separate licenses.
The [release reference](docs/releases.md) gives download contents and source requirements.

Download the [portable Windows x64 beta](https://github.com/MadMatt341/Stitch-Releases/releases/tag/v0.1.0-beta).
Extract the complete ZIP before you start `Stitch.exe`.
The matching FFmpeg source is a separate download on the same release page.

## Use the app

Start `build/Stitch.exe`.
Keep its `runtime` folder beside the executable.

| Input | Action |
| --- | --- |
| A | Open the file picker. |
| File drop | Add one or more video files to the list. |
| Click a row | Select the video clip and put keyboard focus on the list. |
| Up or Down | Select the previous or next video clip. |
| Shift+Up or Shift+Down | Move the selected video clip one position. |
| Drag a row | Move the row with the mouse and change the order on release. |
| Delete or × | Remove a video clip from the list. |
| E | Switch between source mode and encoding presets. |
| + or = | Increase the file size target. In source mode, select Medium. |
| - | Decrease the file size target. In source mode, select Medium. |
| F or Shift+F | Select the next or previous frame rate. |
| Ctrl+S | Open the save dialog. During export, cancel the export. |
| H | Show or hide Help. |
| Esc | Cancel a row drag or hide Help. |
| Alt+F4 or Ctrl+Q | Close the app. |

In the save dialog, select an MP4 filename.
Click Save to start the export.
During the export, click Cancel to stop it.

The numpad + and - keys also change the file size target.
Click Keep source, Small, Medium, or Large to select the file size behavior.
Click Original, 30, or 60 to select the frame rate.
The divider and summary above these settings show the clip count, total length, and approximate output size.
During export, the app disables the setting buttons and prevents setting changes.

The app stores the list, selection, export settings, output folder, and window position in `%LOCALAPPDATA%/Stitch/state.json`.
It saves state after 250 milliseconds without a new state change.
On close, it tries to save the current state before exit.

It restores these settings when you open it again.
If this file is missing, the app can read settings from `%LOCALAPPDATA%/Stich/state.json`.
Missing video files stay in the list with an error indication.
The [UI reference](docs/ui.md) gives the input, window, and file-dialog contracts.

## Export behavior

Keep source is the default file size selection.
For compatible H.264 or H.265 (HEVC) clips, this mode copies the video packets.
If the clips contain audio, it makes one AAC audio track to remove gaps at clip boundaries.
For other supported clips, it converts to H.265 and AAC with automatic color handling.
The default conversion uses quality encoding without a fixed file size target.

Small, Medium, and Large are file size targets.
In source mode, E selects Medium.
With a fixed frame rate, these presets use NVIDIA encoding when available and CPU encoding otherwise.
If NVIDIA encoding fails, the app can retry once with the CPU.
The retry keeps the selected codec and target.

File size values are approximate.
Short clips or simple picture content can give much smaller files than the target.
The app selects H.265 (HEVC) in MP4 for new encoding.
You do not need to select a format.
On Windows, HEVC playback can require a [codec extension](https://support.microsoft.com/en-us/windows/apps/windowsmediaplayer/troubleshoot-windows-media-player-errors).
The output summary shows `Size varies` for the default quality conversion.

Original keeps each clip's original frame timing by default.
Mixed rates produce variable frame rate output when conversion is necessary.
Conversion with original timing uses the CPU for correct encoder rate control with the pinned runtime.
The size presets do not select a frame rate.

F cycles through Original, 30, and 60 fps.
Shift+F cycles in the opposite direction.
A fixed rate requires conversion and can remove or repeat frames.
It keeps the playback speed.

The app converts HDR and 10-bit clips without an extra setting.
It keeps 10-bit standard color input in 10-bit H.265 output.
It keeps HDR color and converts mixed clips to one common HDR color format.
With a fixed frame rate, quality encoding can use NVIDIA with CPU recovery after failure.
The [media reference](docs/media.md) gives compatibility conditions, output protection, failure behavior, and checks.

## Build and checks

Requirements: Windows 10 or a subsequent version, Rust 1.98.1, and PowerShell 7.

From the project root, run:

```powershell
./scripts/build.ps1
```

The script prepares `build/Stitch.exe` and its pinned FFmpeg runtime.
The [development reference](docs/development.md) gives build prerequisites, runtime provenance, and check commands.
The [contributor guide](AGENTS.md) gives source owners and the task map.
