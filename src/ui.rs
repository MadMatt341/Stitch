use serde::{Deserialize, Serialize};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    ptr::{null, null_mut},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicIsize, Ordering},
        mpsc,
    },
    thread,
};
use stitch::media::FrameRate;
use stitch::{
    media::{
        self, AudioMode, ClipInfo, EncodeEngine, EncodingReason, ExportMode, ExportOptions,
        ExportPlan, ExportPreset, ExportSettings, NvidiaSupport, Progress, Runtime, Source,
        THUMB_HEIGHT, THUMB_WIDTH, VideoCodec,
    },
    queue::Queue,
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::{Dwm::*, Gdi::*},
    System::LibraryLoader::*,
    UI::{
        Controls::Dialogs::*, Controls::*, HiDpi::*, Input::KeyboardAndMouse::*, Shell::*,
        WindowsAndMessaging::*,
    },
};

#[cfg(feature = "native-ui-checks")]
mod smoke;

const EVENTS: u32 = WM_APP + 1;
const HELP: usize = 102;
const STITCH: usize = 103;
const FOLDER: usize = 104;
const LIST: usize = 105;
const SIZE_FIRST: usize = 200;
const RATE_FIRST: usize = 210;
const SIZE_CHOICES: [ExportPreset; 4] = [
    ExportPreset::Source,
    ExportPreset::Small,
    ExportPreset::Medium,
    ExportPreset::Large,
];
const RATE_CHOICES: [FrameRate; 3] = [FrameRate::Original, FrameRate::Fps30, FrameRate::Fps60];
const BG: u32 = 0x0018100B;
const PANEL: u32 = 0x00251A13;
const TEXT: u32 = 0x00FBF7F4;
const MUTED: u32 = 0x00B8A394;
const ACCENT: u32 = 0x00BED655;
const LINE: u32 = 0x003C2A20;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn dip(value: i32, dpi: u32) -> i32 {
    (value as i64 * dpi as i64 / 96) as i32
}
fn time(seconds: f64) -> String {
    let n = seconds.max(0.0).round() as u64;
    if n >= 3600 {
        format!("{}:{:02}:{:02}", n / 3600, n / 60 % 60, n % 60)
    } else {
        format!("{}:{:02}", n / 60, n % 60)
    }
}
fn filename(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

fn file_size(bytes: f64) -> String {
    if bytes >= 1_000_000_000.0 {
        format!("{:.1} GB", bytes / 1_000_000_000.0)
    } else if bytes >= 1_000_000.0 {
        format!("{:.0} MB", bytes / 1_000_000.0)
    } else if bytes >= 10_000.0 {
        format!("{:.0} KB", (bytes / 10_000.0).round() * 10.0)
    } else {
        "<10 KB".into()
    }
}

fn simplified_settings(mut settings: ExportSettings) -> ExportSettings {
    settings.codec = VideoCodec::H265;
    settings.frame_rate = settings.frame_rate.simplified();
    settings
}

#[derive(Clone, Copy, Default)]
struct Widgets {
    list: HWND,
    help: HWND,
    stitch: HWND,
    folder: HWND,
    sizes: [HWND; 4],
    rates: [HWND; 3],
}

impl Widgets {
    fn controls(self) -> impl Iterator<Item = HWND> {
        [self.list, self.help, self.stitch, self.folder]
            .into_iter()
            .chain(self.sizes)
            .chain(self.rates)
    }
}

#[derive(Clone, Copy, Debug)]
struct Drag {
    id: u64,
    start_x: i32,
    start_y: i32,
    offset_y: i32,
    y: i32,
    active: bool,
    gap: usize,
}

struct Clip {
    info: Option<media::Result<ClipInfo>>,
    thumb: Option<Vec<u8>>,
    canceled: Arc<AtomicBool>,
    thumbnail_done: bool,
}

struct Run {
    canceled: Arc<AtomicBool>,
    percent: f64,
    mode: ExportMode,
    audio_mode: AudioMode,
    reasons: Vec<EncodingReason>,
    engine: Option<EncodeEngine>,
    destination: PathBuf,
}

#[derive(Default, Serialize, Deserialize)]
struct Saved {
    #[serde(default)]
    paths: Vec<PathBuf>,
    selected: Option<usize>,
    output_folder: Option<PathBuf>,
    rect: Option<[i32; 4]>,
    #[serde(default)]
    export_settings: ExportSettings,
}

struct Core {
    queue: Queue,
    clips: HashMap<u64, Clip>,
    busy: Option<Run>,
    help: bool,
    closing: bool,
    dialog: bool,
    status: Option<String>,
    output: Option<PathBuf>,
    output_folder: Option<PathBuf>,
    drag: Option<Drag>,
    settings: ExportSettings,
    nvidia: Option<NvidiaSupport>,
    #[cfg(feature = "native-ui-checks")]
    smoke_stage: u32,
}

impl Core {
    fn infos(&self) -> Option<Vec<&ClipInfo>> {
        self.queue
            .entries
            .iter()
            .map(|entry| self.clips.get(&entry.id)?.info.as_ref()?.as_ref().ok())
            .collect()
    }

    fn export_plan(&self) -> Option<ExportPlan> {
        media::plan(&self.infos()?, self.settings).ok()
    }

    fn preview(&self) -> (String, Option<String>) {
        let Some(infos) = self.infos() else {
            let detail = if self
                .clips
                .values()
                .any(|clip| matches!(clip.info, Some(Err(_))))
            {
                "Fix unreadable clips"
            } else {
                "Reading videos…"
            }
            .into();
            return (detail, None);
        };
        if infos.is_empty() {
            return ("Add videos for an estimate".into(), None);
        }
        let mut plan = match media::plan(&infos, self.settings) {
            Ok(plan) => plan,
            Err(error) => return (error, None),
        };
        if let Some(run) = &self.busy {
            plan.mode = run.mode;
            plan.audio_mode = run.audio_mode;
            plan.estimated_bytes =
                media::estimate_export_bytes(&infos, self.settings, run.mode, run.audio_mode);
        }
        let size = plan
            .estimated_bytes
            .map(|bytes| {
                format!(
                    "{} ≈ {}",
                    if self.settings.preset == ExportPreset::Source {
                        "Est."
                    } else {
                        "Target"
                    },
                    file_size(bytes)
                )
            })
            .unwrap_or_else(|| "Size unavailable".into());
        let action = media::copy::action(plan.mode, plan.audio_mode);
        let action = match self.busy.as_ref().and_then(|run| run.engine) {
            Some(EncodeEngine::Nvidia) => format!("GPU · {action}"),
            Some(EncodeEngine::CpuFallback) => format!("CPU retry · {action}"),
            _ => action.into(),
        };
        (action, Some(size))
    }

    fn encoding_reasons(&self) -> Vec<EncodingReason> {
        if let Some(run) = &self.busy {
            return run.reasons.clone();
        }
        self.export_plan()
            .map(|plan| {
                [plan.video_reason, plan.audio_reason]
                    .into_iter()
                    .flatten()
                    .collect()
            })
            .unwrap_or_default()
    }

    fn export_explanation(&self) -> String {
        self.encoding_reasons()
            .into_iter()
            .map(|reason| {
                let detail = reason.text();
                if reason.clip > 0 {
                    self.queue
                        .entries
                        .get(reason.clip - 1)
                        .map(|entry| format!("{detail}\n{}", filename(&entry.path)))
                        .unwrap_or(detail)
                } else {
                    detail
                }
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    fn label(&self, index: usize) -> String {
        let entry = &self.queue.entries[index];
        let status = match self.clips.get(&entry.id).and_then(|c| c.info.as_ref()) {
            Some(Ok(info)) => time(info.duration),
            Some(Err(_)) => "Cannot read video".into(),
            None => "Reading video".into(),
        };
        format!("{}. {}, {}", index + 1, filename(&entry.path), status)
    }
}

enum Event {
    Nvidia(NvidiaSupport),
    Info(u64, Box<media::Result<ClipInfo>>),
    Thumb(u64, Option<Vec<u8>>),
    Progress(Progress),
    Finished(media::Result<ExportMode>),
}
#[derive(Clone)]
struct Port {
    tx: mpsc::Sender<Event>,
    hwnd: Arc<AtomicIsize>,
    pending: Arc<AtomicBool>,
}
impl Port {
    fn send(&self, event: Event) {
        if self.tx.send(event).is_err() {
            return;
        }
        let hwnd = self.hwnd.load(Ordering::Relaxed) as HWND;
        unsafe {
            if !hwnd.is_null()
                && !self.pending.swap(true, Ordering::Relaxed)
                && PostMessageW(hwnd, EVENTS, 0, 0) == 0
            {
                self.pending.store(false, Ordering::Relaxed);
            }
        }
    }
}
struct Request {
    id: u64,
    path: PathBuf,
    canceled: Arc<AtomicBool>,
}

struct App {
    hwnd: Cell<HWND>,
    widgets: Cell<Widgets>,
    dpi: Cell<u32>,
    font: Cell<HFONT>,
    small: Cell<HFONT>,
    title: Cell<HFONT>,
    brush: HBRUSH,
    core: RefCell<Core>,
    runtime: media::Result<Runtime>,
    requests: mpsc::Sender<Request>,
    rx: mpsc::Receiver<Event>,
    port: Port,
    inspector: RefCell<Option<thread::JoinHandle<()>>>,
    exporter: RefCell<Option<thread::JoinHandle<()>>>,
    shutdown: Arc<AtomicBool>,
    state_file: PathBuf,
    smoke: Option<PathBuf>,
    #[cfg(feature = "native-ui-checks")]
    smoke_picker_ticks: Cell<u32>,
    #[cfg(feature = "native-ui-checks")]
    smoke_picker_foreground: Cell<usize>,
    #[cfg(feature = "native-ui-checks")]
    smoke_picker_owned_foreground: Cell<bool>,
    #[cfg(feature = "native-ui-checks")]
    smoke_foreground_result: Cell<i32>,
    #[cfg(feature = "native-ui-checks")]
    smoke_foreground_before_restore: Cell<(usize, bool)>,
}

impl App {
    fn new(runtime: media::Result<Runtime>, state_file: PathBuf, smoke: Option<PathBuf>) -> Self {
        let (tx, rx) = mpsc::channel();
        let port = Port {
            tx,
            hwnd: Arc::new(AtomicIsize::new(0)),
            pending: Arc::new(AtomicBool::new(false)),
        };
        let (requests, work) = mpsc::channel::<Request>();
        let shutdown = Arc::new(AtomicBool::new(false));
        let engine = runtime.clone();
        let worker_port = port.clone();
        let worker_shutdown = shutdown.clone();
        let inspector = thread::spawn(move || {
            let support = engine
                .as_ref()
                .map(|runtime| media::probe_nvidia(runtime, &worker_shutdown))
                .unwrap_or_default();
            worker_port.send(Event::Nvidia(support));
            while let Ok(request) = work.recv() {
                if worker_shutdown.load(Ordering::Relaxed) {
                    break;
                }
                if request.canceled.load(Ordering::Relaxed) {
                    continue;
                }
                let info = engine
                    .as_ref()
                    .map_err(Clone::clone)
                    .and_then(|r| media::inspect(r, &request.path, &request.canceled));
                worker_port.send(Event::Info(request.id, Box::new(info.clone())));
                let thumb = info.as_ref().ok().and_then(|i| {
                    engine
                        .as_ref()
                        .ok()
                        .and_then(|r| media::thumbnail(r, &request.path, i, &request.canceled).ok())
                });
                worker_port.send(Event::Thumb(request.id, thumb));
            }
        });
        Self {
            hwnd: Cell::new(null_mut()),
            widgets: Cell::new(Widgets::default()),
            dpi: Cell::new(96),
            font: Cell::new(null_mut()),
            small: Cell::new(null_mut()),
            title: Cell::new(null_mut()),
            brush: unsafe { CreateSolidBrush(BG) },
            core: RefCell::new(Core {
                queue: Queue::default(),
                clips: HashMap::new(),
                busy: None,
                help: false,
                closing: false,
                dialog: false,
                status: None,
                output: None,
                output_folder: None,
                drag: None,
                settings: ExportSettings::default(),
                nvidia: None,
                #[cfg(feature = "native-ui-checks")]
                smoke_stage: 0,
            }),
            runtime,
            requests,
            rx,
            port,
            inspector: RefCell::new(Some(inspector)),
            exporter: RefCell::new(None),
            shutdown,
            state_file,
            smoke,
            #[cfg(feature = "native-ui-checks")]
            smoke_picker_ticks: Cell::new(0),
            #[cfg(feature = "native-ui-checks")]
            smoke_picker_foreground: Cell::new(0),
            #[cfg(feature = "native-ui-checks")]
            smoke_picker_owned_foreground: Cell::new(false),
            #[cfg(feature = "native-ui-checks")]
            smoke_foreground_result: Cell::new(0),
            #[cfg(feature = "native-ui-checks")]
            smoke_foreground_before_restore: Cell::new((0, false)),
        }
    }

    unsafe fn initialize(&self) {
        let hwnd = self.hwnd.get();
        self.dpi.set(GetDpiForWindow(hwnd).max(96));
        self.fonts();
        let instance = GetModuleHandleW(null());
        let list = CreateWindowExW(
            WS_EX_ACCEPTFILES,
            wide("LISTBOX").as_ptr(),
            null(),
            WS_CHILD
                | WS_VISIBLE
                | WS_TABSTOP
                | WS_VSCROLL
                | LBS_NOTIFY as u32
                | LBS_HASSTRINGS as u32
                | LBS_OWNERDRAWFIXED as u32
                | LBS_NOINTEGRALHEIGHT as u32,
            0,
            0,
            0,
            0,
            hwnd,
            LIST as _,
            instance,
            null(),
        );
        SetWindowSubclass(list, Some(list_proc), 1, self as *const Self as usize);
        SetWindowTheme(list, wide("DarkMode_Explorer").as_ptr(), null());
        let button = |id: usize, text: &str| {
            CreateWindowExW(
                0,
                wide("BUTTON").as_ptr(),
                wide(text).as_ptr(),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW as u32,
                0,
                0,
                0,
                0,
                hwnd,
                id as _,
                instance,
                null(),
            )
        };
        let widgets = Widgets {
            list,
            help: button(HELP, "Help"),
            stitch: button(STITCH, "Stitch it!"),
            folder: button(FOLDER, "Open folder"),
            sizes: std::array::from_fn(|index| {
                button(SIZE_FIRST + index, SIZE_CHOICES[index].label())
            }),
            rates: std::array::from_fn(|index| {
                button(RATE_FIRST + index, ["Original", "30", "60"][index])
            }),
        };
        self.widgets.set(widgets);
        ShowWindow(widgets.folder, SW_HIDE);
        for child in widgets.controls() {
            SendMessageW(child, WM_SETFONT, self.font.get() as usize, 1);
        }
        for child in widgets.sizes.into_iter().chain(widgets.rates) {
            SendMessageW(child, WM_SETFONT, self.small.get() as usize, 1);
        }
        SendMessageW(list, LB_SETITEMHEIGHT, 0, dip(74, self.dpi.get()) as isize);
        DragAcceptFiles(hwnd, 1);
        DragAcceptFiles(list, 1);
        self.port.hwnd.store(hwnd as isize, Ordering::Relaxed);
        for (attribute, color) in [(20, 1u32), (33, 2), (34, 0x00373737)] {
            let result = DwmSetWindowAttribute(hwnd, attribute, &color as *const u32 as _, 4);
            if attribute == 34 && result < 0 {
                let disabled = DWMNCRP_DISABLED;
                DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_NCRENDERING_POLICY as u32,
                    &disabled as *const _ as _,
                    4,
                );
            }
        }
        SetWindowPos(
            hwnd,
            null_mut(),
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
        self.layout();
        self.events();
        self.refresh();
    }

    unsafe fn fonts(&self) {
        let previous = [self.font.get(), self.small.get(), self.title.get()];
        let create = |size, weight| {
            CreateFontW(
                -dip(size, self.dpi.get()),
                0,
                0,
                0,
                weight,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                OUT_DEFAULT_PRECIS as u32,
                CLIP_DEFAULT_PRECIS as u32,
                CLEARTYPE_QUALITY as u32,
                DEFAULT_PITCH as u32,
                wide("Segoe UI Variable Text").as_ptr(),
            )
        };
        self.font.set(create(14, 400));
        self.small.set(create(12, 400));
        self.title.set(create(18, 500));
        let widgets = self.widgets.get();
        for child in widgets.controls() {
            if !child.is_null() {
                SendMessageW(child, WM_SETFONT, self.font.get() as usize, 1);
            }
        }
        for child in widgets.sizes.into_iter().chain(widgets.rates) {
            if !child.is_null() {
                SendMessageW(child, WM_SETFONT, self.small.get() as usize, 1);
            }
        }
        for font in previous {
            if !font.is_null() {
                DeleteObject(font as _);
            }
        }
    }

    unsafe fn layout(&self) {
        let mut rect: RECT = std::mem::zeroed();
        GetClientRect(self.hwnd.get(), &mut rect);
        let d = self.dpi.get();
        let p = |n| dip(n, d);
        let w = self.widgets.get();
        MoveWindow(
            w.list,
            p(16),
            p(54),
            rect.right - p(32),
            (rect.bottom - p(236)).max(p(74)),
            1,
        );
        MoveWindow(w.help, p(18), rect.bottom - p(52), p(66), p(32), 1);
        MoveWindow(
            w.stitch,
            rect.right - p(140),
            rect.bottom - p(52),
            p(122),
            p(32),
            1,
        );
        MoveWindow(
            w.folder,
            rect.right - p(255),
            rect.bottom - p(52),
            p(105),
            p(32),
            1,
        );
        let available = (rect.right - p(126)).min(p(380));
        for (controls, bottom) in [(&w.sizes[..], 126), (&w.rates[..], 92)] {
            let width = (available - p(6) * (controls.len() as i32 - 1)) / controls.len() as i32;
            for (index, control) in controls.iter().enumerate() {
                MoveWindow(
                    *control,
                    p(108) + index as i32 * (width + p(6)),
                    rect.bottom - p(bottom),
                    width,
                    p(28),
                    1,
                );
            }
        }
        InvalidateRect(w.list, null(), 0);
        InvalidateRect(self.hwnd.get(), null(), 0);
    }

    fn append(&self, paths: Vec<PathBuf>) {
        if self.core.borrow().busy.is_some() {
            return;
        }
        let drag = self.core.borrow_mut().drag.take();
        if drag.is_some() {
            unsafe {
                ReleaseCapture();
            }
        }
        let mut requests = Vec::new();
        let mut core = self.core.borrow_mut();
        for path in paths {
            if path.is_dir() {
                core.status = Some(format!(
                    "Cannot add {}. Choose video files.",
                    filename(&path)
                ));
                continue;
            }
            let id = core.queue.append(path.clone());
            let canceled = Arc::new(AtomicBool::new(false));
            core.clips.insert(
                id,
                Clip {
                    info: None,
                    thumb: None,
                    canceled: canceled.clone(),
                    thumbnail_done: false,
                },
            );
            requests.push(Request { id, path, canceled });
        }
        if !requests.is_empty() {
            core.status = None;
            core.output = None;
        }
        drop(core);
        for request in requests {
            let _ = self.requests.send(request);
        }
        unsafe {
            self.refresh_list();
            self.refresh();
        }
        self.save();
    }

    unsafe fn refresh_list(&self) {
        let list = self.widgets.get().list;
        if self.core.borrow().drag.is_some() {
            // Resetting native list contents can release mouse capture.
            // The buffered renderer reads new metadata without this reset.
            InvalidateRect(list, null(), 0);
            return;
        }
        let (labels, selected) = {
            let core = self.core.borrow();
            let labels: Vec<_> = core
                .queue
                .entries
                .iter()
                .enumerate()
                .map(|(i, _)| core.label(i))
                .collect();
            (labels, core.queue.selected_index())
        };
        let top = SendMessageW(list, LB_GETTOPINDEX, 0, 0);
        SendMessageW(list, WM_SETREDRAW, 0, 0);
        SendMessageW(list, LB_RESETCONTENT, 0, 0);
        for label in labels {
            let s = wide(&label);
            SendMessageW(list, LB_ADDSTRING, 0, s.as_ptr() as isize);
        }
        if let Some(index) = selected {
            SendMessageW(list, LB_SETCURSEL, index, 0);
        }
        SendMessageW(list, LB_SETTOPINDEX, top.max(0) as usize, 0);
        SendMessageW(list, WM_SETREDRAW, 1, 0);
        InvalidateRect(list, null(), 0);
    }

    unsafe fn refresh_labels(&self, ids: &[u64]) {
        if ids.is_empty() {
            return;
        }
        let list = self.widgets.get().list;
        let (labels, selected) = {
            let core = self.core.borrow();
            // A drag keeps native contents and capture until release or cancellation.
            if core.drag.is_some() {
                return;
            }
            // A burst that changes most rows is cheaper as one list rebuild.
            if ids.len() > core.queue.entries.len() / 2 {
                drop(core);
                self.refresh_list();
                return;
            }
            let dirty: HashSet<_> = ids.iter().copied().collect();
            let labels: Vec<_> = core
                .queue
                .entries
                .iter()
                .enumerate()
                .filter(|(_, entry)| dirty.contains(&entry.id))
                .map(|(index, _)| (index, wide(&core.label(index))))
                .collect();
            (labels, core.queue.selected_index())
        };
        let top = SendMessageW(list, LB_GETTOPINDEX, 0, 0).max(0) as usize;
        SendMessageW(list, WM_SETREDRAW, 0, 0);
        for (index, label) in labels {
            SendMessageW(list, LB_DELETESTRING, index, 0);
            SendMessageW(list, LB_INSERTSTRING, index, label.as_ptr() as isize);
        }
        if let Some(selected) = selected {
            SendMessageW(list, LB_SETCURSEL, selected, 0);
        }
        SendMessageW(list, LB_SETTOPINDEX, top, 0);
        SendMessageW(list, WM_SETREDRAW, 1, 0);
    }

    unsafe fn refresh(&self) {
        let w = self.widgets.get();
        let (busy, help, ready, count, button, output) = {
            let core = self.core.borrow();
            let ready = core.export_plan().is_some();
            let label = core
                .busy
                .as_ref()
                .map(|r| {
                    if core.closing || r.canceled.load(Ordering::Relaxed) {
                        "Canceling…".into()
                    } else {
                        format!("Cancel · {}%", r.percent as u32)
                    }
                })
                .unwrap_or("Stitch it!".into());
            (
                core.busy.is_some(),
                core.help,
                ready,
                core.queue.entries.len(),
                label,
                core.output.is_some(),
            )
        };
        EnableWindow(w.stitch, i32::from((ready || busy) && !help));
        EnableWindow(w.list, i32::from(!busy));
        for control in w.sizes.into_iter().chain(w.rates) {
            EnableWindow(control, i32::from(!busy && !help));
            ShowWindow(control, if help { SW_HIDE } else { SW_SHOW });
        }
        ShowWindow(w.list, if help || count == 0 { SW_HIDE } else { SW_SHOW });
        ShowWindow(
            w.folder,
            if output && !busy && !help {
                SW_SHOW
            } else {
                SW_HIDE
            },
        );
        SetWindowTextW(w.stitch, wide(&button).as_ptr());
        SetWindowTextW(
            self.hwnd.get(),
            wide(&format!(
                "{} — Stitch",
                if count == 0 {
                    "Video stitcher".into()
                } else {
                    format!("{count} clips")
                }
            ))
            .as_ptr(),
        );
        for control in [w.help, w.stitch, w.folder]
            .into_iter()
            .chain(w.sizes)
            .chain(w.rates)
        {
            InvalidateRect(control, null(), 0);
        }
        InvalidateRect(self.hwnd.get(), null(), 0);
    }

    fn select_setting(&self, id: usize) {
        let mut core = self.core.borrow_mut();
        if core.busy.is_some() || core.help || core.dialog || core.closing {
            return;
        }
        let before = core.settings;
        if let Some(preset) = id
            .checked_sub(SIZE_FIRST)
            .and_then(|index| SIZE_CHOICES.get(index))
        {
            core.settings.preset = *preset;
        } else if let Some(rate) = id
            .checked_sub(RATE_FIRST)
            .and_then(|index| RATE_CHOICES.get(index))
        {
            core.settings.frame_rate = *rate;
        }
        if before != core.settings {
            core.status = None;
        }
        drop(core);
        unsafe {
            self.refresh();
        }
        self.save();
    }

    fn save(&self) {
        if self.smoke.is_some() {
            return;
        }
        unsafe {
            SetTimer(self.hwnd.get(), 3, 250, None);
        }
    }

    fn write_state(&self) -> bool {
        unsafe {
            KillTimer(self.hwnd.get(), 3);
        }
        if self.smoke.is_some() {
            return true;
        }
        let core = self.core.borrow();
        let mut placement: WINDOWPLACEMENT = unsafe { std::mem::zeroed() };
        placement.length = std::mem::size_of_val(&placement) as u32;
        unsafe {
            GetWindowPlacement(self.hwnd.get(), &mut placement);
        }
        let r = placement.rcNormalPosition;
        let saved = Saved {
            paths: core.queue.entries.iter().map(|e| e.path.clone()).collect(),
            selected: core.queue.selected_index(),
            output_folder: core.output_folder.clone(),
            rect: Some([r.left, r.top, r.right, r.bottom]),
            export_settings: core.settings,
        };
        drop(core);
        if let Some(parent) = self.state_file.parent()
            && fs::create_dir_all(parent).is_err()
        {
            return false;
        }
        let Ok(bytes) = serde_json::to_vec_pretty(&saved) else {
            return false;
        };
        let partial = self.state_file.with_extension("json.partial");
        if fs::write(&partial, bytes).is_err() {
            return false;
        }
        use std::os::windows::ffi::OsStrExt;
        let from: Vec<_> = partial.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<_> = self
            .state_file
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        unsafe {
            windows_sys::Win32::Storage::FileSystem::MoveFileExW(
                from.as_ptr(),
                to.as_ptr(),
                windows_sys::Win32::Storage::FileSystem::MOVEFILE_REPLACE_EXISTING
                    | windows_sys::Win32::Storage::FileSystem::MOVEFILE_WRITE_THROUGH,
            ) != 0
        }
    }

    unsafe fn add_dialog(&self) {
        if self.core.borrow().busy.is_some() || self.core.borrow().help {
            return;
        }
        self.core.borrow_mut().dialog = true;
        let result = crate::picker::open(self.hwnd.get(), self.smoke.as_deref());
        self.core.borrow_mut().dialog = false;
        match result {
            Ok(paths) if !paths.is_empty() => self.append(paths),
            Ok(_) => {}
            Err(error) => self.error(&format!("Cannot open the file picker: {error}")),
        }
        #[cfg(feature = "native-ui-checks")]
        {
            let foreground = GetForegroundWindow();
            let mut process = 0;
            GetWindowThreadProcessId(foreground, &mut process);
            self.smoke_foreground_before_restore
                .set((foreground as usize, process == std::process::id()));
        }
        let activated = SetForegroundWindow(self.hwnd.get());
        #[cfg(feature = "native-ui-checks")]
        self.smoke_foreground_result.set(activated);
        #[cfg(not(feature = "native-ui-checks"))]
        let _ = activated;
        SetFocus(if self.core.borrow().queue.entries.is_empty() {
            self.hwnd.get()
        } else {
            self.widgets.get().list
        });
    }

    unsafe fn stitch(&self) {
        let canceled = self
            .core
            .borrow()
            .busy
            .as_ref()
            .map(|run| run.canceled.clone());
        if let Some(canceled) = canceled {
            canceled.store(true, Ordering::Relaxed);
            self.refresh();
            return;
        }
        let sources: Vec<Source> = {
            let core = self.core.borrow();
            if core.help || core.export_plan().is_none() {
                return;
            }
            let Some(sources) = core
                .queue
                .entries
                .iter()
                .map(|e| {
                    core.clips
                        .get(&e.id)
                        .and_then(|c| c.info.as_ref())
                        .and_then(|v| v.as_ref().ok())
                        .map(|info| Source {
                            path: e.path.clone(),
                            info: info.clone(),
                        })
                })
                .collect::<Option<Vec<_>>>()
            else {
                return;
            };
            sources
        };
        let folder = self
            .core
            .borrow()
            .output_folder
            .clone()
            .filter(|p| p.is_dir())
            .or_else(|| sources[0].path.parent().map(Path::to_path_buf));
        let folder_text = wide(
            &folder
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
        );
        let name = format!(
            "{}_stitched.mp4",
            sources[0]
                .path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
        );
        let mut buffer = vec![0u16; 32768];
        let name = wide(&name);
        let count = name.len().min(buffer.len());
        buffer[..count].copy_from_slice(&name[..count]);
        let filter = wide("MP4 video\0*.mp4\0");
        let extension = wide("mp4");
        let title = wide("Stitch videos");
        let mut ofn: OPENFILENAMEW = std::mem::zeroed();
        ofn.lStructSize = std::mem::size_of_val(&ofn) as u32;
        ofn.hwndOwner = self.hwnd.get();
        ofn.lpstrFilter = filter.as_ptr();
        ofn.lpstrFile = buffer.as_mut_ptr();
        ofn.nMaxFile = buffer.len() as u32;
        ofn.lpstrDefExt = extension.as_ptr();
        ofn.lpstrTitle = title.as_ptr();
        ofn.lpstrInitialDir = folder_text.as_ptr();
        ofn.Flags = OFN_EXPLORER | OFN_PATHMUSTEXIST | OFN_OVERWRITEPROMPT | OFN_NOCHANGEDIR;
        self.core.borrow_mut().dialog = true;
        let result = GetSaveFileNameW(&mut ofn);
        self.core.borrow_mut().dialog = false;
        if result == 0 {
            if CommDlgExtendedError() != 0 {
                self.error("Cannot open the save dialog.");
            }
            return;
        }
        let destination = PathBuf::from(String::from_utf16_lossy(
            &buffer[..buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len())],
        ));
        self.start_export(sources, destination);
    }

    fn start_export(&self, sources: Vec<Source>, destination: PathBuf) {
        let canceled = Arc::new(AtomicBool::new(false));
        let mut core = self.core.borrow_mut();
        let options = ExportOptions {
            settings: core.settings,
            nvidia: core.nvidia.unwrap_or_default(),
        };
        let Ok(plan) = media::plan(
            &sources
                .iter()
                .map(|source| &source.info)
                .collect::<Vec<_>>(),
            options.settings,
        ) else {
            return;
        };
        core.output_folder = destination.parent().map(Path::to_path_buf);
        core.output = None;
        core.status = None;
        core.busy = Some(Run {
            canceled: canceled.clone(),
            percent: 0.0,
            mode: plan.mode,
            audio_mode: plan.audio_mode,
            reasons: [plan.video_reason, plan.audio_reason]
                .into_iter()
                .flatten()
                .collect(),
            engine: plan.engine(options.nvidia),
            destination: destination.clone(),
        });
        drop(core);
        let runtime = self.runtime.clone();
        let port = self.port.clone();
        *self.exporter.borrow_mut() = Some(thread::spawn(move || {
            let result = runtime.and_then(|runtime| {
                media::export(
                    &runtime,
                    &sources,
                    &destination,
                    options,
                    &canceled,
                    |progress| port.send(Event::Progress(progress)),
                )
            });
            port.send(Event::Finished(result));
        }));
        unsafe {
            self.refresh();
        }
        self.save();
    }

    fn events(&self) {
        // Clear before draining so a concurrent sender can post the next wakeup.
        self.port.pending.store(false, Ordering::Relaxed);
        let mut changed = false;
        let mut labels = Vec::new();
        for event in self.rx.try_iter() {
            match event {
                Event::Nvidia(support) => {
                    self.core.borrow_mut().nvidia = Some(support);
                    changed = true;
                }
                Event::Info(id, info) => {
                    if let Some(clip) = self.core.borrow_mut().clips.get_mut(&id) {
                        clip.info = Some(*info);
                        labels.push(id);
                        changed = true;
                    }
                }
                Event::Thumb(id, thumb) => {
                    if let Some(clip) = self.core.borrow_mut().clips.get_mut(&id) {
                        clip.thumb = thumb;
                        clip.thumbnail_done = true;
                        changed = true;
                    }
                }
                Event::Progress(progress) => {
                    let mut core = self.core.borrow_mut();
                    if progress.engine == Some(EncodeEngine::CpuFallback) {
                        let codec = core.settings.codec;
                        core.nvidia
                            .get_or_insert_with(NvidiaSupport::default)
                            .disable(codec);
                    }
                    if let Some(run) = core.busy.as_mut() {
                        run.percent = progress.percent;
                        run.mode = progress.mode;
                        run.audio_mode = progress.audio_mode;
                        if let Some(reason) = progress.reason
                            && !run.reasons.contains(&reason)
                        {
                            run.reasons.push(reason);
                        }
                        run.engine = progress.engine;
                        changed = true;
                    }
                }
                Event::Finished(result) => {
                    changed = true;
                    if let Some(worker) = self.exporter.borrow_mut().take() {
                        let _ = worker.join();
                    }
                    let mut core = self.core.borrow_mut();
                    let run = core.busy.take();
                    match result {
                        Ok(_) => {
                            if let Some(run) = run {
                                core.status = Some(format!(
                                    "Saved {} · {}{}{}",
                                    filename(&run.destination),
                                    media::copy::action(run.mode, run.audio_mode),
                                    run.engine
                                        .map(|engine| format!(" · {}", engine.label()))
                                        .unwrap_or_default(),
                                    if run.reasons.is_empty() {
                                        String::new()
                                    } else {
                                        format!(
                                            " · {}",
                                            run.reasons
                                                .iter()
                                                .map(|reason| reason.text())
                                                .collect::<Vec<_>>()
                                                .join("; ")
                                        )
                                    }
                                ));
                                core.output = Some(run.destination);
                            }
                        }
                        Err(error) if error == "Canceled" => core.status = Some("Canceled".into()),
                        Err(error) => core.status = Some(format!("Export failed: {error}")),
                    }
                    let closing = core.closing;
                    drop(core);
                    if closing {
                        unsafe {
                            DestroyWindow(self.hwnd.get());
                        }
                        return;
                    }
                }
            }
        }
        if !changed {
            return;
        }
        unsafe {
            self.refresh_labels(&labels);
            InvalidateRect(self.widgets.get().list, null(), 0);
            self.refresh();
        }
    }

    fn key(&self, key: u32, shift: bool, control: bool, alt: bool) -> bool {
        if self.core.borrow().dialog || alt {
            return false;
        }
        if key != VK_ESCAPE as u32 {
            let drag = self.core.borrow_mut().drag.take();
            if drag.is_some() {
                unsafe {
                    ReleaseCapture();
                    self.refresh_list();
                }
            }
        }
        if key == VK_F4 as u32 {
            return false;
        }
        if key == VK_F1 as u32 {
            let core = self.core.borrow();
            let explanation = core.export_explanation();
            let detail = if core.export_plan().is_none() {
                "Add readable local videos to see the export plan.".into()
            } else if explanation.is_empty() {
                "No re-encoding is needed for compatible selected tracks.".into()
            } else {
                explanation
            };
            let message = format!(
                "{}\n\nExport plan: {}\n\n{}\n\nStitch automatically encodes tracks that cannot be copied safely.",
                core.status.as_deref().unwrap_or("Export details"),
                core.preview().0,
                detail
            );
            let focus = if core.queue.entries.is_empty() || core.help || core.busy.is_some() {
                self.hwnd.get()
            } else {
                self.widgets.get().list
            };
            drop(core);
            self.core.borrow_mut().dialog = true;
            unsafe {
                MessageBoxW(
                    self.hwnd.get(),
                    wide(&message).as_ptr(),
                    wide("Export details").as_ptr(),
                    MB_OK | MB_ICONINFORMATION,
                );
            }
            self.core.borrow_mut().dialog = false;
            unsafe {
                SetForegroundWindow(self.hwnd.get());
                SetActiveWindow(self.hwnd.get());
                SetFocus(focus);
            }
            return true;
        }
        if control && key == b'Q' as u32 {
            unsafe {
                PostMessageW(self.hwnd.get(), WM_CLOSE, 0, 0);
            }
            return true;
        }
        if key == VK_ESCAPE as u32 {
            let mut core = self.core.borrow_mut();
            if core.drag.take().is_some() {
                drop(core);
                unsafe {
                    ReleaseCapture();
                    self.refresh_list();
                }
                return true;
            }
            if core.help {
                core.help = false;
                drop(core);
                unsafe {
                    self.refresh();
                    SetFocus(self.widgets.get().list);
                }
                return true;
            }
            return false;
        }
        if !control && key == b'H' as u32 {
            self.core.borrow_mut().help ^= true;
            unsafe {
                self.refresh();
            }
            return true;
        }
        if self.core.borrow().help {
            return false;
        }
        if control && key == b'S' as u32 {
            unsafe {
                self.stitch();
            }
            return true;
        }
        if self.core.borrow().busy.is_some() {
            return false;
        }
        if !control
            && (key == b'F' as u32
                || key == VK_ADD as u32
                || key == VK_SUBTRACT as u32
                || key == VK_OEM_PLUS as u32
                || key == VK_OEM_MINUS as u32)
        {
            let mut core = self.core.borrow_mut();
            let before = core.settings;
            if key == b'F' as u32 {
                core.settings.frame_rate = core.settings.frame_rate.step(!shift);
            } else {
                core.settings.preset = core
                    .settings
                    .preset
                    .step(key == VK_ADD as u32 || key == VK_OEM_PLUS as u32);
            }
            if core.settings != before {
                core.status = None;
            }
            drop(core);
            unsafe {
                self.refresh();
            }
            self.save();
            return true;
        }
        if !control && key == b'A' as u32 {
            unsafe {
                self.add_dialog();
            }
            return true;
        }
        if key == VK_UP as u32 || key == VK_DOWN as u32 {
            let delta = if key == VK_UP as u32 { -1 } else { 1 };
            if shift {
                let mut core = self.core.borrow_mut();
                if core.queue.move_selected(delta) {
                    core.output = None;
                    core.status = None;
                }
            } else {
                self.core.borrow_mut().queue.select_relative(delta);
            }
            unsafe {
                if shift {
                    self.refresh_list();
                }
                let index = self.core.borrow().queue.selected_index();
                if let Some(index) = index {
                    SendMessageW(self.widgets.get().list, LB_SETCURSEL, index, 0);
                }
                SetFocus(self.widgets.get().list);
                InvalidateRect(self.widgets.get().list, null(), 0);
                self.refresh();
            }
            self.save();
            return true;
        }
        if key == VK_DELETE as u32 {
            let mut core = self.core.borrow_mut();
            if let Some(entry) = core.queue.remove_selected() {
                if let Some(clip) = core.clips.remove(&entry.id) {
                    clip.canceled.store(true, Ordering::Relaxed);
                }
                core.output = None;
                core.status = None;
            }
            drop(core);
            unsafe {
                self.refresh_list();
                self.refresh();
            }
            self.save();
            return true;
        }
        false
    }

    unsafe fn error(&self, message: &str) {
        MessageBoxW(
            self.hwnd.get(),
            wide(message).as_ptr(),
            wide("Stitch").as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }

    unsafe fn dropped(&self, hdrop: HDROP) {
        let count = DragQueryFileW(hdrop, u32::MAX, null_mut(), 0);
        let mut paths = Vec::new();
        for index in 0..count {
            let length = DragQueryFileW(hdrop, index, null_mut(), 0);
            let mut path = vec![0u16; length as usize + 1];
            DragQueryFileW(hdrop, index, path.as_mut_ptr(), path.len() as u32);
            paths.push(PathBuf::from(String::from_utf16_lossy(
                &path[..length as usize],
            )));
        }
        DragFinish(hdrop);
        self.append(paths);
    }

    unsafe fn draw(&self, dc: HDC, rect: RECT) {
        fill(dc, rect, BG);
        let brush = CreateSolidBrush(0x00373737);
        FrameRect(dc, &rect, brush);
        DeleteObject(brush as _);
        let d = self.dpi.get();
        let p = |n| dip(n, d);
        text(
            dc,
            self.title.get(),
            "Stitch",
            RECT {
                left: p(20),
                top: p(14),
                right: rect.right - p(20),
                bottom: p(42),
            },
            TEXT,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        );
        let core = self.core.borrow();
        if core.help {
            let x = p(40);
            let top = p(74);
            let right = rect.right - p(40);
            text(
                dc,
                self.title.get(),
                "Help",
                RECT {
                    left: x,
                    top,
                    right,
                    bottom: top + p(34),
                },
                TEXT,
                DT_LEFT | DT_SINGLELINE,
            );
            let rows = [
                ("A", "Add video files"),
                ("↑ / ↓", "Select a clip"),
                ("Shift + ↑ / ↓", "Move the selected clip"),
                ("Ctrl + S", "Choose destination and stitch"),
                ("Delete", "Remove the selected clip"),
                ("+ / −", "Keep source / Small / Medium / Large"),
                ("F / Shift+F", "Original / 30 / 60 fps"),
                ("H / Esc", "Dismiss help"),
                ("F1", "Explain this export"),
                ("Ctrl + Q / Alt + F4", "Quit Stitch"),
            ];
            let row_height = ((rect.bottom - top - p(112)) / rows.len() as i32).clamp(p(16), p(24));
            for (i, (key, description)) in rows.iter().enumerate() {
                let y = top + p(36) + i as i32 * row_height;
                text(
                    dc,
                    self.font.get(),
                    key,
                    RECT {
                        left: x,
                        top: y,
                        right: x + p(140),
                        bottom: y + p(26),
                    },
                    ACCENT,
                    DT_LEFT | DT_SINGLELINE,
                );
                text(
                    dc,
                    self.font.get(),
                    description,
                    RECT {
                        left: x + p(145),
                        top: y,
                        right,
                        bottom: y + p(26),
                    },
                    TEXT,
                    DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS,
                );
            }
            text(
                dc,
                self.small.get(),
                "Drop multiple files to add · Drag rows to reorder",
                RECT {
                    left: x,
                    top: top + p(42) + rows.len() as i32 * row_height,
                    right,
                    bottom: top + p(64) + rows.len() as i32 * row_height,
                },
                MUTED,
                DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS,
            );
        } else if core.queue.entries.is_empty() {
            let cy = (p(54) + rect.bottom - p(182)) / 2;
            text(
                dc,
                self.title.get(),
                "Drop videos here",
                RECT {
                    left: p(24),
                    top: cy - p(34),
                    right: rect.right - p(24),
                    bottom: cy,
                },
                TEXT,
                DT_CENTER | DT_SINGLELINE,
            );
            text(
                dc,
                self.font.get(),
                "or press A to add files",
                RECT {
                    left: p(24),
                    top: cy + p(6),
                    right: rect.right - p(24),
                    bottom: cy + p(34),
                },
                MUTED,
                DT_CENTER | DT_SINGLELINE,
            );
        }
        if !core.help {
            fill(
                dc,
                RECT {
                    left: p(18),
                    top: rect.bottom - p(172),
                    right: rect.right - p(18),
                    bottom: rect.bottom - p(172) + 1,
                },
                LINE,
            );
            let total: Option<f64> = core
                .queue
                .entries
                .iter()
                .map(|entry| {
                    core.clips
                        .get(&entry.id)?
                        .info
                        .as_ref()?
                        .as_ref()
                        .ok()
                        .map(|info| info.duration)
                })
                .sum();
            let (detail, size) = core.preview();
            let mut count = format!(
                "{} {} · Total {}",
                core.queue.entries.len(),
                if core.queue.entries.len() == 1 {
                    "clip"
                } else {
                    "clips"
                },
                total.map(time).unwrap_or_else(|| "—".into())
            );
            if let Some(size) = size {
                count.push_str(&format!(" · {size}"));
            }
            let summary_right = rect.right - p(18);
            let old_font = SelectObject(dc, self.small.get() as _);
            let label = wide(&detail);
            let mut extent: SIZE = std::mem::zeroed();
            GetTextExtentPoint32W(dc, label.as_ptr(), label.len() as i32 - 1, &mut extent);
            SelectObject(dc, old_font);
            let detail_left = summary_right - extent.cx.min((summary_right - p(18)) / 2);
            text(
                dc,
                self.small.get(),
                &count,
                RECT {
                    left: p(18),
                    top: rect.bottom - p(164),
                    right: detail_left - p(16),
                    bottom: rect.bottom - p(140),
                },
                TEXT,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
            );
            if detail_left < summary_right {
                text(
                    dc,
                    self.small.get(),
                    &detail,
                    RECT {
                        left: detail_left,
                        top: rect.bottom - p(164),
                        right: rect.right - p(18),
                        bottom: rect.bottom - p(140),
                    },
                    MUTED,
                    DT_RIGHT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                );
            }
            for (content, bottom) in [("File size", 126), ("Frame rate", 92)] {
                text(
                    dc,
                    self.small.get(),
                    content,
                    RECT {
                        left: p(18),
                        top: rect.bottom - p(bottom),
                        right: p(100),
                        bottom: rect.bottom - p(bottom - 28),
                    },
                    MUTED,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                );
            }
        }
        let status = if core.busy.is_some() {
            core.encoding_reasons()
                .first()
                .map(|reason| format!("{} · F1 for details", reason.text()))
                .unwrap_or_else(|| "Copying selected tracks without re-encoding".into())
        } else if let Some(status) = core.status.as_ref() {
            status.clone()
        } else {
            core.encoding_reasons()
                .first()
                .map(|reason| format!("{} · F1 for details", reason.text()))
                .unwrap_or_default()
        };
        let right = if core.output.is_some() {
            rect.right - p(270)
        } else {
            rect.right - p(154)
        };
        text(
            dc,
            self.small.get(),
            &status,
            RECT {
                left: p(100),
                top: rect.bottom - p(49),
                right,
                bottom: rect.bottom - p(21),
            },
            MUTED,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
        );
    }

    unsafe fn draw_list(&self, dc: HDC, rect: RECT) {
        fill(dc, rect, BG);
        let list = self.widgets.get().list;
        let top = SendMessageW(list, LB_GETTOPINDEX, 0, 0).max(0) as usize;
        let height = dip(74, self.dpi.get());
        let core = self.core.borrow();
        let drag = core.drag.filter(|drag| drag.active);
        let source = drag.and_then(|drag| core.queue.entries.iter().position(|e| e.id == drag.id));
        let target = drag.zip(source).map(|(drag, source)| {
            let gap = drag.gap.min(core.queue.entries.len());
            gap.saturating_sub(usize::from(gap > source))
        });
        for position in top..core.queue.entries.len() {
            let y = (position - top) as i32 * height;
            if y >= rect.bottom {
                break;
            }
            let row = RECT {
                left: 0,
                top: y,
                right: rect.right,
                bottom: y + height,
            };
            if Some(position) == target {
                rounded(dc, row, BG, LINE, dip(5, self.dpi.get()));
            } else {
                let remaining = position - usize::from(target.is_some_and(|t| position > t));
                let index = remaining + usize::from(source.is_some_and(|s| remaining >= s));
                self.draw_row(dc, &core, index, position + 1, row);
            }
        }
        if let (Some(drag), Some(source), Some(target)) = (drag, source, target) {
            let y = (drag.y - drag.offset_y).clamp(0, (rect.bottom - height).max(0));
            let row = RECT {
                left: 0,
                top: y,
                right: rect.right,
                bottom: y + height,
            };
            self.draw_row(dc, &core, source, target + 1, row);
            let brush = CreateSolidBrush(ACCENT);
            FrameRect(dc, &row, brush);
            DeleteObject(brush as _);
        }
    }

    unsafe fn draw_row(&self, dc: HDC, core: &Core, index: usize, number: usize, r: RECT) {
        let d = self.dpi.get();
        let p = |n| dip(n, d);
        let Some(entry) = core.queue.entries.get(index) else {
            return;
        };
        let clip = core.clips.get(&entry.id);
        let selected = Some(entry.id) == core.queue.selected;
        fill(dc, r, if selected { PANEL } else { BG });
        let x = r.left + p(30);
        let y = r.top + p(10);
        if selected {
            fill(
                dc,
                RECT {
                    left: r.left,
                    top: r.top + p(8),
                    right: r.left + p(2),
                    bottom: r.bottom - p(8),
                },
                ACCENT,
            );
        }
        text(
            dc,
            self.small.get(),
            &number.to_string(),
            RECT {
                left: r.left + p(6),
                top: r.top,
                right: x - p(5),
                bottom: r.bottom,
            },
            MUTED,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        let thumbrect = RECT {
            left: x,
            top: y,
            right: x + p(96),
            bottom: y + p(54),
        };
        fill(dc, thumbrect, 0x00251C14);
        if let Some(thumb) = clip.and_then(|c| c.thumb.as_ref()) {
            let mut bitmap: BITMAPINFO = std::mem::zeroed();
            bitmap.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bitmap.bmiHeader.biWidth = THUMB_WIDTH;
            bitmap.bmiHeader.biHeight = -THUMB_HEIGHT;
            bitmap.bmiHeader.biPlanes = 1;
            bitmap.bmiHeader.biBitCount = 24;
            bitmap.bmiHeader.biCompression = BI_RGB;
            SetStretchBltMode(dc, HALFTONE);
            StretchDIBits(
                dc,
                x,
                y,
                p(96),
                p(54),
                0,
                0,
                THUMB_WIDTH,
                THUMB_HEIGHT,
                thumb.as_ptr() as _,
                &bitmap,
                DIB_RGB_COLORS,
                SRCCOPY,
            );
        } else {
            text(
                dc,
                self.small.get(),
                "VIDEO",
                thumbrect,
                MUTED,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
        }
        let start = x + p(110);
        text(
            dc,
            self.font.get(),
            &filename(&entry.path),
            RECT {
                left: start,
                top: y + p(2),
                right: r.right - p(86),
                bottom: y + p(28),
            },
            TEXT,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
        );
        let detail = match clip.and_then(|c| c.info.as_ref()) {
            Some(Ok(_)) => entry
                .path
                .parent()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            Some(Err(_)) => "Cannot read this video · Remove it to continue".into(),
            None => "Reading video…".into(),
        };
        text(
            dc,
            self.small.get(),
            &detail,
            RECT {
                left: start,
                top: y + p(29),
                right: r.right - p(86),
                bottom: y + p(53),
            },
            MUTED,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
        );
        let duration = clip
            .and_then(|c| c.info.as_ref())
            .and_then(|i| i.as_ref().ok())
            .map(|i| time(i.duration))
            .unwrap_or("—".into());
        text(
            dc,
            self.small.get(),
            &duration,
            RECT {
                left: r.right - p(82),
                top: r.top,
                right: r.right - p(32),
                bottom: r.bottom,
            },
            MUTED,
            DT_RIGHT | DT_VCENTER | DT_SINGLELINE,
        );
        text(
            dc,
            self.font.get(),
            "×",
            RECT {
                left: r.right - p(29),
                top: r.top,
                right: r.right - p(5),
                bottom: r.bottom,
            },
            MUTED,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
    }

    unsafe fn draw_item(&self, item: &DRAWITEMSTRUCT) {
        // List rows are painted together in the list's buffered WM_PAINT.
        if item.CtlID == LIST as u32 {
            return;
        }
        let dc = item.hDC;
        let p = |n| dip(n, self.dpi.get());
        let disabled = item.itemState & ODS_DISABLED != 0;
        let pressed = item.itemState & ODS_SELECTED != 0;
        let focus = item.itemState & ODS_FOCUS != 0;
        let r = item.rcItem;
        let selected = {
            let core = self.core.borrow();
            (item.CtlID as usize)
                .checked_sub(SIZE_FIRST)
                .and_then(|index| SIZE_CHOICES.get(index))
                .is_some_and(|preset| *preset == core.settings.preset)
                || (item.CtlID as usize)
                    .checked_sub(RATE_FIRST)
                    .and_then(|index| RATE_CHOICES.get(index))
                    .is_some_and(|rate| *rate == core.settings.frame_rate)
        };
        fill(dc, r, BG);
        rounded(
            dc,
            RECT {
                left: r.left + 1,
                top: r.top + 1,
                right: r.right - 1,
                bottom: r.bottom - 1,
            },
            if pressed || selected { PANEL } else { BG },
            if focus || selected { ACCENT } else { LINE },
            p(5),
        );
        let w = self.widgets.get();
        let control = match item.CtlID as usize {
            HELP => w.help,
            STITCH => w.stitch,
            FOLDER => w.folder,
            id if (SIZE_FIRST..SIZE_FIRST + w.sizes.len()).contains(&id) => {
                w.sizes[id - SIZE_FIRST]
            }
            id if (RATE_FIRST..RATE_FIRST + w.rates.len()).contains(&id) => {
                w.rates[id - RATE_FIRST]
            }
            _ => null_mut(),
        };
        let mut buffer = [0u16; 128];
        let n = GetWindowTextW(control, buffer.as_mut_ptr(), buffer.len() as i32);
        let label = String::from_utf16_lossy(&buffer[..n.max(0) as usize]);
        text(
            dc,
            if (SIZE_FIRST..RATE_FIRST + 3).contains(&(item.CtlID as usize)) {
                self.small.get()
            } else {
                self.font.get()
            },
            &label,
            r,
            if disabled {
                MUTED
            } else if selected {
                ACCENT
            } else {
                TEXT
            },
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        if !disabled
            && matches!(item.CtlID as usize, HELP | STITCH)
            && self.core.borrow().busy.is_none()
        {
            let first = label
                .chars()
                .next()
                .map(|c| c.to_string())
                .unwrap_or_default();
            let mut size: SIZE = std::mem::zeroed();
            let full = wide(&label);
            let previous = SelectObject(dc, self.font.get() as _);
            GetTextExtentPoint32W(dc, full.as_ptr(), full.len() as i32 - 1, &mut size);
            SelectObject(dc, previous);
            let start = (r.right - size.cx) / 2;
            text(
                dc,
                self.font.get(),
                &first,
                RECT {
                    left: start,
                    top: r.top,
                    right: r.right,
                    bottom: r.bottom,
                },
                ACCENT,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            );
        }
    }

    unsafe fn close(&self) {
        self.write_state();
        let mut core = self.core.borrow_mut();
        if let Some(run) = core.busy.as_ref() {
            run.canceled.store(true, Ordering::Relaxed);
            core.closing = true;
            drop(core);
            self.refresh();
        } else {
            drop(core);
            DestroyWindow(self.hwnd.get());
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(run) = self.core.borrow().busy.as_ref() {
            run.canceled.store(true, Ordering::Relaxed);
        }
        for clip in self.core.borrow().clips.values() {
            clip.canceled.store(true, Ordering::Relaxed);
        }
        let _ = self.requests.send(Request {
            id: 0,
            path: PathBuf::new(),
            canceled: Arc::new(AtomicBool::new(true)),
        });
        if let Some(worker) = self.inspector.borrow_mut().take() {
            let _ = worker.join();
        }
        if let Some(worker) = self.exporter.borrow_mut().take() {
            let _ = worker.join();
        }
        unsafe {
            for handle in [self.font.get(), self.small.get(), self.title.get()] {
                if !handle.is_null() {
                    DeleteObject(handle as _);
                }
            }
            DeleteObject(self.brush as _);
        }
    }
}

unsafe fn app(hwnd: HWND) -> Option<&'static App> {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const App;
    ptr.as_ref()
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_NCCREATE {
        let create = &*(lp as *const CREATESTRUCTW);
        let state = &*(create.lpCreateParams as *const App);
        state.hwnd.set(hwnd);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
    }
    let Some(state) = app(hwnd) else {
        return DefWindowProcW(hwnd, msg, wp, lp);
    };
    match msg {
        WM_CREATE => {
            state.initialize();
            0
        }
        WM_NCCALCSIZE => 0,
        WM_NCPAINT => 0,
        WM_NCACTIVATE => DefWindowProcW(hwnd, msg, wp, if IsIconic(hwnd) != 0 { lp } else { -1 }),
        WM_NCHITTEST => {
            let mut r: RECT = std::mem::zeroed();
            GetWindowRect(hwnd, &mut r);
            let x = (lp as u32 & 0xffff) as i16 as i32 - r.left;
            let y = ((lp as u32 >> 16) & 0xffff) as i16 as i32 - r.top;
            let d = state.dpi.get();
            let edge = if IsZoomed(hwnd) != 0 { 0 } else { dip(8, d) };
            let left = x < edge;
            let right = x >= r.right - r.left - edge;
            let top = y < edge;
            let bottom = y >= r.bottom - r.top - edge;
            let hit = match (left, right, top, bottom) {
                (true, _, true, _) => HTTOPLEFT,
                (_, true, true, _) => HTTOPRIGHT,
                (true, _, _, true) => HTBOTTOMLEFT,
                (_, true, _, true) => HTBOTTOMRIGHT,
                (true, _, _, _) => HTLEFT,
                (_, true, _, _) => HTRIGHT,
                (_, _, true, _) => HTTOP,
                (_, _, _, true) => HTBOTTOM,
                _ if y < dip(48, d) => HTCAPTION,
                _ => HTCLIENT,
            };
            hit as isize
        }
        WM_GETMINMAXINFO => {
            let info = &mut *(lp as *mut MINMAXINFO);
            info.ptMinTrackSize = POINT {
                x: dip(480, state.dpi.get()),
                y: dip(360, state.dpi.get()),
            };
            let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            let mut mi: MONITORINFO = std::mem::zeroed();
            mi.cbSize = std::mem::size_of_val(&mi) as u32;
            if GetMonitorInfoW(monitor, &mut mi) != 0 {
                info.ptMaxPosition = POINT {
                    x: mi.rcWork.left - mi.rcMonitor.left,
                    y: mi.rcWork.top - mi.rcMonitor.top,
                };
                info.ptMaxSize = POINT {
                    x: mi.rcWork.right - mi.rcWork.left,
                    y: mi.rcWork.bottom - mi.rcWork.top,
                };
            }
            0
        }
        WM_SIZE => {
            state.layout();
            0
        }
        WM_DPICHANGED => {
            state.dpi.set((wp & 0xffff) as u32);
            state.fonts();
            let w = state.widgets.get();
            SendMessageW(
                w.list,
                LB_SETITEMHEIGHT,
                0,
                dip(74, state.dpi.get()) as isize,
            );
            let r = &*(lp as *const RECT);
            SetWindowPos(
                hwnd,
                null_mut(),
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            state.layout();
            0
        }
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            let mut r: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut r);
            let memory = CreateCompatibleDC(dc);
            let bitmap = CreateCompatibleBitmap(dc, r.right.max(1), r.bottom.max(1));
            let old = SelectObject(memory, bitmap as _);
            state.draw(memory, r);
            BitBlt(dc, 0, 0, r.right, r.bottom, memory, 0, 0, SRCCOPY);
            SelectObject(memory, old);
            DeleteObject(bitmap as _);
            DeleteDC(memory);
            EndPaint(hwnd, &ps);
            0
        }
        WM_DRAWITEM => {
            let item = &*(lp as *const DRAWITEMSTRUCT);
            let saved = SaveDC(item.hDC);
            state.draw_item(item);
            if saved != 0 {
                RestoreDC(item.hDC, saved);
            }
            1
        }
        WM_CTLCOLORLISTBOX | WM_CTLCOLORBTN => {
            SetTextColor(wp as HDC, TEXT);
            SetBkColor(wp as HDC, BG);
            state.brush as isize
        }
        WM_COMMAND => {
            let id = wp & 0xffff;
            let notification = (wp >> 16) & 0xffff;
            if id == LIST && notification == LBN_SELCHANGE as usize {
                let index = SendMessageW(state.widgets.get().list, LB_GETCURSEL, 0, 0);
                let mut core = state.core.borrow_mut();
                core.queue.selected = core.queue.entries.get(index as usize).map(|e| e.id);
                drop(core);
                state.save();
                InvalidateRect(state.widgets.get().list, null(), 0);
            } else if notification == BN_CLICKED as usize {
                match id {
                    HELP => {
                        state.core.borrow_mut().help ^= true;
                        state.refresh();
                    }
                    STITCH => state.stitch(),
                    id if (SIZE_FIRST..SIZE_FIRST + 4).contains(&id)
                        || (RATE_FIRST..RATE_FIRST + 3).contains(&id) =>
                    {
                        state.select_setting(id)
                    }
                    FOLDER => {
                        let folder = state
                            .core
                            .borrow()
                            .output
                            .as_ref()
                            .and_then(|p| p.parent())
                            .map(|p| p.to_string_lossy().into_owned());
                        if let Some(folder) = folder {
                            ShellExecuteW(
                                hwnd,
                                wide("open").as_ptr(),
                                wide(&folder).as_ptr(),
                                null(),
                                null(),
                                SW_SHOWNORMAL,
                            );
                        }
                    }
                    _ => {}
                }
            }
            0
        }
        WM_DROPFILES => {
            state.dropped(wp as HDROP);
            0
        }
        EVENTS => {
            state.events();
            0
        }
        WM_TIMER if wp == 3 => {
            if !state.write_state() && !state.core.borrow().closing {
                SetTimer(hwnd, 3, 1000, None);
            }
            0
        }
        #[cfg(feature = "native-ui-checks")]
        WM_TIMER if wp == 1 => {
            state.smoke_tick();
            0
        }
        #[cfg(feature = "native-ui-checks")]
        WM_TIMER if wp == 2 && state.smoke.is_some() => {
            let popup = GetLastActivePopup(hwnd);
            if popup != hwnd && IsWindowVisible(popup) != 0 {
                let ticks = state.smoke_picker_ticks.get() + 1;
                state.smoke_picker_ticks.set(ticks);
                let foreground = GetForegroundWindow();
                let mut owner_process = 0;
                let mut foreground_process = 0;
                GetWindowThreadProcessId(hwnd, &mut owner_process);
                GetWindowThreadProcessId(foreground, &mut foreground_process);
                let owned = owner_process != 0 && owner_process == foreground_process;
                state
                    .smoke_picker_owned_foreground
                    .set(owned && (ticks == 1 || state.smoke_picker_owned_foreground.get()));
                let folder = state.smoke.as_ref().unwrap();
                if ticks == 1 {
                    state.smoke_picker_foreground.set(foreground as usize);
                    let _ = smoke::capture(popup, &folder.join("picker-first.bmp"));
                } else if ticks >= 6 {
                    let _ = smoke::capture(popup, &folder.join("picker-ready.bmp"));
                    PostMessageW(popup, WM_CLOSE, 0, 0);
                    KillTimer(hwnd, 2);
                }
            }
            0
        }
        WM_CLOSE => {
            state.close();
            0
        }
        WM_DESTROY => {
            state.port.hwnd.store(0, Ordering::Relaxed);
            PostQuitMessage(0);
            0
        }
        WM_NCDESTROY => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe extern "system" fn list_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    _: usize,
    data: usize,
) -> LRESULT {
    let state = &*(data as *const App);
    match msg {
        WM_NCDESTROY => {
            RemoveWindowSubclass(hwnd, Some(list_proc), 1);
            return DefSubclassProc(hwnd, msg, wp, lp);
        }
        WM_ERASEBKGND => return 1,
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            let mut rect: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut rect);
            let memory = CreateCompatibleDC(dc);
            let bitmap = CreateCompatibleBitmap(dc, rect.right.max(1), rect.bottom.max(1));
            let old = SelectObject(memory, bitmap as _);
            state.draw_list(memory, rect);
            BitBlt(dc, 0, 0, rect.right, rect.bottom, memory, 0, 0, SRCCOPY);
            SelectObject(memory, old);
            DeleteObject(bitmap as _);
            DeleteDC(memory);
            EndPaint(hwnd, &ps);
            return 0;
        }
        WM_PRINTCLIENT => {
            let mut rect: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut rect);
            state.draw_list(wp as HDC, rect);
            return 0;
        }
        WM_VSCROLL | WM_MOUSEWHEEL => {
            let result = DefSubclassProc(hwnd, msg, wp, lp);
            InvalidateRect(hwnd, null(), 0);
            return result;
        }
        WM_DROPFILES => {
            state.dropped(wp as HDROP);
            return 0;
        }
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK if state.core.borrow().busy.is_none() => {
            let x = (lp as u32 & 0xffff) as i16 as i32;
            let y = ((lp as u32 >> 16) & 0xffff) as i16 as i32;
            let hit = SendMessageW(hwnd, LB_ITEMFROMPOINT, 0, lp) as u32;
            let index = (hit & 0xffff) as usize;
            let mut rect: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut rect);
            let mut item: RECT = std::mem::zeroed();
            let in_item = hit >> 16 == 0
                && SendMessageW(hwnd, LB_GETITEMRECT, index, &mut item as *mut RECT as isize)
                    != LB_ERR as isize
                && y >= item.top
                && y < item.bottom;
            SetFocus(hwnd);
            if in_item {
                let id = state
                    .core
                    .borrow()
                    .queue
                    .entries
                    .get(index)
                    .map(|entry| entry.id);
                let Some(id) = id else {
                    return 0;
                };
                state.core.borrow_mut().queue.selected = Some(id);
                SendMessageW(hwnd, LB_SETCURSEL, index, 0);
                InvalidateRect(hwnd, null(), 0);
                state.save();
                if x > rect.right - dip(30, state.dpi.get()) {
                    state.key(VK_DELETE as u32, false, false, false);
                } else {
                    SetCapture(hwnd);
                    state.core.borrow_mut().drag = Some(Drag {
                        id,
                        start_x: x,
                        start_y: y,
                        offset_y: y - item.top,
                        y,
                        active: false,
                        gap: index,
                    });
                }
            }
            return 0;
        }
        WM_MOUSEMOVE => {
            let x = (lp as u32 & 0xffff) as i16 as i32;
            let y = ((lp as u32 >> 16) & 0xffff) as i16 as i32;
            let drag = state.core.borrow().drag;
            if let Some(mut drag) = drag {
                if (x - drag.start_x).abs() > GetSystemMetricsForDpi(SM_CXDRAG, state.dpi.get())
                    || (y - drag.start_y).abs() > GetSystemMetricsForDpi(SM_CYDRAG, state.dpi.get())
                {
                    drag.active = true;
                }
                if drag.active {
                    let mut r: RECT = std::mem::zeroed();
                    GetClientRect(hwnd, &mut r);
                    if y < dip(16, state.dpi.get()) {
                        SendMessageW(hwnd, WM_VSCROLL, SB_LINEUP as usize, 0);
                    } else if y > r.bottom - dip(16, state.dpi.get()) {
                        SendMessageW(hwnd, WM_VSCROLL, SB_LINEDOWN as usize, 0);
                    }
                    let top = SendMessageW(hwnd, LB_GETTOPINDEX, 0, 0).max(0) as usize;
                    let height = dip(74, state.dpi.get());
                    let count = state.core.borrow().queue.entries.len();
                    let gap = (top + ((y + height / 2).max(0) / height) as usize).min(count);
                    drag.y = y;
                    drag.gap = gap;
                    state.core.borrow_mut().drag = Some(drag);
                    InvalidateRect(hwnd, null(), 0);
                    UpdateWindow(hwnd);
                }
                // The app controls the pointer during a drag.
                // The list control must not change selection during this drag.
                return 0;
            }
        }
        WM_LBUTTONUP => {
            let drag = state.core.borrow_mut().drag.take();
            ReleaseCapture();
            if let Some(drag) = drag {
                if drag.active {
                    let mut core = state.core.borrow_mut();
                    core.queue.move_to_gap(drag.id, drag.gap);
                    core.output = None;
                    core.status = None;
                    drop(core);
                    state.refresh_list();
                    state.refresh();
                    state.save();
                } else {
                    state.refresh_list();
                }
                InvalidateRect(hwnd, null(), 0);
                return 0;
            }
        }
        WM_CANCELMODE => {
            let drag = state.core.borrow_mut().drag.take();
            if GetCapture() == hwnd {
                ReleaseCapture();
            }
            if drag.is_some() {
                state.refresh_list();
            }
            return 0;
        }
        WM_CAPTURECHANGED => {
            if lp as HWND != hwnd {
                let drag = state.core.borrow_mut().drag.take();
                if drag.is_some() {
                    state.refresh_list();
                }
            }
            InvalidateRect(hwnd, null(), 0);
        }
        _ => {}
    }
    DefSubclassProc(hwnd, msg, wp, lp)
}

unsafe fn fill(dc: HDC, r: RECT, color: u32) {
    let brush = CreateSolidBrush(color);
    FillRect(dc, &r, brush);
    DeleteObject(brush as _);
}
unsafe fn rounded(dc: HDC, r: RECT, color: u32, border: u32, radius: i32) {
    let brush = CreateSolidBrush(color);
    let pen = CreatePen(PS_SOLID, 1, border);
    let b = SelectObject(dc, brush as _);
    let p = SelectObject(dc, pen as _);
    RoundRect(dc, r.left, r.top, r.right, r.bottom, radius * 2, radius * 2);
    SelectObject(dc, b);
    SelectObject(dc, p);
    DeleteObject(brush as _);
    DeleteObject(pen as _);
}
unsafe fn text(dc: HDC, font: HFONT, value: &str, mut r: RECT, color: u32, flags: u32) {
    let old = SelectObject(dc, font as _);
    SetBkMode(dc, TRANSPARENT as i32);
    SetTextColor(dc, color);
    let text = wide(value);
    DrawTextW(
        dc,
        text.as_ptr(),
        text.len() as i32 - 1,
        &mut r,
        flags | DT_NOPREFIX,
    );
    SelectObject(dc, old);
}

pub fn run(args: Vec<String>, runtime: media::Result<Runtime>) {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        #[cfg(not(feature = "native-ui-checks"))]
        let smoke: Option<PathBuf> = None;
        #[cfg(feature = "native-ui-checks")]
        let smoke = args
            .first()
            .is_some_and(|s| s == "--ui-smoke")
            .then(|| args.get(1).map(PathBuf::from))
            .flatten();
        let state_folder = if let Some(folder) = smoke.as_ref() {
            folder.clone()
        } else if args.first().is_some_and(|s| s == "--state-dir") {
            args.get(1).map(PathBuf::from).unwrap_or_default()
        } else {
            PathBuf::from(
                std::env::var_os("LOCALAPPDATA")
                    .unwrap_or_else(|| std::env::temp_dir().into_os_string()),
            )
            .join("Stitch")
        };
        let state_file = state_folder.join("state.json");
        let default_state = smoke.is_none() && !args.first().is_some_and(|s| s == "--state-dir");
        let saved: Saved = fs::read(&state_file)
            .or_else(|error| {
                if default_state && error.kind() == std::io::ErrorKind::NotFound {
                    fs::read(state_folder.with_file_name("Stich").join("state.json"))
                } else {
                    Err(error)
                }
            })
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        let initial = if smoke.is_some() || args.first().is_some_and(|s| s == "--state-dir") {
            if args.len() > 2 {
                args.iter().skip(2).map(PathBuf::from).collect()
            } else {
                saved.paths.clone()
            }
        } else if !args.is_empty() {
            args.iter().map(PathBuf::from).collect()
        } else {
            saved.paths.clone()
        };
        if let Some(folder) = smoke.as_ref() {
            let _ = fs::create_dir_all(folder);
        }
        let state = Box::new(App::new(runtime, state_file, smoke));
        state.core.borrow_mut().output_folder = saved.output_folder;
        state.core.borrow_mut().settings = simplified_settings(saved.export_settings);
        let instance = GetModuleHandleW(null());
        let class = wide("Stitch.Main");
        let mut wc: WNDCLASSW = std::mem::zeroed();
        wc.lpfnWndProc = Some(window_proc);
        wc.hInstance = instance;
        wc.lpszClassName = class.as_ptr();
        wc.hCursor = LoadCursorW(null_mut(), IDC_ARROW);
        RegisterClassW(&wc);
        let mut work: RECT = std::mem::zeroed();
        SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut work as *mut RECT as _, 0);
        let mut rect = [
            work.left + (work.right - work.left - 720) / 2,
            work.top + (work.bottom - work.top - 520) / 2,
            720,
            520,
        ];
        if let Some(r) = saved.rect {
            let area = RECT {
                left: r[0],
                top: r[1],
                right: r[2],
                bottom: r[3],
            };
            if !MonitorFromRect(&area, MONITOR_DEFAULTTONULL).is_null()
                && r[2] - r[0] >= 480
                && r[3] - r[1] >= 360
            {
                rect = [r[0], r[1], r[2] - r[0], r[3] - r[1]];
            }
        }
        let hwnd = CreateWindowExW(
            WS_EX_APPWINDOW,
            class.as_ptr(),
            wide("Stitch").as_ptr(),
            WS_POPUP
                | WS_THICKFRAME
                | WS_MINIMIZEBOX
                | WS_MAXIMIZEBOX
                | WS_SYSMENU
                | WS_CLIPCHILDREN,
            rect[0],
            rect[1],
            rect[2],
            rect[3],
            null_mut(),
            null_mut(),
            instance,
            &*state as *const App as _,
        );
        if hwnd.is_null() {
            MessageBoxW(
                null_mut(),
                wide("Cannot create the Stitch window.").as_ptr(),
                wide("Stitch").as_ptr(),
                MB_OK | MB_ICONERROR,
            );
            return;
        }
        state.append(initial);
        if let Some(index) = saved.selected {
            let id = state.core.borrow().queue.entries.get(index).map(|e| e.id);
            if let Some(id) = id {
                state.core.borrow_mut().queue.selected = Some(id);
                state.refresh_list();
            }
        }
        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);
        SetFocus(state.widgets.get().list);
        #[cfg(feature = "native-ui-checks")]
        if state.smoke.is_some() {
            SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
            SetForegroundWindow(hwnd);
            SetTimer(hwnd, 1, 650, None);
        }
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            if msg.message == WM_KEYDOWN && (msg.hwnd == hwnd || IsChild(hwnd, msg.hwnd) != 0) {
                let repeat = msg.lParam & (1 << 30) != 0;
                let key = msg.wParam as u32;
                let one_shot = matches!(key, 0x41 | 0x48 | 0x53 | 0x51 | 0x45 | 0x46);
                if repeat && one_shot {
                    continue;
                }
                if state.key(
                    key,
                    GetKeyState(VK_SHIFT as i32) < 0,
                    GetKeyState(VK_CONTROL as i32) < 0,
                    GetKeyState(VK_MENU as i32) < 0,
                ) {
                    continue;
                }
            }
            if IsDialogMessageW(hwnd, &msg) != 0 {
                continue;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        // The App remains alive until all supervised workers stop.
        drop(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready_core() -> Core {
        let info = media::parse_probe(&serde_json::json!({"streams":[{"index":0,"codec_type":"video","codec_name":"h264","width":1920,"height":1080,"duration":"8","avg_frame_rate":"30/1","bit_rate":"4000000","extradata_hash":"SHA256:one"}]})).unwrap();
        let mut queue = Queue::default();
        let id = queue.append("one.mp4".into());
        let mut clips = HashMap::new();
        clips.insert(
            id,
            Clip {
                info: Some(Ok(info)),
                thumb: None,
                canceled: Arc::new(AtomicBool::new(false)),
                thumbnail_done: true,
            },
        );
        Core {
            queue,
            clips,
            busy: None,
            help: false,
            closing: false,
            dialog: false,
            status: None,
            output: None,
            output_folder: None,
            drag: None,
            settings: ExportSettings::default(),
            nvidia: Some(NvidiaSupport {
                h264: true,
                hevc: true,
            }),
            #[cfg(feature = "native-ui-checks")]
            smoke_stage: 0,
        }
    }

    #[test]
    fn quality_conversion_keeps_an_estimate_when_copy_falls_back() {
        let mut core = ready_core();
        assert_eq!(
            core.preview(),
            ("Copy video".into(), Some("Est. ≈ 4 MB".into()))
        );
        core.busy = Some(Run {
            canceled: Arc::new(AtomicBool::new(false)),
            percent: 0.0,
            mode: ExportMode::Convert,
            audio_mode: AudioMode::None,
            reasons: vec![],
            engine: Some(EncodeEngine::Cpu),
            destination: "out.mp4".into(),
        });
        let summary = core.preview();
        assert_eq!(summary, ("Encode video".into(), Some("Est. ≈ 4 MB".into())));
    }

    #[test]
    fn active_cpu_fallback_overrides_available_nvidia_in_preview() {
        let mut core = ready_core();
        core.settings = ExportSettings {
            preset: ExportPreset::Medium,
            codec: VideoCodec::H265,
            frame_rate: FrameRate::Fps30,
        };
        assert!(core.preview().1.unwrap().starts_with("Target ≈"));
        core.busy = Some(Run {
            canceled: Arc::new(AtomicBool::new(false)),
            percent: 0.0,
            mode: ExportMode::Convert,
            audio_mode: AudioMode::None,
            reasons: vec![],
            engine: Some(EncodeEngine::CpuFallback),
            destination: "out.mp4".into(),
        });
        let (detail, size) = core.preview();
        assert_eq!(detail, "CPU retry · Encode video");
        assert!(size.unwrap().starts_with("Target ≈"));
        let legacy: Saved = serde_json::from_str(r#"{"paths":["one.mp4"],"selected":0}"#).unwrap();
        assert_eq!(legacy.export_settings, ExportSettings::default());
    }

    #[test]
    fn simplified_settings_migrate_format_and_old_fixed_rates() {
        let mut core = ready_core();
        assert_eq!(core.preview().0, "Copy video");
        core.settings.frame_rate = FrameRate::Fps60;
        assert_eq!(core.preview().0, "Encode video");
        let legacy = simplified_settings(ExportSettings {
            codec: VideoCodec::H264,
            frame_rate: FrameRate::Fps23976,
            ..ExportSettings::default()
        });
        assert_eq!(legacy.codec, VideoCodec::H265);
        assert_eq!(legacy.frame_rate, FrameRate::Fps30);
        assert_eq!(FrameRate::Fps50.simplified(), FrameRate::Fps60);
    }

    #[test]
    fn hdr_video_can_export_at_60_fps_without_an_extra_setting() {
        let mut core = ready_core();
        for clip in core.clips.values_mut() {
            let info = clip.info.as_mut().unwrap().as_mut().unwrap();
            info.hdr = true;
            info.color.bit_depth = 10;
            info.color.transfer = Some(16);
        }
        core.settings.frame_rate = FrameRate::Fps60;
        let plan = core.export_plan().unwrap();
        assert_eq!(plan.mode, ExportMode::Convert);
        assert_eq!(
            plan.engine(core.nvidia.unwrap()),
            Some(EncodeEngine::Nvidia)
        );
        assert_eq!(
            core.preview(),
            ("Encode video".into(), Some("Est. ≈ 8 MB".into()))
        );
    }

    #[test]
    fn export_details_name_audio_reasons_without_claiming_video_encoding() {
        let mut core = ready_core();
        let info = core
            .clips
            .values_mut()
            .next()
            .unwrap()
            .info
            .as_mut()
            .unwrap()
            .as_mut()
            .unwrap();
        info.audio_index = Some(1);
        assert_eq!(core.preview().0, "Copy video · encode audio");
        let detail = core.export_explanation();
        assert!(detail.contains("Clip 1: Audio copying could not be verified"));
        assert!(detail.contains("one.mp4"));
        core.settings.preset = ExportPreset::Small;
        let detail = core.export_explanation();
        assert!(detail.contains("size choice requires video encoding"));
        assert!(detail.contains("Audio copying could not be verified"));
    }
}
