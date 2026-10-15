# 0.1.3 beta checks, 2026-10-15

## Scope and cause

This release corrects runtime notices and the default build input.
It keeps the application behavior from `0.1.2-beta`.
The Cargo version changes to `0.1.3`.
The release manifest identifies the packaged source revision.

The earlier collector read notice files from source directories.
It did not read notices inside original source archives.
The correction adds 47 unchanged notice files from GCC, binutils, libiconv, libopenmpt, LZO, and Xvid.
This includes the GCC runtime license exception.
The runtime ZIP now contains 367 dependency notice files.
Each added record identifies its source archive, member path, size, and SHA-256.

The source download excludes the unused `mpv-packaging` package and its prebuilt Windows SDK binaries.
That package copies `d3dcompiler_43.dll` into MPV packages.
It does not supply code to the FFmpeg tools.
The inventory keeps its original revision, tree, and source URL as an `unused-reference` record.
All compiled runtime sources stay in the archive.

## Source and runtime checks

The corrected runtime ZIP keeps the original FFmpeg and FFprobe executable bytes.
Their hashes match the successful historical Windows validation and the previous public runtime.
The new source archive keeps 65,456 files unchanged.
Its four changed files are the archive introduction, inventory, capture script, and notice collector.
It adds `metadata/SOURCE-AUDIT.json` with the derivation record.
Archive user and group names are absent, while numeric IDs remain.
All 73,633 retained archive entries keep their modes, numeric IDs, and normalized link targets.

The archive audit passed 114 inventory entries, 87 retained Git HEAD records, and 839 non-Git source checksums.
All 85 specified dependency references passed their revision or patched-tree checks.
All 367 notice files matched their captured source bytes.
The audit also checked source pins, build records, local changes, archive hashes, and unsafe Git credential patterns.

The source comparison and audit reports stay under `build/runtime-audited-v2/`.
The [distribution manifest](../../.github/runtime/distribution.json) owns the selected archive hashes and public URLs.
The runtime release includes `SOURCE-AUDIT.json` and the source comparison report.

## Local application checks

The checks used Windows x64, Rust 1.98.1, and the selected runtime.
GitHub Actions stayed disabled.
No new native compilation was necessary because the executable bytes did not change.

| Check | Result |
| --- | --- |
| Format, unit tests, Clippy | Formatting, all 34 unit tests, and Clippy with all targets and features passed. |
| Candidate media | All five media acceptance tests passed with the corrected runtime ZIP. |
| Track copy | The packet, decoded sample, audio recovery, and output protection check passed. |
| NVIDIA | Actual H.264, H.265, and Main10 HDR output checks passed. |
| Native UI | Export, cancellation, selection, scrolling, saved state, reopening, and name migration passed. |
| Default build | `scripts/build.ps1 -SkipDownload` passed with the verified distribution pin. |
| Archive procedures | Bash syntax, SDK exclusion, notice bytes, notice provenance, unsafe paths, and Windows path collisions passed. |

Raw media reports stay in `build/test-output/media-flow-10136/`, `frame-flow-10136/`, `color-flow-10136/`, `edit-flow-10136/`, and `container-flow-10136/`.
The track-copy report stays in `build/test-output/copy-flow-16504/`.
The native UI reports stay in `build/test-output/native-ui-20261015-134114-534/`.

## Package and publication

The package procedure includes the complete corrected runtime and its notice files.
It also includes the source audit record as a separate release asset.
The default build uses the same verified runtime as public packaging.
It verifies the archive, executable, and dependency notice hashes.

The portable preflight used source commit `f767e4227623922dd3baa26a5c2d921abde951e2`.
Its application SHA-256 is `F29DA0EF17A1C89DADEE3E264B724C706B3F4AACD391791AC01B811F7C150649`.
Fresh extraction passed GUI startup and close with isolated state.
It passed H.264 video copying, H.265 conversion, complete output decoding, and source byte protection.
All release asset, executable, runtime, 367 notice, and Rust standard library notice hashes passed.
Its report stays in `build/test-output/portable-preflight-ebba7a56796a44c9b81679840d407750/summary.json`.

The corrected runtime and corresponding source passed complete anonymous downloads and SHA-256 comparisons.
Their reports stay in `build/test-output/public-runtime-correction/`.
The final application package must repeat extraction and anonymous download checks after this record is committed and pushed.
The [release procedure](../releases.md#publication-checks) gives these checks.
Keep their generated reports under ignored `build/test-output/` folders.

## Remaining beta limits

Physical keyboard and mouse acceptance remains incomplete.
Native messages do not prove each displayed file-picker frame or continuous drag appearance.
The full 192-file export remains incomplete.
HDR appearance on target displays remains unverified.
Converted output does not keep dynamic HDR or source Dolby Vision metadata.
Dolby Vision profile 5 conversion is unavailable.
HEVC playback can require an additional codec in Windows players.
