use std::{
    fs,
    os::windows::{fs::OpenOptionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
};
use stitch::media::{self, AudioMode, CopyIssue, ExportMode, ExportOptions, Runtime, Source};

fn run(exe: &Path, args: &[&str]) -> Vec<u8> {
    let result = Command::new(exe)
        .args(args)
        .creation_flags(0x08000000)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    result.stdout
}

fn text(path: &Path) -> &str {
    path.to_str().unwrap()
}

fn source(runtime: &Runtime, path: &Path) -> Source {
    Source {
        path: path.into(),
        info: media::inspect(runtime, path, &AtomicBool::new(false)).unwrap(),
    }
}

fn pcm(runtime: &Runtime, path: &Path) -> Vec<u8> {
    run(
        &runtime.ffmpeg,
        &[
            "-v",
            "error",
            "-nostdin",
            "-i",
            text(path),
            "-map",
            "0:a:0",
            "-ac",
            "2",
            "-ar",
            "48000",
            "-f",
            "f32le",
            "-",
        ],
    )
}

fn hashes(runtime: &Runtime, path: &Path, track: &str) -> Vec<String> {
    let data = run(
        &runtime.ffprobe,
        &[
            "-v",
            "error",
            "-select_streams",
            track,
            "-show_packets",
            "-show_data_hash",
            "sha256",
            "-show_entries",
            "packet=data_hash",
            "-of",
            "json",
            "-i",
            text(path),
        ],
    );
    let root: serde_json::Value = serde_json::from_slice(&data).unwrap();
    root["packets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|packet| packet["data_hash"].as_str().unwrap().into())
        .collect()
}

fn make(runtime: &Runtime, path: &Path, duration: &str, frequency: &str) {
    run(
        &runtime.ffmpeg,
        &[
            "-v",
            "error",
            "-nostdin",
            "-f",
            "lavfi",
            "-i",
            &format!("color=c=blue:s=160x90:r=25:d={duration}"),
            "-f",
            "lavfi",
            "-i",
            &format!("sine=frequency={frequency}:sample_rate=48000:duration={duration}"),
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-bf",
            "0",
            "-g",
            "40",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-ac",
            "2",
            "-b:a",
            "192k",
            text(path),
        ],
    );
}

fn export(runtime: &Runtime, sources: &[Source], output: &Path) -> Vec<media::Progress> {
    let mut events = Vec::new();
    media::export(
        runtime,
        sources,
        output,
        ExportOptions::default(),
        &AtomicBool::new(false),
        |progress| events.push(progress),
    )
    .unwrap();
    events
}

#[test]
#[ignore = "requires the prepared FFmpeg engine; verifies copied audio and decoded sample timing"]
fn copied_tracks_keep_packets_samples_and_automatic_audio_recovery() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let runtime = Runtime::beside(&root.join("build/Stitch.exe")).unwrap();
    let folder = root.join(format!(
        "build/test-output/copy-flow-{}",
        std::process::id()
    ));
    fs::create_dir_all(folder.parent().unwrap()).unwrap();
    fs::create_dir(&folder).unwrap();
    let continuous = folder.join("continuous.mp4");
    make(&runtime, &continuous, "3.2", "440");
    let segments = folder.join("segment-%02d.mp4");
    run(
        &runtime.ffmpeg,
        &[
            "-v",
            "error",
            "-nostdin",
            "-copyts",
            "-i",
            text(&continuous),
            "-map",
            "0",
            "-c",
            "copy",
            "-avoid_negative_ts",
            "disabled",
            "-f",
            "segment",
            "-segment_time",
            "1.6",
            "-segment_format_options",
            "video_track_timescale=48000:avoid_negative_ts=disabled",
            "-reset_timestamps",
            "1",
            text(&segments),
        ],
    );
    let a = folder.join("segment-00.mp4");
    let b = folder.join("segment-01.mp4");
    let sources = vec![source(&runtime, &a), source(&runtime, &b)];
    let original_bytes: Vec<_> = sources.iter().map(|s| fs::read(&s.path).unwrap()).collect();
    for item in &sources {
        let _lock = fs::File::options()
            .read(true)
            .share_mode(1)
            .open(&item.path)
            .unwrap();
        let again = media::inspect(&runtime, &item.path, &AtomicBool::new(false)).unwrap();
        assert_eq!(
            again, item.info,
            "Inspection must be stable under a source lock"
        );
    }
    let plan = media::plan(
        &sources.iter().map(|s| &s.info).collect::<Vec<_>>(),
        Default::default(),
    )
    .unwrap();
    assert_eq!(plan.mode, ExportMode::Copy);
    assert_eq!(plan.audio_mode, AudioMode::Copy, "{plan:?}");
    let copied = folder.join("copied.mp4");
    let events = export(&runtime, &sources, &copied);
    assert!(
        events
            .iter()
            .all(|event| event.mode == ExportMode::Copy && event.audio_mode == AudioMode::Copy)
    );
    for track in ["v:0", "a:0"] {
        assert!(
            hashes(&runtime, &copied, track) == hashes(&runtime, &continuous, track),
            "Copied {track} payload changed"
        );
    }
    assert!(
        pcm(&runtime, &copied) == pcm(&runtime, &continuous),
        "Decoded copied audio changed"
    );

    let resized = folder.join("resized.mp4");
    run(
        &runtime.ffmpeg,
        &[
            "-v",
            "error",
            "-nostdin",
            "-i",
            text(&b),
            "-map",
            "0",
            "-vf",
            "scale=320:180",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-bf",
            "0",
            "-c:a",
            "copy",
            text(&resized),
        ],
    );
    let changed_video = vec![sources[0].clone(), source(&runtime, &resized)];
    let converted = folder.join("converted-video.mp4");
    let events = export(&runtime, &changed_video, &converted);
    assert!(
        events
            .iter()
            .all(|event| event.mode == ExportMode::Convert && event.audio_mode == AudioMode::Copy)
    );
    assert_eq!(
        hashes(&runtime, &converted, "a:0"),
        hashes(&runtime, &continuous, "a:0")
    );
    assert_eq!(pcm(&runtime, &converted), pcm(&runtime, &continuous));
    assert_eq!(source(&runtime, &converted).info.codec, "hevc");

    let silent = folder.join("silent.mp4");
    run(
        &runtime.ffmpeg,
        &[
            "-v",
            "error",
            "-nostdin",
            "-i",
            text(&b),
            "-map",
            "0:v:0",
            "-c",
            "copy",
            text(&silent),
        ],
    );
    let mixed = vec![sources[0].clone(), source(&runtime, &silent)];
    let filled = folder.join("audio-and-silence.mp4");
    let events = export(&runtime, &mixed, &filled);
    assert!(
        events
            .iter()
            .all(|event| event.mode == ExportMode::Copy && event.audio_mode == AudioMode::Encode)
    );
    assert_eq!(
        hashes(&runtime, &filled, "v:0"),
        hashes(&runtime, &continuous, "v:0")
    );
    let samples = pcm(&runtime, &filled);
    assert_eq!(samples.len() / 8, 153600);
    assert!(
        samples[48000 * 8 * 2..]
            .as_chunks::<4>()
            .0
            .iter()
            .all(|sample| f32::from_le_bytes(*sample).abs() < 0.0001)
    );

    let independent_a = folder.join("independent-a.mp4");
    let independent_b = folder.join("independent-b.mp4");
    make(&runtime, &independent_a, "1.6", "440");
    make(&runtime, &independent_b, "1.6", "880");
    let independent = vec![
        source(&runtime, &independent_a),
        source(&runtime, &independent_b),
    ];
    let plan = media::plan(
        &independent.iter().map(|s| &s.info).collect::<Vec<_>>(),
        Default::default(),
    )
    .unwrap();
    assert_eq!(plan.mode, ExportMode::Copy);
    assert_eq!(plan.audio_reason.unwrap().issue, CopyIssue::AudioPadding);
    let joined = folder.join("independent-joined.mp4");
    let events = export(&runtime, &independent, &joined);
    assert!(
        events
            .iter()
            .all(|event| event.audio_mode == AudioMode::Encode)
    );
    assert_eq!(pcm(&runtime, &joined).len() / 8, 153600);

    let single = folder.join("single.mp4");
    let events = export(&runtime, &independent[..1], &single);
    assert!(
        events
            .iter()
            .all(|event| event.audio_mode == AudioMode::Copy)
    );
    assert_eq!(pcm(&runtime, &single), pcm(&runtime, &independent_a));
    assert_eq!(
        hashes(&runtime, &single, "a:0"),
        hashes(&runtime, &independent_a, "a:0")
    );
    let mkv = folder.join("single.mkv");
    run(
        &runtime.ffmpeg,
        &[
            "-v",
            "error",
            "-nostdin",
            "-i",
            text(&independent_a),
            "-map",
            "0",
            "-c",
            "copy",
            text(&mkv),
        ],
    );
    let remuxed = folder.join("remuxed-mkv.mp4");
    let events = export(&runtime, &[source(&runtime, &mkv)], &remuxed);
    assert!(
        events
            .iter()
            .all(|event| event.audio_mode == AudioMode::Copy),
        "{events:?}"
    );
    assert_eq!(pcm(&runtime, &remuxed).len() / 8, 76800);
    assert_eq!(
        hashes(&runtime, &remuxed, "a:0"),
        hashes(&runtime, &mkv, "a:0")
    );

    let high_rate_audio = folder.join("pcm-192khz.mkv");
    run(
        &runtime.ffmpeg,
        &[
            "-v",
            "error",
            "-nostdin",
            "-i",
            text(&independent_a),
            "-map",
            "0",
            "-c:v",
            "copy",
            "-c:a",
            "pcm_f32le",
            "-ar",
            "192000",
            text(&high_rate_audio),
        ],
    );
    let high_rate_source = source(&runtime, &high_rate_audio);
    let normalized = folder.join("normalized-audio.mp4");
    let events = export(&runtime, &[high_rate_source], &normalized);
    assert!(
        events
            .iter()
            .all(|event| event.mode == ExportMode::Copy && event.audio_mode == AudioMode::Encode)
    );
    let info = source(&runtime, &normalized).info;
    assert_eq!(info.audio_rate, 48000);
    assert_eq!(info.audio_channels, 2);
    assert_eq!(
        hashes(&runtime, &normalized, "v:0"),
        hashes(&runtime, &high_rate_audio, "v:0")
    );

    // Fault injection changes only output-probe results, using generated filenames.
    // Source inspection and FFmpeg remain the actual prepared engine.
    let probe_script = folder.join("output-probe.ps1");
    fs::write(&probe_script, format!(r#"$ErrorActionPreference = 'Stop'
$probeArguments = @($args)
$data = & '{}' @probeArguments
if ($LASTEXITCODE -ne 0) {{ exit $LASTEXITCODE }}
$outputPath = $probeArguments[-1]
$trackIndex = [Array]::IndexOf($probeArguments, '-select_streams')
if ((Split-Path -Leaf $outputPath) -eq 'output.mp4' -and $probeArguments -contains '-show_packets' -and $trackIndex -ge 0 -and $probeArguments[$trackIndex + 1] -eq '1' -and -not (Test-Path -LiteralPath (Join-Path (Split-Path -Parent $outputPath) 'audio.m4a'))) {{
    $root = ($data -join "`n") | ConvertFrom-Json
    foreach ($packet in $root.packets) {{ $packet.pts_time = ([double]$packet.pts_time + 0.05).ToString('F9', [Globalization.CultureInfo]::InvariantCulture) }}
    $root | ConvertTo-Json -Depth 12
}} else {{ $data }}
"#, text(&runtime.ffprobe).replace('\'', "''"))).unwrap();
    let probe_wrapper = folder.join("output-probe.cmd");
    fs::write(&probe_wrapper, format!("@echo off\r\n\"{}\\System32\\WindowsPowerShell\\v1.0\\powershell.exe\" -NoLogo -NoProfile -ExecutionPolicy Bypass -File \"{}\" %*\r\n",
        std::env::var("WINDIR").unwrap(), text(&probe_script))).unwrap();
    let injected = Runtime {
        ffmpeg: runtime.ffmpeg.clone(),
        ffprobe: probe_wrapper,
    };
    let retry_output = folder.join("audio-retry.mp4");
    let mut events = Vec::new();
    media::export(
        &injected,
        &sources,
        &retry_output,
        Default::default(),
        &AtomicBool::new(false),
        |event| events.push(event),
    )
    .unwrap();
    assert!(events.iter().any(|event| {
        event
            .reason
            .is_some_and(|reason| reason.issue == CopyIssue::AudioCopyFailed)
    }));
    assert!(events.iter().all(|event| event.mode == ExportMode::Copy));
    assert_eq!(
        hashes(&runtime, &retry_output, "v:0"),
        hashes(&runtime, &continuous, "v:0")
    );
    assert_eq!(pcm(&runtime, &retry_output).len() / 8, 153600);
    let previous_output = folder.join("cancelled-retry.mp4");
    fs::write(&previous_output, b"previous destination").unwrap();
    let cancel = AtomicBool::new(false);
    let result = media::export(
        &injected,
        &sources,
        &previous_output,
        Default::default(),
        &cancel,
        |event| {
            if event
                .reason
                .is_some_and(|reason| reason.issue == CopyIssue::AudioCopyFailed)
            {
                cancel.store(true, Ordering::Relaxed);
            }
        },
    );
    assert_eq!(result, Err("Canceled".into()));
    assert_eq!(fs::read(&previous_output).unwrap(), b"previous destination");
    for (index, source) in sources.iter().enumerate() {
        assert_eq!(fs::read(&source.path).unwrap(), original_bytes[index]);
    }
    assert!(fs::read_dir(&folder).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".stitch-")
    }));
    fs::write(folder.join("summary.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "copy_video_audio":true, "decoded_continuous_audio_identical":true,
        "encode_video_copy_audio":true, "copy_video_with_silence":true,
        "independent_audio_padding_rebuilt":true, "single_mp4_and_mkv_audio_copy":true,
        "audio_validation_retries_without_video_encoding":true, "cancelled_retry_keeps_destination":true,
        "unsupported_aac_rate_normalized_without_video_encoding":true,
        "sources_unchanged":true, "temporary_folders_removed":true
    })).unwrap()).unwrap();
}
