use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    os::windows::{fs::OpenOptionsExt, io::AsRawHandle, process::CommandExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, FILE_SHARE_READ, GetFileInformationByHandle,
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    },
    System::{
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        },
        Threading::CREATE_NO_WINDOW,
    },
};

pub type Result<T> = std::result::Result<T, String>;
pub const THUMB_WIDTH: i32 = 96;
pub const THUMB_HEIGHT: i32 = 54;

#[derive(Clone, Debug)]
pub struct Runtime {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}

impl Runtime {
    pub fn beside(exe: &Path) -> Result<Self> {
        let folder = exe.parent().ok_or("Cannot find the application folder.")?;
        let runtime = Self {
            ffmpeg: folder.join("runtime/ffmpeg.exe"),
            ffprobe: folder.join("runtime/ffprobe.exe"),
        };
        if !runtime.ffmpeg.is_file() || !runtime.ffprobe.is_file() {
            return Err("The video engine is missing. Run scripts/build.ps1 to prepare the complete application folder.".into());
        }
        Ok(runtime)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClipInfo {
    #[serde(default)]
    pub file_stamp: Option<(u64, u64)>,
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub rate: String,
    pub video_index: usize,
    pub audio_index: Option<usize>,
    pub audio_offset: f64,
    pub audio_rate: u32,
    pub audio_channels: u32,
    pub audio_layout: String,
    pub codec: String,
    #[serde(default)]
    pub video_bitrate: Option<f64>,
    pub hdr: bool,
    #[serde(default)]
    pub color: VideoColor,
    pub copy_key: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoColor {
    pub bit_depth: u8,
    pub primaries: Option<u8>,
    pub transfer: Option<u8>,
    pub matrix: Option<u8>,
    pub full_range: bool,
    pub dolby_vision_profile: Option<u8>,
}

impl Default for VideoColor {
    fn default() -> Self {
        Self {
            bit_depth: 8,
            primaries: None,
            transfer: None,
            matrix: None,
            full_range: false,
            dolby_vision_profile: None,
        }
    }
}

impl VideoColor {
    fn resolved(self, info: &ClipInfo) -> Self {
        let sd = info.width < 1280 && info.height < 720;
        Self {
            primaries: Some(self.primaries.unwrap_or(if info.hdr { 9 } else { 1 })),
            transfer: Some(self.transfer.unwrap_or(if info.hdr { 16 } else { 1 })),
            matrix: Some(self.matrix.unwrap_or(if info.hdr {
                9
            } else if sd {
                6
            } else {
                1
            })),
            ..self
        }
    }

    fn pixel_format(self, hardware: bool) -> &'static str {
        if self.bit_depth > 8 {
            if hardware { "p010le" } else { "yuv420p10le" }
        } else {
            "yuv420p"
        }
    }

    fn encoding_args(self) -> Vec<String> {
        [
            ("-color_primaries", self.primaries.unwrap()),
            ("-color_trc", self.transfer.unwrap()),
            ("-colorspace", self.matrix.unwrap()),
            ("-color_range", if self.full_range { 2 } else { 1 }),
        ]
        .into_iter()
        .flat_map(|(key, value)| [key.to_owned(), value.to_string()])
        .collect()
    }
}

fn conversion_color(infos: &[&ClipInfo], codec: VideoCodec) -> VideoColor {
    let hdr = codec == VideoCodec::H265 && infos.iter().any(|info| info.hdr);
    let hlg = hdr && infos.iter().all(|info| info.color.transfer == Some(18));
    VideoColor {
        bit_depth: if codec == VideoCodec::H265
            && (hdr || infos.iter().any(|info| info.color.bit_depth > 8))
        {
            10
        } else {
            8
        },
        primaries: Some(if hdr { 9 } else { 1 }),
        transfer: Some(if hlg {
            18
        } else if hdr {
            16
        } else {
            1
        }),
        matrix: Some(if hdr { 9 } else { 1 }),
        ..VideoColor::default()
    }
}

fn check_conversion_color(info: &ClipInfo) -> Result<()> {
    if info.color.dolby_vision_profile == Some(5) {
        return Err("This Dolby Vision video has no standard HDR base layer. Convert it to HDR10 or HLG before adding it.".into());
    }
    Ok(())
}

fn color_filter(info: &ClipInfo, output: VideoColor) -> Result<String> {
    check_conversion_color(info)?;
    let input = info.color.resolved(info);
    let initial = format!(
        "zscale=pin={}:tin={}:min={}:rin={}:npl=100:agamma=0",
        input.primaries.unwrap(),
        input.transfer.unwrap(),
        input.matrix.unwrap(),
        if input.full_range { "full" } else { "limited" }
    );
    let tone_map = info.hdr && output.transfer == Some(1);
    let normalize = format!(
        "p={}:t={}:m={}:r=limited:dither=error_diffusion",
        output.primaries.unwrap(),
        output.transfer.unwrap(),
        output.matrix.unwrap()
    );
    let mut filter = if tone_map {
        // Map HDR highlights to SDR only for the explicit legacy H.264 library output.
        // Linear frames cannot identify PQ/HLG for automatic peak inference.
        let peak = if input.transfer == Some(18) { 10 } else { 100 };
        format!(
            "{initial}:t=linear,format=gbrpf32le,tonemap=tonemap=mobius:desat=2:peak={peak},zscale=npl=100:agamma=0:{normalize}"
        )
    } else {
        format!("{initial}:{normalize}")
    };
    filter.push_str(&format!(",format={}", output.pixel_format(false)));
    if tone_map {
        filter.push_str(",limiter=min=16:max=235:planes=1");
    }
    // Metadata for a source color grade cannot describe a newly combined timeline.
    for kind in [
        "MASTERING_DISPLAY_METADATA",
        "CONTENT_LIGHT_LEVEL",
        "DYNAMIC_HDR_PLUS",
        "DOVI_RPU_BUFFER",
        "DOVI_METADATA",
        "DYNAMIC_HDR_VIVID",
    ] {
        filter.push_str(&format!(",sidedata=mode=delete:type={kind}"));
    }
    Ok(filter)
}

#[derive(Clone)]
pub struct Source {
    pub path: PathBuf,
    pub info: ClipInfo,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum ExportMode {
    Copy,
    Convert,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportPreset {
    Small,
    Medium,
    Large,
    #[default]
    #[serde(other)]
    Source,
}

impl ExportPreset {
    pub fn step(self, increase: bool) -> Self {
        match (self, increase) {
            (Self::Source, _) => Self::Medium,
            (Self::Small, false) => Self::Small,
            (Self::Small, true) | (Self::Large, false) => Self::Medium,
            (Self::Medium, false) => Self::Small,
            (Self::Medium, true) | (Self::Large, true) => Self::Large,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Source => "Keep source",
            Self::Small => "Small",
            Self::Medium => "Medium",
            Self::Large => "Large",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoCodec {
    H264,
    #[default]
    #[serde(other)]
    H265,
}

impl VideoCodec {
    pub fn label(self) -> &'static str {
        match self {
            Self::H264 => "H.264",
            Self::H265 => "H.265",
        }
    }

    fn probe_name(self) -> &'static str {
        match self {
            Self::H264 => "h264",
            Self::H265 => "hevc",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportSettings {
    pub preset: ExportPreset,
    pub codec: VideoCodec,
    pub frame_rate: FrameRate,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameRate {
    Fps23976,
    Fps24,
    Fps25,
    Fps2997,
    Fps30,
    Fps50,
    Fps5994,
    Fps60,
    #[default]
    #[serde(other)]
    Original,
}

impl FrameRate {
    pub fn step(self, increase: bool) -> Self {
        const RATES: [FrameRate; 3] = [FrameRate::Original, FrameRate::Fps30, FrameRate::Fps60];
        let index = RATES
            .iter()
            .position(|rate| *rate == self.simplified())
            .unwrap();
        RATES[(index + if increase { 1 } else { RATES.len() - 1 }) % RATES.len()]
    }

    pub fn simplified(self) -> Self {
        match self.fps() {
            None => Self::Original,
            Some(fps) if fps >= 45.0 => Self::Fps60,
            Some(_) => Self::Fps30,
        }
    }

    pub fn rate(self) -> Option<&'static str> {
        match self {
            Self::Original => None,
            Self::Fps23976 => Some("24000/1001"),
            Self::Fps24 => Some("24/1"),
            Self::Fps25 => Some("25/1"),
            Self::Fps2997 => Some("30000/1001"),
            Self::Fps30 => Some("30/1"),
            Self::Fps50 => Some("50/1"),
            Self::Fps5994 => Some("60000/1001"),
            Self::Fps60 => Some("60/1"),
        }
    }

    pub fn fps(self) -> Option<f64> {
        self.rate().and_then(rate)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Original => "Original frame rates",
            Self::Fps23976 => "Fixed 23.976 fps",
            Self::Fps24 => "Fixed 24 fps",
            Self::Fps25 => "Fixed 25 fps",
            Self::Fps2997 => "Fixed 29.97 fps",
            Self::Fps30 => "Fixed 30 fps",
            Self::Fps50 => "Fixed 50 fps",
            Self::Fps5994 => "Fixed 59.94 fps",
            Self::Fps60 => "Fixed 60 fps",
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NvidiaSupport {
    pub h264: bool,
    pub hevc: bool,
}

impl NvidiaSupport {
    pub fn supports(self, codec: VideoCodec) -> bool {
        match codec {
            VideoCodec::H264 => self.h264,
            VideoCodec::H265 => self.hevc,
        }
    }

    pub fn disable(&mut self, codec: VideoCodec) {
        match codec {
            VideoCodec::H264 => self.h264 = false,
            VideoCodec::H265 => self.hevc = false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ExportOptions {
    pub settings: ExportSettings,
    pub nvidia: NvidiaSupport,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum EncodeEngine {
    Cpu,
    Nvidia,
    CpuFallback,
}

impl EncodeEngine {
    pub fn label(self) -> &'static str {
        match self {
            Self::Cpu => "CPU",
            Self::Nvidia => "NVIDIA",
            Self::CpuFallback => "CPU fallback",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ExportPlan {
    pub mode: ExportMode,
    pub codec: VideoCodec,
    pub video_bitrate: Option<u64>,
    pub estimated_bytes: Option<f64>,
    pub nvidia_allowed: bool,
}

impl ExportPlan {
    pub fn engine(self, nvidia: NvidiaSupport) -> Option<EncodeEngine> {
        (self.mode == ExportMode::Convert).then(|| {
            if self.nvidia_allowed && nvidia.supports(self.codec) {
                EncodeEngine::Nvidia
            } else {
                EncodeEngine::Cpu
            }
        })
    }
}

pub fn plan(infos: &[&ClipInfo], settings: ExportSettings) -> Result<ExportPlan> {
    let first = infos.first().ok_or("Add a video before stitching.")?;
    let copy = settings.preset == ExportPreset::Source
        && settings.frame_rate == FrameRate::Original
        && copy_compatible(infos.iter().copied());
    if !copy {
        for info in infos {
            check_conversion_color(info)?;
        }
    }
    let codec = if copy {
        if first.codec == "hevc" {
            VideoCodec::H265
        } else {
            VideoCodec::H264
        }
    } else {
        settings.codec
    };
    let bitrate = target_bitrate(infos, settings);
    let total: f64 = infos.iter().map(|info| info.duration).sum();
    let audio = infos.iter().any(|info| info.audio_index.is_some());
    let bits = if copy {
        infos
            .iter()
            .try_fold(0.0, |bits, info| {
                let rate = info
                    .video_bitrate
                    .filter(|rate| rate.is_finite() && *rate > 0.0)?;
                Some(bits + rate * info.duration)
            })
            .map(|bits| {
                bits + if audio {
                    total * (first.audio_channels.clamp(1, 8) * 96_000) as f64
                } else {
                    0.0
                }
            })
    } else {
        bitrate.map(|bitrate| total * (bitrate as f64 + if audio { 192_000.0 } else { 0.0 }))
    };
    Ok(ExportPlan {
        mode: if copy {
            ExportMode::Copy
        } else {
            ExportMode::Convert
        },
        codec,
        video_bitrate: if copy { None } else { bitrate },
        estimated_bytes: bits
            .map(|bits| bits / 8.0)
            .filter(|bytes| bytes.is_finite() && *bytes > 0.0),
        // CPU encoders accept a nominal-rate hint without changing passthrough timestamps.
        nvidia_allowed: settings.frame_rate != FrameRate::Original,
    })
}

fn target_bitrate(infos: &[&ClipInfo], settings: ExportSettings) -> Option<u64> {
    let first = infos.first()?;
    let base = match settings.preset {
        ExportPreset::Source => return None,
        ExportPreset::Small => 3_000_000.0,
        ExportPreset::Medium => 6_000_000.0,
        ExportPreset::Large => 10_000_000.0,
    };
    let width = first.width.div_ceil(2) * 2;
    let height = first.height.div_ceil(2) * 2;
    let duration: f64 = infos.iter().map(|info| info.duration).sum();
    let fps = settings.frame_rate.fps().unwrap_or_else(|| {
        infos
            .iter()
            .map(|info| info.fps * info.duration)
            .sum::<f64>()
            / duration
    });
    let scale = (width as f64 * height as f64 * fps / (1920.0 * 1080.0 * 30.0)).clamp(0.05, 16.0);
    Some(((base * scale / 1000.0).round() * 1000.0) as u64)
}

#[derive(Clone, Copy, Debug)]
pub struct Progress {
    pub percent: f64,
    pub mode: ExportMode,
    pub engine: Option<EncodeEngine>,
}

fn number(v: &Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_str()?.parse().ok())
        .filter(|n: &f64| n.is_finite())
}

fn rate(text: &str) -> Option<f64> {
    let (a, b) = text.split_once('/')?;
    let result = a.parse::<f64>().ok()? / b.parse::<f64>().ok()?;
    (result.is_finite() && result > 0.0 && result <= 240.0).then_some(result)
}

fn probe_color(video: &Value) -> VideoColor {
    let pixel = video["pix_fmt"].as_str().unwrap_or("");
    let component_bits = number(&video["bits_per_raw_sample"])
        .filter(|bits| *bits >= 8.0 && *bits <= 16.0)
        .map(|bits| bits as u8);
    let bit_depth = component_bits.unwrap_or_else(|| {
        let pixel = pixel.trim_end_matches("le").trim_end_matches("be");
        if let Some((_, depth)) = pixel.rsplit_once('p')
            && let Ok(depth @ 9..=16) = depth.parse::<u8>()
        {
            depth
        } else if pixel.starts_with("p010")
            || pixel.starts_with("p210")
            || pixel.ends_with("rgb10")
            || pixel.ends_with("bgr10")
            || pixel.ends_with("gray10")
        {
            10
        } else if pixel.starts_with("p012")
            || pixel.starts_with("p212")
            || pixel.ends_with("gray12")
        {
            12
        } else if pixel.starts_with("p016")
            || pixel.starts_with("p216")
            || pixel.ends_with("48")
            || pixel.ends_with("64")
            || pixel.ends_with("gray16")
            || pixel.contains("f32")
        {
            16
        } else {
            8
        }
    });
    let code = |key: &str, names: &[(&str, u8)]| {
        let name = video[key].as_str()?;
        names
            .iter()
            .find_map(|(text, code)| (*text == name).then_some(*code))
    };
    let rgb = pixel.starts_with("gbr") || pixel.contains("rgb") || pixel.contains("bgr");
    VideoColor {
        bit_depth,
        primaries: code(
            "color_primaries",
            &[
                ("bt709", 1),
                ("bt470m", 4),
                ("bt470bg", 5),
                ("smpte170m", 6),
                ("smpte240m", 7),
                ("film", 8),
                ("bt2020", 9),
                ("smpte428", 10),
                ("smpte431", 11),
                ("smpte432", 12),
                ("ebu3213", 22),
            ],
        ),
        transfer: code(
            "color_transfer",
            &[
                ("bt709", 1),
                ("gamma22", 4),
                ("gamma28", 5),
                ("smpte170m", 6),
                ("smpte240m", 7),
                ("linear", 8),
                ("iec61966-2-4", 11),
                ("iec61966-2-1", 13),
                ("bt2020-10", 14),
                ("bt2020-12", 15),
                ("smpte2084", 16),
                ("arib-std-b67", 18),
            ],
        ),
        matrix: code(
            "color_space",
            &[
                ("gbr", 0),
                ("bt709", 1),
                ("fcc", 4),
                ("bt470bg", 5),
                ("smpte170m", 6),
                ("smpte240m", 7),
                ("ycgco", 8),
                ("bt2020nc", 9),
                ("bt2020c", 10),
                ("chroma-derived-nc", 12),
                ("chroma-derived-c", 13),
                ("ictcp", 14),
            ],
        )
        .or_else(|| rgb.then_some(0)),
        full_range: match video["color_range"].as_str() {
            Some("pc") => true,
            Some("tv") => false,
            _ => rgb,
        },
        dolby_vision_profile: video["side_data_list"].as_array().and_then(|data| {
            data.iter().find_map(|side| {
                side["dv_profile"]
                    .as_u64()
                    .and_then(|profile| u8::try_from(profile).ok())
            })
        }),
    }
}

pub fn parse_probe(root: &Value) -> Result<ClipInfo> {
    let streams = root["streams"]
        .as_array()
        .ok_or("Cannot read video information.")?;
    let videos: Vec<_> = streams
        .iter()
        .filter(|s| {
            s["codec_type"] == "video" && s["disposition"]["attached_pic"].as_i64() != Some(1)
        })
        .collect();
    let video = videos
        .iter()
        .find(|s| s["disposition"]["default"] == 1)
        .or_else(|| videos.first())
        .ok_or("This file has no video track.")?;
    let audio = streams
        .iter()
        .filter(|s| s["codec_type"] == "audio")
        .find(|s| s["disposition"]["default"] == 1)
        .or_else(|| streams.iter().find(|s| s["codec_type"] == "audio"));
    let duration = number(&video["duration"])
        .or_else(|| number(&root["format"]["duration"]))
        .filter(|n| *n > 0.0)
        .ok_or("This video's duration is missing or invalid.")?;
    let dimension = |value: &Value| {
        value
            .as_u64()
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or(0)
    };
    let mut width = dimension(&video["width"]);
    let mut height = dimension(&video["height"]);
    if !(2..=16384).contains(&width) || !(2..=16384).contains(&height) {
        return Err("The video's dimensions are invalid.".into());
    }
    let rotation = video["side_data_list"]
        .as_array()
        .and_then(|list| list.iter().find_map(|s| s["rotation"].as_i64()))
        .or_else(|| {
            video["tags"]["rotate"]
                .as_str()
                .and_then(|s| s.parse().ok())
        })
        .unwrap_or(0);
    if rotation.rem_euclid(180) == 90 {
        std::mem::swap(&mut width, &mut height);
    }
    // Display geometry includes non-square pixels before conversion to square pixels.
    if let Some((a, b)) = video["sample_aspect_ratio"]
        .as_str()
        .and_then(|s| s.split_once(':'))
        && let (Ok(a), Ok(b)) = (a.parse::<f64>(), b.parse::<f64>())
        && b > 0.0
        && a > 0.0
    {
        if rotation.rem_euclid(180) == 90 {
            height = (height as f64 * a / b).round() as u32;
        } else {
            width = (width as f64 * a / b).round() as u32;
        }
    }
    if !(2..=16384).contains(&width) || !(2..=16384).contains(&height) {
        return Err("The video's display dimensions are invalid.".into());
    }
    let rate_text = ["avg_frame_rate", "r_frame_rate"]
        .iter()
        .find_map(|key| {
            let text = video[*key].as_str()?;
            rate(text).map(|fps| (text.to_owned(), fps))
        })
        .unwrap_or(("30/1".into(), 30.0));
    let codec = video["codec_name"].as_str().unwrap_or("").to_owned();
    let color = probe_color(video);
    let hdr = matches!(color.transfer, Some(16 | 18));
    let video_start = number(&video["start_time"]).unwrap_or(0.0);
    let audio_offset = audio
        .map(|a| number(&a["start_time"]).unwrap_or(video_start) - video_start)
        .unwrap_or(0.0);
    let fields = [
        "index",
        "codec_type",
        "codec_name",
        "codec_tag_string",
        "profile",
        "level",
        "width",
        "height",
        "pix_fmt",
        "sample_aspect_ratio",
        "r_frame_rate",
        "avg_frame_rate",
        "time_base",
        "color_range",
        "color_space",
        "color_transfer",
        "color_primaries",
        "sample_rate",
        "channels",
        "channel_layout",
        "extradata_hash",
        "side_data_list",
    ];
    let layout: Vec<BTreeMap<_, _>> = streams
        .iter()
        .map(|s| {
            fields
                .iter()
                .map(|key| ((*key).to_owned(), s[*key].clone()))
                .collect()
        })
        .collect();
    let simple = videos.len() == 1
        && matches!(codec.as_str(), "h264" | "hevc")
        && streams
            .iter()
            .all(|s| matches!(s["codec_type"].as_str(), Some("video" | "audio")))
        && streams
            .iter()
            .all(|s| s["extradata_hash"].as_str().is_some_and(|v| !v.is_empty()))
        && streams
            .iter()
            .filter(|s| s["codec_type"] == "audio")
            .all(|s| s["codec_name"] == "aac")
        && audio_offset.abs() <= 0.04
        && audio.is_none_or(|a| {
            number(&a["duration"])
                .is_some_and(|d| (d - duration).abs() <= 0.04_f64.max(1.0 / rate_text.1))
        });
    Ok(ClipInfo {
        file_stamp: None,
        duration,
        width,
        height,
        fps: rate_text.1,
        rate: rate_text.0,
        video_index: video["index"].as_u64().unwrap_or(0) as usize,
        audio_index: audio.and_then(|a| a["index"].as_u64()).map(|i| i as usize),
        audio_offset,
        audio_rate: audio
            .and_then(|a| number(&a["sample_rate"]))
            .unwrap_or(48000.0) as u32,
        audio_channels: audio.and_then(|a| a["channels"].as_u64()).unwrap_or(2) as u32,
        audio_layout: audio
            .and_then(|a| a["channel_layout"].as_str())
            .unwrap_or("stereo")
            .to_owned(),
        codec,
        video_bitrate: number(&video["bit_rate"]).filter(|rate| *rate > 0.0),
        hdr,
        color,
        copy_key: simple.then(|| serde_json::to_string(&layout).unwrap()),
    })
}

pub fn inspect(runtime: &Runtime, path: &Path, cancel: &AtomicBool) -> Result<ClipInfo> {
    if !path.is_file() {
        return Err("This file is missing or is not a video file.".into());
    }
    let stamp = |path: &Path| -> Result<(u64, u64)> {
        let metadata = fs::metadata(path).map_err(|e| e.to_string())?;
        let modified = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos().min(u64::MAX as u128) as u64)
            .unwrap_or(0);
        Ok((metadata.len(), modified))
    };
    let before = stamp(path)?;
    let output = process(
        &runtime.ffprobe,
        &[
            "-v".into(),
            "error".into(),
            "-show_format".into(),
            "-show_streams".into(),
            "-show_data_hash".into(),
            "sha256".into(),
            "-of".into(),
            "json".into(),
            "-i".into(),
            path_arg(path),
        ],
        cancel,
        None,
    )?;
    let root: Value =
        serde_json::from_slice(&output).map_err(|_| "Cannot read the video's metadata.")?;
    let mut info = parse_probe(&root)?;
    if root["format"]["format_name"]
        .as_str()
        .is_some_and(|s| s.split(',').any(|n| n == "mp4" || n == "mov"))
        && !single_mp4_descriptions(path)
    {
        info.copy_key = None;
    }
    if stamp(path)? != before {
        return Err("This video changed while it was being read. Add it again after recording has finished.".into());
    }
    info.file_stamp = Some(before);
    Ok(info)
}

pub fn thumbnail(
    runtime: &Runtime,
    path: &Path,
    info: &ClipInfo,
    cancel: &AtomicBool,
) -> Result<Vec<u8>> {
    let display_color = conversion_color(&[info], VideoCodec::H264);
    let color = color_filter(info, display_color)?;
    let output = process(
        &runtime.ffmpeg,
        &[
            "-v".into(),
            "error".into(),
            "-nostdin".into(),
            "-threads".into(),
            "1".into(),
            "-ss".into(),
            format!("{:.6}", (info.duration * 0.12).min(10.0)),
            "-i".into(),
            path_arg(path),
            "-map".into(),
            format!("0:{}", info.video_index),
            "-frames:v".into(),
            "1".into(),
            "-vf".into(),
            format!(
                "{color},scale={THUMB_WIDTH}:{THUMB_HEIGHT}:force_original_aspect_ratio=decrease:reset_sar=1,pad={THUMB_WIDTH}:{THUMB_HEIGHT}:(ow-iw)/2:(oh-ih)/2,setsar=1"
            ),
            "-f".into(),
            "rawvideo".into(),
            "-pix_fmt".into(),
            "bgr24".into(),
            "pipe:1".into(),
        ],
        cancel,
        None,
    )?;
    if output.len() != (THUMB_WIDTH * THUMB_HEIGHT * 3) as usize {
        return Err("A thumbnail is not available.".into());
    }
    Ok(output)
}

pub fn can_copy(sources: &[Source]) -> bool {
    copy_compatible(sources.iter().map(|source| &source.info))
}

fn copy_compatible<'a>(mut infos: impl Iterator<Item = &'a ClipInfo>) -> bool {
    infos.next().is_some_and(|first| {
        first.copy_key.is_some()
            && infos.all(|info| {
                info.copy_key == first.copy_key && info.audio_index == first.audio_index
            })
    })
}

fn single_mp4_descriptions(path: &Path) -> bool {
    fn scan(
        file: &mut File,
        start: u64,
        end: u64,
        depth: u8,
        count: &mut u32,
        boxes: &mut u32,
    ) -> std::io::Result<bool> {
        let mut position = start;
        while position < end {
            *boxes += 1;
            if *boxes > 100_000 || end - position < 8 {
                return Ok(false);
            }
            file.seek(SeekFrom::Start(position))?;
            let mut header = [0u8; 8];
            file.read_exact(&mut header)?;
            let short = u32::from_be_bytes(header[..4].try_into().unwrap()) as u64;
            let mut header_size = 8;
            let length = if short == 1 {
                let mut extended = [0u8; 8];
                file.read_exact(&mut extended)?;
                header_size = 16;
                u64::from_be_bytes(extended)
            } else if short == 0 {
                end - position
            } else {
                short
            };
            if length < header_size || length > end - position {
                return Ok(false);
            }
            let body = position + header_size;
            let finish = position + length;
            let expected = [b"moov", b"trak", b"mdia", b"minf", b"stbl", b"stsd"];
            if &header[4..] == expected[depth as usize] {
                if depth == 5 {
                    if finish - body < 8 {
                        return Ok(false);
                    }
                    let mut description = [0u8; 8];
                    file.read_exact(&mut description)?;
                    if u32::from_be_bytes(description[4..].try_into().unwrap()) != 1 {
                        return Ok(false);
                    }
                    *count += 1;
                } else if !scan(file, body, finish, depth + 1, count, boxes)? {
                    return Ok(false);
                }
            }
            position = finish;
        }
        Ok(position == end)
    }
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let Ok(length) = file.metadata().map(|m| m.len()) else {
        return false;
    };
    let mut count = 0;
    let mut boxes = 0;
    scan(&mut file, 0, length, 0, &mut count, &mut boxes).unwrap_or(false) && count > 0
}

pub fn concatenate(sources: &[Source]) -> Result<String> {
    let mut text = String::from("ffconcat version 1.0\n");
    for source in sources {
        let path = path_arg(
            &fs::canonicalize(&source.path)
                .map_err(|e| format!("Cannot open {}: {e}", source.path.display()))?,
        );
        if path.contains(['\n', '\r', '\0']) {
            return Err("A filename contains unsupported control characters.".into());
        }
        let escaped = path.replace('\\', "/").replace('\'', "'\\''");
        text.push_str(&format!("file '{escaped}'\n"));
    }
    Ok(text)
}

pub fn conversion_args(
    sources: &[Source],
    output: &Path,
    settings: ExportSettings,
    engine: EncodeEngine,
) -> Result<Vec<String>> {
    let first = sources.first().ok_or("Add a video before stitching.")?;
    let infos: Vec<_> = sources.iter().map(|source| &source.info).collect();
    let color = conversion_color(&infos, settings.codec);
    let width = first.info.width.div_ceil(2) * 2;
    let height = first.info.height.div_ceil(2) * 2;
    let audio = sources.iter().any(|s| s.info.audio_index.is_some());
    let mut args = vec![
        "-v".into(),
        "error".into(),
        "-nostdin".into(),
        "-y".into(),
        "-filter_complex_threads".into(),
        "1".into(),
    ];
    for source in sources {
        args.extend([
            "-threads".into(),
            "1".into(),
            "-i".into(),
            path_arg(&source.path),
        ]);
    }
    let mut graph = String::new();
    let mut offset = 0.0;
    for (i, source) in sources.iter().enumerate() {
        // Keep a bounded final-image tail until the fixed-rate trim stops the graph.
        // This also covers a source shorter than one output frame.
        let tail = if let Some(fps) = settings.frame_rate.fps()
            && i + 1 == sources.len()
        {
            // Use the inspection rate ceiling to bound enough padding for the final interval.
            let frames = (240.0 * (source.info.duration + 2.0 / fps))
                .ceil()
                .clamp(1.0, i32::MAX as f64) as u64;
            format!(",tpad=stop_mode=clone:stop={frames}")
        } else {
            String::new()
        };
        let normalize = color_filter(&source.info, color)?;
        graph.push_str(&format!("[{i}:{}]settb=AVTB,setpts=PTS-STARTPTS+{offset:.6}/TB,{normalize},scale={width}:{height}:force_original_aspect_ratio=decrease:force_divisible_by=2:reset_sar=1,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2,setsar=1{tail}[v{i}];",source.info.video_index));
        if audio {
            if let Some(index) = source.info.audio_index {
                graph.push_str(&format!("[{i}:{index}]asetpts=PTS-STARTPTS+{:.6}/TB,aresample=48000:async=1:first_pts=0,aformat=sample_fmts=fltp:channel_layouts=stereo,apad,atrim=duration={:.6},asetpts=PTS-STARTPTS[a{i}];",source.info.audio_offset,source.info.duration));
            } else {
                graph.push_str(&format!(
                    "anullsrc=r=48000:cl=stereo,atrim=duration={:.6},asetpts=PTS-STARTPTS[a{i}];",
                    source.info.duration
                ));
            }
        }
        offset += source.info.duration;
    }
    // The concat filter estimates a final video frame duration from its average cadence.
    // Place frames on the known clip timeline instead, including irregular source timing.
    for i in 0..sources.len() {
        graph.push_str(&format!("[v{i}]"));
    }
    graph.push_str(&format!(
        "interleave=nb_inputs={}:duration=longest[joined]",
        sources.len()
    ));
    graph.push_str(";[joined]");
    if let Some(rate) = settings.frame_rate.rate() {
        let frames = (offset * settings.frame_rate.fps().unwrap())
            .ceil()
            .max(1.0) as u64;
        graph.push_str(&format!("fps={rate},trim=end_frame={frames}"));
    } else {
        graph.push_str("null");
    }
    graph.push_str("[v]");
    if audio {
        graph.push(';');
        for i in 0..sources.len() {
            graph.push_str(&format!("[a{i}]"));
        }
        graph.push_str(&format!("concat=n={}:v=0:a=1[a]", sources.len()));
    }
    args.extend(["-filter_complex".into(), graph, "-map".into(), "[v]".into()]);
    if audio {
        args.extend([
            "-map".into(),
            "[a]".into(),
            "-c:a".into(),
            "aac".into(),
            "-b:a".into(),
            "192k".into(),
        ]);
    }
    let codec = settings.codec;
    let engine = if settings.frame_rate == FrameRate::Original {
        EncodeEngine::Cpu
    } else {
        engine
    };
    args.extend(video_encoding_args(
        codec,
        engine,
        target_bitrate(&infos, settings),
        (settings.frame_rate == FrameRate::Original).then(|| {
            sources
                .iter()
                .max_by(|a, b| a.info.fps.total_cmp(&b.info.fps))
                .unwrap()
                .info
                .rate
                .as_str()
        }),
        color,
    ));
    args.extend([
        "-fps_mode:v".into(),
        "passthrough".into(),
        "-enc_time_base:v".into(),
        "filter".into(),
        "-movflags".into(),
        "+faststart".into(),
        "-progress".into(),
        "pipe:1".into(),
        "-nostats".into(),
        path_arg(output),
    ]);
    Ok(args)
}

fn video_encoding_args(
    codec: VideoCodec,
    engine: EncodeEngine,
    bitrate: Option<u64>,
    nominal_rate: Option<&str>,
    color: VideoColor,
) -> Vec<String> {
    let hardware = engine == EncodeEngine::Nvidia;
    let encoder = match (codec, hardware) {
        (VideoCodec::H264, false) => "libx264",
        (VideoCodec::H265, false) => "libx265",
        (VideoCodec::H264, true) => "h264_nvenc",
        (VideoCodec::H265, true) => "hevc_nvenc",
    };
    let mut args = vec![
        "-c:v".into(),
        encoder.into(),
        "-preset".into(),
        if hardware { "p4" } else { "veryfast" }.into(),
    ];
    if hardware {
        args.extend(["-rc".into(), "vbr".into()]);
    }
    if let Some(bitrate) = bitrate {
        args.extend(["-b:v".into(), bitrate.to_string()]);
    } else if hardware {
        args.extend(["-cq".into(), "18".into(), "-b:v".into(), "0".into()]);
    } else {
        args.extend(["-crf".into(), "18".into()]);
    }
    if codec == VideoCodec::H265 {
        args.extend(["-tag:v".into(), "hvc1".into()]);
        if color.bit_depth > 8 {
            args.extend(["-profile:v".into(), "main10".into()]);
        }
        if !hardware {
            args.extend([
                "-x265-params".into(),
                format!(
                    "pools=4:frame-threads=2:log-level=error{}{}",
                    nominal_rate
                        .map(|rate| format!(":fps={rate}"))
                        .unwrap_or_default(),
                    if color.transfer == Some(16) {
                        ":hdr10-opt=1"
                    } else {
                        ""
                    }
                ),
            ]);
        }
    }
    if codec == VideoCodec::H264
        && !hardware
        && let Some(rate) = nominal_rate
    {
        args.extend(["-x264-params".into(), format!("fps={rate}")]);
    }
    args.extend([
        "-threads".into(),
        "4".into(),
        "-pix_fmt".into(),
        color.pixel_format(hardware).into(),
    ]);
    args.extend(color.encoding_args());
    args
}

pub fn probe_nvidia(runtime: &Runtime, cancel: &AtomicBool) -> NvidiaSupport {
    let available = |codec| {
        let mut args: Vec<String> = [
            "-v",
            "error",
            "-nostdin",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=256x256:r=30",
            "-frames:v",
            "2",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        args.extend(video_encoding_args(
            codec,
            EncodeEngine::Nvidia,
            Some(3_000_000),
            None,
            VideoColor {
                bit_depth: if codec == VideoCodec::H265 { 10 } else { 8 },
                primaries: Some(1),
                transfer: Some(1),
                matrix: Some(1),
                ..VideoColor::default()
            },
        ));
        args.extend(["-f".into(), "null".into(), "pipe:1".into()]);
        process_with_timeout(
            &runtime.ffmpeg,
            &args,
            cancel,
            None,
            Some(Duration::from_secs(5)),
        )
        .is_ok()
    };
    NvidiaSupport {
        h264: available(VideoCodec::H264),
        hevc: available(VideoCodec::H265),
    }
}

fn encode_with_cpu_fallback(
    initial: EncodeEngine,
    cancel: &AtomicBool,
    mut attempt: impl FnMut(EncodeEngine) -> Result<()>,
) -> Result<EncodeEngine> {
    if cancel.load(Ordering::Relaxed) {
        return Err("Canceled".into());
    }
    match attempt(initial) {
        Ok(()) => Ok(initial),
        Err(_) if cancel.load(Ordering::Relaxed) => Err("Canceled".into()),
        Err(_) if initial == EncodeEngine::Nvidia => {
            attempt(EncodeEngine::CpuFallback)?;
            Ok(EncodeEngine::CpuFallback)
        }
        Err(error) => Err(error),
    }
}

fn joined_audio_args(sources: &[Source], output: &Path, script: &Path) -> Result<Vec<String>> {
    let first = &sources[0].info;
    let layout = match first.audio_layout.as_str() {
        "mono" | "stereo" | "2.1" | "3.0" | "4.0" | "quad" | "5.0" | "5.1" | "5.1(side)"
        | "7.1" | "7.1(wide)" => &first.audio_layout,
        _ => return Err("This audio channel layout cannot be joined safely.".into()),
    };
    let mut args = vec![
        "-v".into(),
        "error".into(),
        "-nostdin".into(),
        "-y".into(),
        "-stats_period".into(),
        "0.1".into(),
        "-filter_complex_threads".into(),
        "1".into(),
    ];
    let mut graph = String::new();
    for (i, source) in sources.iter().enumerate() {
        args.extend([
            "-threads".into(),
            "1".into(),
            "-i".into(),
            path_arg(&source.path),
        ]);
        let index = source
            .info
            .audio_index
            .ok_or("The source audio track is missing.")?;
        graph.push_str(&format!("[{i}:{index}]asetpts=PTS-STARTPTS+{:.6}/TB,aresample={}:async=1:first_pts=0,aformat=sample_fmts=fltp:channel_layouts={layout},apad,atrim=duration={:.6},asetpts=PTS-STARTPTS[a{i}];",source.info.audio_offset,first.audio_rate,source.info.duration));
    }
    for i in 0..sources.len() {
        graph.push_str(&format!("[a{i}]"));
    }
    graph.push_str(&format!("concat=n={}:v=0:a=1[a]", sources.len()));
    fs::write(script, graph).map_err(|e| e.to_string())?;
    args.extend([
        "-/filter_complex".into(),
        path_arg(script),
        "-map".into(),
        "[a]".into(),
        "-c:a".into(),
        "aac".into(),
        "-b:a".into(),
        format!("{}k", first.audio_channels.clamp(1, 8) * 96),
        "-progress".into(),
        "pipe:1".into(),
        "-nostats".into(),
        path_arg(output),
    ]);
    Ok(args)
}

pub fn export(
    runtime: &Runtime,
    sources: &[Source],
    destination: &Path,
    options: ExportOptions,
    cancel: &AtomicBool,
    mut update: impl FnMut(Progress),
) -> Result<ExportMode> {
    if sources.is_empty() {
        return Err("Add a video before stitching.".into());
    }
    if !destination
        .extension()
        .is_some_and(|v| v.eq_ignore_ascii_case("mp4"))
    {
        return Err("Choose an MP4 output filename.".into());
    }
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if !parent.is_dir() {
        return Err("The output folder does not exist.".into());
    }
    let mut locks = Vec::new();
    for source in sources {
        let file = File::options()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(&source.path)
            .map_err(|e| {
                format!(
                    "Cannot read {}. Close programs that are changing this file. {e}",
                    source.path.display()
                )
            })?;
        if destination.exists()
            && same_file(&file, &File::open(destination).map_err(|e| e.to_string())?)?
        {
            return Err(
                "The output is one of the source videos. Choose a different filename.".into(),
            );
        }
        locks.push(file);
        let current = inspect(runtime, &source.path, cancel)?;
        if current != source.info {
            return Err(format!(
                "{} changed after it was added. Remove it and add it again before stitching.",
                source.path.display()
            ));
        }
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("Canceled".into());
    }
    let work = WorkDir::new(parent)?;
    let output = work.path.join("output.mp4");
    let manifest = work.path.join("inputs.ffconcat");
    let total: f64 = sources.iter().map(|s| s.info.duration).sum();
    let planned = plan(
        &sources
            .iter()
            .map(|source| &source.info)
            .collect::<Vec<_>>(),
        options.settings,
    )?;
    let mut mode = planned.mode;
    let mut engine = planned.engine(options.nvidia);
    if mode == ExportMode::Copy {
        fs::write(&manifest, concatenate(sources)?).map_err(|e| e.to_string())?;
        update(Progress {
            percent: 0.0,
            mode,
            engine: None,
        });
        // Decode each source's AAC preroll and trim its padding before a single audio encode.
        // This preserves the copied video while avoiding audible gaps and AAC boundary artifacts.
        let joined_audio = if sources[0].info.audio_index.is_some() {
            let audio = work.path.join("audio.m4a");
            let args = joined_audio_args(sources, &audio, &work.path.join("audio-filters.txt"))?;
            let mut progress = |seconds: f64| {
                update(Progress {
                    percent: (seconds / total * 25.0).clamp(0.0, 25.0),
                    mode,
                    engine: None,
                })
            };
            process(&runtime.ffmpeg, &args, cancel, Some(&mut progress))?;
            Some(audio)
        } else {
            None
        };
        let mut args = vec![
            "-v".into(),
            "error".into(),
            "-nostdin".into(),
            "-y".into(),
            "-f".into(),
            "concat".into(),
            "-auto_convert".into(),
            "0".into(),
            "-safe".into(),
            "0".into(),
            "-i".into(),
            path_arg(&manifest),
        ];
        if let Some(audio) = joined_audio.as_ref() {
            args.extend(["-i".into(), path_arg(audio)]);
        }
        args.extend(["-map".into(), format!("0:{}", sources[0].info.video_index)]);
        if joined_audio.is_some() {
            args.extend(["-map".into(), "1:a:0".into()]);
        }
        args.extend([
            "-c".into(),
            "copy".into(),
            "-movflags".into(),
            "+faststart".into(),
            "-progress".into(),
            "pipe:1".into(),
            "-nostats".into(),
            path_arg(&output),
        ]);
        let base = if joined_audio.is_some() { 25.0 } else { 0.0 };
        let mut progress = |seconds: f64| {
            update(Progress {
                percent: base + (seconds / total * (96.0 - base)).clamp(0.0, 96.0 - base),
                mode,
                engine: None,
            })
        };
        let copied = process(&runtime.ffmpeg, &args, cancel, Some(&mut progress)).and_then(|_| {
            validate(
                runtime,
                &output,
                sources,
                mode,
                planned.codec,
                options.settings.frame_rate,
                cancel,
            )
        });
        if copied.is_err() {
            if cancel.load(Ordering::Relaxed) {
                return Err("Canceled".into());
            }
            mode = ExportMode::Convert;
        }
    }
    if mode == ExportMode::Convert {
        let script = work.path.join("filters.txt");
        let codec = options.settings.codec;
        engine = Some(encode_with_cpu_fallback(
            engine.unwrap_or(EncodeEngine::Cpu),
            cancel,
            |attempt| {
                let mut args = conversion_args(sources, &output, options.settings, attempt)?;
                let index = args.iter().position(|s| s == "-filter_complex").unwrap();
                fs::write(&script, &args[index + 1]).map_err(|e| e.to_string())?;
                args[index] = "-/filter_complex".into();
                args[index + 1] = path_arg(&script);
                args.splice(0..0, ["-stats_period".into(), "0.1".into()]);
                update(Progress {
                    percent: 0.0,
                    mode,
                    engine: Some(attempt),
                });
                let mut progress = |seconds: f64| {
                    update(Progress {
                        percent: (seconds / total * 96.0).clamp(0.0, 96.0),
                        mode,
                        engine: Some(attempt),
                    })
                };
                process(&runtime.ffmpeg, &args, cancel, Some(&mut progress))?;
                validate(
                    runtime,
                    &output,
                    sources,
                    mode,
                    codec,
                    options.settings.frame_rate,
                    cancel,
                )
            },
        )?);
    }
    update(Progress {
        percent: 98.0,
        mode,
        engine,
    });
    if cancel.load(Ordering::Relaxed) {
        return Err("Canceled".into());
    }
    File::options()
        .read(true)
        .write(true)
        .open(&output)
        .and_then(|f| f.sync_all())
        .map_err(|e| format!("Cannot finish the output file: {e}"))?;
    let from = wide(&output);
    let to = wide(destination);
    if unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        return Err(format!(
            "Cannot save the output. Close applications that are using the destination. {}",
            std::io::Error::last_os_error()
        ));
    }
    update(Progress {
        percent: 100.0,
        mode,
        engine,
    });
    Ok(mode)
}

fn validate(
    runtime: &Runtime,
    output: &Path,
    sources: &[Source],
    mode: ExportMode,
    codec: VideoCodec,
    frame_rate: FrameRate,
    cancel: &AtomicBool,
) -> Result<()> {
    let info = inspect(runtime, output, cancel)?;
    let total: f64 = sources.iter().map(|s| s.info.duration).sum();
    let fps = frame_rate.fps().unwrap_or_else(|| {
        sources
            .iter()
            .map(|source| source.info.fps)
            .fold(f64::INFINITY, f64::min)
    });
    let tolerance = (sources.len() as f64 / fps + 0.05).max(0.15);
    if (info.duration - total).abs() > tolerance
        || info.audio_index.is_some() != sources.iter().any(|s| s.info.audio_index.is_some())
    {
        return Err("The output duration or audio does not match the clip list.".into());
    }
    if mode == ExportMode::Convert && info.codec != codec.probe_name() {
        return Err("The output video format is invalid.".into());
    }
    if mode == ExportMode::Convert {
        let expected_color = conversion_color(
            &sources
                .iter()
                .map(|source| &source.info)
                .collect::<Vec<_>>(),
            codec,
        );
        if info.color.bit_depth != expected_color.bit_depth
            || info.color.primaries != expected_color.primaries
            || info.color.transfer != expected_color.transfer
            || info.color.matrix != expected_color.matrix
            || info.color.full_range
        {
            return Err("The output color format or bit depth is invalid.".into());
        }
    }
    if frame_rate
        .fps()
        .is_some_and(|fps| (info.fps - fps).abs() > 0.01)
    {
        return Err("The output frame rate is invalid.".into());
    }
    let reference = &sources[0].info;
    let expected = if mode == ExportMode::Copy {
        (reference.width, reference.height)
    } else {
        (
            reference.width.div_ceil(2) * 2,
            reference.height.div_ceil(2) * 2,
        )
    };
    if (info.width, info.height) != expected {
        return Err("The output dimensions do not match the clip list.".into());
    }
    Ok(())
}

fn same_file(a: &File, b: &File) -> Result<bool> {
    let mut aa: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    let mut bb: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    if unsafe { GetFileInformationByHandle(a.as_raw_handle() as HANDLE, &mut aa) } == 0
        || unsafe { GetFileInformationByHandle(b.as_raw_handle() as HANDLE, &mut bb) } == 0
    {
        return Err("Cannot verify the output filename.".into());
    }
    Ok(aa.dwVolumeSerialNumber == bb.dwVolumeSerialNumber
        && aa.nFileIndexHigh == bb.nFileIndexHigh
        && aa.nFileIndexLow == bb.nFileIndexLow)
}

fn path_arg(path: &Path) -> String {
    let text = path.to_string_lossy();
    if let Some(p) = text.strip_prefix("\\\\?\\UNC\\") {
        format!("\\\\{p}")
    } else {
        text.strip_prefix("\\\\?\\").unwrap_or(&text).to_owned()
    }
}

fn wide(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

static NEXT_WORK: AtomicU64 = AtomicU64::new(0);
struct WorkDir {
    path: PathBuf,
}
impl WorkDir {
    fn new(parent: &Path) -> Result<Self> {
        for _ in 0..100 {
            let path = parent.join(format!(
                ".stitch-{}-{}",
                std::process::id(),
                NEXT_WORK.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(format!("Cannot create output in this folder: {e}")),
            }
        }
        Err("Cannot create a temporary output folder.".into())
    }
}
impl Drop for WorkDir {
    fn drop(&mut self) {
        // Only this newly created directory contains our own manifest and partial output.
        for _ in 0..5 {
            if fs::remove_dir_all(&self.path).is_ok() {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
}

struct Job(HANDLE);
impl Job {
    fn attach(child: &std::process::Child) -> Result<Self> {
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return Err("Cannot prepare the video process.".into());
            }
            let job = Self(handle);
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as _,
                std::mem::size_of_val(&limits) as u32,
            ) == 0
                || AssignProcessToJobObject(handle, child.as_raw_handle() as HANDLE) == 0
            {
                return Err("Cannot supervise the video process.".into());
            }
            Ok(job)
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

fn bounded_read(mut reader: impl Read, limit: usize) -> std::io::Result<Vec<u8>> {
    let mut data = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        data.extend_from_slice(&buffer[..count]);
        if data.len() > limit {
            data.drain(..data.len() - limit);
        }
    }
    Ok(data)
}

fn process(
    executable: &Path,
    args: &[String],
    cancel: &AtomicBool,
    progress: Option<&mut dyn FnMut(f64)>,
) -> Result<Vec<u8>> {
    process_with_timeout(executable, args, cancel, progress, None)
}

fn process_with_timeout(
    executable: &Path,
    args: &[String],
    cancel: &AtomicBool,
    mut progress: Option<&mut dyn FnMut(f64)>,
    timeout: Option<Duration>,
) -> Result<Vec<u8>> {
    if cancel.load(Ordering::Relaxed) {
        return Err("Canceled".into());
    }
    let mut local_args = Vec::new();
    let mut index = 0;
    while index < args.len() {
        if args[index] == "-i" && index + 1 < args.len() {
            local_args.extend([
                "-protocol_whitelist".to_owned(),
                "file,pipe".to_owned(),
                "-i".to_owned(),
                args[index + 1].clone(),
            ]);
            index += 2;
        } else {
            local_args.push(args[index].clone());
            index += 1;
        }
    }
    let deadline = timeout.map(|timeout| Instant::now() + timeout);
    let mut child = Command::new(executable)
        .args(&local_args)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Cannot start the video engine: {e}"))?;
    let mut job = match Job::attach(&child) {
        Ok(job) => Some(job),
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
    };
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let (tx, rx) = mpsc::channel();
    let reporting = progress.is_some();
    let out = thread::spawn(move || {
        if reporting {
            for line in BufReader::new(stdout).lines() {
                let line = line?;
                if let Some(us) = line
                    .strip_prefix("out_time_us=")
                    .and_then(|s| s.parse::<f64>().ok())
                {
                    let _ = tx.send(us / 1_000_000.0);
                }
            }
            Ok(Vec::new())
        } else {
            bounded_read(stdout, 8 * 1024 * 1024)
        }
    });
    let err = thread::spawn(move || bounded_read(stderr, 64 * 1024));
    let mut timed_out = false;
    let status = loop {
        if let Some(callback) = progress.as_mut() {
            for time in rx.try_iter() {
                callback(time);
            }
        }
        timed_out |= deadline.is_some_and(|deadline| Instant::now() >= deadline);
        if cancel.load(Ordering::Relaxed) || timed_out {
            drop(job.take());
            let _ = child.kill();
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(e.to_string());
            }
        }
    };
    // Descendants can keep the pipes open after the direct child exits.
    // End the whole job before either pipe reader waits for EOF.
    drop(job);
    let output = out.join();
    let diagnostics = err.join();
    let output = output
        .map_err(|_| "The video output reader stopped.")?
        .map_err(|e| e.to_string())?;
    let diagnostics = diagnostics
        .map_err(|_| "The video log reader stopped.")?
        .map_err(|e| e.to_string())?;
    if cancel.load(Ordering::Relaxed) {
        return Err("Canceled".into());
    }
    if timed_out {
        return Err("The video engine check timed out.".into());
    }
    if !status?.success() {
        let message = String::from_utf8_lossy(&diagnostics);
        return Err(if message.trim().is_empty() {
            "The video engine could not process this file.".into()
        } else {
            message.trim().to_owned()
        });
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspection_separates_hdr_transfer_from_high_bit_depth() {
        for (pixel, bits) in [
            ("yuv420p", 8),
            ("yuv410p", 8),
            ("nv12", 8),
            ("yuv420p10le", 10),
            ("gbrp12le", 12),
            ("p010le", 10),
            ("p016le", 16),
            ("rgb48le", 16),
        ] {
            let mut raw = fixture();
            raw["streams"][0]["pix_fmt"] = pixel.into();
            let info = parse_probe(&raw).unwrap();
            assert_eq!(info.color.bit_depth, bits, "{pixel}");
            assert!(!info.hdr, "SDR {pixel} is not HDR");
            for transfer in ["smpte2084", "arib-std-b67"] {
                raw["streams"][0]["color_transfer"] = transfer.into();
                assert!(parse_probe(&raw).unwrap().hdr);
            }
        }
    }

    #[test]
    fn mixed_hdr_uses_main10_for_quality_encoding_and_cpu_recovery() {
        let mut sdr = parse_probe(&fixture()).unwrap();
        sdr.color.bit_depth = 10;
        let mut hlg = sdr.clone();
        hlg.hdr = true;
        hlg.color.transfer = Some(18);
        let sources = [
            Source {
                path: "sdr.mp4".into(),
                info: sdr,
            },
            Source {
                path: "hlg.mp4".into(),
                info: hlg,
            },
        ];
        let settings = ExportSettings {
            frame_rate: FrameRate::Fps60,
            ..ExportSettings::default()
        };
        let infos: Vec<_> = sources.iter().map(|source| &source.info).collect();
        let plan = plan(&infos, settings).unwrap();
        assert!(plan.estimated_bytes.is_none());
        assert_eq!(
            plan.engine(NvidiaSupport {
                h264: false,
                hevc: true
            }),
            Some(EncodeEngine::Nvidia)
        );
        for engine in [
            EncodeEngine::Cpu,
            EncodeEngine::Nvidia,
            EncodeEngine::CpuFallback,
        ] {
            let args = conversion_args(&sources, Path::new("out.mp4"), settings, engine).unwrap();
            assert!(args.windows(2).any(|pair| pair == ["-color_trc", "16"]));
            assert!(
                args.windows(2)
                    .any(|pair| pair == ["-color_primaries", "9"])
            );
            assert!(args.windows(2).any(|pair| pair == ["-profile:v", "main10"]));
            let hardware = engine == EncodeEngine::Nvidia;
            assert!(
                args.windows(2)
                    .any(|pair| pair
                        == ["-pix_fmt", if hardware { "p010le" } else { "yuv420p10le" }])
            );
            assert!(
                args.windows(2)
                    .any(|pair| pair == [if hardware { "-cq" } else { "-crf" }, "18"])
            );
            let graph = &args[args
                .iter()
                .position(|arg| arg == "-filter_complex")
                .unwrap()
                + 1];
            assert!(!graph.contains("tonemap="));
            assert!(graph.contains("npl=100:agamma=0"));
        }
        let legacy = conversion_args(
            &sources,
            Path::new("out.mp4"),
            ExportSettings {
                codec: VideoCodec::H264,
                ..settings
            },
            EncodeEngine::Cpu,
        )
        .unwrap();
        assert!(
            legacy
                .iter()
                .any(|arg| arg.contains("tonemap=tonemap=mobius"))
        );
        assert!(legacy.windows(2).any(|pair| pair == ["-color_trc", "1"]));
    }

    #[test]
    fn incompatible_dolby_vision_base_has_a_specific_conversion_error() {
        let mut info = parse_probe(&fixture()).unwrap();
        info.color.dolby_vision_profile = Some(5);
        info.copy_key = None;
        let error = plan(&[&info], ExportSettings::default()).unwrap_err();
        assert!(error.contains("Dolby Vision"));
        assert!(error.contains("HDR10 or HLG"));
    }

    #[test]
    fn frame_rate_selection_preserves_defaults_and_uses_exact_fractional_rates() {
        let saved: ExportSettings =
            serde_json::from_str(r#"{"preset":"large","codec":"h265"}"#).unwrap();
        assert_eq!(saved.frame_rate, FrameRate::Original);
        assert_eq!(FrameRate::Original.step(false), FrameRate::Fps60);
        assert_eq!(FrameRate::Fps60.step(true), FrameRate::Original);
        assert_eq!(FrameRate::Fps2997.rate(), Some("30000/1001"));
        assert_eq!(FrameRate::Fps5994.rate(), Some("60000/1001"));
        let first = parse_probe(&fixture()).unwrap();
        let mut second = first.clone();
        second.fps = 30.0;
        second.duration = 6.0;
        let settings = ExportSettings {
            preset: ExportPreset::Medium,
            ..ExportSettings::default()
        };
        let forward = plan(&[&first, &second], settings).unwrap();
        let reverse = plan(&[&second, &first], settings).unwrap();
        assert_eq!(forward.video_bitrate, reverse.video_bitrate);
        assert_eq!(forward.video_bitrate, Some(3_333_000));
        assert_eq!(
            forward.engine(NvidiaSupport {
                h264: true,
                hevc: true
            }),
            Some(EncodeEngine::Cpu)
        );
        assert_eq!(
            plan(
                &[&first],
                ExportSettings {
                    frame_rate: FrameRate::Fps30,
                    ..ExportSettings::default()
                }
            )
            .unwrap()
            .mode,
            ExportMode::Convert
        );
        let sources = [Source {
            path: "one.mp4".into(),
            info: first,
        }];
        let args =
            conversion_args(&sources, Path::new("out.mp4"), settings, EncodeEngine::Cpu).unwrap();
        let graph = &args[args
            .iter()
            .position(|arg| arg == "-filter_complex")
            .unwrap()
            + 1];
        assert!(!graph.contains("fps="));
        assert!(
            args.windows(2)
                .any(|pair| pair == ["-enc_time_base:v", "filter"])
        );
        let args = conversion_args(
            &sources,
            Path::new("out.mp4"),
            ExportSettings {
                frame_rate: FrameRate::Fps2997,
                ..settings
            },
            EncodeEngine::Cpu,
        )
        .unwrap();
        let graph = &args[args
            .iter()
            .position(|arg| arg == "-filter_complex")
            .unwrap()
            + 1];
        assert!(graph.contains(";[joined]fps=30000/1001,trim=end_frame="));
    }

    #[test]
    fn settings_defaults_and_size_steps_keep_source_outside_the_scale() {
        assert_eq!(
            serde_json::from_str::<ExportSettings>("{}").unwrap(),
            ExportSettings::default()
        );
        assert_eq!(
            serde_json::from_str::<ExportSettings>(r#"{"preset":"future","codec":"future"}"#)
                .unwrap(),
            ExportSettings::default()
        );
        assert_eq!(ExportPreset::Source.step(true), ExportPreset::Medium);
        assert_eq!(ExportPreset::Source.step(false), ExportPreset::Medium);
        assert_eq!(ExportPreset::Small.step(false), ExportPreset::Small);
        assert_eq!(ExportPreset::Large.step(true), ExportPreset::Large);
        assert_eq!(ExportPreset::Medium.step(true), ExportPreset::Large);
        assert_eq!(ExportPreset::Medium.step(false), ExportPreset::Small);
    }

    #[test]
    fn selected_stream_bitrate_is_optional_and_does_not_change_copy_compatibility() {
        for value in [
            serde_json::Value::Null,
            serde_json::json!("0"),
            serde_json::json!("-2"),
            serde_json::json!("NaN"),
            serde_json::json!("inf"),
        ] {
            let mut raw = fixture();
            raw["streams"][0]["bit_rate"] = value;
            assert!(parse_probe(&raw).unwrap().video_bitrate.is_none());
        }
        let mut raw = fixture();
        raw["streams"][0]["bit_rate"] = "8000000".into();
        let first = parse_probe(&raw).unwrap();
        raw["streams"][0]["bit_rate"] = "3000000".into();
        let second = parse_probe(&raw).unwrap();
        assert_eq!(first.video_bitrate, Some(8_000_000.0));
        assert!(copy_compatible([&first, &second].into_iter()));
        raw["streams"].as_array_mut().unwrap().push(serde_json::json!({"index":2,"codec_type":"video","codec_name":"h264","width":1920,"height":1080,"duration":"2","bit_rate":"9000000","disposition":{"default":1}}));
        let selected = parse_probe(&raw).unwrap();
        assert_eq!(selected.video_index, 2);
        assert_eq!(selected.video_bitrate, Some(9_000_000.0));
    }

    #[test]
    fn copy_estimate_counts_repeated_entries_and_rebuilt_surround_audio() {
        let mut info = parse_probe(&fixture()).unwrap();
        info.video_bitrate = Some(8_000_000.0);
        info.audio_index = Some(1);
        info.audio_channels = 6;
        let copied = plan(&[&info, &info], ExportSettings::default()).unwrap();
        assert_eq!(copied.mode, ExportMode::Copy);
        assert_eq!(copied.estimated_bytes, Some(4_288_000.0));
        info.audio_index = None;
        assert_eq!(
            plan(&[&info, &info], ExportSettings::default())
                .unwrap()
                .estimated_bytes,
            Some(4_000_000.0)
        );
        info.video_bitrate = None;
        assert!(
            plan(&[&info], ExportSettings::default())
                .unwrap()
                .estimated_bytes
                .is_none()
        );
        info.hdr = true;
        assert_eq!(
            plan(&[&info], ExportSettings::default()).unwrap().mode,
            ExportMode::Copy
        );
        assert_eq!(
            plan(
                &[&info],
                ExportSettings {
                    preset: ExportPreset::Small,
                    codec: VideoCodec::H265,
                    ..ExportSettings::default()
                }
            )
            .unwrap()
            .mode,
            ExportMode::Convert
        );
    }

    #[test]
    fn source_conversion_keeps_quality_settings_and_forced_targets_follow_the_first_clip() {
        let mut first = parse_probe(&fixture()).unwrap();
        first.width = 1919;
        first.height = 1079;
        first.fps = 30.0;
        first.audio_index = Some(1);
        let mut second = first.clone();
        second.width = 320;
        second.height = 180;
        let settings = ExportSettings {
            preset: ExportPreset::Medium,
            codec: VideoCodec::H265,
            ..ExportSettings::default()
        };
        let large = plan(&[&first, &second], settings).unwrap();
        assert_eq!(large.video_bitrate, Some(6_000_000));
        assert_eq!(large.estimated_bytes, Some(3_096_000.0));
        assert_eq!(
            plan(&[&second, &first], settings).unwrap().video_bitrate,
            Some(300_000)
        );
        first.copy_key = None;
        let automatic = plan(
            &[&first],
            ExportSettings {
                preset: ExportPreset::Source,
                codec: VideoCodec::H265,
                ..ExportSettings::default()
            },
        )
        .unwrap();
        assert_eq!(automatic.codec, VideoCodec::H265);
        assert!(automatic.estimated_bytes.is_none());
        assert_eq!(
            automatic.engine(NvidiaSupport {
                h264: true,
                hevc: true
            }),
            Some(EncodeEngine::Cpu)
        );
    }

    #[test]
    fn encoder_arguments_keep_target_bitrate_separate_from_quality_encoding() {
        let sources = [Source {
            path: "one.mp4".into(),
            info: parse_probe(&fixture()).unwrap(),
        }];
        for codec in [VideoCodec::H264, VideoCodec::H265] {
            for engine in [
                EncodeEngine::Cpu,
                EncodeEngine::Nvidia,
                EncodeEngine::CpuFallback,
            ] {
                let settings = ExportSettings {
                    preset: ExportPreset::Medium,
                    codec,
                    frame_rate: FrameRate::Fps30,
                };
                let args =
                    conversion_args(&sources, Path::new("out.mp4"), settings, engine).unwrap();
                assert!(
                    !args
                        .iter()
                        .any(|arg| matches!(arg.as_str(), "-crf" | "-cq" | "-qp"))
                );
                let encoder = &args[args.iter().position(|arg| arg == "-c:v").unwrap() + 1];
                assert_eq!(
                    encoder,
                    match (codec, engine == EncodeEngine::Nvidia) {
                        (VideoCodec::H264, false) => "libx264",
                        (VideoCodec::H265, false) => "libx265",
                        (VideoCodec::H264, true) => "h264_nvenc",
                        (VideoCodec::H265, true) => "hevc_nvenc",
                    }
                );
                assert_eq!(
                    args[args.iter().position(|arg| arg == "-b:v").unwrap() + 1],
                    target_bitrate(&[&sources[0].info], settings)
                        .unwrap()
                        .to_string()
                );
                assert_eq!(
                    args.iter().any(|arg| arg == "hvc1"),
                    codec == VideoCodec::H265
                );
            }
        }
        let args = conversion_args(
            &sources,
            Path::new("out.mp4"),
            ExportSettings {
                preset: ExportPreset::Source,
                codec: VideoCodec::H265,
                ..ExportSettings::default()
            },
            EncodeEngine::Nvidia,
        )
        .unwrap();
        assert!(args.iter().any(|arg| arg == "libx265"));
        assert!(args.windows(2).any(|pair| pair == ["-crf", "18"]));
    }

    #[test]
    fn nvidia_failure_retries_cpu_once_and_cancellation_prevents_retry() {
        let cancel = AtomicBool::new(false);
        let mut attempts = Vec::new();
        let result = encode_with_cpu_fallback(EncodeEngine::Nvidia, &cancel, |engine| {
            attempts.push(engine);
            if engine == EncodeEngine::Nvidia {
                Err("GPU failed".into())
            } else {
                Ok(())
            }
        });
        assert_eq!(result.unwrap(), EncodeEngine::CpuFallback);
        assert_eq!(attempts, [EncodeEngine::Nvidia, EncodeEngine::CpuFallback]);
        attempts.clear();
        assert!(
            encode_with_cpu_fallback(EncodeEngine::Nvidia, &cancel, |engine| {
                attempts.push(engine);
                Err("failed".into())
            })
            .is_err()
        );
        assert_eq!(attempts.len(), 2);
        attempts.clear();
        assert_eq!(
            encode_with_cpu_fallback(EncodeEngine::Nvidia, &cancel, |engine| {
                attempts.push(engine);
                cancel.store(true, Ordering::Relaxed);
                Err("GPU failed".into())
            })
            .unwrap_err(),
            "Canceled"
        );
        assert_eq!(attempts, [EncodeEngine::Nvidia]);
        attempts.clear();
        assert_eq!(
            encode_with_cpu_fallback(EncodeEngine::Nvidia, &cancel, |engine| {
                attempts.push(engine);
                Ok(())
            })
            .unwrap_err(),
            "Canceled"
        );
        assert!(attempts.is_empty());
        cancel.store(false, Ordering::Relaxed);
        assert_eq!(
            encode_with_cpu_fallback(EncodeEngine::Nvidia, &cancel, |_| Ok(())).unwrap(),
            EncodeEngine::Nvidia
        );
    }

    #[test]
    fn probe_timeout_ends_descendants_without_setting_shutdown_cancellation() {
        let cancel = AtomicBool::new(false);
        let mut saw_progress = false;
        let mut progress = |_| {
            saw_progress = true;
        };
        let started = Instant::now();
        let result = process_with_timeout(Path::new("cmd.exe"), &["/d".into(), "/c".into(), "start /b powershell.exe -NoProfile -Command Start-Sleep -Seconds 8 & echo out_time_us=1000000& powershell.exe -NoProfile -Command Start-Sleep -Seconds 8".into()], &cancel, Some(&mut progress), Some(Duration::from_secs(2)));
        assert!(saw_progress);
        assert_eq!(result.unwrap_err(), "The video engine check timed out.");
        assert!(!cancel.load(Ordering::Relaxed));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    fn fixture() -> Value {
        serde_json::json!({"streams":[{"index":0,"codec_type":"video","codec_name":"h264","width":1280,"height":720,"duration":"2.0","avg_frame_rate":"60/1","pix_fmt":"yuv420p","extradata_hash":"SHA256:one","time_base":"1/60000"}],"format":{"duration":"2.0"}})
    }

    #[test]
    fn different_encoder_configuration_requires_conversion_even_with_same_dimensions() {
        let a = parse_probe(&fixture()).unwrap();
        let mut raw = fixture();
        raw["streams"][0]["extradata_hash"] = "SHA256:two".into();
        let b = parse_probe(&raw).unwrap();
        let sources = vec![
            Source {
                path: "a.mp4".into(),
                info: a,
            },
            Source {
                path: "b.mp4".into(),
                info: b,
            },
        ];
        assert!(!can_copy(&sources));
    }

    #[test]
    fn multiple_mp4_descriptions_and_truncated_boxes_cannot_use_the_copy_path() {
        fn atom(kind: &[u8; 4], data: Vec<u8>) -> Vec<u8> {
            let mut bytes = ((data.len() + 8) as u32).to_be_bytes().to_vec();
            bytes.extend(kind);
            bytes.extend(data);
            bytes
        }
        let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("build/test-output/unit");
        fs::create_dir_all(&folder).unwrap();
        let path = folder.join(format!(
            "stitch-description-test-{}.mp4",
            std::process::id()
        ));
        for count in [1u32, 2] {
            let mut data = vec![0u8; 4];
            data.extend(count.to_be_bytes());
            let mut data = atom(b"stsd", data);
            for kind in [b"stbl", b"minf", b"mdia", b"trak", b"moov"] {
                data = atom(kind, data);
            }
            fs::write(&path, &data).unwrap();
            assert_eq!(single_mp4_descriptions(&path), count == 1);
            data.pop();
            fs::write(&path, &data).unwrap();
            assert!(!single_mp4_descriptions(&path));
        }
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn invalid_duration_and_missing_video_are_rejected() {
        let mut raw = fixture();
        raw["streams"][0]["duration"] = "NaN".into();
        raw["format"]["duration"] = "0".into();
        assert!(parse_probe(&raw).is_err());
        assert!(parse_probe(&serde_json::json!({"streams":[]})).is_err());
    }

    #[test]
    fn display_dimensions_must_stay_valid_after_aspect_ratio() {
        for rotation in [0, 90] {
            let mut raw = fixture();
            raw["streams"][0]["sample_aspect_ratio"] = "1:8192".into();
            raw["streams"][0]["side_data_list"] = serde_json::json!([{"rotation": rotation}]);
            assert!(parse_probe(&raw).is_err(), "rotation: {rotation}");
        }
        let mut raw = fixture();
        raw["streams"][0]["width"] = (u32::MAX as u64 + 1281).into();
        assert!(parse_probe(&raw).is_err());
    }

    #[test]
    fn conversion_pads_silent_clips_and_preserves_delayed_audio() {
        let silent = parse_probe(&fixture()).unwrap();
        let mut sound = silent.clone();
        sound.audio_index = Some(1);
        sound.audio_offset = 0.25;
        let args = conversion_args(
            &[
                Source {
                    path: "one.mp4".into(),
                    info: sound,
                },
                Source {
                    path: "two.mp4".into(),
                    info: silent,
                },
            ],
            Path::new("out.mp4"),
            ExportSettings::default(),
            EncodeEngine::Cpu,
        )
        .unwrap();
        let graph = &args[args.iter().position(|s| s == "-filter_complex").unwrap() + 1];
        assert!(graph.contains("anullsrc="));
        assert!(graph.contains("+0.250000/TB"));
    }

    #[test]
    fn finished_process_does_not_wait_for_descendants_with_inherited_pipes() {
        let mut ready = None;
        let mut progress = |_| ready = Some(Instant::now());
        let result = process_with_timeout(
            Path::new("powershell.exe"),
            &inherited_pipe_helper(200),
            &AtomicBool::new(false),
            Some(&mut progress),
            Some(Duration::from_secs(30)),
        );
        let elapsed = ready
            .expect("The inherited-pipe helper did not become ready.")
            .elapsed();
        println!("Process shutdown after readiness: {elapsed:?}; output: {result:?}");
        assert!(result.is_ok(), "{result:?}");
        assert!(
            elapsed < Duration::from_secs(4),
            "An inherited pipe delayed process shutdown for {elapsed:?}"
        );
    }

    #[test]
    fn cancellation_ends_descendants_with_inherited_pipes() {
        let cancel = AtomicBool::new(false);
        let mut saw_progress = false;
        let mut ready = None;
        let mut progress = |_| {
            saw_progress = true;
            ready = Some(Instant::now());
            cancel.store(true, Ordering::Relaxed);
        };
        let result = process_with_timeout(
            Path::new("powershell.exe"),
            &inherited_pipe_helper(30_000),
            &cancel,
            Some(&mut progress),
            Some(Duration::from_secs(30)),
        );
        let elapsed = ready
            .expect("The inherited-pipe helper did not become ready.")
            .elapsed();
        println!("Canceled process shutdown after readiness: {elapsed:?}");
        assert!(saw_progress);
        assert_eq!(result.unwrap_err(), "Canceled");
        assert!(
            elapsed < Duration::from_secs(4),
            "An inherited pipe delayed cancellation for {elapsed:?}"
        );
    }

    fn inherited_pipe_helper(parent_wait_ms: u64) -> Vec<String> {
        vec![
            "-NoProfile".into(),
            "-Command".into(),
            format!(
                "$null = Start-Process powershell.exe -ArgumentList @('-NoProfile','-Command','Start-Sleep -Seconds 30') -NoNewWindow -PassThru; Write-Output 'out_time_us=1000000'; Start-Sleep -Milliseconds {parent_wait_ms}"
            ),
        ]
    }
}
