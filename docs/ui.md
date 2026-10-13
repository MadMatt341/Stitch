# UI contract

`src/ui.rs` owns the Windows window, input, display, saved state, and worker events.
`src/queue.rs` owns list order and selection.
`src/picker.rs` owns the file picker.
`src/ui/smoke.rs` contains native checks for the check executable.
The [development reference](development.md#checks) gives automated check commands and their limits.

## List state

Each list entry has a stable ID.
The filename does not identify the entry.
Thus, the list can contain the same file more than one time.
Selection stays on the same entry after a reorder.
After removal, selection moves to the next entry or the last remaining entry.

The UI thread owns application state.
Worker threads inspect videos, make thumbnails, and export media.
They send results through the event channel.
The UI ignores results for removed entries.
Missing files stay in the list with an error indication.

## Input and window

The [user input table](../README.md#use-the-app) gives the controls.
A click in empty space does not open the file picker.
The app has no Add files button.
The empty view and Help give the instructions to add files.

A row click selects the entry and puts keyboard focus on the list.
The selected row has the same indication for mouse and arrow input.
The row includes its number, thumbnail, filename, media information, and remove control.

After the mouse passes the Windows drag threshold, the full row moves with the pointer.
The list shows a placeholder at the proposed destination.
The pointer position changes the proposed destination during the drag.
Mouse release changes the list order.
Esc or loss of mouse capture cancels the drag and keeps the previous order.

The list paints its rows and drag display together through a memory bitmap to prevent partial erase frames.
When metadata changes during a drag, the app updates the display without resetting the native list.
This keeps mouse capture during video inspection.

When no drag is active, the app changes only the native list labels with new metadata.
If one event batch changes most labels, the app rebuilds the list once.
Selection changes do not rebuild the native list.
The renderer calculates only the rows in view and the drag row.
Worker events use one pending window message until the UI starts to read the event channel.

Cancel and shell actions release state borrows before native calls that can send callbacks.
Thus, the Cancel action can receive `WM_CANCELMODE` without a second state borrow failure.
Button drawing restores the supplied GDI drawing context before it returns.
The app replaces control fonts before it deletes the previous fonts.

The window has no standard title bar.
Its top area moves the window, and its edges let you resize it.
The window has a dark background and a thin border.
On Windows 11, the app requests rounded window corners.
Windows 10 keeps the same controls without this corner effect.
The window uses its monitor's work area when maximized.

During export, the app prevents list changes and file additions.
It also prevents changes to the file size and frame rate selections.
Ctrl+S or the Cancel button requests cancellation.
If you close the app during export, it requests cancellation before the window closes.

## Output summary and export controls

A divider separates the list from the summary and settings at the bottom of the window.
The summary shows the clip count, total length, and file size indication.
It stays above the settings when the list scrolls or changes.
The app counts repeated list entries separately.
Until all clip lengths are available, the total length shows a dash.

The File size row has four buttons: Keep source, Small, Medium, and Large.
Keep source is the default selection and is separate from the three size targets.
If the clips permit copying, Keep source copies the source video packets.
Otherwise, it uses H.265 quality conversion.
An explicit fixed frame rate also requires conversion with Keep source selected.
The [media reference](media.md#export-settings) owns the format, target, and conversion conditions.

The Frame rate row has three buttons: Original, 30, and 60.
Original is the default and keeps each clip's source timing.
The frame rate selection is independent of the file size selection.
The app shows the selected buttons with the accent color.
Setting changes update the file size indication immediately from the list metadata.
They do not start a media process or read source files.

The app selects the encoding format and available encoder.
It has no format control or C shortcut.
The [frame rate contract](media.md#frame-rates) owns timing preservation and encoder conditions.

E switches between Keep source and Medium.
The + or = key selects one larger target, and the - key selects one smaller target.
The numpad + and - keys have the same effects.
With Keep source selected, either direction selects Medium.
The app keeps the selection at the Small and Large limits.

F selects the next frame rate, and Shift+F selects the previous rate.
The three rate choices repeat after the last choice.
These keyboard controls have the same export effects as the buttons.

Copy mode uses the `Est. ≈` label.
Encoding presets use the `Target ≈` label.
Quality conversion shows `Size varies`.
A missing video bitrate prevents the copy estimate.

The indication uses approximate decimal KB, MB, or GB values.
One KB is 1,000 bytes, one MB is 1,000,000 bytes, and one GB is 1,000,000,000 bytes.
Values smaller than one MB use 10 KB increments.

During export, the app disables the setting buttons and locks the keyboard setting controls.
The clip count and total length stay visible above the settings.
The progress and result status identify a CPU retry after NVIDIA failure.
HDR and 10-bit input do not prevent export with a fixed frame rate or size target.
Color conversion needs no additional UI setting.

If conversion cannot process a color format, the summary shows the specific media error.
The app disables export until the clips or settings permit an export.

## File dialogs

A opens the Windows `IFileOpenDialog` file picker.
The picker lets you select multiple local files.
Its filter includes video extensions and an All files option.
File selection does not bypass media inspection.

The app supplies its window as the dialog owner.
After cancellation or file selection, the app requests foreground activation and restores keyboard focus within its UI thread.
If the list contains entries, keyboard focus returns to the list.
Otherwise, keyboard focus returns to the application window.
Cancellation does not change the list.

The Windows shell owns the picker display and file enumeration.
The app uses the modern picker to replace the previous legacy open-dialog display.
An interactive visual check is necessary for the first picker frame that you see.

Native-message checks verify modal cancellation, the UI thread's active window, and keyboard focus.
They verify foreground return only when Stitch owns foreground activation before and throughout the sampled picker interval.
Otherwise, the report records an unverified foreground result for an interactive check.
A different application can take foreground activation during this interval.
They do not prove that each system shows the first picker frame without a white flash.

The save dialog accepts an MP4 destination and requests confirmation before replacement.
The [media reference](media.md#output-protection) owns source protection and output publication.

## Saved state

The default state file is `%LOCALAPPDATA%/Stitch/state.json`.
The app saves paths in list order, the selected index, export settings, output folder, and window rectangle.
The export settings contain the preset and frame rate selections and an internal codec value.
The app uses H.265 for new encoding, including after it restores a different codec value.
State files without these settings use Keep source, H.265 for new encoding, and original frame timing.
An older export settings object without a frame rate also uses original timing.

Previous fixed rates below 45 fps become 30 fps.
Previous fixed rates of 45 fps or more become 60 fps.
The app saves state after 250 milliseconds without a new state change.
On close, it tries to save the current state before cancellation or window destruction.

The app writes a partial state file before it replaces `state.json`.
If a state save cannot complete, the app keeps the previous state file.
If the app is not closing, it tries the state save again after one second.
On close, a state-file lock can prevent the final save.

It restores this state on the next start without command-line video paths.
Command-line video paths supply the initial list when present.

If the new state file is missing, the app can read `%LOCALAPPDATA%/Stich/state.json`.
Subsequent saves use the Stitch folder.
An unreadable or invalid state file causes the app to use default state.
`--state-dir <folder>` selects a separate state folder for checks.
The automated UI check does not read or change the user's default state.

## Interactive checks

From the project root, start `build/Stitch.exe` with a separate state folder:

```powershell
./build/Stitch.exe --state-dir ./build/test-output/manual-state
```

Use typical local videos for these checks:

1. Click the empty view. Make sure that no file picker opens.
2. Press A. Examine the first picker frame that you see for a white flash.
3. Cancel the picker. Press H. Make sure that Help opens in Stitch.
4. Add multiple files with A. Make sure that keyboard focus returns to Stitch.
5. Click a row. Make sure that its selection indication matches arrow-key selection.
6. Drag a row slowly. Make sure that the full row moves without flicker.
7. Release the row at a different position. Make sure that the list order changes.
8. During a different drag, press Esc. Make sure that the previous order stays the same.
9. Drop video files on the list. Make sure that the app adds each file.
10. Resize the window. Make sure that you can use the list and controls.
11. Close the app. Open it again with the same state folder. Make sure that the list returns.
12. Add clips with matching video settings. Make sure that the summary shows `Copy source video`.
13. Press E. Make sure that Medium and an approximate target file size appear.
14. Press -. Make sure that Small appears.
15. Press - again. Make sure that Small stays selected.
16. Press + two times. Make sure that Large appears.
17. Press + again. Make sure that Large stays selected.
18. Use = and the numpad + and - keys. Make sure that they change the targets correctly.
19. Click each file size choice. Make sure that its selection indication changes.
20. Press E. Make sure that the copy indication returns.
21. Press - in source mode. Make sure that Medium appears.
22. Start an export. Make sure that E, F, +, and - cannot change its settings.
    Make sure that the setting buttons are disabled.
23. Examine the encoder indication during export. Examine the result status after completion.
24. Select a size preset and a fixed rate. Close the app. Open it again. Make sure that the selections return.
25. Add PQ HDR, HLG HDR, or standard color 10-bit clips that cannot use copy mode.
    Select 60 fps. Make sure that export is available and the summary shows conversion.
    Examine the output color and the source thumbnails.
26. Press F and Shift+F. Make sure that they select the next and previous rates.
27. Click Original, 30, and 60. Make sure that the selected rate indication changes.
28. Examine the divider and summary above the settings. Make sure that the clip count and total length are correct.
29. Return to source mode with a fixed rate. Make sure that the summary shows conversion.

If a check fails, record the build, Windows version, input method, and screen result.
Keep automated reports and observed acceptance results separate.
