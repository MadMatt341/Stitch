use std::{
    fs,
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};
use stitch::media::{
    self, EncodeEngine, ExportMode, ExportOptions, ExportPreset, ExportSettings, FrameRate,
    NvidiaSupport, Runtime, Source, VideoCodec,
};

fn engine() -> Runtime {
    Runtime::beside(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("build/Stitch.exe"))
        .expect("Run scripts/build.ps1 first")
}
fn run(tool: &Path, args: &[&str]) -> Vec<u8> {
    let output = Command::new(tool).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}
fn make(
    runtime: &Runtime,
    path: &Path,
    color: &str,
    size: &str,
    fps: &str,
    audio: bool,
    duration: &str,
) {
    let input = format!("color=c={color}:s={size}:r={fps}:d={duration}");
    let sound = format!("sine=frequency=440:sample_rate=48000:duration={duration}");
    let mut args = vec!["-v", "error", "-y", "-f", "lavfi", "-i", &input];
    if audio {
        args.extend(["-f", "lavfi", "-i", &sound]);
    }
    args.extend([
        "-c:v", "libx264", "-preset", "veryfast", "-bf", "0", "-pix_fmt", "yuv420p",
    ]);
    if audio {
        args.extend(["-c:a", "aac", "-b:a", "192k"]);
    }
    let name = path.to_str().unwrap();
    args.push(name);
    run(&runtime.ffmpeg, &args);
}
fn source(runtime: &Runtime, path: &Path) -> Source {
    Source {
        path: path.into(),
        info: media::inspect(runtime, path, &AtomicBool::new(false)).unwrap(),
    }
}
fn packet_hashes(runtime: &Runtime, path: &Path) -> Vec<String> {
    let raw = run(
        &runtime.ffprobe,
        &[
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_packets",
            "-show_data_hash",
            "sha256",
            "-of",
            "json",
            path.to_str().unwrap(),
        ],
    );
    let json: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    json["packets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["data_hash"].as_str().unwrap().to_owned())
        .collect()
}
fn colors(runtime: &Runtime, path: &Path) -> Vec<[u8; 3]> {
    let raw = run(
        &runtime.ffmpeg,
        &[
            "-v",
            "error",
            "-i",
            path.to_str().unwrap(),
            "-map",
            "0:v:0",
            "-vf",
            "scale=1:1",
            "-pix_fmt",
            "rgb24",
            "-fps_mode",
            "passthrough",
            "-f",
            "rawvideo",
            "pipe:1",
        ],
    );
    raw.as_chunks::<3>()
        .0
        .iter()
        .map(|p| [p[0], p[1], p[2]])
        .collect()
}
fn clean_partials(folder: &Path) {
    assert!(
        !fs::read_dir(folder).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".stitch-")),
        "Partial output was left behind"
    );
}

fn pq_signal(nits: f64) -> f64 {
    let power = (nits / 10_000.0).powf(2610.0 / 16384.0);
    ((3424.0 / 4096.0 + 2413.0 / 128.0 * power) / (1.0 + 2392.0 / 128.0 * power))
        .powf(2523.0 / 32.0)
}

fn display_nits(signal: f64, transfer: u8) -> f64 {
    let signal = signal.max(0.0);
    match transfer {
        16 => {
            let power = signal.powf(32.0 / 2523.0);
            10_000.0
                * ((power - 3424.0 / 4096.0).max(0.0) / (2413.0 / 128.0 - 2392.0 / 128.0 * power))
                    .powf(16384.0 / 2610.0)
        }
        18 => {
            let linear = if signal <= 0.5 {
                signal * signal / 3.0
            } else {
                (((signal - 0.55991073) / 0.17883277).exp() + 0.28466892) / 12.0
            };
            1000.0 * linear.powf(1.2)
        }
        _ => 100.0 * signal.powf(2.4),
    }
}

fn make_color_bars(
    runtime: &Runtime,
    path: &Path,
    transfer: u8,
    bits: u8,
    fps: usize,
    audio: bool,
) {
    let raw_path = path.with_extension("yuv");
    let signals = match transfer {
        16 => [0.0, 100.0, 400.0, 1000.0].map(pq_signal),
        18 => [0.0, 0.5, 0.75, 1.0],
        _ => [0.0, 0.25, 0.5, 1.0],
    };
    let (black, white, neutral): (f64, f64, u16) = if bits == 10 {
        (64.0, 940.0, 512)
    } else {
        (16.0, 235.0, 128)
    };
    let mut frame = Vec::new();
    let chroma = |x: usize| {
        let rgb = if x < 80 {
            [400.0, 20.0, 10.0]
        } else {
            [10.0, 20.0, 400.0]
        }
        .map(pq_signal);
        let y = 0.2627 * rgb[0] + 0.6780 * rgb[1] + 0.0593 * rgb[2];
        [
            y,
            (rgb[2] - y) / (2.0 * (1.0 - 0.0593)),
            (rgb[0] - y) / (2.0 * (1.0 - 0.2627)),
        ]
    };
    for y in 0..90 {
        for x in 0..160 {
            let signal = if transfer == 16 && y >= 72 {
                chroma(x)[0]
            } else {
                signals[x / 40]
            };
            let value = (black + (white - black) * signal).round() as u16;
            if bits == 10 {
                frame.extend(value.to_le_bytes());
            } else {
                frame.push(value as u8);
            }
        }
    }
    for plane in [1, 2] {
        for y in 0..45 {
            for x in 0..80 {
                let value = if transfer == 16 && y >= 36 {
                    (neutral as f64 + if bits == 10 { 896.0 } else { 224.0 } * chroma(x * 2)[plane])
                        .round() as u16
                } else {
                    neutral
                };
                if bits == 10 {
                    frame.extend(value.to_le_bytes());
                } else {
                    frame.push(value as u8);
                }
            }
        }
    }
    fs::write(&raw_path, frame.repeat(fps / 2)).unwrap();
    let rate = fps.to_string();
    let pixel = if bits == 10 { "yuv420p10le" } else { "yuv420p" };
    let hdr = transfer == 16 || transfer == 18;
    let color_transfer = transfer.to_string();
    let tags = format!(
        "setparams=range=limited:color_primaries={}:color_trc={transfer}:colorspace={}",
        if hdr { 9 } else { 1 },
        if hdr { 9 } else { 1 }
    );
    let mut args = vec![
        "-v",
        "error",
        "-y",
        "-f",
        "rawvideo",
        "-pixel_format",
        pixel,
        "-video_size",
        "160x90",
        "-framerate",
        &rate,
        "-i",
        raw_path.to_str().unwrap(),
    ];
    if audio {
        args.extend([
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=0.5",
            "-c:a",
            "aac",
        ]);
    }
    args.extend([
        "-c:v",
        "libx265",
        "-vf",
        &tags,
        "-preset",
        "veryfast",
        "-x265-params",
        "pools=1:frame-threads=1:log-level=error:lossless=1",
        "-pix_fmt",
        pixel,
        "-color_primaries",
        if hdr { "9" } else { "1" },
        "-color_trc",
        &color_transfer,
        "-colorspace",
        if hdr { "9" } else { "1" },
        "-color_range",
        "tv",
        "-tag:v",
        "hvc1",
        path.to_str().unwrap(),
    ]);
    run(&runtime.ffmpeg, &args);
}

fn gray_frames(runtime: &Runtime, path: &Path, transfer: u8) -> Vec<[f64; 4]> {
    let raw = run(
        &runtime.ffmpeg,
        &[
            "-v",
            "error",
            "-threads",
            "1",
            "-i",
            path.to_str().unwrap(),
            "-map",
            "0:v:0",
            "-pix_fmt",
            "yuv420p10le",
            "-fps_mode",
            "passthrough",
            "-f",
            "rawvideo",
            "pipe:1",
        ],
    );
    let frame_bytes = 160 * 90 * 3;
    assert!(raw.len().is_multiple_of(frame_bytes));
    raw.chunks_exact(frame_bytes)
        .map(|frame| {
            [20, 60, 100, 140].map(|x| {
                let offset = (45 * 160 + x) * 2;
                let value = u16::from_le_bytes([frame[offset], frame[offset + 1]]);
                display_nits((value as f64 - 64.0) / 876.0, transfer)
            })
        })
        .collect()
}

fn rgb_nits_at(runtime: &Runtime, path: &Path, seconds: f64) -> [[f64; 3]; 2] {
    let time = seconds.to_string();
    let raw = run(
        &runtime.ffmpeg,
        &[
            "-v",
            "error",
            "-ss",
            &time,
            "-i",
            path.to_str().unwrap(),
            "-map",
            "0:v:0",
            "-vf",
            "zscale=t=linear:npl=100:agamma=0,format=gbrpf32le",
            "-frames:v",
            "1",
            "-f",
            "rawvideo",
            "pipe:1",
        ],
    );
    assert_eq!(raw.len(), 160 * 90 * 12);
    [20, 100].map(|x| {
        [2, 0, 1].map(|plane| {
            let offset = (plane * 160 * 90 + 80 * 160 + x) * 4;
            f32::from_le_bytes(raw[offset..offset + 4].try_into().unwrap()) as f64 * 100.0
        })
    })
}

#[test]
#[ignore = "requires the pinned FFmpeg engine; verifies HDR and high bit depth color conversion"]
fn hdr_and_ten_bit_conversion_preserve_brightness_and_timing() {
    let runtime = engine();
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "build/test-output/color-flow-{}",
        std::process::id()
    ));
    fs::create_dir(&folder).unwrap();
    let pq_path = folder.join("pq.mp4");
    let hlg_path = folder.join("hlg.mp4");
    let sdr_path = folder.join("sdr10.mp4");
    let sdr8_path = folder.join("sdr8.mp4");
    for (path, transfer, bits, fps, audio) in [
        (&pq_path, 16, 10, 12, false),
        (&hlg_path, 18, 10, 24, false),
        (&sdr_path, 1, 10, 12, true),
        (&sdr8_path, 1, 8, 12, false),
    ] {
        make_color_bars(&runtime, path, transfer, bits, fps, audio);
    }
    let pq = source(&runtime, &pq_path);
    let hlg = source(&runtime, &hlg_path);
    let sdr = source(&runtime, &sdr_path);
    let sdr8 = source(&runtime, &sdr8_path);
    assert!(pq.info.hdr && hlg.info.hdr && !sdr.info.hdr && !sdr8.info.hdr);
    let originals: Vec<_> = [&pq, &hlg, &sdr, &sdr8]
        .into_iter()
        .map(|source| (&source.path, fs::read(&source.path).unwrap()))
        .collect();
    let cancel = AtomicBool::new(false);
    let support = media::probe_nvidia(&runtime, &cancel);
    let check = |name: &str,
                 sources: &[Source],
                 settings: ExportSettings,
                 nvidia: NvidiaSupport,
                 expected_engine: EncodeEngine,
                 output_transfer: u8| {
        let output = folder.join(format!("encoded-{name}.mp4"));
        let mut engines = Vec::new();
        assert_eq!(
            media::export(
                &runtime,
                sources,
                &output,
                ExportOptions { settings, nvidia },
                &cancel,
                |progress| {
                    if let Some(engine) = progress.engine {
                        engines.push(engine);
                    }
                }
            )
            .unwrap(),
            ExportMode::Convert
        );
        assert_eq!(engines.last(), Some(&expected_engine));
        let info = media::inspect(&runtime, &output, &cancel).unwrap();
        assert_eq!(info.color.bit_depth, 10);
        assert_eq!(info.color.transfer, Some(output_transfer));
        assert_eq!(
            info.color.primaries,
            Some(if output_transfer == 1 { 1 } else { 9 })
        );
        assert_eq!(
            info.color.matrix,
            Some(if output_transfer == 1 { 1 } else { 9 })
        );
        assert!(!info.color.full_range);
        assert_eq!(
            info.audio_index.is_some(),
            sources
                .iter()
                .any(|source| source.info.audio_index.is_some())
        );
        let times = frame_times(&runtime, &output);
        let frames = gray_frames(&runtime, &output, output_transfer);
        assert_eq!(frames.len(), times.len());
        if let Some(fps) = settings.frame_rate.fps() {
            assert_eq!(
                frames.len(),
                (sources.len() as f64 * 0.5 * fps).round() as usize
            );
            for (index, time) in times.iter().enumerate() {
                assert!((*time - index as f64 / fps).abs() < 0.00001);
            }
        } else {
            let mut expected = Vec::new();
            for (index, source) in sources.iter().enumerate() {
                expected.extend(
                    frame_times(&runtime, &source.path)
                        .iter()
                        .map(|time| time + index as f64 * 0.5),
                );
            }
            assert_eq!(frames.len(), expected.len());
            for (time, expected) in times.iter().zip(expected) {
                assert!((*time - expected).abs() < 0.00002);
            }
        }
        let references: Vec<_> = sources
            .iter()
            .map(|source| {
                gray_frames(&runtime, &source.path, source.info.color.transfer.unwrap())[0]
            })
            .collect();
        for (time, frame) in times.iter().zip(frames) {
            let reference = references[((*time + 0.00001) / 0.5).floor() as usize];
            for (actual, expected) in frame.into_iter().zip(reference) {
                assert!(
                    (actual - expected).abs() < 2.0_f64.max(expected * 0.06),
                    "{name} at {time}: expected {expected} nits, got {actual}"
                );
            }
        }
        for (index, source) in sources.iter().enumerate() {
            if source.info.color.transfer == Some(16) {
                let pixels = rgb_nits_at(&runtime, &output, index as f64 * 0.5 + 0.25);
                for (actual, expected) in pixels
                    .into_iter()
                    .zip([[400.0, 20.0, 10.0], [10.0, 20.0, 400.0]])
                {
                    for (actual, expected) in actual.into_iter().zip(expected) {
                        assert!(
                            (actual - expected).abs() < 3.0_f64.max(expected * 0.06),
                            "{name}: expected color component {expected} nits, got {actual}"
                        );
                    }
                }
            }
        }
        if info.audio_index.is_some() {
            let audio = run(
                &runtime.ffmpeg,
                &[
                    "-v",
                    "error",
                    "-i",
                    output.to_str().unwrap(),
                    "-map",
                    "0:a:0",
                    "-ac",
                    "1",
                    "-ar",
                    "48000",
                    "-f",
                    "s16le",
                    "pipe:1",
                ],
            );
            let samples: Vec<_> = audio
                .as_chunks::<2>()
                .0
                .iter()
                .map(|bytes| i16::from_le_bytes(*bytes))
                .collect();
            assert!((samples.len() as f64 / 48000.0 - sources.len() as f64 * 0.5).abs() < 0.08);
            for (index, source) in sources.iter().enumerate() {
                let interval = &samples[index * 24000 + 4800..index * 24000 + 19200];
                if source.info.audio_index.is_some() {
                    assert!(interval.iter().any(|sample| sample.abs() > 200));
                } else {
                    assert!(interval.iter().all(|sample| sample.abs() < 30));
                }
            }
        }
        clean_partials(&folder);
    };
    let mixed = vec![sdr.clone(), pq.clone(), hlg.clone(), sdr.clone()];
    for rate in [FrameRate::Original, FrameRate::Fps30, FrameRate::Fps60] {
        for preset in [
            ExportPreset::Source,
            ExportPreset::Small,
            ExportPreset::Medium,
            ExportPreset::Large,
        ] {
            check(
                &format!("mixed-{preset:?}-{rate:?}"),
                &mixed,
                ExportSettings {
                    preset,
                    frame_rate: rate,
                    ..ExportSettings::default()
                },
                NvidiaSupport::default(),
                EncodeEngine::Cpu,
                16,
            );
        }
    }
    for (name, sources, transfer) in [
        ("pq", vec![pq.clone(), pq.clone()], 16),
        ("hlg", vec![hlg.clone(), hlg.clone()], 18),
        ("sdr10", vec![sdr.clone(), sdr.clone()], 1),
        ("reverse", vec![hlg.clone(), pq.clone(), sdr.clone()], 16),
        ("mixed8", vec![sdr8.clone(), pq.clone()], 16),
    ] {
        check(
            name,
            &sources,
            ExportSettings {
                preset: ExportPreset::Medium,
                frame_rate: FrameRate::Fps60,
                ..ExportSettings::default()
            },
            NvidiaSupport::default(),
            EncodeEngine::Cpu,
            transfer,
        );
    }
    for preset in [ExportPreset::Source, ExportPreset::Medium] {
        check(
            &format!("hardware-{preset:?}"),
            &mixed,
            ExportSettings {
                preset,
                frame_rate: FrameRate::Fps60,
                ..ExportSettings::default()
            },
            NvidiaSupport {
                h264: true,
                hevc: true,
            },
            if support.hevc {
                EncodeEngine::Nvidia
            } else {
                EncodeEngine::CpuFallback
            },
            16,
        );
    }
    let copied = folder.join("pq-copy.mp4");
    assert_eq!(
        media::export(
            &runtime,
            &[pq.clone(), pq.clone()],
            &copied,
            ExportOptions::default(),
            &cancel,
            |_| {}
        )
        .unwrap(),
        ExportMode::Copy
    );
    let packets = packet_hashes(&runtime, &pq.path);
    let packet_reference = [packets.clone(), packets].concat();
    assert_eq!(packet_hashes(&runtime, &copied), packet_reference);
    let legacy_output = folder.join("hdr-to-legacy-sdr.mp4");
    media::export(
        &runtime,
        &[pq.clone(), hlg.clone()],
        &legacy_output,
        ExportOptions {
            settings: ExportSettings {
                codec: VideoCodec::H264,
                frame_rate: FrameRate::Fps60,
                ..ExportSettings::default()
            },
            nvidia: NvidiaSupport::default(),
        },
        &cancel,
        |_| {},
    )
    .unwrap();
    let legacy = media::inspect(&runtime, &legacy_output, &cancel).unwrap();
    assert!(!legacy.hdr);
    assert_eq!(legacy.color.bit_depth, 8);
    for frame in gray_frames(&runtime, &legacy_output, 1) {
        assert!(frame.windows(2).all(|pair| pair[1] > pair[0]));
        assert!(frame[3] <= 103.0);
    }
    let thumbnail = media::thumbnail(&runtime, &pq.path, &pq.info, &cancel).unwrap();
    let levels = [12, 36, 60, 84].map(|x| thumbnail[(27 * 96 + x) * 3]);
    assert!(levels.windows(2).all(|pair| pair[1] > pair[0]));
    assert!(levels[0] < 10 && levels[3] > 180);
    let destination = folder.join("existing.mp4");
    fs::write(&destination, b"previous output").unwrap();
    let result = media::export(
        &runtime,
        &mixed,
        &destination,
        ExportOptions {
            settings: ExportSettings {
                frame_rate: FrameRate::Fps60,
                ..ExportSettings::default()
            },
            nvidia: support,
        },
        &AtomicBool::new(true),
        |_| {},
    );
    assert!(result.is_err());
    assert_eq!(fs::read(&destination).unwrap(), b"previous output");
    for (path, original) in originals {
        assert_eq!(fs::read(path).unwrap(), original);
    }
    clean_partials(&folder);
    fs::write(folder.join("summary.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "pq_brightness_preserved": true, "hlg_brightness_preserved": true, "mixed_sdr_hdr_brightness_preserved": true, "hdr_color_components_preserved": true,
        "ten_bit_output": true, "original_and_fixed_timestamps": true, "all_size_presets": true, "audio_sync_and_silence": true,
        "compatible_hdr_copy_packets": true, "legacy_sdr_tone_mapping": true, "hdr_thumbnail_tone_mapping": true,
        "source_bytes_unchanged": true, "precanceled_output_preserved": true,
        "nvidia_hevc_main10_available": support.hevc, "nvidia_hdr_quality_and_target": support.hevc.then_some(true),
        "unavailable_nvidia_hdr_quality_and_target_fallback": (!support.hevc).then_some(true)
    })).unwrap()).unwrap();
    println!("Verified HDR and high bit depth in {}", folder.display());
}

fn frame_times(runtime: &Runtime, path: &Path) -> Vec<f64> {
    let raw = run(
        &runtime.ffprobe,
        &[
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_frames",
            "-show_entries",
            "frame=best_effort_timestamp_time",
            "-of",
            "json",
            path.to_str().unwrap(),
        ],
    );
    let json: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    json["frames"]
        .as_array()
        .unwrap()
        .iter()
        .map(|frame| {
            frame["best_effort_timestamp_time"]
                .as_str()
                .unwrap()
                .parse()
                .unwrap()
        })
        .collect()
}

#[test]
#[ignore = "requires the pinned FFmpeg engine; verifies actual frame timestamps"]
fn original_and_fixed_frame_rates_keep_timing_and_audio() {
    let runtime = engine();
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "build/test-output/frame-flow-{}",
        std::process::id()
    ));
    fs::create_dir(&folder).unwrap();
    let paths: Vec<_> = ["24.mp4", "2997.mp4", "60.mp4", "variable.mp4"]
        .into_iter()
        .map(|name| folder.join(name))
        .collect();
    make(&runtime, &paths[0], "red", "320x180", "24", true, "0.5");
    make(
        &runtime,
        &paths[1],
        "blue",
        "320x180",
        "30000/1001",
        false,
        "0.5",
    );
    make(&runtime, &paths[2], "green", "320x180", "60", true, "0.5");
    run(
        &runtime.ffmpeg,
        &[
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=magenta:s=320x180:r=60:d=0.5",
            "-vf",
            "select='not(mod(n,3))+eq(mod(n,5),0)'",
            "-fps_mode",
            "passthrough",
            "-enc_time_base",
            "filter",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-bf",
            "0",
            paths[3].to_str().unwrap(),
        ],
    );
    let sources: Vec<_> = paths.iter().map(|path| source(&runtime, path)).collect();
    let before: Vec<_> = paths.iter().map(|path| fs::read(path).unwrap()).collect();
    let total: f64 = sources.iter().map(|source| source.info.duration).sum();
    let mut expected = Vec::new();
    let mut offset = 0.0;
    for source in &sources {
        let times = frame_times(&runtime, &source.path);
        for time in &times {
            expected.push(offset + time - times[0]);
        }
        offset += source.info.duration;
    }
    let nvidia = media::probe_nvidia(&runtime, &AtomicBool::new(false));
    for codec in [VideoCodec::H264, VideoCodec::H265] {
        let mut modes = vec![(NvidiaSupport::default(), EncodeEngine::Cpu, "cpu")];
        if nvidia.supports(codec) {
            modes.push((nvidia, EncodeEngine::Nvidia, "nvidia"));
        } else {
            modes.push((
                NvidiaSupport {
                    h264: true,
                    hevc: true,
                },
                EncodeEngine::CpuFallback,
                "fallback",
            ));
        }
        for (support, expected_engine, name) in modes {
            for frame_rate in [
                FrameRate::Original,
                FrameRate::Fps23976,
                FrameRate::Fps24,
                FrameRate::Fps25,
                FrameRate::Fps2997,
                FrameRate::Fps30,
                FrameRate::Fps50,
                FrameRate::Fps5994,
                FrameRate::Fps60,
            ] {
                let output = folder.join(format!("{codec:?}-{name}-{frame_rate:?}.mp4"));
                let options = ExportOptions {
                    settings: ExportSettings {
                        preset: ExportPreset::Medium,
                        codec,
                        frame_rate,
                    },
                    nvidia: support,
                };
                let mut final_engine = None;
                media::export(
                    &runtime,
                    &sources,
                    &output,
                    options,
                    &AtomicBool::new(false),
                    |progress| final_engine = progress.engine,
                )
                .unwrap();
                assert_eq!(
                    final_engine,
                    Some(if frame_rate == FrameRate::Original {
                        EncodeEngine::Cpu
                    } else {
                        expected_engine
                    })
                );
                let times = frame_times(&runtime, &output);
                if let Some(fps) = frame_rate.fps() {
                    assert!(
                        (times.len() as f64 - total * fps).abs() <= 1.1,
                        "Frame count for {frame_rate:?}"
                    );
                    for pair in times.windows(2) {
                        assert!(
                            (pair[1] - pair[0] - 1.0 / fps).abs() < 0.00001,
                            "Fixed timing for {frame_rate:?}: {pair:?}"
                        );
                    }
                } else {
                    assert_eq!(
                        times.len(),
                        expected.len(),
                        "Original frames were removed or repeated"
                    );
                    for (index, (time, wanted)) in times.iter().zip(&expected).enumerate() {
                        assert!(
                            (time - wanted).abs() < 0.00002,
                            "Original timestamp {index}: {time} expected {wanted}"
                        );
                    }
                }
                let audio = run(
                    &runtime.ffmpeg,
                    &[
                        "-v",
                        "error",
                        "-i",
                        output.to_str().unwrap(),
                        "-map",
                        "0:a:0",
                        "-ac",
                        "1",
                        "-ar",
                        "48000",
                        "-f",
                        "s16le",
                        "pipe:1",
                    ],
                );
                assert!((audio.len() as f64 / 96000.0 - total).abs() < 0.04);
                let samples: Vec<_> = audio
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|bytes| i16::from_le_bytes(*bytes))
                    .collect();
                assert!(samples[4800..19200].iter().any(|sample| sample.abs() > 200));
                assert!(samples[33600..43200].iter().all(|sample| sample.abs() < 30));
                assert!(
                    samples[52800..67200]
                        .iter()
                        .any(|sample| sample.abs() > 200)
                );
                assert!(samples[81600..91200].iter().all(|sample| sample.abs() < 30));
                clean_partials(&folder);
            }
        }
    }
    // The default source-mode conversion also preserves timing, including on a reordered list.
    let mut reversed = sources.clone();
    reversed.reverse();
    let output = folder.join("source-mode-reversed.mp4");
    media::export(
        &runtime,
        &reversed,
        &output,
        ExportOptions::default(),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    let times = frame_times(&runtime, &output);
    let mut index = 0;
    let mut offset = 0.0;
    for source in &reversed {
        let original = frame_times(&runtime, &source.path);
        for time in &original {
            assert!((times[index] - offset - time + original[0]).abs() < 0.00002);
            index += 1;
        }
        offset += source.info.duration;
    }
    assert_eq!(index, times.len());
    let silent_sources = [sources[3].clone(), sources[1].clone()];
    for codec in [VideoCodec::H264, VideoCodec::H265] {
        let output = folder.join(format!("silent-{codec:?}.mp4"));
        media::export(
            &runtime,
            &silent_sources,
            &output,
            ExportOptions {
                settings: ExportSettings {
                    preset: ExportPreset::Medium,
                    codec,
                    ..ExportSettings::default()
                },
                ..ExportOptions::default()
            },
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        assert!(
            media::inspect(&runtime, &output, &AtomicBool::new(false))
                .unwrap()
                .audio_index
                .is_none()
        );
        let times = frame_times(&runtime, &output);
        let mut index = 0;
        let mut offset = 0.0;
        for source in &silent_sources {
            let original = frame_times(&runtime, &source.path);
            for time in &original {
                assert!((times[index] - offset - time + original[0]).abs() < 0.00002);
                index += 1;
            }
            offset += source.info.duration;
        }
        assert_eq!(index, times.len());
    }
    for (path, bytes) in paths.iter().zip(before) {
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
    let short = folder.join("one-frame.mp4");
    make(&runtime, &short, "red", "320x180", "240", false, "0.003");
    assert_eq!(frame_times(&runtime, &short).len(), 1);
    let short_source = source(&runtime, &short);
    let short_before = fs::read(&short).unwrap();
    for codec in [VideoCodec::H264, VideoCodec::H265] {
        for frame_rate in [FrameRate::Fps24, FrameRate::Fps60] {
            let output = folder.join(format!("one-frame-{codec:?}-{frame_rate:?}.mp4"));
            let cancel = Arc::new(AtomicBool::new(false));
            let watchdog_cancel = cancel.clone();
            let (done, completion) = mpsc::channel();
            let watchdog = std::thread::spawn(move || {
                if completion
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .is_err()
                {
                    watchdog_cancel.store(true, Ordering::Relaxed);
                }
            });
            let result = media::export(
                &runtime,
                std::slice::from_ref(&short_source),
                &output,
                ExportOptions {
                    settings: ExportSettings {
                        preset: ExportPreset::Medium,
                        codec,
                        frame_rate,
                    },
                    ..ExportOptions::default()
                },
                &cancel,
                |_| {},
            );
            done.send(()).unwrap();
            watchdog.join().unwrap();
            result.unwrap();
            assert_eq!(frame_times(&runtime, &output).len(), 1);
            assert!(
                colors(&runtime, &output)
                    .iter()
                    .all(|pixel| pixel[0] > 200 && pixel[2] < 40)
            );
        }
    }
    assert_eq!(fs::read(short).unwrap(), short_before);
    clean_partials(&folder);
    fs::write(
        folder.join("summary.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "original_timestamps_and_frame_count": true,
            "reordered_source_conversion": true,
            "all_fixed_rates": true,
            "fractional_rates_exact": true,
            "audio_sync_and_silence": true,
            "source_bytes_unchanged": true,
            "cpu_h264_original_timing": true,
            "cpu_hevc_original_timing": true,
            "silent_video_timing": true,
            "single_frame_fixed_output": true,
            "nvidia_h264_available": nvidia.h264,
            "nvidia_hevc_available": nvidia.hevc,
            "nvidia_h264_fixed_rates": nvidia.h264.then_some(true),
            "nvidia_hevc_fixed_rates": nvidia.hevc.then_some(true),
            "unavailable_nvidia_h264_fallback": (!nvidia.h264).then_some(true),
            "unavailable_nvidia_hevc_fallback": (!nvidia.hevc).then_some(true)
        }))
        .unwrap(),
    )
    .unwrap();
    println!("Frame timing acceptance: {}", folder.display());
}

fn check_forced_export(
    runtime: &Runtime,
    sources: &[Source],
    output: &Path,
    options: ExportOptions,
    expected_engine: EncodeEngine,
) {
    let cancel = AtomicBool::new(false);
    let mut engines = Vec::new();
    assert_eq!(
        media::export(runtime, sources, output, options, &cancel, |progress| {
            assert_eq!(progress.mode, ExportMode::Convert);
            if let Some(engine) = progress.engine {
                engines.push(engine);
            }
        })
        .unwrap(),
        ExportMode::Convert
    );
    engines.dedup();
    let expected_engines = if expected_engine == EncodeEngine::CpuFallback {
        vec![EncodeEngine::Nvidia, EncodeEngine::CpuFallback]
    } else {
        vec![expected_engine]
    };
    assert_eq!(
        engines, expected_engines,
        "Wrong engine for {:?}: {engines:?}",
        options.settings
    );
    let info = media::inspect(runtime, output, &cancel).unwrap();
    let codec = match options.settings.codec {
        VideoCodec::H264 => "h264",
        VideoCodec::H265 => "hevc",
    };
    assert_eq!(info.codec, codec);
    assert_eq!((info.width, info.height), (320, 180));
    let expected_fps = options.settings.frame_rate.fps().unwrap_or(16.0);
    assert!((info.fps - expected_fps).abs() < 0.1);
    assert!((info.duration - 1.5).abs() < 0.15);
    assert!(info.audio_index.is_some());
    assert_eq!((info.audio_rate, info.audio_channels), (48000, 2));
    let frames = colors(runtime, output);
    let expected_frames = if options.settings.frame_rate == FrameRate::Original {
        24
    } else {
        (1.5 * expected_fps).round() as usize
    };
    assert_eq!(frames.len(), expected_frames);
    for (index, pixel) in frames.iter().enumerate() {
        assert!(
            if if options.settings.frame_rate == FrameRate::Original {
                (6..18).contains(&index)
            } else {
                ((expected_fps * 0.5).round() as usize..expected_fps.round() as usize)
                    .contains(&index)
            } {
                pixel[2] > 200 && pixel[0] < 40
            } else {
                pixel[0] > 200 && pixel[2] < 40
            },
            "Wrong preset frame at {index}: {pixel:?}"
        );
    }
    let audio = run(
        &runtime.ffmpeg,
        &[
            "-v",
            "error",
            "-i",
            output.to_str().unwrap(),
            "-map",
            "0:a:0",
            "-ac",
            "1",
            "-ar",
            "48000",
            "-f",
            "s16le",
            "pipe:1",
        ],
    );
    assert!((audio.len() as f64 / 96000.0 - 1.5).abs() < 0.08);
    let samples: Vec<i16> = audio
        .as_chunks::<2>()
        .0
        .iter()
        .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]))
        .collect();
    assert!(samples[4800..19200].iter().any(|sample| sample.abs() > 200));
    assert!(
        samples[33600..43200].iter().all(|sample| sample.abs() < 30),
        "Preset output has sound in the silent clip"
    );
    assert!(
        samples[52800..67200]
            .iter()
            .any(|sample| sample.abs() > 200)
    );
    clean_partials(output.parent().unwrap());
}

#[test]
#[ignore = "requires the pinned FFmpeg engine; generates actual videos"]
fn actual_copy_conversion_cancellation_and_failure_boundaries() {
    let runtime = engine();
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "build/test-output/media-flow-{}",
        std::process::id()
    ));
    fs::create_dir(&folder).unwrap();
    let red = folder.join("α 'intro one'.mp4");
    let blue = folder.join("β second clip.mp4");
    let silent = folder.join("silent different size.mp4");
    make(&runtime, &red, "red", "160x90", "12", true, "0.5");
    make(&runtime, &blue, "blue", "160x90", "12", true, "0.5");
    make(&runtime, &silent, "blue", "320x180", "24", false, "0.5");
    let original_sources: Vec<_> = [&red, &blue, &silent]
        .into_iter()
        .map(|path| (path, fs::read(path).unwrap()))
        .collect();
    let cancel = AtomicBool::new(false);
    let sources = vec![
        source(&runtime, &blue),
        source(&runtime, &red),
        source(&runtime, &blue),
    ];
    assert!(media::can_copy(&sources));
    let output = folder.join("copied.mp4");
    let mode = media::export(
        &runtime,
        &sources,
        &output,
        ExportOptions::default(),
        &cancel,
        |_| {},
    )
    .unwrap();
    assert_eq!(mode, ExportMode::Copy);
    let expected: Vec<_> = sources
        .iter()
        .flat_map(|s| packet_hashes(&runtime, &s.path))
        .collect();
    assert_eq!(
        packet_hashes(&runtime, &output),
        expected,
        "Compressed video packets changed or clips are out of order"
    );
    let frames = colors(&runtime, &output);
    assert_eq!(frames.len(), 18);
    for (i, pixel) in frames.iter().enumerate() {
        let blue = !(6..12).contains(&i);
        assert!(
            if blue {
                pixel[2] > 200 && pixel[0] < 40
            } else {
                pixel[0] > 200 && pixel[2] < 40
            },
            "Wrong frame color at {i}: {pixel:?}"
        );
    }
    let audio = run(
        &runtime.ffmpeg,
        &[
            "-v",
            "error",
            "-i",
            output.to_str().unwrap(),
            "-map",
            "0:a:0",
            "-ac",
            "1",
            "-ar",
            "48000",
            "-f",
            "s16le",
            "pipe:1",
        ],
    );
    assert!(
        (audio.len() as f64 / 96000.0 - 1.5).abs() < 0.08,
        "Copied audio length is wrong"
    );
    let mixed = vec![source(&runtime, &red), source(&runtime, &silent)];
    let converted = folder.join("converted.mp4");
    assert_eq!(
        media::export(
            &runtime,
            &mixed,
            &converted,
            ExportOptions::default(),
            &cancel,
            |_| {},
        )
        .unwrap(),
        ExportMode::Convert
    );
    let info = media::inspect(&runtime, &converted, &cancel).unwrap();
    assert_eq!((info.width, info.height), (160, 90));
    assert!(info.audio_index.is_some());
    let frames = colors(&runtime, &converted);
    assert_eq!(frames.len(), 18);
    for (i, pixel) in frames.iter().enumerate() {
        assert!(
            if i < 6 {
                pixel[0] > 200 && pixel[2] < 40
            } else {
                pixel[2] > 200 && pixel[0] < 40
            },
            "Wrong converted frame at {i}"
        );
    }
    let audio = run(
        &runtime.ffmpeg,
        &[
            "-v",
            "error",
            "-i",
            converted.to_str().unwrap(),
            "-map",
            "0:a:0",
            "-ac",
            "1",
            "-ar",
            "48000",
            "-f",
            "s16le",
            "pipe:1",
        ],
    );
    let samples: Vec<i16> = audio
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| i16::from_le_bytes([p[0], p[1]]))
        .collect();
    assert!(samples[4800..19200].iter().any(|s| s.abs() > 200));
    assert!(
        samples[33600..43200].iter().all(|s| s.abs() < 30),
        "Silent clip contains audio from the previous clip"
    );
    // Exercise the advertised HEVC copy path with reordered frames and 10-bit video.
    let mut hevc_sources = Vec::new();
    for (name, color) in [("hevc red.mp4", "red"), ("hevc blue.mp4", "blue")] {
        let path = folder.join(name);
        let input = format!("color=c={color}:s=160x90:r=12:d=0.5");
        run(
            &runtime.ffmpeg,
            &[
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                &input,
                "-c:v",
                "libx265",
                "-preset",
                "veryfast",
                "-x265-params",
                "pools=1:frame-threads=1:log-level=error",
                "-pix_fmt",
                "yuv420p10le",
                "-tag:v",
                "hvc1",
                path.to_str().unwrap(),
            ],
        );
        hevc_sources.push(source(&runtime, &path));
    }
    let original_hevc: Vec<_> = hevc_sources
        .iter()
        .map(|source| (source.path.clone(), fs::read(&source.path).unwrap()))
        .collect();
    assert!(media::can_copy(&hevc_sources));
    let hevc_output = folder.join("hevc-copied.mp4");
    assert_eq!(
        media::export(
            &runtime,
            &hevc_sources,
            &hevc_output,
            ExportOptions::default(),
            &cancel,
            |_| {},
        )
        .unwrap(),
        ExportMode::Copy
    );
    assert_eq!(
        packet_hashes(&runtime, &hevc_output),
        hevc_sources
            .iter()
            .flat_map(|s| packet_hashes(&runtime, &s.path))
            .collect::<Vec<_>>()
    );
    let frames = colors(&runtime, &hevc_output);
    assert_eq!(frames.len(), 12);
    for (i, pixel) in frames.iter().enumerate() {
        assert!(
            if i < 6 {
                pixel[0] > 200 && pixel[2] < 40
            } else {
                pixel[2] > 200 && pixel[0] < 40
            },
            "Wrong HEVC frame at {i}: {pixel:?}"
        );
    }
    let anamorphic = folder.join("anamorphic.mp4");
    run(
        &runtime.ffmpeg,
        &[
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=red:s=160x120:r=12:d=0.5",
            "-vf",
            "setsar=2/1",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-pix_fmt",
            "yuv420p",
            anamorphic.to_str().unwrap(),
        ],
    );
    let rotated = folder.join("rotated anamorphic.mp4");
    run(
        &runtime.ffmpeg,
        &[
            "-v",
            "error",
            "-y",
            "-display_rotation",
            "90",
            "-i",
            anamorphic.to_str().unwrap(),
            "-c",
            "copy",
            rotated.to_str().unwrap(),
        ],
    );
    let original_rotated = fs::read(&rotated).unwrap();
    let rotated_source = source(&runtime, &rotated);
    assert_eq!(
        (rotated_source.info.width, rotated_source.info.height),
        (120, 320)
    );
    let thumbnail = media::thumbnail(&runtime, &rotated, &rotated_source.info, &cancel).unwrap();
    let middle_row = &thumbnail
        .as_chunks::<{ (media::THUMB_WIDTH * 3) as usize }>()
        .0[(media::THUMB_HEIGHT / 2) as usize];
    let red_width = middle_row
        .as_chunks::<3>()
        .0
        .iter()
        .filter(|pixel| pixel[2] > 200 && pixel[0] < 40 && pixel[1] < 40)
        .count();
    assert!(
        (20..=22).contains(&red_width),
        "Rotated anamorphic thumbnail has the wrong picture width: {red_width}"
    );
    let anamorphic_output = folder.join("anamorphic converted.mp4");
    assert_eq!(
        media::export(
            &runtime,
            &[rotated_source, source(&runtime, &silent)],
            &anamorphic_output,
            ExportOptions::default(),
            &cancel,
            |_| {}
        )
        .unwrap(),
        ExportMode::Convert
    );
    let info = media::inspect(&runtime, &anamorphic_output, &cancel).unwrap();
    assert_eq!((info.width, info.height), (120, 320));
    let frames = colors(&runtime, &anamorphic_output);
    assert_eq!(frames.len(), 18);
    assert!(
        frames[..6].iter().all(|pixel| pixel[0] > 200),
        "Rotated anamorphic conversion adds padding inside the first clip: {:?}",
        &frames[..6]
    );
    assert_eq!(fs::read(&rotated).unwrap(), original_rotated);
    // Existing destinations and sources must survive every failed path.
    let destination = folder.join("existing.mp4");
    let sentinel = b"existing output must survive";
    fs::write(&destination, sentinel).unwrap();
    let canceled = AtomicBool::new(true);
    assert!(
        media::export(
            &runtime,
            &sources,
            &destination,
            ExportOptions::default(),
            &canceled,
            |_| {},
        )
        .is_err()
    );
    assert_eq!(fs::read(&destination).unwrap(), sentinel);
    let alias = folder.join("source alias.mp4");
    fs::hard_link(&red, &alias).unwrap();
    let original = &original_sources[0].1;
    assert!(
        media::export(
            &runtime,
            &sources,
            &alias,
            ExportOptions::default(),
            &cancel,
            |_| {},
        )
        .is_err()
    );
    assert_eq!(&fs::read(&red).unwrap(), original);
    let lock = fs::File::options()
        .read(true)
        .share_mode(1)
        .open(&destination)
        .unwrap();
    assert!(
        media::export(
            &runtime,
            &sources,
            &destination,
            ExportOptions::default(),
            &cancel,
            |_| {},
        )
        .is_err()
    );
    drop(lock);
    assert_eq!(fs::read(&destination).unwrap(), sentinel);
    clean_partials(&folder);
    for (path, original) in &original_sources {
        assert_eq!(&fs::read(path).unwrap(), original, "{}", path.display());
    }
    let preset_red = folder.join("preset red.mp4");
    let preset_blue = folder.join("preset silent blue.mp4");
    make(&runtime, &preset_red, "red", "320x180", "12", true, "0.5");
    make(
        &runtime,
        &preset_blue,
        "blue",
        "640x360",
        "24",
        false,
        "0.5",
    );
    let original_presets: Vec<_> = [&preset_red, &preset_blue]
        .into_iter()
        .map(|path| (path, fs::read(path).unwrap()))
        .collect();
    let preset_sources = vec![
        source(&runtime, &preset_red),
        source(&runtime, &preset_blue),
        source(&runtime, &preset_red),
    ];
    let infos: Vec<_> = preset_sources.iter().map(|source| &source.info).collect();
    let nvidia = media::probe_nvidia(&runtime, &cancel);
    for (codec, name, supported) in [
        (VideoCodec::H264, "h264", nvidia.h264),
        (VideoCodec::H265, "hevc", nvidia.hevc),
    ] {
        let mut previous_estimate = 0.0;
        for (preset, label) in [
            (ExportPreset::Small, "small"),
            (ExportPreset::Medium, "medium"),
            (ExportPreset::Large, "large"),
        ] {
            let settings = ExportSettings {
                preset,
                codec,
                ..ExportSettings::default()
            };
            let plan = media::plan(&infos, settings).unwrap();
            assert_eq!(plan.mode, ExportMode::Convert);
            assert_eq!(plan.codec, codec);
            assert!(plan.video_bitrate.is_some_and(|bitrate| bitrate > 0));
            let estimate = plan.estimated_bytes.unwrap();
            assert!(estimate.is_finite() && estimate > previous_estimate);
            previous_estimate = estimate;
            check_forced_export(
                &runtime,
                &preset_sources,
                &folder.join(format!("cpu-{name}-{label}.mp4")),
                ExportOptions {
                    settings,
                    nvidia: NvidiaSupport::default(),
                },
                EncodeEngine::Cpu,
            );
            if supported {
                check_forced_export(
                    &runtime,
                    &preset_sources,
                    &folder.join(format!("nvidia-{name}-{label}.mp4")),
                    ExportOptions {
                        settings: ExportSettings {
                            frame_rate: FrameRate::Fps30,
                            ..settings
                        },
                        nvidia,
                    },
                    EncodeEngine::Nvidia,
                );
            }
            let ten_bit_output = folder.join(format!("ten-bit-{name}-{label}.mp4"));
            assert_eq!(
                media::export(
                    &runtime,
                    &hevc_sources,
                    &ten_bit_output,
                    ExportOptions { settings, nvidia },
                    &cancel,
                    |_| {},
                )
                .unwrap(),
                ExportMode::Convert
            );
            let ten_bit_info = media::inspect(&runtime, &ten_bit_output, &cancel).unwrap();
            assert_eq!(
                ten_bit_info.color.bit_depth,
                if codec == VideoCodec::H265 { 10 } else { 8 }
            );
            assert!(!ten_bit_info.hdr);
            let frames = colors(&runtime, &ten_bit_output);
            assert_eq!(frames.len(), 12);
            for (index, pixel) in frames.iter().enumerate() {
                assert!(
                    if index < 6 {
                        pixel[0] > 200 && pixel[2] < 40
                    } else {
                        pixel[2] > 200 && pixel[0] < 40
                    },
                    "Wrong converted 10-bit frame at {index}: {pixel:?}"
                );
            }
            // A canceled export must keep the previous output.
            assert_eq!(fs::read(&destination).unwrap(), sentinel);
            let precanceled = AtomicBool::new(true);
            assert!(
                media::export(
                    &runtime,
                    &preset_sources,
                    &destination,
                    ExportOptions { settings, nvidia },
                    &precanceled,
                    |_| {},
                )
                .is_err()
            );
            assert_eq!(fs::read(&destination).unwrap(), sentinel);
            clean_partials(&folder);
        }
        if !supported {
            // Simulate support cached before the NVIDIA device became unavailable.
            check_forced_export(
                &runtime,
                &preset_sources,
                &folder.join(format!("fallback-{name}.mp4")),
                ExportOptions {
                    settings: ExportSettings {
                        preset: ExportPreset::Medium,
                        codec,
                        frame_rate: FrameRate::Fps30,
                    },
                    nvidia: NvidiaSupport {
                        h264: true,
                        hevc: true,
                    },
                },
                EncodeEngine::CpuFallback,
            );
        }
    }
    for (path, original) in &original_presets {
        assert_eq!(&fs::read(path).unwrap(), original, "{}", path.display());
    }
    for (path, original) in &original_hevc {
        assert_eq!(&fs::read(path).unwrap(), original, "{}", path.display());
    }
    let stale = vec![source(&runtime, &silent)];
    make(&runtime, &silent, "green", "640x360", "30", false, "1");
    assert!(
        media::export(
            &runtime,
            &stale,
            &destination,
            ExportOptions::default(),
            &cancel,
            |_| {},
        )
        .is_err()
    );
    assert_eq!(fs::read(&destination).unwrap(), sentinel);
    let long = folder.join("long.mp4");
    run(
        &runtime.ffmpeg,
        &[
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=s=1280x720:r=30:d=8",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-pix_fmt",
            "yuv420p",
            long.to_str().unwrap(),
        ],
    );
    let active = vec![source(&runtime, &long), source(&runtime, &silent)];
    let canceled = AtomicBool::new(false);
    let mut saw_progress = false;
    let result = media::export(
        &runtime,
        &active,
        &destination,
        ExportOptions::default(),
        &canceled,
        |p| {
            if p.percent > 0.0 && p.percent < 96.0 {
                saw_progress = true;
                canceled.store(true, Ordering::Relaxed);
            }
        },
    );
    assert!(saw_progress, "Did not observe an active export");
    assert!(result.is_err());
    assert_eq!(fs::read(&destination).unwrap(), sentinel);
    clean_partials(&folder);
    fs::write(
        folder.join("summary.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "copy_packet_order": true,
            "decoded_frame_order": true,
            "mixed_size_and_rate": true,
            "silent_audio": true,
            "active_cancellation": true,
            "existing_output_preserved": true,
            "source_alias_rejected": true,
            "changed_source_rejected": true,
            "source_bytes_unchanged": true,
            "rotated_anamorphic_geometry": true,
            "cpu_h264_presets": true,
            "cpu_hevc_presets": true,
            "preset_estimates_increase": true,
            "cpu_ten_bit_conversion": true,
            "forced_precanceled_output_preserved": true,
            "nvidia_h264_presets": nvidia.h264.then_some(true),
            "nvidia_hevc_presets": nvidia.hevc.then_some(true),
            "nvidia_h264_available": nvidia.h264,
            "nvidia_hevc_available": nvidia.hevc,
            "unavailable_nvidia_h264_fallback": (!nvidia.h264).then_some(true),
            "unavailable_nvidia_hevc_fallback": (!nvidia.hevc).then_some(true),
        }))
        .unwrap(),
    )
    .unwrap();
    println!("Verified actual videos in {}", folder.display());
}
