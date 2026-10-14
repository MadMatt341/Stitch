use super::*;
use std::io::Write;
use windows_sys::Win32::System::SystemServices::MK_LBUTTON;

impl App {
    pub(super) unsafe fn smoke_tick(&self) {
        let Some(folder) = self.smoke.as_ref() else {
            return;
        };
        if self.core.borrow().dialog {
            return;
        }
        if self.core.borrow().clips.values().any(|c| !c.thumbnail_done) {
            return;
        }
        let stage = self.core.borrow().smoke_stage;
        if stage == 0 {
            let original_settings = self.core.borrow().settings;
            self.key(b'F' as u32, true, false, false);
            let fixed_selected = self.core.borrow().settings.frame_rate == FrameRate::Fps60
                && self.core.borrow().export_plan().unwrap().mode == ExportMode::Convert
                && self.core.borrow().preview().0 == "Re-encode";
            self.key(b'F' as u32, false, false, false);
            let original_restored = self.core.borrow().settings == original_settings;
            for _ in 0..3 {
                self.key(b'F' as u32, false, false, false);
            }
            let frame_rate_controls_passed = fixed_selected
                && original_restored
                && self.core.borrow().settings == original_settings;
            let mut setting_buttons_passed = true;
            for (index, expected) in SIZE_CHOICES.iter().enumerate() {
                SendMessageW(self.widgets.get().sizes[index], BM_CLICK, 0, 0);
                setting_buttons_passed &= self.core.borrow().settings.preset == *expected;
            }
            for (index, expected) in RATE_CHOICES.iter().enumerate() {
                SendMessageW(self.widgets.get().rates[index], BM_CLICK, 0, 0);
                setting_buttons_passed &= self.core.borrow().settings.frame_rate == *expected;
            }
            self.core.borrow_mut().settings = original_settings;
            self.refresh();
            self.key(b'C' as u32, false, false, false);
            let source_codec_ignored = self.core.borrow().settings == original_settings;
            let e_key_removed = !self.key(b'E' as u32, false, false, false)
                && self.core.borrow().settings == original_settings;
            self.key(VK_SUBTRACT as u32, false, false, false);
            let source_clamped = self.core.borrow().settings.preset == ExportPreset::Source;
            self.key(VK_OEM_PLUS as u32, true, false, false);
            let small = self
                .core
                .borrow()
                .export_plan()
                .unwrap()
                .estimated_bytes
                .unwrap();
            let small_selected = self.core.borrow().settings.preset == ExportPreset::Small;
            self.key(VK_ADD as u32, false, false, false);
            let medium = self
                .core
                .borrow()
                .export_plan()
                .unwrap()
                .estimated_bytes
                .unwrap();
            self.key(b'C' as u32, false, false, false);
            let codec_changed = self.core.borrow().settings.codec == VideoCodec::H265;
            self.key(VK_ADD as u32, false, false, false);
            self.key(VK_OEM_PLUS as u32, false, false, false);
            let large = self
                .core
                .borrow()
                .export_plan()
                .unwrap()
                .estimated_bytes
                .unwrap();
            let large_clamped = self.core.borrow().settings.preset == ExportPreset::Large;
            UpdateWindow(self.hwnd.get());
            let _ = capture(self.hwnd.get(), &folder.join("encoding.bmp"));
            self.key(VK_SUBTRACT as u32, false, false, false);
            self.key(VK_OEM_MINUS as u32, false, false, false);
            let small_restored = self.core.borrow().settings.preset == ExportPreset::Small;
            self.key(VK_SUBTRACT as u32, false, false, false);
            let source_restored = self.core.borrow().settings.preset == ExportPreset::Source;
            self.key(VK_ADD as u32, false, false, false);
            self.key(VK_OEM_PLUS as u32, false, false, false);
            let medium_restored = self.core.borrow().settings.preset == ExportPreset::Medium
                && self.core.borrow().settings.codec == VideoCodec::H265;
            let preset_controls_passed = source_codec_ignored
                && codec_changed
                && large_clamped
                && small_selected
                && small_restored
                && source_clamped
                && source_restored
                && medium_restored
                && e_key_removed
                && small < medium
                && medium < large;
            self.core.borrow_mut().settings = original_settings;
            self.refresh();
            let screen = GetDC(self.hwnd.get());
            let dc = CreateCompatibleDC(screen);
            let bitmap = CreateCompatibleBitmap(screen, 4, 4);
            let old_bitmap = SelectObject(dc, bitmap as _);
            ReleaseDC(self.hwnd.get(), screen);
            let buttons_erase_dark = [self.widgets.get().help, self.widgets.get().stitch]
                .into_iter()
                .chain(self.widgets.get().sizes)
                .chain(self.widgets.get().rates)
                .all(|control| {
                    fill(
                        dc,
                        RECT {
                            left: 0,
                            top: 0,
                            right: 4,
                            bottom: 4,
                        },
                        0x00ffffff,
                    );
                    SendMessageW(control, WM_ERASEBKGND, dc as usize, 0);
                    GetPixel(dc, 2, 2) == BG
                });
            let original_dc = (
                GetCurrentObject(dc, OBJ_FONT as u32),
                GetTextColor(dc),
                GetBkMode(dc),
            );
            let mut item: DRAWITEMSTRUCT = std::mem::zeroed();
            item.CtlID = HELP as u32;
            item.hDC = dc;
            GetClientRect(self.widgets.get().help, &mut item.rcItem);
            SendMessageW(self.hwnd.get(), WM_DRAWITEM, 0, &item as *const _ as isize);
            let owner_draw_restored = original_dc
                == (
                    GetCurrentObject(dc, OBJ_FONT as u32),
                    GetTextColor(dc),
                    GetBkMode(dc),
                );
            SelectObject(dc, old_bitmap);
            DeleteObject(bitmap as _);
            DeleteDC(dc);
            UpdateWindow(self.hwnd.get());
            let _ = capture(self.hwnd.get(), &folder.join("list.bmp"));
            let original: Vec<_> = self
                .core
                .borrow()
                .queue
                .entries
                .iter()
                .map(|e| e.id)
                .collect();
            let mut client: RECT = std::mem::zeroed();
            let mut window: RECT = std::mem::zeroed();
            GetClientRect(self.hwnd.get(), &mut client);
            GetWindowRect(self.hwnd.get(), &mut window);
            let mut frame_color = 0u32;
            DwmGetWindowAttribute(self.hwnd.get(), 34, &mut frame_color as *mut u32 as _, 4);
            let list = self.widgets.get().list;
            let coord = |x: i32, y: i32| ((y as u32 & 0xffff) << 16 | (x as u32 & 0xffff)) as isize;
            let d = self.dpi.get();
            let pixel = |x, y| {
                let dc = GetDC(list);
                let color = GetPixel(dc, dip(x, d), dip(y, d));
                ReleaseDC(list, dc);
                color
            };
            let mut click_selected = false;
            let mut click_kept_order = false;
            if original.len() >= 2 {
                SendMessageW(
                    list,
                    WM_LBUTTONDOWN,
                    MK_LBUTTON as usize,
                    coord(dip(180, d), dip(111, d)),
                );
                UpdateWindow(list);
                click_selected = self.core.borrow().queue.selected == Some(original[1])
                    && SendMessageW(list, LB_GETCURSEL, 0, 0) == 1
                    && GetFocus() == list
                    && pixel(4, 111) == PANEL;
                SendMessageW(
                    list,
                    WM_MOUSEMOVE,
                    MK_LBUTTON as usize,
                    coord(dip(181, d), dip(112, d)),
                );
                SendMessageW(list, WM_LBUTTONUP, 0, coord(dip(181, d), dip(112, d)));
                click_kept_order = self
                    .core
                    .borrow()
                    .queue
                    .entries
                    .iter()
                    .map(|e| e.id)
                    .collect::<Vec<_>>()
                    == original
                    && self.core.borrow().drag.is_none()
                    && GetCapture() != list;
                self.key(VK_UP as u32, false, false, false);
            }
            self.key(VK_DOWN as u32, false, false, false);
            self.key(VK_UP as u32, true, false, false);
            let reordered: Vec<_> = self
                .core
                .borrow()
                .queue
                .entries
                .iter()
                .map(|e| e.id)
                .collect();
            let okay = original.len() < 2 || reordered[0] == original[1];
            let thumbnail_count = self
                .core
                .borrow()
                .clips
                .values()
                .filter(|c| c.thumb.is_some())
                .count();
            let mut drag_trace = Vec::new();
            let mut drag_thumbnail_moved = false;
            let mut drag_repaint_stable = false;
            let mut drag_cancel_passed = false;
            let mut capture_cancel_passed = false;
            if original.len() >= 2 {
                UpdateWindow(list);
                let thumbnail_color = pixel(78, 37);
                SendMessageW(
                    list,
                    WM_LBUTTONDOWN,
                    MK_LBUTTON as usize,
                    coord(dip(110, self.dpi.get()), dip(37, self.dpi.get())),
                );
                drag_trace.push(format!(
                    "down: {:?}, capture:{}",
                    self.core.borrow().drag,
                    GetCapture() == list
                ));
                SendMessageW(
                    list,
                    WM_MOUSEMOVE,
                    MK_LBUTTON as usize,
                    coord(dip(110, self.dpi.get()), dip(135, self.dpi.get())),
                );
                drag_trace.push(format!("move: {:?}", self.core.borrow().drag));
                drag_thumbnail_moved =
                    pixel(78, 135) == thumbnail_color && pixel(78, 37) != thumbnail_color;
                InvalidateRect(list, null(), 0);
                UpdateWindow(list);
                drag_repaint_stable = pixel(78, 135) == thumbnail_color;
                let _ = capture(self.hwnd.get(), &folder.join("drag.bmp"));
                self.key(VK_ESCAPE as u32, false, false, false);
                UpdateWindow(list);
                drag_cancel_passed = self.core.borrow().drag.is_none()
                    && self
                        .core
                        .borrow()
                        .queue
                        .entries
                        .iter()
                        .map(|e| e.id)
                        .collect::<Vec<_>>()
                        == reordered
                    && pixel(78, 37) == thumbnail_color
                    && GetCapture() != list;
                SendMessageW(
                    list,
                    WM_LBUTTONDOWN,
                    MK_LBUTTON as usize,
                    coord(dip(110, d), dip(37, d)),
                );
                SendMessageW(
                    list,
                    WM_MOUSEMOVE,
                    MK_LBUTTON as usize,
                    coord(dip(110, d), dip(135, d)),
                );
                ReleaseCapture();
                capture_cancel_passed = self.core.borrow().drag.is_none()
                    && self
                        .core
                        .borrow()
                        .queue
                        .entries
                        .iter()
                        .map(|e| e.id)
                        .collect::<Vec<_>>()
                        == reordered;
                SendMessageW(
                    list,
                    WM_LBUTTONDOWN,
                    MK_LBUTTON as usize,
                    coord(dip(110, d), dip(37, d)),
                );
                SendMessageW(
                    list,
                    WM_MOUSEMOVE,
                    MK_LBUTTON as usize,
                    coord(dip(110, d), dip(135, d)),
                );
                SendMessageW(
                    list,
                    WM_LBUTTONUP,
                    0,
                    coord(dip(110, self.dpi.get()), dip(135, self.dpi.get())),
                );
                drag_trace.push(format!("up: {:?}", self.core.borrow().drag));
            }
            let mouse_order: Vec<_> = self
                .core
                .borrow()
                .queue
                .entries
                .iter()
                .map(|e| e.id)
                .collect();
            KillTimer(self.hwnd.get(), 1);
            self.smoke_picker_ticks.set(0);
            let picker_before_foreground = GetForegroundWindow() as usize;
            let picker_observer = observe_test_picker(self.hwnd.get(), folder.clone());
            SetTimer(self.hwnd.get(), 2, 100, None);
            self.key(b'A' as u32, false, false, false);
            KillTimer(self.hwnd.get(), 2);
            let _ = picker_observer.join();
            let picker_focus_trace = [
                self.hwnd.get() as usize,
                GetForegroundWindow() as usize,
                GetActiveWindow() as usize,
                GetFocus() as usize,
                list as usize,
                self.smoke_picker_ticks.get() as usize,
            ];
            let picker_cancel_focus = self.smoke_picker_ticks.get() > 0
                && !self.core.borrow().dialog
                && GetActiveWindow() == self.hwnd.get()
                && GetFocus() == list
                && self
                    .core
                    .borrow()
                    .queue
                    .entries
                    .iter()
                    .map(|e| e.id)
                    .collect::<Vec<_>>()
                    == mouse_order;
            let picker_owned_foreground = self.smoke_picker_owned_foreground.get();
            let picker_during_foreground = self.smoke_picker_foreground.get();
            let picker_foreground_restore_result = self.smoke_foreground_result.get();
            let picker_before_restore = self.smoke_foreground_before_restore.get();
            let picker_foreground_restored = (picker_before_foreground == self.hwnd.get() as usize
                && picker_owned_foreground)
                .then(|| GetForegroundWindow() == self.hwnd.get());
            let saved_queue = std::mem::take(&mut self.core.borrow_mut().queue);
            self.refresh_list();
            self.refresh();
            UpdateWindow(self.hwnd.get());
            self.smoke_picker_ticks.set(0);
            SetTimer(self.hwnd.get(), 2, 100, None);
            SendMessageW(
                self.hwnd.get(),
                WM_LBUTTONUP,
                0,
                coord(dip(200, d), dip(200, d)),
            );
            KillTimer(self.hwnd.get(), 2);
            let empty_click_ignored = self.smoke_picker_ticks.get() == 0
                && !self.core.borrow().dialog
                && self.core.borrow().queue.entries.is_empty();
            let add_button_removed = GetDlgItem(self.hwnd.get(), 101).is_null();
            let _ = capture(self.hwnd.get(), &folder.join("empty.bmp"));
            SetTimer(self.hwnd.get(), 2, 100, None);
            self.key(b'A' as u32, false, false, false);
            KillTimer(self.hwnd.get(), 2);
            let picker_empty_focus_trace = [
                self.hwnd.get() as usize,
                GetForegroundWindow() as usize,
                GetActiveWindow() as usize,
                GetFocus() as usize,
                self.smoke_picker_ticks.get() as usize,
            ];
            let picker_empty_cancel_focus = self.smoke_picker_ticks.get() > 0
                && GetActiveWindow() == self.hwnd.get()
                && GetFocus() == self.hwnd.get();
            self.core.borrow_mut().queue = saved_queue;
            self.refresh_list();
            self.refresh();
            SetFocus(list);
            SetTimer(self.hwnd.get(), 1, 650, None);
            let paths: Vec<_> = self
                .core
                .borrow()
                .queue
                .entries
                .iter()
                .take(2)
                .map(|e| e.path.clone())
                .collect();
            let count_before = self.core.borrow().queue.entries.len();
            send_test_drop(self.hwnd.get(), &paths);
            let drop_added = self.core.borrow().queue.entries.len() - count_before;
            // Remove a newly queued item before its thumbnail completes.
            let last = self.core.borrow().queue.entries.last().map(|e| e.id);
            self.core.borrow_mut().queue.selected = last;
            self.key(VK_DELETE as u32, false, false, false);
            let report = serde_json::json!({"owner_draw_restored": owner_draw_restored,
                "buttons_erase_dark": buttons_erase_dark,
                "reorder_passed": okay,
                "mouse_event_reorder_passed": mouse_order==original,
                "click_selected": click_selected,
                "click_kept_order": click_kept_order,
                "drag_thumbnail_moved": drag_thumbnail_moved,
                "drag_repaint_stable": drag_repaint_stable,
                "drag_cancel_passed": drag_cancel_passed,
                "capture_cancel_passed": capture_cancel_passed,
                "picker_foreground_restored": picker_foreground_restored,
                "picker_before_foreground": picker_before_foreground,
                "picker_during_foreground": picker_during_foreground,
                "picker_owned_foreground": picker_owned_foreground,
                "picker_foreground_restore_result": picker_foreground_restore_result,
                "picker_before_restore": picker_before_restore,
                "picker_focus_trace": picker_focus_trace,
                "picker_empty_focus_trace": picker_empty_focus_trace,
                "picker_cancel_focus": picker_cancel_focus,
                "picker_empty_cancel_focus": picker_empty_cancel_focus,
                "empty_click_ignored": empty_click_ignored,
                "add_button_removed": add_button_removed,
                "preset_controls_passed": preset_controls_passed,
                "frame_rate_controls_passed": frame_rate_controls_passed,
                "setting_buttons_passed": setting_buttons_passed,
                "drop_added_count": drop_added,
                "clips": original.len(),
                "thumbnail_count": thumbnail_count,
                "client_insets": [window.right-window.left-client.right,window.bottom-window.top-client.bottom],
                "frame_color": frame_color,
                "drag_trace": drag_trace,
                "original": original,
                "after_keyboard": reordered,
                "after_mouse": mouse_order,
                "note": "Native messages and window flow; physical input is not simulated."});
            let _ = fs::write(
                folder.join("interaction.json"),
                serde_json::to_vec_pretty(&report).unwrap(),
            );
            self.core.borrow_mut().smoke_stage = 1;
            self.key(b'H' as u32, false, false, false);
        } else if stage == 1 {
            UpdateWindow(self.hwnd.get());
            let _ = capture(self.hwnd.get(), &folder.join("help.bmp"));
            self.key(VK_ESCAPE as u32, false, false, false);
            self.core.borrow_mut().smoke_stage = 2;
            ShowWindow(self.hwnd.get(), SW_RESTORE);
            let resized = SetWindowPos(
                self.hwnd.get(),
                null_mut(),
                0,
                0,
                dip(480, self.dpi.get()),
                dip(360, self.dpi.get()),
                SWP_NOMOVE | SWP_NOZORDER,
            );
            let mut immediate: RECT = std::mem::zeroed();
            GetClientRect(self.hwnd.get(), &mut immediate);
            let trace = serde_json::json!({"set_window_pos":resized,"last_error":windows_sys::Win32::Foundation::GetLastError(),"zoomed":IsZoomed(self.hwnd.get()),"dpi":self.dpi.get(),"immediate_size":[immediate.right,immediate.bottom]});
            fs::write(
                folder.join("resize.json"),
                serde_json::to_vec_pretty(&trace).unwrap(),
            )
            .unwrap();
        } else if stage == 2 {
            ShowWindow(self.hwnd.get(), SW_RESTORE);
            SetWindowPos(
                self.hwnd.get(),
                null_mut(),
                0,
                0,
                dip(480, self.dpi.get()),
                dip(360, self.dpi.get()),
                SWP_NOMOVE | SWP_NOZORDER,
            );
            let mut compact_rect: RECT = std::mem::zeroed();
            GetClientRect(self.hwnd.get(), &mut compact_rect);
            let compact_size_verified = compact_rect.right == dip(480, self.dpi.get())
                && compact_rect.bottom == dip(360, self.dpi.get());
            UpdateWindow(self.hwnd.get());
            UpdateWindow(self.widgets.get().list);
            thread::sleep(std::time::Duration::from_millis(75));
            let _ = capture(self.hwnd.get(), &folder.join("compact.bmp"));
            self.key(VK_ADD as u32, false, false, false);
            self.key(b'C' as u32, false, false, false);
            UpdateWindow(self.hwnd.get());
            thread::sleep(std::time::Duration::from_millis(75));
            let _ = capture(self.hwnd.get(), &folder.join("compact-encoding.bmp"));
            self.key(b'F' as u32, true, false, false);
            UpdateWindow(self.hwnd.get());
            thread::sleep(std::time::Duration::from_millis(75));
            let _ = capture(self.hwnd.get(), &folder.join("compact-fixed-rate.bmp"));
            SendMessageW(self.widgets.get().sizes[0], BM_CLICK, 0, 0);
            let quality_estimate_passed = {
                let core = self.core.borrow();
                let plan = core.export_plan().unwrap();
                plan.mode == ExportMode::Convert
                    && plan.video_bitrate.is_none()
                    && core
                        .preview()
                        .1
                        .is_some_and(|size| size.starts_with("Est. ≈"))
            };
            UpdateWindow(self.hwnd.get());
            let _ = capture(self.hwnd.get(), &folder.join("compact-quality.bmp"));
            SendMessageW(self.widgets.get().sizes[2], BM_CLICK, 0, 0);
            self.key(b'F' as u32, false, false, false);
            self.key(b'H' as u32, false, false, false);
            UpdateWindow(self.hwnd.get());
            thread::sleep(std::time::Duration::from_millis(75));
            let _ = capture(self.hwnd.get(), &folder.join("compact-help.bmp"));
            self.key(VK_ESCAPE as u32, false, false, false);
            SendMessageW(self.widgets.get().sizes[0], BM_CLICK, 0, 0);
            let sources: Vec<_> = {
                let core = self.core.borrow();
                core.queue
                    .entries
                    .iter()
                    .filter_map(|entry| {
                        core.clips
                            .get(&entry.id)?
                            .info
                            .as_ref()?
                            .as_ref()
                            .ok()
                            .map(|info| Source {
                                path: entry.path.clone(),
                                info: info.clone(),
                            })
                    })
                    .collect()
            };
            self.core.borrow_mut().smoke_stage = 3;
            self.start_export(sources, folder.join("native-output.mp4"));
            let before = self.core.borrow().settings;
            let controls_disabled = self
                .widgets
                .get()
                .sizes
                .into_iter()
                .chain(self.widgets.get().rates)
                .all(|control| IsWindowEnabled(control) == 0);
            SendMessageW(self.hwnd.get(), WM_COMMAND, SIZE_FIRST + 3, 0);
            SendMessageW(self.hwnd.get(), WM_COMMAND, RATE_FIRST + 1, 0);
            for key in [VK_ADD as u32, VK_SUBTRACT as u32, b'C' as u32, b'F' as u32] {
                self.key(key, false, false, false);
            }
            let locked = controls_disabled && before == self.core.borrow().settings;
            let report = folder.join("interaction.json");
            let mut data: serde_json::Value =
                serde_json::from_slice(&fs::read(&report).unwrap()).unwrap();
            data["preset_busy_locked"] = locked.into();
            data["compact_size_verified"] = compact_size_verified.into();
            data["quality_estimate_passed"] = quality_estimate_passed.into();
            data["compact_client_size"] =
                serde_json::json!([compact_rect.right, compact_rect.bottom]);
            fs::write(report, serde_json::to_vec_pretty(&data).unwrap()).unwrap();
        } else if stage == 3 && self.core.borrow().busy.is_none() {
            let output = self.core.borrow().output.clone();
            let passed = output.as_ref().is_some_and(|p| p.is_file());
            let report = folder.join("interaction.json");
            let mut data: serde_json::Value = fs::read(&report)
                .ok()
                .and_then(|v| serde_json::from_slice(&v).ok())
                .unwrap_or_default();
            data["native_export_passed"] = passed.into();
            data["native_export_status"] = self.core.borrow().status.clone().into();
            let _ = fs::write(&report, serde_json::to_vec_pretty(&data).unwrap());
            UpdateWindow(self.hwnd.get());
            let _ = capture(self.hwnd.get(), &folder.join("saved.bmp"));
            if let Some(output) = output {
                let _ = fs::copy(&output, folder.join("before-cancel.mp4"));
            }
            // Exercise both native Cancel and Ctrl+S before closing the canceled export.
            let sources: Vec<_> = {
                let core = self.core.borrow();
                core.queue
                    .entries
                    .iter()
                    .filter_map(|entry| {
                        core.clips
                            .get(&entry.id)?
                            .info
                            .as_ref()?
                            .as_ref()
                            .ok()
                            .map(|info| Source {
                                path: entry.path.clone(),
                                info: info.clone(),
                            })
                    })
                    .collect()
            };
            KillTimer(self.hwnd.get(), 1);
            self.core.borrow_mut().smoke_stage = 4;
            self.start_export(sources, folder.join("native-output.mp4"));
            SendMessageW(self.hwnd.get(), WM_COMMAND, STITCH, 0);
            self.key(b'S' as u32, false, true, false);
            data["cancel_action_requested"] = self
                .core
                .borrow()
                .busy
                .as_ref()
                .is_some_and(|run| run.canceled.load(Ordering::Relaxed))
                .into();
            let _ = fs::write(&report, serde_json::to_vec_pretty(&data).unwrap());
            self.key(b'Q' as u32, false, true, false);
        }
    }
}

pub(super) unsafe fn capture(hwnd: HWND, path: &Path) -> std::io::Result<()> {
    let mut r: RECT = std::mem::zeroed();
    GetWindowRect(hwnd, &mut r);
    let width = r.right - r.left;
    let height = r.bottom - r.top;
    let screen = GetDC(null_mut());
    let memory = CreateCompatibleDC(screen);
    let bitmap = CreateCompatibleBitmap(screen, width, height);
    let old = SelectObject(memory, bitmap as _);
    BitBlt(
        memory,
        0,
        0,
        width,
        height,
        screen,
        r.left,
        r.top,
        SRCCOPY | CAPTUREBLT,
    );
    SelectObject(memory, old);
    let stride = (width * 3 + 3) & !3;
    let mut pixels = vec![0u8; (stride * height) as usize];
    let mut info: BITMAPINFO = std::mem::zeroed();
    info.bmiHeader.biSize = 40;
    info.bmiHeader.biWidth = width;
    info.bmiHeader.biHeight = height;
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 24;
    GetDIBits(
        memory,
        bitmap,
        0,
        height as u32,
        pixels.as_mut_ptr() as _,
        &mut info,
        DIB_RGB_COLORS,
    );
    DeleteObject(bitmap as _);
    DeleteDC(memory);
    ReleaseDC(null_mut(), screen);
    let mut file = fs::File::create(path)?;
    file.write_all(b"BM")?;
    file.write_all(&(54u32 + pixels.len() as u32).to_le_bytes())?;
    file.write_all(&[0u8; 4])?;
    file.write_all(&54u32.to_le_bytes())?;
    let bytes = std::slice::from_raw_parts(&info.bmiHeader as *const _ as *const u8, 40);
    file.write_all(bytes)?;
    file.write_all(&pixels)
}

unsafe fn send_test_drop(hwnd: HWND, paths: &[PathBuf]) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::Memory::*;
    let mut names: Vec<u16> = Vec::new();
    for path in paths {
        names.extend(path.as_os_str().encode_wide());
        names.push(0);
    }
    names.push(0);
    let size = std::mem::size_of::<DROPFILES>();
    let block = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, size + names.len() * 2);
    if block.is_null() {
        return;
    }
    let ptr = GlobalLock(block) as *mut u8;
    if ptr.is_null() {
        GlobalFree(block);
        return;
    }
    std::ptr::write(
        ptr as *mut DROPFILES,
        DROPFILES {
            pFiles: size as u32,
            pt: POINT { x: 0, y: 0 },
            fNC: 0,
            fWide: 1,
        },
    );
    std::ptr::copy_nonoverlapping(names.as_ptr() as *const u8, ptr.add(size), names.len() * 2);
    GlobalUnlock(block);
    SendMessageW(hwnd, WM_DROPFILES, block as usize, 0);
}

fn observe_test_picker(owner: HWND, folder: PathBuf) -> thread::JoinHandle<()> {
    let owner = owner as isize;
    thread::spawn(move || unsafe {
        // Observe independently of the modal UI thread's message processing.
        for _ in 0..400 {
            let popup = GetLastActivePopup(owner as HWND);
            if popup != owner as HWND && IsWindowVisible(popup) != 0 {
                for frame in 0..10 {
                    if IsWindowVisible(popup) == 0 {
                        break;
                    }
                    let _ = capture(
                        popup,
                        &folder.join(format!("picker-observed-{frame:02}.bmp")),
                    );
                    thread::sleep(std::time::Duration::from_millis(16));
                }
                return;
            }
            thread::sleep(std::time::Duration::from_millis(5));
        }
    })
}
