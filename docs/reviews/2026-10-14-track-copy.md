# Track copy checks, 2026-10-14

The user selected video file joining and automatic encoding when copying cannot keep correct timing.
Trimming stays in TVC.
This change kept the media and encoder changes from the chat `Test stitching speed and file size`.
The [media contract](../media.md#copy-and-conversion) gives the implementation conditions.

## Build and checks

The checks used the local release build and the prepared source-captured FFmpeg 9.0.2 runtime.
The dependency pins did not change.
The application SHA-256 was `C938D9478FC78C30BE47D761C27C72566C0F51ADDFD60C8FFF3A5FBDD2FCA187`.
The release build used `cargo build --release --locked`.
The check kept the prepared runtime because `scripts/build.ps1` replaces it.

The following checks passed from the project root:

```powershell
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked
cargo test --locked --test media_flow -- --ignored
cargo test --locked --test copy_flow -- --ignored
./scripts/test-ui.ps1
```

The unit checks passed 34 tests.
The media checks passed five tests, including CPU H.264, CPU H.265, NVIDIA encoding, HDR, frame timing, cancellation, and source protection.
The track-copy check passed with the following results:

- Video and audio packets stayed unchanged for correctly split recordings.
- Decoded copied audio matched the uninterrupted recording.
- Video conversion kept compatible audio copied.
- Independent AAC recordings used audio encoding to remove internal padding.
- Clips without audio supplied silence without video encoding.
- PCM at 192 kHz became stereo AAC at 48 kHz without video encoding.
- Audio validation failure caused an audio retry without video encoding.
- Cancellation before the retry kept the previous destination.
- Source bytes stayed unchanged, and the app removed its temporary export folders.

## Recording and UI results

The recording check used two 1080p60 H.264 MKV fixtures with stereo AAC at 48 kHz.
Their source size was 80.51 MB, and the MP4 output was 80.57 MB.
Export took 1.50 seconds, including source inspection and output validation.
All compressed video packets stayed unchanged and in list order.
The app identified an audio timing difference and encoded only audio.

The native UI script passed with isolated app state.
An additional native-message check opened F1 Export details.
It showed the audio padding cause, the clip number, and the filename.
Keyboard focus returned to Stitch after the dialog closed.
Visual examination confirmed the compact output summary and setting controls.

The generated reports stay in `build/test-output/`.
The recording test source stays in `build/test-output/feedback-investigation-20261014/recording-check-source.rs` because it uses local recording fixtures.

## Remaining checks

Physical keyboard and mouse checks remain necessary.
The native messages do not prove physical F1 input or file-picker appearance on each displayed frame.
The recording checks do not show compatibility with each OBS configuration, camera, or media player.
