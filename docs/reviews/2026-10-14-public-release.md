# Public release checks, 2026-10-14

## Build and scope

The release version is `0.1.0-beta` for Windows x64.
The application code is from commit `c309f568b09b061dad3b555d62a6fbdbdb116f87`.
Release preparation changes the runtime record and documentation.
It does not change the application code.
The release manifest identifies the complete packaged source revision.

The checks used Rust 1.98.1 and Windows build 26200.
The production executable SHA-256 is `6524C9EE4E80F06B82DDD59C424FBA52197AFA74B78F7BCD26A0D0D42CF8E9AA`.
The [distribution manifest](../../.github/runtime/distribution.json) identifies the checked FFmpeg runtime.
The [release reference](../releases.md) owns the package procedure and license requirements.

## Runtime and automated checks

The native runtime build completed compilation, source capture, and source audit.
The separate [Windows validation](https://github.com/MadMatt341/Stitch/actions/runs/38512516251) passed all three media acceptance checks.
Local source inspection checked 85 dependency pins, 114 source records, and the archive's build inputs.
The embedded manifest, source inventory, and bootstrap patch matched their selected records.
The captured MinGW changes are generated build-system files.

All 24 unit tests passed.
Formatting and Clippy with all features passed.
All three local media acceptance checks passed with the selected runtime.
These checks included CPU and NVIDIA H.264, H.265, Main10, and HDR encoding.
They also checked frame timing, audio, cancellation, source protection, and output protection.

The isolated native UI suite passed export, cancellation, scrolling, selection, saved settings, reopening, and name migration.
The message-based picker check reported foreground return in this environment.
The large and compact window captures showed usable list and export controls.
These results do not establish physical input acceptance.

## Package checks

The colleague package used the same production executable and selected runtime.
A fresh extraction passed inspection, packet-copy export, H.265 conversion, and complete output decoding.
The source video hashes stayed unchanged.
The package included matching runtime source and license notices.
It excluded personal state and the application's Git metadata.

The public package keeps the FFmpeg source archive as a separate release asset.
The application source archive and checksums also accompany the portable ZIP.
Final public-package extraction and anonymous download checks are recorded in generated release output.
Raw reports, captures, and generated videos stay under ignored `build/test-output/` folders.

## Remaining beta limits

Physical input, each displayed picker frame, and continuous drag appearance remain unverified for this revision.
HDR appearance on target displays and playback in target players or editors also remain unverified.
Converted output does not retain dynamic HDR or source Dolby Vision metadata.
Dolby Vision profile 5 conversion remains unavailable.
These limits do not replace the completed automated results.
