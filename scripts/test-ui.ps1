$ErrorActionPreference = 'Stop'
$stitchRoot = Split-Path -Parent $PSScriptRoot
$stitchExe = Join-Path $stitchRoot 'build\Stitch-ui-check.exe'
$stitchEngine = Join-Path $stitchRoot 'build\runtime\ffmpeg.exe'
foreach ($required in @((Join-Path $stitchRoot 'build\Stitch.exe'), $stitchEngine)) {
    if (-not (Test-Path -LiteralPath $required -PathType Leaf)) { throw "Build prerequisite is missing: $required. Run scripts/build.ps1 first." }
}
Push-Location -LiteralPath $stitchRoot
try {
    cargo build --release --locked --features native-ui-checks --target-dir build/target/native-ui
    if ($LASTEXITCODE -ne 0) { throw 'Cannot build the native UI check executable.' }
    Copy-Item -LiteralPath (Join-Path $stitchRoot 'build\target\native-ui\release\stitch.exe') -Destination $stitchExe
} finally { Pop-Location }
$stitchCase = Join-Path $stitchRoot ('build\test-output\native-ui-' + [DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss-fff'))
New-Item -ItemType Directory -Path $stitchCase | Out-Null
$stitchInputs = @((Join-Path $stitchCase 'First recording with a long descriptive filename.mp4'),(Join-Path $stitchCase 'Second recording.mp4'))
foreach ($i in 0..1) {
    $color = @('red','blue')[$i]
    & $stitchEngine -v error -y -f lavfi -i "color=c=${color}:s=320x180:r=30:d=1" -c:v libx264 -preset veryfast -pix_fmt yuv420p $stitchInputs[$i]
    if ($LASTEXITCODE -ne 0) { throw 'Cannot make the UI fixtures.' }
}
if (-not ('StitchUiCheck.Native' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;
namespace StitchUiCheck {
    public static class Native {
        [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
        [StructLayout(LayoutKind.Sequential)] public struct GuiThreadInfo {
            public uint Size, Flags;
            public IntPtr Active, Focus, Capture, MenuOwner, MoveSize, Caret;
            public Rect CaretRect;
        }
        [DllImport("user32.dll", SetLastError=true)] private static extern IntPtr SendMessageTimeoutW(IntPtr window, uint message, UIntPtr wParam, IntPtr lParam, uint flags, uint timeout, out UIntPtr result);
        [DllImport("user32.dll", SetLastError=true, CharSet=CharSet.Unicode)] private static extern IntPtr SendMessageTimeoutW(IntPtr window, uint message, UIntPtr wParam, StringBuilder lParam, uint flags, uint timeout, out UIntPtr result);
        [DllImport("user32.dll", SetLastError=true)] private static extern bool GetClientRect(IntPtr window, out Rect rect);
        [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr window, int id);
        [DllImport("user32.dll")] public static extern bool IsWindowEnabled(IntPtr window);
        [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr window);
        [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
        [DllImport("user32.dll")] public static extern bool AllowSetForegroundWindow(uint process);
        [DllImport("user32.dll", SetLastError=true)] private static extern bool PostMessageW(IntPtr window, uint message, UIntPtr wParam, IntPtr lParam);
        public static void Key(IntPtr window, uint key) {
            if (!PostMessageW(window, 0x100, new UIntPtr(key), IntPtr.Zero))
                throw new Win32Exception(Marshal.GetLastWin32Error(), "Cannot post the keyboard check.");
        }
        [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
        [DllImport("user32.dll", SetLastError=true)] private static extern bool GetGUIThreadInfo(uint thread, ref GuiThreadInfo info);
        [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetClassNameW(IntPtr window, StringBuilder name, int capacity);
        public static bool IsMainWindow(IntPtr window, uint process) {
            uint owner; GetWindowThreadProcessId(window, out owner);
            var name = new StringBuilder(128); GetClassNameW(window, name, name.Capacity);
            return owner == process && IsWindowVisible(window) && name.ToString() == "Stitch.Main";
        }
        public static Rect ClientRect(IntPtr window) {
            Rect rect;
            if (!GetClientRect(window, out rect)) throw new Win32Exception(Marshal.GetLastWin32Error());
            return rect;
        }
        public static GuiThreadInfo ThreadState(IntPtr window) {
            uint owner;
            uint thread = GetWindowThreadProcessId(window, out owner);
            var info = new GuiThreadInfo { Size = (uint)Marshal.SizeOf<GuiThreadInfo>() };
            if (thread == 0 || !GetGUIThreadInfo(thread, ref info)) throw new Win32Exception(Marshal.GetLastWin32Error(), "Cannot read Stitch's UI-thread input state.");
            return info;
        }
        public static long Send(IntPtr window, uint message, ulong wParam = 0, long lParam = 0) {
            UIntPtr result;
            if (SendMessageTimeoutW(window, message, new UIntPtr(wParam), new IntPtr(lParam), 2, 5000, out result) == IntPtr.Zero)
                throw new Win32Exception(Marshal.GetLastWin32Error(), "Stitch UI message failed or timed out: " + message);
            return unchecked((long)result.ToUInt64());
        }
        public static long Mouse(IntPtr window, uint message, int x, int y) {
            long point = ((long)(ushort)y << 16) | (ushort)x;
            return Send(window, message, message == 0x202 ? 0UL : 1UL, point);
        }
        public static string ListLabel(IntPtr window, int index) {
            var length = Send(window, 0x18A, (ulong)index);
            if (length < 0 || length > 32767) throw new InvalidOperationException("Cannot read the list label.");
            var label = new StringBuilder((int)length + 1);
            UIntPtr result;
            if (SendMessageTimeoutW(window, 0x189, new UIntPtr((uint)index), label, 2, 5000, out result) == IntPtr.Zero)
                throw new Win32Exception(Marshal.GetLastWin32Error(), "The list label request timed out.");
            return label.ToString();
        }
    }
}
'@
}
function Start-StitchCheck([string[]]$Arguments = @(), [string]$LocalAppData) {
    $start = [Diagnostics.ProcessStartInfo]::new($stitchExe)
    $start.UseShellExecute = $false
    $start.RedirectStandardError = $true
    foreach ($argument in $Arguments) { $start.ArgumentList.Add($argument) }
    if ($LocalAppData) { $start.Environment['LOCALAPPDATA'] = $LocalAppData }
    $child = [Diagnostics.Process]::Start($start)
    # A grant can fail when the runner itself has no foreground permission.
    [StitchUiCheck.Native]::AllowSetForegroundWindow([uint32]$child.Id) | Out-Null
    $child | Add-Member -NotePropertyName StitchErrorTask -NotePropertyValue $child.StandardError.ReadToEndAsync()
    return $child
}
function Wait-StitchWindow([Diagnostics.Process]$Child, [int]$Count) {
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    do {
        if ($Child.HasExited) { throw "Stitch exited before its window was ready: $($Child.StitchErrorTask.GetAwaiter().GetResult())" }
        $Child.Refresh()
        $window = $Child.MainWindowHandle
        if ($window -ne [IntPtr]::Zero -and [StitchUiCheck.Native]::IsMainWindow($window, $Child.Id)) {
            $list = [StitchUiCheck.Native]::GetDlgItem($window, 105)
            if ($list -ne [IntPtr]::Zero -and [StitchUiCheck.Native]::Send($list, 0x18B) -eq $Count) {
                return [pscustomobject]@{ Window = $window; List = $list }
            }
        }
        Start-Sleep -Milliseconds 25
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Stitch did not show a list with $Count entries within 10 seconds."
}
function Complete-StitchCheck([Diagnostics.Process]$Child, [string]$Name, [switch]$Close) {
    if ($Close) {
        if ($Child.HasExited) { throw "Stitch exited before the $Name check closed it." }
        $Child.Refresh()
        if (-not $Child.CloseMainWindow()) { throw "The $Name check could not close the app." }
    }
    $timeout = if ($Close) { 10000 } else { 30000 }
    if (-not $Child.WaitForExit($timeout)) { $Child.Kill($true); $Child.WaitForExit(5000) | Out-Null; throw "The $Name check timed out." }
    $log = $Child.StitchErrorTask.GetAwaiter().GetResult()
    Set-Content -LiteralPath (Join-Path $stitchCase "$Name.stderr.txt") -Value $log
    if ($Child.ExitCode -ne 0 -or $log) { throw "The $Name check failed (exit $($Child.ExitCode)): $log" }
}
function Click-StitchRow([IntPtr]$List, [int]$Index, [switch]$Remove) {
    $top = [StitchUiCheck.Native]::Send($List, 0x18E)
    $height = [StitchUiCheck.Native]::Send($List, 0x1A1)
    $rect = [StitchUiCheck.Native]::ClientRect($List)
    $y = [int](($Index - $top) * $height + [Math]::Floor($height / 2))
    if ($height -gt 0 -and ($y -lt 0 -or $y -ge $rect.Bottom)) {
        [StitchUiCheck.Native]::Send($List, 0x197, [ulong]$Index) | Out-Null
        $top = [StitchUiCheck.Native]::Send($List, 0x18E)
        $y = [int](($Index - $top) * $height + [Math]::Floor($height / 2))
    }
    if ($height -le 0 -or $y -lt 0 -or $y -ge $rect.Bottom) { throw "The requested row is outside the visible list: $Index." }
    $dpi = [StitchUiCheck.Native]::GetDpiForWindow($List)
    $x = if ($Remove) { $rect.Right - [int](15 * $dpi / 96) } else { [int](110 * $dpi / 96) }
    [StitchUiCheck.Native]::Mouse($List, 0x201, $x, $y) | Out-Null
    [StitchUiCheck.Native]::Mouse($List, 0x202, $x, $y) | Out-Null
}
function Get-StitchListSnapshot([IntPtr]$List, [string]$Phase) {
    $state = [StitchUiCheck.Native]::ThreadState($List)
    $rect = [StitchUiCheck.Native]::ClientRect($List)
    $count = [StitchUiCheck.Native]::Send($List, 0x18B)
    $pending = 0
    for ($index = 0; $index -lt $count; $index++) {
        if ([StitchUiCheck.Native]::ListLabel($List, $index).Contains('Reading video')) { $pending++ }
    }
    return [ordered]@{
        phase = $Phase; list = $List.ToInt64(); capture = $state.Capture.ToInt64()
        focus = $state.Focus.ToInt64(); active = $state.Active.ToInt64()
        width = $rect.Right; height = $rect.Bottom; dpi = [StitchUiCheck.Native]::GetDpiForWindow($List)
        selected = [StitchUiCheck.Native]::Send($List, 0x188)
        top = [StitchUiCheck.Native]::Send($List, 0x18E); count = $count; reading_labels = $pending
    }
}
function Read-StitchState([string]$Path) {
    $stream = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, ([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
    $reader = [IO.StreamReader]::new($stream)
    try { return ($reader.ReadToEnd() | ConvertFrom-Json) } finally { $reader.Dispose() }
}
function Wait-StitchState([string]$Path, [int]$Selected) {
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
        if (Test-Path -LiteralPath $Path) {
            $saved = Read-StitchState $Path
            if ($saved.selected -eq $Selected) { return $saved }
        }
        Start-Sleep -Milliseconds 25
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "The state save did not publish selection $Selected."
}
$stitchEvidence = [ordered]@{ executable = $stitchExe; native_messages_only = $true; checks = [ordered]@{} }
$process = $null
try {
    $process = Start-StitchCheck (@('--ui-smoke',$stitchCase) + $stitchInputs)
    Complete-StitchCheck $process 'native-ui'
    $report = Get-Content -Raw -LiteralPath (Join-Path $stitchCase 'interaction.json') | ConvertFrom-Json
    if (-not $report.reorder_passed -or -not $report.mouse_event_reorder_passed -or $report.drop_added_count -ne 2 -or -not $report.native_export_passed -or $report.thumbnail_count -ne 2) { throw 'The native UI behavior check failed.' }
    foreach ($check in @('click_selected','click_kept_order','drag_thumbnail_moved','drag_repaint_stable','drag_cancel_passed','capture_cancel_passed','picker_cancel_focus','picker_empty_cancel_focus','empty_click_ignored','add_button_removed','cancel_action_requested','owner_draw_restored','preset_controls_passed','frame_rate_controls_passed','setting_buttons_passed','preset_busy_locked','compact_size_verified')) {
        if (-not $report.$check) { throw "The native UI check failed: $check" }
    }
    if ($null -ne $report.picker_foreground_restored -and -not $report.picker_foreground_restored) { throw 'Cancel lost foreground activation after the picker opened from the foreground app.' }
    $stitchEvidence.checks.picker_foreground_restored = $report.picker_foreground_restored
    if ($null -eq $report.picker_foreground_restored) {
        $stitchEvidence.remaining_interactive_check = 'Foreground restoration requires physical input: Stitch did not own foreground activation before and throughout the sampled picker interval.'
    }
    $before = (Get-FileHash -LiteralPath (Join-Path $stitchCase 'before-cancel.mp4')).Hash
    $after = (Get-FileHash -LiteralPath (Join-Path $stitchCase 'native-output.mp4')).Hash
    if ($before -ne $after) { throw 'Closing during export changed the existing output.' }

    # Use the production executable for saved-state and external native-message checks.
    $stitchExe = Join-Path $stitchRoot 'build\Stitch.exe'
    # Use a separate state folder. Never read or modify the user's actual queue.
    $stitchState = Join-Path $stitchCase 'state'
    $process = Start-StitchCheck (@('--state-dir',$stitchState) + $stitchInputs)
    $ui = Wait-StitchWindow $process 2
    Click-StitchRow $ui.List 1
    if ([StitchUiCheck.Native]::Send($ui.List, 0x188) -ne 1) { throw 'The second row was not selected before saving.' }
    foreach ($key in @(0x6B, 0x6B, 0x46)) { [StitchUiCheck.Native]::Key($ui.Window, $key) }
    $settingsDeadline = [DateTime]::UtcNow.AddSeconds(5)
    $settingsReady = $false
    do {
        $settingsFile = Join-Path $stitchState 'state.json'
        if (Test-Path -LiteralPath $settingsFile) {
            $settingsState = Get-Content -Raw -LiteralPath $settingsFile | ConvertFrom-Json
            $settingsReady = $settingsState.export_settings.preset -eq 'large' -and $settingsState.export_settings.codec -eq 'h265' -and $settingsState.export_settings.frame_rate -eq 'fps30'
        }
        if (-not $settingsReady) { Start-Sleep -Milliseconds 25 }
    } while (-not $settingsReady -and [DateTime]::UtcNow -lt $settingsDeadline)
    if (-not $settingsReady) { throw 'The file size and frame rate keys did not save their settings.' }
    Complete-StitchCheck $process 'state' -Close
    $first = Get-Content -Raw -LiteralPath (Join-Path $stitchState 'state.json') | ConvertFrom-Json
    if ($first.paths.Count -ne 2 -or $first.selected -ne 1) { throw 'The list and second-row selection were not saved.' }
    $process = Start-StitchCheck @('--state-dir',$stitchState)
    $ui = Wait-StitchWindow $process 2
    if ([StitchUiCheck.Native]::Send($ui.List, 0x188) -ne $first.selected) { throw 'Reopening did not restore the selected row.' }
    Complete-StitchCheck $process 'reopen' -Close
    $second = Get-Content -Raw -LiteralPath (Join-Path $stitchState 'state.json') | ConvertFrom-Json
    if (($first.paths -join '|') -ne ($second.paths -join '|') -or $second.selected -ne $first.selected) { throw 'Reopening did not preserve the saved list and selection.' }
    if ($second.export_settings.preset -ne 'large' -or $second.export_settings.codec -ne 'h265' -or $second.export_settings.frame_rate -ne 'fps30') { throw 'Reopening did not restore the simplified settings.' }
    $stitchEvidence.checks.encoding_settings_reopen = $second.export_settings
    $stitchEvidence.checks.selection_reopen = @{ selected = $second.selected; count = $second.paths.Count }

    foreach ($colorTransfer in @('bt709', 'smpte2084')) {
        $stitchColorInput = Join-Path $stitchCase "ten-bit-$colorTransfer.mp4"
        $stitchPrimaries = if ($colorTransfer -eq 'smpte2084') { 'bt2020' } else { 'bt709' }
        $stitchMatrix = if ($colorTransfer -eq 'smpte2084') { 'bt2020nc' } else { 'bt709' }
        & $stitchEngine -v error -y -f lavfi -i 'color=c=white:s=160x90:r=30:d=1' -vf "setparams=range=limited:color_primaries=${stitchPrimaries}:color_trc=${colorTransfer}:colorspace=${stitchMatrix}" -c:v libx265 -preset veryfast -x265-params 'pools=1:frame-threads=1:log-level=error' -pix_fmt yuv420p10le $stitchColorInput
        if ($LASTEXITCODE -ne 0) { throw 'Cannot make the 10-bit UI fixture.' }
        $stitchColorState = Join-Path $stitchCase "color-state-$colorTransfer"
        New-Item -ItemType Directory -Path $stitchColorState | Out-Null
        @{ paths = @($stitchInputs[0], $stitchColorInput); selected = 0; export_settings = @{ preset = 'source'; codec = 'h265'; frame_rate = 'fps60' } } |
            ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $stitchColorState 'state.json')
        $process = Start-StitchCheck @('--state-dir',$stitchColorState)
        $ui = Wait-StitchWindow $process 2
        $stitchReadyDeadline = [DateTime]::UtcNow.AddSeconds(10)
        while (-not [StitchUiCheck.Native]::IsWindowEnabled([StitchUiCheck.Native]::GetDlgItem($ui.Window, 103))) {
            if ([DateTime]::UtcNow -ge $stitchReadyDeadline) { throw "The $colorTransfer 10-bit input did not permit a 60 fps export." }
            Start-Sleep -Milliseconds 25
        }
        foreach ($sizeId in 200..203) {
            foreach ($rateId in 210..212) {
                [StitchUiCheck.Native]::Send($ui.Window, 0x111, [ulong]$sizeId) | Out-Null
                [StitchUiCheck.Native]::Send($ui.Window, 0x111, [ulong]$rateId) | Out-Null
                if (-not [StitchUiCheck.Native]::IsWindowEnabled([StitchUiCheck.Native]::GetDlgItem($ui.Window, 103))) { throw "The $colorTransfer 10-bit input blocked a supported setting." }
            }
        }
        Complete-StitchCheck $process "color-readiness-$colorTransfer" -Close
        $stitchEvidence.checks["color_readiness_$colorTransfer"] = @{ all_size_and_rate_choices_enabled = $true; mixed_with_eight_bit = $true }
    }

    $stitchPreviousState = Join-Path $stitchCase 'previous-settings-state'
    New-Item -ItemType Directory -Path $stitchPreviousState | Out-Null
    foreach ($previousRate in @('fps23976', 'fps5994')) {
        @{ paths = $stitchInputs; selected = 0; export_settings = @{ preset = 'small'; codec = 'h264'; frame_rate = $previousRate } } |
            ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $stitchPreviousState 'state.json')
        $process = Start-StitchCheck @('--state-dir',$stitchPreviousState)
        $ui = Wait-StitchWindow $process 2
        Complete-StitchCheck $process "previous-settings-$previousRate" -Close
        $restored = Read-StitchState (Join-Path $stitchPreviousState 'state.json')
        $expectedRate = if ($previousRate -eq 'fps23976') { 'fps30' } else { 'fps60' }
        if ($restored.export_settings.preset -ne 'small' -or $restored.export_settings.codec -ne 'h265' -or $restored.export_settings.frame_rate -ne $expectedRate) { throw "The previous $previousRate settings did not use the simplified selections." }
        $stitchEvidence.checks["previous_settings_$previousRate"] = $restored.export_settings
    }

    $stitchLongState = Join-Path $stitchCase 'long-list-state'
    $longPaths = @(0..23 | ForEach-Object { $stitchInputs[1] })
    $source = 4
    $longPaths[$source] = $stitchInputs[0]
    $process = Start-StitchCheck (@('--state-dir',$stitchLongState) + $longPaths)
    $ui = Wait-StitchWindow $process $longPaths.Count
    $rect = [StitchUiCheck.Native]::ClientRect($ui.List)
    $height = [StitchUiCheck.Native]::Send($ui.List, 0x1A1)
    $longTrace = [Collections.Generic.List[object]]::new()
    $stitchEvidence.checks.long_list = [ordered]@{ count = $longPaths.Count; row_height = $height; viewport_height = $rect.Bottom; trace = $longTrace }
    if ($height -le 0 -or $longPaths.Count * $height -le $rect.Bottom) { throw 'The long-list fixture does not exceed the viewport.' }
    $initialTop = if ($rect.Bottom -gt $height * 1.5) { $source - 1 } else { $source }
    [StitchUiCheck.Native]::Send($ui.List, 0x197, [ulong]$initialTop) | Out-Null
    $top = [StitchUiCheck.Native]::Send($ui.List, 0x18E)
    if ($top -ne $initialTop) { throw "The native list did not scroll to row ${initialTop}: $top." }
    if (-not [StitchUiCheck.Native]::ListLabel($ui.List, $source).Contains([IO.Path]::GetFileName($stitchInputs[0]))) { throw 'The unique red source is missing from its initial row.' }
    $dpi = [StitchUiCheck.Native]::GetDpiForWindow($ui.List)
    $x = [int](110 * $dpi / 96)
    $startY = [int](($source - $top) * $height + [Math]::Floor($height / 2))
    if ($startY -ge $rect.Bottom) { throw 'The drag source is outside the viewport.' }
    $longTrace.Add((Get-StitchListSnapshot $ui.List 'before-down'))
    [StitchUiCheck.Native]::Mouse($ui.List, 0x201, $x, $startY) | Out-Null
    $longTrace.Add((Get-StitchListSnapshot $ui.List 'after-down'))
    for ($step = 0; $step -lt $longPaths.Count + 4; $step++) {
        # The desktop can resize a new window after its list becomes available.
        $moveRect = [StitchUiCheck.Native]::ClientRect($ui.List)
        $moveHeight = [StitchUiCheck.Native]::Send($ui.List, 0x1A1)
        $moveDpi = [StitchUiCheck.Native]::GetDpiForWindow($ui.List)
        if ($moveHeight -le 0 -or $longPaths.Count * $moveHeight -le $moveRect.Bottom) { throw 'The resized long-list fixture does not exceed its viewport.' }
        $x = [int](110 * $moveDpi / 96)
        $endY = [int]($moveRect.Bottom + $moveHeight)
        [StitchUiCheck.Native]::Mouse($ui.List, 0x200, $x, $endY) | Out-Null
        $longTrace.Add((Get-StitchListSnapshot $ui.List "move-$step"))
    }
    $endTop = [StitchUiCheck.Native]::Send($ui.List, 0x18E)
    if ($endTop -le $top) { throw 'Dragging below the viewport did not scroll the list.' }
    $releaseRect = [StitchUiCheck.Native]::ClientRect($ui.List)
    $releaseHeight = [StitchUiCheck.Native]::Send($ui.List, 0x1A1)
    $endY = [int]($releaseRect.Bottom + $releaseHeight)
    [StitchUiCheck.Native]::Mouse($ui.List, 0x200, $x, $endY) | Out-Null
    [StitchUiCheck.Native]::Mouse($ui.List, 0x202, $x, $endY) | Out-Null
    $longTrace.Add((Get-StitchListSnapshot $ui.List 'after-up'))
    $selected = [StitchUiCheck.Native]::Send($ui.List, 0x188)
    if ($selected -ne $longPaths.Count - 1 -or -not [StitchUiCheck.Native]::ListLabel($ui.List, [int]$selected).Contains([IO.Path]::GetFileName($stitchInputs[0]))) { throw 'The dragged source and selection did not move to the final row.' }
    $expectedDrag = @($longPaths | Where-Object { $_ -ne $stitchInputs[0] }) + @($stitchInputs[0])
    $saveDeadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
        Start-Sleep -Milliseconds 25
        $dragSaved = if (Test-Path -LiteralPath (Join-Path $stitchLongState 'state.json')) {
            Read-StitchState (Join-Path $stitchLongState 'state.json')
        }
        if ([DateTime]::UtcNow -gt $saveDeadline) { throw 'The deferred list-state save did not complete.' }
    } while (-not $dragSaved -or ($dragSaved.paths -join '|') -ne ($expectedDrag -join '|') -or $dragSaved.selected -ne $longPaths.Count - 1)
    if (($dragSaved.paths -join '|') -ne ($expectedDrag -join '|') -or $dragSaved.selected -ne $longPaths.Count - 1) { throw 'The long-list drag did not save the expected order and source selection.' }
    [StitchUiCheck.Native]::Send($ui.List, 0x197, 0) | Out-Null
    Click-StitchRow $ui.List 0 -Remove
    if ([StitchUiCheck.Native]::Send($ui.List, 0x18B) -ne $longPaths.Count - 1 -or [StitchUiCheck.Native]::Send($ui.List, 0x188) -ne 0) { throw 'The remove control did not remove the clicked row and select its successor.' }
    Complete-StitchCheck $process 'long-list' -Close
    $removed = Get-Content -Raw -LiteralPath (Join-Path $stitchLongState 'state.json') | ConvertFrom-Json
    $expectedRemoved = @($expectedDrag | Select-Object -Skip 1)
    if (($removed.paths -join '|') -ne ($expectedRemoved -join '|') -or $removed.selected -ne 0) { throw 'The remove control changed the wrong row or saved the wrong selection.' }
    $stitchEvidence.checks.long_list.scroll_top = $top
    $stitchEvidence.checks.long_list.drag_scroll_top = $endTop
    $stitchEvidence.checks.long_list.final_viewport_height = $releaseRect.Bottom
    $stitchEvidence.checks.long_list.dragged_selected = $selected
    $stitchEvidence.checks.long_list.count_after_remove = $removed.paths.Count

    $stitchLockedState = Join-Path $stitchCase 'locked-state'
    $process = Start-StitchCheck (@('--state-dir',$stitchLockedState) + $stitchInputs)
    $ui = Wait-StitchWindow $process 2
    $lockedFile = Join-Path $stitchLockedState 'state.json'
    $initialLocked = Wait-StitchState $lockedFile 0
    $stateLock = [IO.File]::Open($lockedFile, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try {
        Click-StitchRow $ui.List 1
        $lockDeadline = [DateTime]::UtcNow.AddSeconds(5)
        do {
            Start-Sleep -Milliseconds 25
            if ([DateTime]::UtcNow -gt $lockDeadline) { throw 'The locked-state save did not attempt publication.' }
        } while (-not (Test-Path -LiteralPath ($lockedFile + '.partial')))
        Start-Sleep -Milliseconds 350
        if ((Read-StitchState $lockedFile).selected -ne 0) { throw 'A state lock did not preserve the previous state.' }
    } finally { $stateLock.Dispose() }
    $retried = Wait-StitchState $lockedFile 1
    if (($retried.paths -join '|') -ne ($initialLocked.paths -join '|')) { throw 'The state retry changed the saved paths.' }
    if (Test-Path -LiteralPath ($lockedFile + '.partial')) { throw 'The state retry left its partial file.' }
    Complete-StitchCheck $process 'state-lock-retry' -Close
    $stitchEvidence.checks.state_lock_retry = @{ previous_state_preserved = $true; selection_after_unlock = $retried.selected }

    # Override LOCALAPPDATA only in the child process for the name-migration check.
    $stitchProfile = Join-Path $stitchCase 'migration-profile'
    $legacyFolder = Join-Path $stitchProfile 'Stich'
    New-Item -ItemType Directory -Path $legacyFolder | Out-Null
    $legacyFile = Join-Path $legacyFolder 'state.json'
    @{ paths = $stitchInputs; selected = 1; output_folder = $stitchCase; rect = $null } | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $legacyFile
    $legacyHash = (Get-FileHash -LiteralPath $legacyFile).Hash
    $process = Start-StitchCheck -LocalAppData $stitchProfile
    $ui = Wait-StitchWindow $process 2
    if ([StitchUiCheck.Native]::Send($ui.List, 0x188) -ne 1) { throw 'The legacy state did not restore its selected row.' }
    Complete-StitchCheck $process 'name-migration' -Close
    $migrated = Get-Content -Raw -LiteralPath (Join-Path $stitchProfile 'Stitch\state.json') | ConvertFrom-Json
    if ($migrated.export_settings.preset -ne 'source' -or $migrated.export_settings.codec -ne 'h265' -or $migrated.export_settings.frame_rate -ne 'original') { throw 'Older state did not use keep-source mode, automatic HEVC, and original timing defaults.' }
    if (($migrated.paths -join '|') -ne ($stitchInputs -join '|') -or $migrated.selected -ne 1 -or $migrated.output_folder -ne $stitchCase) { throw 'The name migration did not preserve paths, selection, and output folder.' }
    if ((Get-FileHash -LiteralPath $legacyFile).Hash -ne $legacyHash) { throw 'The name migration changed the legacy state file.' }
    $stitchEvidence.checks.name_migration = @{ selected = $migrated.selected; output_folder = $migrated.output_folder; legacy_unchanged = $true; child_localappdata = $stitchProfile }
    $stitchEvidence.passed = $true
} catch {
    $stitchEvidence.passed = $false
    $stitchEvidence.failure = $_.ToString()
    throw "Native UI checks failed. Evidence: $stitchCase. $($_.ToString())"
} finally {
    if ($process -and -not $process.HasExited) { $process.Kill($true); $process.WaitForExit(5000) | Out-Null }
    $stitchEvidence | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $stitchCase 'state-checks.json')
}
Write-Output "Native UI, export, close cancellation, scrolling, selection, reopen, and name-migration checks passed: $stitchCase"
