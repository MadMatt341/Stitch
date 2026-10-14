use super::{ClipInfo, ExportMode, Result, Runtime, number, path_arg, process};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};

mod mp4;

pub(super) fn mp4_audio_safe(path: &std::path::Path, info: &ClipInfo) -> bool {
    mp4::audio_safe(path, info.copy_details.audio_track_id)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioMode {
    #[default]
    None,
    Copy,
    Encode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CopyIssue {
    VideoFormat,
    TrackLayout,
    VideoConfiguration,
    VideoDimensions,
    VideoTiming,
    EditedVideo,
    AudioMissing,
    AudioFormat,
    AudioConfiguration,
    AudioTiming,
    AudioPadding,
    AudioEdits,
    AudioUnverified,
    SizePreset,
    FrameRate,
    AudioCopyFailed,
    VideoCopyFailed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncodingReason {
    pub clip: usize,
    pub issue: CopyIssue,
}

impl EncodingReason {
    pub fn text(self) -> String {
        let message = match self.issue {
            CopyIssue::VideoFormat => "Video format needs conversion",
            CopyIssue::TrackLayout => "Track layout cannot be copied safely",
            CopyIssue::VideoConfiguration => "Video stream settings differ",
            CopyIssue::VideoDimensions => "Video dimensions differ",
            CopyIssue::VideoTiming => "Video timing differs",
            CopyIssue::EditedVideo => "Video edits need conversion",
            CopyIssue::AudioMissing => "Missing audio needs silence",
            CopyIssue::AudioFormat => "Audio format needs conversion",
            CopyIssue::AudioConfiguration => "Audio settings differ",
            CopyIssue::AudioTiming => "Audio timing needs correction",
            CopyIssue::AudioPadding => "Audio padding at the join needs removal",
            CopyIssue::AudioEdits => "Audio edits need encoding",
            CopyIssue::AudioUnverified => "Audio copying could not be verified",
            CopyIssue::SizePreset => "The size choice requires video encoding",
            CopyIssue::FrameRate => "The frame rate choice requires video encoding",
            CopyIssue::AudioCopyFailed => "Copy failed; retrying with encoded audio",
            CopyIssue::VideoCopyFailed => "Video copying failed; encoding video",
        };
        if self.clip == 0 {
            message.into()
        } else {
            format!("Clip {}: {message}", self.clip)
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CopyDetails {
    pub video_key: Option<String>,
    pub video_issue: Option<CopyIssue>,
    pub video_start: f64,
    pub format_start: f64,
    pub audio_key: Option<String>,
    pub audio_issue: Option<CopyIssue>,
    pub audio_bitrate: Option<f64>,
    pub audio_duration: Option<f64>,
    pub audio_track_id: Option<u32>,
    pub audio_time_base: f64,
    pub audio_boundary: Option<AudioBoundary>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AudioBoundary {
    pub first_pts: f64,
    pub last_pts: f64,
    pub skip_samples: u64,
    pub discard_padding: u64,
    pub verified: bool,
}

pub(super) fn metadata(video: &Value, audio: Option<&Value>, streams: &[Value]) -> CopyDetails {
    let video_issue = if !matches!(video["codec_name"].as_str(), Some("h264" | "hevc")) {
        Some(CopyIssue::VideoFormat)
    } else if streams
        .iter()
        .filter(|s| s["codec_type"] == "video")
        .count()
        != 1
        || streams
            .iter()
            .any(|s| !matches!(s["codec_type"].as_str(), Some("video" | "audio")))
    {
        Some(CopyIssue::TrackLayout)
    } else if video["extradata_hash"].as_str().is_none_or(str::is_empty) {
        Some(CopyIssue::VideoConfiguration)
    } else {
        None
    };
    let key = |stream: &Value, fields: &[&str]| {
        let values: std::collections::BTreeMap<_, _> = fields
            .iter()
            .map(|field| (*field, &stream[*field]))
            .collect();
        serde_json::to_string(&values).unwrap()
    };
    let video_fields = [
        "index",
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
        "extradata_hash",
        "side_data_list",
    ];
    let audio_issue = audio.and_then(|a| {
        if a["codec_name"] != "aac" || a["profile"] != "LC" {
            Some(CopyIssue::AudioFormat)
        } else if a["extradata_hash"].as_str().is_none_or(str::is_empty)
            || number(&a["sample_rate"]).is_none_or(|rate| rate <= 0.0)
            || a["channels"].as_u64().is_none_or(|channels| channels == 0)
            || a["time_base"].as_str().and_then(super::rate).is_none()
        {
            Some(CopyIssue::AudioUnverified)
        } else {
            None
        }
    });
    let audio_key = audio.filter(|_| audio_issue.is_none()).map(|a| {
        key(
            a,
            &[
                "codec_name",
                "profile",
                "sample_rate",
                "channels",
                "channel_layout",
                "extradata_hash",
            ],
        )
    });
    CopyDetails {
        video_key: video_issue.is_none().then(|| key(video, &video_fields)),
        video_issue,
        video_start: number(&video["start_time"]).unwrap_or(0.0),
        format_start: number(&video["start_time"]).unwrap_or(0.0),
        audio_key,
        audio_issue,
        audio_bitrate: audio
            .and_then(|a| number(&a["bit_rate"]).or_else(|| number(&a["tags"]["BPS"])))
            .filter(|rate| *rate > 0.0),
        audio_track_id: audio.and_then(|a| a["id"].as_str()).and_then(|id| {
            if let Some(hex) = id.strip_prefix("0x") {
                u32::from_str_radix(hex, 16).ok()
            } else {
                id.parse().ok()
            }
        }),
        audio_duration: audio.and_then(super::stream_duration),
        audio_time_base: audio
            .and_then(|a| a["time_base"].as_str())
            .and_then(super::rate)
            .unwrap_or(0.0),
        audio_boundary: None,
    }
}

pub(super) fn video_reason(infos: &[&ClipInfo]) -> Option<EncodingReason> {
    let first = *infos.first()?;
    let key = |info: &ClipInfo| {
        info.copy_details
            .video_key
            .as_ref()
            .or_else(|| {
                info.copy_details
                    .video_issue
                    .is_none()
                    .then_some(info.copy_key.as_ref())
                    .flatten()
            })
            .cloned()
    };
    for (index, info) in infos.iter().enumerate() {
        let issue = if let Some(issue) = info.copy_details.video_issue {
            issue
        } else if key(info).is_none() {
            CopyIssue::VideoConfiguration
        } else if info.video_index != first.video_index {
            CopyIssue::TrackLayout
        } else if info.width != first.width || info.height != first.height {
            CopyIssue::VideoDimensions
        } else if info.rate != first.rate {
            CopyIssue::VideoTiming
        } else if key(info) != key(first) {
            CopyIssue::VideoConfiguration
        } else {
            continue;
        };
        return Some(EncodingReason {
            clip: index + 1,
            issue,
        });
    }
    None
}

pub(super) fn tolerance(info: &ClipInfo) -> f64 {
    (info.copy_details.audio_time_base * 0.51).max(0.51 / info.audio_rate.max(1) as f64) + 0.000002
}

pub(super) fn audio_reason(infos: &[&ClipInfo]) -> Option<EncodingReason> {
    let first = *infos.first()?;
    for (index, info) in infos.iter().enumerate() {
        let issue = if info.audio_index.is_none() {
            CopyIssue::AudioMissing
        } else if info.copy_details.audio_key.is_none() {
            info.copy_details
                .audio_issue
                .unwrap_or(CopyIssue::AudioUnverified)
        } else if info.copy_details.audio_key != first.copy_details.audio_key
            || info.audio_index != first.audio_index
        {
            CopyIssue::AudioConfiguration
        } else if let Some(boundary) = &info.copy_details.audio_boundary {
            if !boundary.verified
                || info.audio_rate == 0
                || boundary.skip_samples > 1024
                || boundary.discard_padding >= 1024
            {
                CopyIssue::AudioUnverified
            } else if (index > 0 && boundary.skip_samples > 0)
                || (index + 1 < infos.len() && boundary.discard_padding > 0)
            {
                CopyIssue::AudioPadding
            } else {
                let rate = info.audio_rate.max(1) as f64;
                let start = boundary.first_pts + boundary.skip_samples as f64 / rate
                    - info.copy_details.video_start;
                let end = boundary.last_pts + (1024 - boundary.discard_padding) as f64 / rate
                    - info.copy_details.video_start;
                if start.abs() > tolerance(info)
                    || (end - info.duration).abs() > tolerance(info)
                    || (infos.len() > 1
                        && (info.copy_details.format_start - info.copy_details.video_start).abs()
                            > tolerance(info))
                {
                    CopyIssue::AudioTiming
                } else {
                    continue;
                }
            }
        } else {
            CopyIssue::AudioUnverified
        };
        return Some(EncodingReason {
            clip: index + 1,
            issue,
        });
    }
    None
}

pub(super) fn inspect_audio(
    runtime: &Runtime,
    path: &std::path::Path,
    info: &ClipInfo,
    cancel: &AtomicBool,
) -> Result<Option<AudioBoundary>> {
    if info.copy_details.audio_key.is_none() {
        return Ok(None);
    }
    let index = info.audio_index.unwrap();
    let entries = "pts_time,duration_time,side_data_list";
    let read = || -> Result<Vec<Value>> {
        if info.duration <= 5.0 {
            return probe_packets(runtime, path, index, entries, None, cancel);
        }
        let mut packets = probe_packets(runtime, path, index, entries, Some("%+0.25"), cancel)?;
        let end = info.copy_details.video_start
            + info.audio_offset
            + info.copy_details.audio_duration.unwrap_or(info.duration);
        let tail = format!("{:.9}%", (end - 0.25).max(0.0));
        packets.extend(probe_packets(
            runtime,
            path,
            index,
            entries,
            Some(&tail),
            cancel,
        )?);
        Ok(packets)
    };
    let packets = match read() {
        Ok(packets) => packets,
        Err(_) if !cancel.load(Ordering::Relaxed) => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut bounds = boundary(&serde_json::json!({"packets":packets}), info);
    if info.duration > 5.0
        && let Some(bounds) = bounds.as_mut()
    {
        let expected_end = info.copy_details.video_start
            + info.audio_offset
            + info.copy_details.audio_duration.unwrap_or(info.duration);
        let actual_end = bounds.last_pts
            + (1024_u64.saturating_sub(bounds.discard_padding)) as f64
                / info.audio_rate.max(1) as f64;
        if actual_end < expected_end - 0.25 {
            bounds.verified = false;
        }
    }
    Ok(bounds)
}

pub(super) fn probe_packets(
    runtime: &Runtime,
    path: &std::path::Path,
    index: usize,
    entries: &str,
    interval: Option<&str>,
    cancel: &AtomicBool,
) -> Result<Vec<Value>> {
    let mut args = vec![
        "-v".into(),
        "error".into(),
        "-select_streams".into(),
        index.to_string(),
        "-show_packets".into(),
        "-show_entries".into(),
        format!("packet={entries}"),
        "-of".into(),
        "json".into(),
    ];
    if let Some(interval) = interval {
        args.extend(["-read_intervals".into(), interval.into()]);
    }
    args.extend(["-i".into(), path_arg(path)]);
    let output = process(&runtime.ffprobe, &args, cancel, None)?;
    let root: Value = serde_json::from_slice(&output).map_err(|_| "Cannot read audio timing.")?;
    root["packets"]
        .as_array()
        .cloned()
        .ok_or_else(|| "Cannot read packet timing.".into())
}

fn skips(packet: &Value) -> (u64, u64) {
    packet["side_data_list"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|data| data["side_data_type"] == "Skip Samples")
        .map(|data| {
            (
                data["skip_samples"].as_u64().unwrap_or(0),
                data["discard_padding"].as_u64().unwrap_or(0),
            )
        })
        .unwrap_or_default()
}

fn boundary(root: &Value, info: &ClipInfo) -> Option<AudioBoundary> {
    let packets = root["packets"].as_array()?;
    let first = packets.first()?;
    let last = packets.last()?;
    let first_pts = number(&first["pts_time"])?;
    let last_pts = number(&last["pts_time"])?;
    let (skip_samples, _) = skips(first);
    let (_, discard_padding) = skips(last);
    let rate = info.audio_rate.max(1) as f64;
    let step = 1024.0 / rate;
    let tick = tolerance(info) * 2.0;
    let verified = skip_samples <= 1024
        && discard_padding < 1024
        && number(&first["duration_time"]).is_some_and(|duration| (duration - step).abs() <= tick)
        && number(&last["duration_time"]).is_some_and(|duration| {
            (duration + discard_padding as f64 / rate - step).abs() <= tick
        })
        && packets.iter().enumerate().all(|(index, packet)| {
            let (skip, discard) = skips(packet);
            (index == 0 || skip == 0) && (index + 1 == packets.len() || discard == 0)
        });
    Some(AudioBoundary {
        first_pts,
        last_pts,
        skip_samples,
        discard_padding,
        verified,
    })
}

pub(super) fn validate_audio(
    runtime: &Runtime,
    path: &std::path::Path,
    sources: &[super::Source],
    output: &ClipInfo,
    cancel: &AtomicBool,
) -> Result<()> {
    let total: f64 = sources.iter().map(|s| s.info.duration).sum();
    let boundary = output
        .copy_details
        .audio_boundary
        .as_ref()
        .ok_or("Cannot verify copied audio.")?;
    let tick = sources
        .iter()
        .map(|s| tolerance(&s.info))
        .fold(tolerance(output), f64::max)
        * 2.0;
    let rate = output.audio_rate.max(1) as f64;
    let first = sources[0]
        .info
        .copy_details
        .audio_boundary
        .as_ref()
        .ok_or("Cannot verify copied audio.")?;
    let final_source = &sources.last().unwrap().info;
    let last = final_source
        .copy_details
        .audio_boundary
        .as_ref()
        .ok_or("Cannot verify copied audio.")?;
    if !boundary.verified
        || boundary.discard_padding >= 1024
        || output.copy_details.audio_key != sources[0].info.copy_details.audio_key
        || output.audio_rate != sources[0].info.audio_rate
        || output.audio_channels != sources[0].info.audio_channels
        || (boundary.first_pts
            - output.copy_details.video_start
            - (first.first_pts - sources[0].info.copy_details.video_start))
            .abs()
            > tick
        || (boundary.last_pts + 1024.0 / rate
            - output.copy_details.video_start
            - total
            - (last.last_pts + 1024.0 / rate
                - final_source.copy_details.video_start
                - final_source.duration))
            .abs()
            > tick
        || (boundary.first_pts + boundary.skip_samples as f64 / rate
            - output.copy_details.video_start)
            .abs()
            > tick
        || (boundary.last_pts + (1024 - boundary.discard_padding) as f64 / rate
            - output.copy_details.video_start
            - total)
            .abs()
            > tick
    {
        return Err("Copied audio timing does not match the clip list.".into());
    }
    let mut position = 0.0;
    let mut positions = Vec::new();
    for source in sources.iter().take(sources.len().saturating_sub(1)) {
        position += source.info.duration;
        positions.push(output.copy_details.video_start + position);
    }
    for group in positions.chunks(64) {
        let intervals = group
            .iter()
            .map(|at| format!("{:.9}%{:.9}", (at - 0.1).max(0.0), at + 0.1))
            .collect::<Vec<_>>()
            .join(",");
        let args = vec![
            "-v".into(),
            "error".into(),
            "-select_streams".into(),
            output.audio_index.unwrap().to_string(),
            "-read_intervals".into(),
            intervals,
            "-show_packets".into(),
            "-show_entries".into(),
            "packet=pts_time,side_data_list".into(),
            "-of".into(),
            "json".into(),
            "-i".into(),
            path_arg(path),
        ];
        let data = process(&runtime.ffprobe, &args, cancel, None)?;
        let root: Value =
            serde_json::from_slice(&data).map_err(|_| "Cannot verify the audio join.")?;
        let packets = root["packets"]
            .as_array()
            .ok_or("Cannot verify the audio join.")?;
        for at in group {
            let joined = packets.windows(2).any(|pair| {
                let (Some(previous), Some(next)) =
                    (number(&pair[0]["pts_time"]), number(&pair[1]["pts_time"]))
                else {
                    return false;
                };
                (next - at).abs() <= tick
                    && (previous + 1024.0 / rate - next).abs() <= tick
                    && skips(&pair[0]).1 == 0
                    && skips(&pair[1]).0 == 0
            });
            if !joined {
                return Err("Copied audio has a gap or padding at a clip boundary.".into());
            }
        }
    }
    Ok(())
}

pub fn action(mode: ExportMode, audio: AudioMode) -> &'static str {
    match (mode, audio) {
        (ExportMode::Copy, AudioMode::None) => "Copy video",
        (ExportMode::Copy, AudioMode::Copy) => "Copy video and audio",
        (ExportMode::Copy, AudioMode::Encode) => "Copy video · encode audio",
        (ExportMode::Convert, AudioMode::Copy) => "Encode video · copy audio",
        (ExportMode::Convert, AudioMode::None) => "Encode video",
        (ExportMode::Convert, AudioMode::Encode) => "Encode video and audio",
    }
}

pub(super) fn audio_encoding_format(info: &ClipInfo, conversion: bool) -> (u32, &str, u32) {
    let supported_layout = matches!(
        info.audio_layout.as_str(),
        "mono"
            | "stereo"
            | "2.1"
            | "3.0"
            | "4.0"
            | "quad"
            | "5.0"
            | "5.1"
            | "5.1(side)"
            | "7.1"
            | "7.1(wide)"
    );
    let supported_rate = [
        96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350,
    ]
    .contains(&info.audio_rate);
    if conversion || !supported_layout || !supported_rate {
        (48000, "stereo", 192)
    } else {
        (
            info.audio_rate,
            &info.audio_layout,
            info.audio_channels.clamp(1, 8) * 96,
        )
    }
}

pub(super) fn mux_args(
    manifest: &std::path::Path,
    sources: &[super::Source],
    video: Option<&std::path::Path>,
    audio_mode: AudioMode,
    audio: Option<&std::path::Path>,
    output: &std::path::Path,
) -> Vec<String> {
    let mut args = vec![
        "-v".into(),
        "error".into(),
        "-nostdin".into(),
        "-y".into(),
        "-copyts".into(),
    ];
    let timeline = |args: &mut Vec<String>| {
        if sources.len() == 1 {
            args.extend([
                "-itsoffset".into(),
                format!("{:.9}", -sources[0].info.copy_details.video_start),
                "-i".into(),
                path_arg(&sources[0].path),
            ]);
            return;
        }
        args.extend([
            "-f".into(),
            "concat".into(),
            "-auto_convert".into(),
            "0".into(),
            "-safe".into(),
            "0".into(),
            "-i".into(),
            path_arg(manifest),
        ])
    };
    if let Some(video) = video {
        args.extend(["-i".into(), path_arg(video)]);
    } else {
        timeline(&mut args);
    }
    if audio_mode == AudioMode::Copy && video.is_some() {
        timeline(&mut args);
    } else if let Some(audio) = audio {
        args.extend(["-i".into(), path_arg(audio)]);
    }
    args.extend([
        "-map".into(),
        if video.is_some() {
            "0:v:0".into()
        } else {
            format!("0:{}", sources[0].info.video_index)
        },
    ]);
    match audio_mode {
        AudioMode::Copy => args.extend([
            "-map".into(),
            format!(
                "{}:{}",
                usize::from(video.is_some()),
                sources[0].info.audio_index.unwrap()
            ),
        ]),
        AudioMode::Encode => args.extend(["-map".into(), "1:a:0".into()]),
        AudioMode::None => {}
    }
    args.extend([
        "-c".into(),
        "copy".into(),
        "-avoid_negative_ts".into(),
        "disabled".into(),
        "-movflags".into(),
        "+faststart".into(),
        "-progress".into(),
        "pipe:1".into(),
        "-nostats".into(),
        path_arg(output),
    ]);
    args
}

#[cfg(test)]
mod tests {
    use super::super::{ExportPreset, ExportSettings, FrameRate, plan};
    use super::*;

    fn clip() -> ClipInfo {
        let mut info = super::super::parse_probe(&serde_json::json!({
            "streams":[
                {"index":0,"codec_type":"video","codec_name":"h264","width":1920,"height":1080,
                 "duration":"1.6","avg_frame_rate":"25/1","bit_rate":"4000000","extradata_hash":"SHA256:video"},
                {"index":1,"codec_type":"audio","codec_name":"aac","profile":"LC","sample_rate":"48000",
                 "channels":2,"channel_layout":"stereo","time_base":"1/48000","duration":"1.6",
                 "bit_rate":"128000","extradata_hash":"SHA256:audio"}
            ]})).unwrap();
        info.copy_details.audio_boundary = Some(AudioBoundary {
            first_pts: 0.0,
            last_pts: 1.6 - 1024.0 / 48000.0,
            skip_samples: 0,
            discard_padding: 0,
            verified: true,
        });
        info
    }

    #[test]
    fn internal_aac_delay_requires_only_audio_encoding() {
        let first = clip();
        let mut second = clip();
        let boundary = second.copy_details.audio_boundary.as_mut().unwrap();
        boundary.first_pts = -1024.0 / 48000.0;
        boundary.skip_samples = 1024;
        let planned = plan(&[&first, &second], ExportSettings::default()).unwrap();
        assert_eq!(planned.mode, ExportMode::Copy);
        assert_eq!(planned.audio_mode, AudioMode::Encode);
        assert_eq!(
            planned.audio_reason,
            Some(EncodingReason {
                clip: 2,
                issue: CopyIssue::AudioPadding
            })
        );
        let single = plan(&[&second], ExportSettings::default()).unwrap();
        assert_eq!(single.audio_mode, AudioMode::Copy);
    }

    #[test]
    fn audio_gaps_rate_mismatch_and_missing_audio_do_not_force_video_encoding() {
        let first = clip();
        let mut second = clip();
        second
            .copy_details
            .audio_boundary
            .as_mut()
            .unwrap()
            .last_pts -= 0.03;
        assert_eq!(
            plan(&[&first, &second], ExportSettings::default())
                .unwrap()
                .audio_reason
                .unwrap()
                .issue,
            CopyIssue::AudioTiming
        );
        second = clip();
        second.copy_details.audio_key = Some("different sample rate".into());
        let planned = plan(&[&first, &second], ExportSettings::default()).unwrap();
        assert_eq!(planned.mode, ExportMode::Copy);
        assert_eq!(
            planned.audio_reason.unwrap().issue,
            CopyIssue::AudioConfiguration
        );
        second.audio_index = None;
        let planned = plan(&[&first, &second], ExportSettings::default()).unwrap();
        assert_eq!(planned.mode, ExportMode::Copy);
        assert_eq!(planned.audio_reason.unwrap().issue, CopyIssue::AudioMissing);
    }

    #[test]
    fn video_encoding_keeps_copyable_audio_and_reports_the_actual_trigger() {
        let first = clip();
        let mut second = clip();
        second.width = 1280;
        let planned = plan(&[&first, &second], ExportSettings::default()).unwrap();
        assert_eq!(planned.mode, ExportMode::Convert);
        assert_eq!(planned.audio_mode, AudioMode::Copy);
        assert_eq!(
            planned.video_reason.unwrap().issue,
            CopyIssue::VideoDimensions
        );
        for (settings, issue) in [
            (
                ExportSettings {
                    preset: ExportPreset::Small,
                    ..Default::default()
                },
                CopyIssue::SizePreset,
            ),
            (
                ExportSettings {
                    frame_rate: FrameRate::Fps60,
                    ..Default::default()
                },
                CopyIssue::FrameRate,
            ),
        ] {
            let planned = plan(&[&first, &first], settings).unwrap();
            assert_eq!(planned.mode, ExportMode::Convert);
            assert_eq!(planned.audio_mode, AudioMode::Copy);
            assert_eq!(planned.video_reason.unwrap().issue, issue);
        }
    }

    #[test]
    fn copy_estimates_use_selected_audio_rates_and_unknown_timing_requires_audio_encoding() {
        let mut info = clip();
        let planned = plan(&[&info, &info], ExportSettings::default()).unwrap();
        assert_eq!(planned.audio_mode, AudioMode::Copy);
        assert_eq!(
            planned.estimated_bytes,
            Some((4_000_000.0 + 128_000.0) * 3.2 / 8.0)
        );
        info.copy_details.audio_boundary = None;
        let planned = plan(&[&info], ExportSettings::default()).unwrap();
        assert_eq!(planned.mode, ExportMode::Copy);
        assert_eq!(
            planned.audio_reason.unwrap().issue,
            CopyIssue::AudioUnverified
        );
    }

    #[test]
    fn packet_duration_and_padding_must_describe_1024_sample_aac_frames() {
        let info = clip();
        let mut packets = serde_json::json!({"packets":[
            {"pts_time":"0.0","duration_time":"0.021333"},
            {"pts_time":"1.578667","duration_time":"0.021333"}
        ]});
        assert!(boundary(&packets, &info).unwrap().verified);
        packets["packets"][1]["duration_time"] = "0.020".into();
        assert!(!boundary(&packets, &info).unwrap().verified);
        packets["packets"][1]["duration_time"] = "0.021333".into();
        packets["packets"][1]["side_data_list"] =
            serde_json::json!([{"side_data_type":"Skip Samples","skip_samples":1024}]);
        assert!(!boundary(&packets, &info).unwrap().verified);
    }
}
