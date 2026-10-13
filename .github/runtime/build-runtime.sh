#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-or-later
# Copyright (c) 2026 MadMatt.
set -euo pipefail

repository_root=$(realpath "${1:?Repository root is required.}")
distribution_root="$repository_root/.github/runtime"
manifest="$distribution_root/build.json"
python3 - "$manifest" <<'PY'
import json
import sys
manifest = json.load(open(sys.argv[1], encoding="utf-8"))
assert manifest["schemaVersion"] == 1
assert all(value is False for value in manifest["policy"].values())
assert manifest["ffmpeg"]["repository"] == "https://github.com/FFmpeg/FFmpeg.git"
assert manifest["environment"]["container"] == "ghcr.io/shinchiro/archlinux@sha256:c7dffe77b57d98b10e327dde12d3977faf4cb90aa7cb4f5eeac4e9d68d724239"
PY
work_root="$repository_root/build/runtime-work"
output_root="$repository_root/build/runtime-output"
if [[ -e "$work_root" || -e "$output_root" ]]; then
    echo 'The runtime build requires empty work and output directories.' >&2
    exit 1
fi
read_value() {
    python3 - "$manifest" "$1" <<'PY'
import json
import sys
value = json.load(open(sys.argv[1], encoding="utf-8"))
for key in sys.argv[2].split("."):
    value = value[key]
print(value)
PY
}
build_commit=$(read_value buildScripts.commit)
ffmpeg_commit=$(read_value ffmpeg.commit)
nvcodec_commit=$(read_value nvcodec.commit)
gnu_mirror=$(read_value environment.gnuMirror)
patch_relative=$(read_value buildScripts.patch.path)
patch_file="$distribution_root/$patch_relative"
patch_hash=$(read_value buildScripts.patch.sha256)
printf '%s  %s\n' "$patch_hash" "$patch_file" | sha256sum --check
export GIT_COMMITTER_NAME GIT_COMMITTER_EMAIL SOURCE_DATE_EPOCH
GIT_COMMITTER_NAME=$(read_value environment.gitCommitter.name)
GIT_COMMITTER_EMAIL=$(read_value environment.gitCommitter.email)
mkdir -p "$work_root" "$output_root"
build_script_root="$work_root/mpv-winbuild-cmake"
source_root="$work_root/src_packages"
build_root="$work_root/build_x86_64"
install_root="$work_root/install"
git init "$build_script_root"
git -C "$build_script_root" remote add origin "$(read_value buildScripts.repository)"
git -C "$build_script_root" fetch --depth=1 origin "$build_commit"
git -C "$build_script_root" checkout --detach FETCH_HEAD
[[ $(git -C "$build_script_root" rev-parse HEAD) == "$build_commit" ]]
git -C "$build_script_root" apply --check "$patch_file"
git -C "$build_script_root" apply "$patch_file"
git -C "$build_script_root" add --intent-to-add toolchain/windows-rs.cmake
SOURCE_DATE_EPOCH=$(git -C "$build_script_root" show -s --format=%ct HEAD)

# Keep FFmpeg at the current application pin and build both required tools.
python3 - "$build_script_root/packages/ffmpeg.cmake" "$ffmpeg_commit" \
    "$build_script_root/packages/nvcodec-headers.cmake" "$nvcodec_commit" \
    "$build_script_root" "$gnu_mirror" "$manifest" <<'PY'
from pathlib import Path
import json
import re
import sys
path = Path(sys.argv[1])
text = path.read_text()
needle = "    GIT_REPOSITORY https://github.com/FFmpeg/FFmpeg.git\n"
assert text.count(needle) == 1
assert text.count("--disable-ffprobe") == 1
text = text.replace(needle, needle + f"    GIT_TAG {sys.argv[2]}\n")
text = text.replace("--disable-ffprobe", "--enable-ffprobe")
path.write_text(text, newline="\n")
path = Path(sys.argv[3])
text = path.read_text()
needle = "    GIT_REPOSITORY https://github.com/FFmpeg/nv-codec-headers.git\n"
assert text.count(needle) == 1
text = text.replace(needle, needle + f"    GIT_TAG {sys.argv[4]}\n")
path.write_text(text, newline="\n")
assert sys.argv[6] == "https://mirrors.kernel.org/gnu/"
changed = 0
for recipe in Path(sys.argv[5]).rglob("*.cmake"):
    text = recipe.read_text()
    updated = text.replace("https://ftp.gnu.org/pub/gnu/", sys.argv[6])
    updated = updated.replace("https://ftp.gnu.org/gnu/", sys.argv[6])
    if updated != text:
        assert "URL_HASH SHA256=" in text or "URL_HASH SHA512=" in text
        recipe.write_text(updated, newline="\n")
        changed += 1
assert changed == 5, f"Unexpected GNU mirror recipe count: {changed}"
manifest = json.loads(Path(sys.argv[7]).read_text())
for package, record in manifest["dependencySources"].items():
    assert re.fullmatch(r"[0-9a-f]{40}", record["revision"]), package
    recipe = Path(sys.argv[5]) / record["recipe"]
    text = recipe.read_text()
    assert text.count("GIT_REPOSITORY ") == 1, package
    text, replaced = re.subn(
        r"(?m)^\s*GIT_TAG[^\n]*\n", "", text
    )
    assert replaced <= 1, package
    # A fixed commit must not retain branch reset rules from the upstream recipe.
    for key in ("GIT_REMOTE_NAME", "GIT_RESET"):
        text, removed = re.subn(rf"(?m)^\s*{key}[^\n]*\n", "", text)
        assert removed <= 1, (package, key)
    text, inserted = re.subn(
        r"(?m)^(\s*GIT_REPOSITORY[^\n]*\n)",
        lambda match: match.group(1) + f"    GIT_TAG {record['revision']}\n",
        text,
    )
    assert inserted == 1, package
    if "repository" in record:
        text, replaced = re.subn(
            r"GIT_REPOSITORY[^\n]+", "GIT_REPOSITORY " + record["repository"], text
        )
        assert replaced == 1, package
    if package in ("c-ares", "nghttp2", "nghttp3", "ngtcp2"):
        assert "--depth=1" in text, package
        text = text.replace("--depth=1 ", "")
    recipe.write_text(text, newline="\n")
PY
python3 - "$manifest" > "$work_root/SOURCE-DOWNLOAD-TARGETS.txt" <<'PY'
import json
import re
import sys
manifest = json.load(open(sys.argv[1], encoding="utf-8"))
packages = set(manifest["dependencySources"]) | {"windows-rs", "ffmpeg", "nvcodec-headers"}
invalid = [package for package in packages if not re.fullmatch(r"[a-z0-9_-]+", package)]
assert not invalid, invalid
for package in sorted(packages):
    print(f"{package}-download")
PY
mapfile -t source_download_targets < "$work_root/SOURCE-DOWNLOAD-TARGETS.txt"
commands_file="$work_root/BUILD-COMMANDS.txt"
cat > "$commands_file" <<EOF
Build scripts: $(read_value buildScripts.repository)
Build revision: $build_commit
FFmpeg revision: $ffmpeg_commit
NVIDIA headers revision: $nvcodec_commit
NVIDIA API version: $(read_value nvcodec.apiVersion)
GNU archive mirror: $gnu_mirror
Container: $(read_value environment.container)
Patch: $patch_relative
Patch SHA-256: $patch_hash
The source archive includes the complete applied patch in metadata/LOCAL-CHANGES.

cmake -DTARGET_ARCH=x86_64-w64-mingw32 -DCOMPILER_TOOLCHAIN=gcc \\
  -DGCC_ARCH=x86-64 -DCMAKE_INSTALL_PREFIX=<work>/install \\
  -DSINGLE_SOURCE_LOCATION=<work>/src_packages -DRUSTUP_LOCATION=<work>/rustup \\
  -DENABLE_CCACHE=OFF -G Ninja -B <work>/build_x86_64 -S <work>/mpv-winbuild-cmake
ninja -C <work>/build_x86_64 download
ninja -C <work>/build_x86_64 ${source_download_targets[*]}
ninja -C <work>/build_x86_64 gcc
ninja -C <work>/build_x86_64 ffmpeg

The build does not execute the upstream update target.
EOF
cmake -DTARGET_ARCH=x86_64-w64-mingw32 -DCOMPILER_TOOLCHAIN=gcc \
    -DGCC_ARCH=x86-64 -DCMAKE_INSTALL_PREFIX="$install_root" \
    -DSINGLE_SOURCE_LOCATION="$source_root" -DRUSTUP_LOCATION="$work_root/rustup" \
    -DENABLE_CCACHE=OFF -G Ninja -B "$build_root" -S "$build_script_root"
[[ $(git -C "$build_root/cmake/Modules/cmake-source" rev-parse FETCH_HEAD) == a9b14138fe83e134bc3da139857b5616695bab54 ]]
ninja -C "$build_root" download
ninja -C "$build_root" "${source_download_targets[@]}"
[[ $(git -C "$source_root/windows-rs" rev-parse HEAD) == 845e42a4328ec5b54b97f965798589d997cad177 ]]
printf '%s  %s\n' 9DDFC6F5E5C3D29522F35E1CAC566617E9D28CCBC6CCB072C79FA9949CDBE336 \
    "$source_root/windows-rs/crates/libs/default/Windows.winmd" | sha256sum --check
[[ $(git -C "$source_root/ffmpeg" rev-parse HEAD) == "$ffmpeg_commit" ]]
[[ $(git -C "$source_root/nvcodec-headers" rev-parse HEAD) == "$nvcodec_commit" ]]
python3 - "$manifest" "$source_root" <<'PY'
import json
from pathlib import Path
import subprocess
import sys
manifest = json.loads(Path(sys.argv[1]).read_text())
for package, record in manifest["dependencySources"].items():
    source = Path(sys.argv[2]) / package
    assert (source / ".git").exists(), f"Missing independent Git checkout: {package}"
    actual = subprocess.check_output(
        ["git", "-C", str(source), "rev-parse", "HEAD"], text=True
    ).strip()
    assert actual == record["revision"], (package, actual, record["revision"])
PY
ninja -C "$build_root" gcc
ninja -C "$build_root" ffmpeg
[[ $(git -C "$source_root/ffmpeg" rev-parse HEAD) == "$ffmpeg_commit" ]]
python3 - "$manifest" "$source_root" "$build_root" <<'PY'
import json
from pathlib import Path
import subprocess
import sys
manifest = json.loads(Path(sys.argv[1]).read_text())
for package, record in manifest["dependencySources"].items():
    if "patchedTree" not in record:
        continue
    source = Path(sys.argv[2]) / package
    patch_stamp = Path(sys.argv[3]) / "packages" / f"{package}-prefix" / "src" / f"{package}-stamp" / f"{package}-patch"
    if patch_stamp.exists():
        actual = subprocess.check_output(["git", "-C", str(source), "rev-parse", "HEAD^{tree}"], text=True).strip()
        assert actual == record["patchedTree"], (package, actual, record["patchedTree"])
    else:
        actual = subprocess.check_output(["git", "-C", str(source), "rev-parse", "HEAD"], text=True).strip()
        assert actual == record["revision"], (package, actual, record["revision"])
PY

runtime_stage="$work_root/runtime"
mkdir "$runtime_stage"
for tool in ffmpeg ffprobe; do
    cp "$install_root/x86_64-w64-mingw32/bin/$tool.exe" "$runtime_stage/"
    file "$runtime_stage/$tool.exe" | grep -q 'PE32+.*x86-64'
    strings "$runtime_stage/$tool.exe" > "$work_root/$tool.strings"
    for flag in --enable-gpl --enable-version3 --enable-libx264 --enable-libx265 --enable-libzimg --enable-nvenc; do
        grep -q -- "$flag" "$work_root/$tool.strings"
    done
done
cp "$source_root/ffmpeg/COPYING.GPLv3" "$runtime_stage/FFmpeg-LICENSE.txt"
cat > "$runtime_stage/FFmpeg-README.txt" <<EOF
FFmpeg 9.0.2, Windows x64, built for Stitch.
License: GPL-3.0-or-later.
FFmpeg source revision: $ffmpeg_commit
Build scripts revision: $build_commit
NVIDIA headers revision: $nvcodec_commit
NVIDIA API version: $(read_value nvcodec.apiVersion)
The companion source archive contains dependency sources, patches, and build commands.
Keep that archive available with each public runtime download.
EOF
python3 "$distribution_root/collect-licenses.py" "$source_root" "$runtime_stage"
binary_archive="$output_root/ffmpeg-stitch-9.0.2-win-x64.zip"
(cd "$runtime_stage" && 7z a "$binary_archive" ./*)
configured_root="$work_root/configured-build-record"
mkdir "$configured_root"
mv "$build_root/cmake" "$configured_root/"
for name in CMakeCache.txt toolchain.cmake meson_cross.txt exec; do
    cp "$build_root/$name" "$configured_root/"
done

# Retain sources and configuration while removing generated build output.
for candidate in "$build_root" "$install_root" "$work_root/rustup"; do
    resolved=$(realpath -m "$candidate")
    case "$resolved" in "$work_root"/*) ;; *) exit 1 ;; esac
    rm -rf --one-file-system -- "$resolved"
done
available_bytes=$(df -B1 --output=avail "$output_root" | tail -n 1 | tr -d '[:space:]')
if (( available_bytes < 2500000000 )); then
    echo 'The source archive requires at least 2.5 GB of free space.' >&2
    exit 1
fi
source_archive="$output_root/ffmpeg-stitch-source-${ffmpeg_commit:0:12}.tar.zst"
bash "$distribution_root/capture-source.sh" "$source_root" "$build_script_root" \
    "$configured_root" "$distribution_root" "$commands_file" \
    "$source_archive" "$output_root/RESOLVED-SOURCES.tsv"
if (( $(stat -c %s "$source_archive") >= 1900000000 )); then
    echo 'The source archive exceeds the release size limit.' >&2
    exit 1
fi
for package in ffmpeg x264 x265 windows-rs; do
    grep -q "^${package}"$'\t'git$'\t' "$output_root/RESOLVED-SOURCES.tsv"
done
tar --zstd -tf "$source_archive" > "$work_root/source-listing.txt"
for entry in src_packages/ffmpeg/ src_packages/x264/ src_packages/x265/ \
    bootstrap/cmake/Modules/ExternalProject.cmake metadata/LOCAL-CHANGES/mpv-winbuild-cmake.patch; do
    grep -Fq "/$entry" "$work_root/source-listing.txt"
done
python3 - "$output_root" "$runtime_stage" "$build_commit" "$ffmpeg_commit" "$nvcodec_commit" <<'PY'
import hashlib
import json
from pathlib import Path
import sys
output, runtime = map(Path, sys.argv[1:3])
def record(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return {"fileName": path.name, "sizeBytes": path.stat().st_size, "sha256": digest.hexdigest().upper()}
result = {
    "schemaVersion": 1,
    "status": "captured-awaiting-windows-validation",
    "target": "win-x64",
    "buildScriptsCommit": sys.argv[3],
    "ffmpegCommit": sys.argv[4],
    "nvcodecCommit": sys.argv[5],
    "nvcodecApiVersion": "13.0",
    "archive": record(output / "ffmpeg-stitch-9.0.2-win-x64.zip"),
    "binaries": [record(runtime / name) for name in ("ffmpeg.exe", "ffprobe.exe")],
    "correspondingSource": record(next(output.glob("ffmpeg-stitch-source-*.tar.zst"))),
    "resolvedSources": record(output / "RESOLVED-SOURCES.tsv"),
}
(output / "runtime-build-result.json").write_text(json.dumps(result, indent=2) + "\n")
PY
(cd "$output_root" && sha256sum ffmpeg-stitch-9.0.2-win-x64.zip \
    ffmpeg-stitch-source-*.tar.zst RESOLVED-SOURCES.tsv runtime-build-result.json > SHA256SUMS.txt)
echo "Runtime candidate: $output_root"
