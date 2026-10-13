# Development

This reference owns the source layout, build procedure, runtime provenance, and automated check commands.
The [contributor guide](../AGENTS.md) gives the task map.

## Source and output

| Path | Contents |
| --- | --- |
| `src/` | Active Rust application and library source. |
| `tests/` | Active integration tests. |
| `scripts/` | Build and native UI check procedures. |
| `docs/` | Full implementation references. |
| `build/Stitch.exe` | Release application. |
| `build/Stitch-ui-check.exe` | Native UI check executable. |
| `build/runtime/` | FFmpeg, FFprobe, runtime license, and notices. |
| `build/target/` | Cargo build output. |
| `build/test-output/` | Generated videos, isolated state, reports, and UI captures. |
| `.reader-data/` | Ignored runtime download cache and temporary reference files. |

The project uses the standard Rust folders without a separate application-name folder.
Build output and test output stay separate from source and references.
The `.gitignore` file does not include generated output, runtime binaries, or video files in Git.

## Build

Requirements: Windows 10 or a subsequent version, Rust 1.98.1, PowerShell 7, and the MSVC build tools for Rust.
The build script also uses `tar` to extract the runtime archive.

From the project root, run:

```powershell
./scripts/build.ps1
```

The script downloads the pinned FFmpeg archive when the cache has no archive.
It compares the archive SHA-256 with the pin in `scripts/build.ps1` before extraction.
It extracts the verified archive on each build.
It compiles the release executable with the locked Cargo dependencies.
It copies the executable, runtime, license, and notices into `build/`.
After completion, the script prints the path to `build/Stitch.exe`.

If the pinned archive is in the cache, you can prevent a download:

```powershell
./scripts/build.ps1 -SkipDownload
```

If the cache has no archive, this command fails.
If archive validation fails, remove the archive from `.reader-data/` before you download it again.
Do not bypass the hash comparison.
If compilation fails, correct the reported source or toolchain error before you build again.

## Runtime provenance

The runtime pin is FFmpeg 9.0.2 from [Gyan's build archive](https://www.gyan.dev/ffmpeg/builds/).
`scripts/build.ps1` owns the specified archive URL and SHA-256.
The runtime source reference is [FFmpeg commit 946fcce07b](https://github.com/FFmpeg/FFmpeg/commit/946fcce07b).
The runtime uses GPLv3.
The executable folder includes `runtime/FFmpeg-LICENSE.txt` and `runtime/FFmpeg-README.txt`.
The [release reference](releases.md) owns public distribution, license boundaries, and corresponding source requirements.
The Gyan archive remains a development input until the source-captured runtime passes its release checks.

Encoding presets use the pinned runtime's `libx264`, `libx265`, `h264_nvenc`, and `hevc_nvenc` encoders.
The NVIDIA encoders can operate only with a compatible GPU and driver.
The [media reference](media.md#nvidia-encoding-and-cpu-recovery) gives the capability check and CPU recovery conditions.

`Cargo.toml` owns direct Rust dependency pins.
`Cargo.lock` owns the resolved dependency versions.
`.cargo/config.toml` owns the target folder and MSVC static CRT option.
The Git repository contains no runtime binaries or video files.

## Checks

From the project root, run the unit and compile checks:

```powershell
cargo fmt --all -- --check
cargo test --locked
cargo clippy --locked --all-targets --all-features -- -D warnings
```

The format check reports no differences on success.
Unit tests must pass, and Clippy must report no warnings.
The unit test command does not execute the generated-video integration test.
If a command fails, correct the reported problem before you do the check again.

Export settings checks must examine source mode defaults, file size calculations, codec selection, and encoding arguments.
They must distinguish HDR transfer metadata from high bit depth.
They must examine Main10 output, color normalization, quality encoding, and color preservation through CPU recovery.
They must examine one CPU retry after NVIDIA failure and no retry after cancellation.
Process timeout checks must examine cleanup when a child process keeps output pipes open.
These deterministic checks cannot depend on NVIDIA hardware failure.

Before media or native UI checks, run `scripts/build.ps1` from the project root.
These checks use the prepared runtime beside `build/Stitch.exe`:

```powershell
cargo test --locked --test media_flow -- --ignored
./scripts/test-ui.ps1
```

The media check makes videos and `summary.json` in `build/test-output/media-flow-<process-id>/`.
It also makes frame timing and color records in `frame-flow-<process-id>/` and `color-flow-<process-id>/`.
It must pass the [media acceptance checks](media.md#failure-recovery-and-checks).
For encoding changes, it must examine both CPU codecs and the available NVIDIA codecs.
If an NVIDIA codec is not available, record that check limit.
Successful capability checks do not replace actual output validation.

If the runtime is missing, prepare the build before you do the check again.
If a previous run left the same process-ID folder, use a new test process or remove only that generated folder.

The native UI script makes an isolated folder under `build/test-output/`.
It first builds the `native-ui-checks` feature in `build/target/native-ui/` and copies its executable to `build/Stitch-ui-check.exe`.
This executable contains `src/ui/smoke.rs` and accepts `--ui-smoke` for the internal checks.
The production build excludes these checks, capture helpers, and test timer handlers.

The script uses the production executable for external list and saved-state checks.
It verifies row selection, thumbnail movement, repaint, drag cancellation, scrolling, export, and state restoration.
It also verifies native Cancel, Ctrl+S cancellation, and GDI drawing context restoration.
It waits for the state save timer and verifies immediate state publication on close.
It also locks the previous state file and verifies a state save after lock removal.

Export control checks must examine the setting buttons, E, the size keys, and locked controls during export.
Saved state checks must examine preset restoration, automatic H.265 selection, and defaults for older state files.
Frame rate checks must examine Original, 30, 60, F, Shift+F, saved selection, and previous selection migration.
Native checks must permit export for mixed eight-bit, standard color 10-bit, and PQ HDR input with each size and rate choice.
Use the [interactive UI checks](ui.md#interactive-checks) to examine the displayed output summary and physical input.

It also captures window frames and writes `interaction.json` and `state-checks.json`.
It keeps the user's default state folder unchanged.
After completion, the script prints the check output folder.
If it fails, examine its error and reports before you execute it again.

The script verifies file-picker cancellation with empty and populated lists.
It verifies the UI thread's active window and keyboard focus after cancellation.
It also verifies legacy state migration from Stich to Stitch in isolated state folders.

If Stitch owns foreground activation before and throughout the sampled picker interval, the script also verifies foreground return.
Otherwise, `picker_foreground_restored` is `null`, and the report gives the remaining interactive check.
Synthetic Windows messages do not establish foreground permission from the last physical input.
Use the [interactive UI checks](ui.md#interactive-checks) to verify foreground return with physical input.

The native UI checks use Windows messages.
They do not use physical keyboard or mouse input.
They do not prove each file-picker frame that the compositor shows.
Use the [interactive UI checks](ui.md#interactive-checks) for file-picker appearance acceptance.
Screenshots of one stable frame do not prove that no flicker occurs during motion.

The independent observer samples picker frames from a separate thread during shell initialization.
These captures show only the sampled frames for their specified environment.
They do not prove every compositor frame.

Keep automated check results separate from observed acceptance.
For long dated evidence, use a linked reference with the date, build, environment, and remaining checks.
Keep raw logs and generated reports in ignored output folders.

The [2026-10-13 review](reviews/2026-10-13-performance.md) records reproduced failures, measurements, and check limits for this change.

The [2026-10-13 export settings review](reviews/2026-10-13-export-settings.md) records encoder checks, CPU recovery, UI observations, and remaining acceptance.

The [2026-10-13 frame rate review](reviews/2026-10-13-frame-rates.md) records timing checks, encoder conditions, and remaining acceptance.

The [2026-10-13 simplified settings review](reviews/2026-10-13-simple-settings.md) records setting buttons, saved selection migration, summary placement, and remaining acceptance.

The [2026-10-13 color conversion review](reviews/2026-10-13-color-conversion.md) records HDR checks, 10-bit recording checks, automatic encoding, and remaining acceptance.
