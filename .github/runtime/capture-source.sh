#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-or-later
# Copyright (c) 2026 MadMatt. Adapted from Tiny Video Cutter's source capture script.
set -euo pipefail

if [[ $# -ne 7 ]]; then
    echo "Usage: $0 <src-packages> <build-scripts> <configured-build> <distribution-dir> <commands-file> <archive> <resolved-sources-copy>" >&2
    exit 2
fi

source_root=$(realpath "$1")
build_script_root=$(realpath "$2")
configured_build_root=$(realpath "$3")
distribution_root=$(realpath "$4")
commands_file=$(realpath "$5")
output_archive=$(realpath -m "$6")
resolved_sources_copy=$(realpath -m "$7")

for required_directory in \
    "$source_root" \
    "$build_script_root" \
    "$configured_build_root" \
    "$distribution_root"; do
    if [[ ! -d "$required_directory" ]]; then
        echo "Required directory not found: $required_directory" >&2
        exit 1
    fi
done

if [[ ! -f "$commands_file" ]]; then
    echo "Build command record not found: $commands_file" >&2
    exit 1
fi

if [[ -e "$output_archive" || -e "$resolved_sources_copy" ]]; then
    echo "Refusing to overwrite source-bundle output." >&2
    exit 1
fi

mkdir -p "$(dirname "$output_archive")" "$(dirname "$resolved_sources_copy")"

archive_name=$(basename "$output_archive")
bundle_name=${archive_name%.tar.zst}
if [[ "$bundle_name" == "$archive_name" ]]; then
    echo "Source bundle must use the .tar.zst extension." >&2
    exit 1
fi

temp_parent=$(realpath "${TMPDIR:-/tmp}")
stage_root=$(realpath "$(mktemp -d)")
if [[ $(dirname "$stage_root") != "$temp_parent" ]]; then
    echo "Temporary staging directory was created outside $temp_parent: $stage_root" >&2
    exit 1
fi

cleanup_stage() {
    if [[ -n ${stage_root:-} \
        && -d "$stage_root" \
        && $(dirname "$stage_root") == "$temp_parent" ]]; then
        rm -rf -- "$stage_root"
    else
        echo "Refusing unsafe temporary cleanup: ${stage_root:-unset}" >&2
    fi
}
trap cleanup_stage EXIT

bundle_root="$stage_root/$bundle_name"
metadata_root="$bundle_root/metadata"
changes_root="$metadata_root/LOCAL-CHANGES"
bootstrap_root="$bundle_root/bootstrap/cmake"
configured_metadata_root="$metadata_root/CONFIGURED-BUILD"
mkdir -p \
    "$metadata_root" \
    "$changes_root" \
    "$bundle_root/mpv-winbuild-cmake" \
    "$bootstrap_root" \
    "$configured_metadata_root"

cp "$distribution_root/SOURCE-BUNDLE.md" "$bundle_root/README.md"
cp "$distribution_root/build.json" "$metadata_root/build.json"
cp "$commands_file" "$metadata_root/BUILD-COMMANDS.txt"
cp "$distribution_root/build-runtime.sh" "$metadata_root/build-runtime.sh"
cp "$distribution_root/capture-source.sh" "$metadata_root/capture-source.sh"
cp "$distribution_root/collect-licenses.py" "$metadata_root/collect-licenses.py"
cp "$distribution_root/COPYING" "$metadata_root/COPYING"

build_patch_relative=$(python3 - "$distribution_root/build.json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    print(json.load(handle)["buildScripts"]["patch"]["path"])
PY
)
build_patch="$distribution_root/$build_patch_relative"
if [[ ! -f "$build_patch" ]]; then
    echo "Recorded build-script patch not found: $build_patch" >&2
    exit 1
fi

build_patch_root="$metadata_root/BUILD-PATCHES"
mkdir -p "$build_patch_root"
cp "$build_patch" "$build_patch_root/$(basename "$build_patch")"

if [[ ! -f "$configured_build_root/cmake/Modules/setup_done" \
    || ! -f "$configured_build_root/cmake/Modules/ExternalProject.cmake" ]]; then
    echo "Configured CMake bootstrap inputs are incomplete." >&2
    exit 1
fi

cp -a "$configured_build_root/cmake/." "$bootstrap_root/"
for configured_file in \
    CMakeCache.txt \
    toolchain.cmake \
    meson_cross.txt \
    exec; do
    if [[ ! -f "$configured_build_root/$configured_file" ]]; then
        echo "Configured build record not found: $configured_file" >&2
        exit 1
    fi
    cp "$configured_build_root/$configured_file" "$configured_metadata_root/$configured_file"
done

git -C "$build_script_root" archive --format=tar HEAD |
    tar -xf - -C "$bundle_root/mpv-winbuild-cmake"

git -C "$build_script_root" status --porcelain=v1 --untracked-files=all \
    > "$changes_root/mpv-winbuild-cmake.status.txt"
git -C "$build_script_root" diff --binary HEAD \
    > "$changes_root/mpv-winbuild-cmake.patch"
if [[ ! -s "$changes_root/mpv-winbuild-cmake.patch" ]]; then
    echo "The recorded build-script patch produced no local change." >&2
    exit 1
fi

resolved_sources="$metadata_root/RESOLVED-SOURCES.tsv"
non_git_checksums="$metadata_root/NON-GIT-SOURCE-SHA256SUMS.txt"
printf 'package\tkind\trevision\ttree\tremote\tworking_tree\n' > "$resolved_sources"
: > "$non_git_checksums"

while IFS= read -r -d '' source_entry; do
    package_name=$(basename "$source_entry")

    source_git_root=$(git -C "$source_entry" rev-parse --show-toplevel 2>/dev/null || true)
    if [[ -n "$source_git_root" && $(realpath "$source_git_root") == $(realpath "$source_entry") ]]; then
        revision=$(git -C "$source_entry" rev-parse HEAD)
        tree=$(git -C "$source_entry" rev-parse 'HEAD^{tree}')
        remote=$(git -C "$source_entry" remote get-url origin 2>/dev/null || true)
        remote=${remote//$'\t'/ }
        remote=${remote//$'\n'/ }

        status_file="$changes_root/$package_name.status.txt"
        git -C "$source_entry" status --porcelain=v1 --untracked-files=all > "$status_file"
        if [[ -s "$status_file" ]]; then
            working_tree=modified
            git -C "$source_entry" diff --binary HEAD > "$changes_root/$package_name.patch"
            git -C "$source_entry" diff --binary --cached > "$changes_root/$package_name.index.patch"
        else
            working_tree=clean
            rm -f "$status_file"
        fi

        submodule_file="$changes_root/$package_name.submodules.txt"
        git -C "$source_entry" submodule status --recursive > "$submodule_file" 2>/dev/null || true
        [[ -s "$submodule_file" ]] || rm -f "$submodule_file"

        source_kind=git
        if [[ "$package_name" == mpv-packaging ]]; then
            source_kind=unused-reference
            working_tree=not-included
        fi
        printf '%s\t%s\t%s\t%s\t%s\t%s\n' \
            "$package_name" "$source_kind" "$revision" "$tree" "$remote" "$working_tree" \
            >> "$resolved_sources"
    else
        printf '%s\tarchive\t-\t-\t-\tcaptured\n' "$package_name" \
            >> "$resolved_sources"

        python3 - "$source_entry" "$source_root" >> "$non_git_checksums" <<'PY'
import hashlib
import os
from pathlib import Path
import sys

entry = Path(sys.argv[1])
source_root = Path(sys.argv[2])
files = [entry] if entry.is_file() else sorted(
    (
        path
        for path in entry.rglob("*")
        if path.is_file() and not path.is_symlink()
    ),
    key=lambda path: os.fsencode(path.relative_to(source_root).as_posix()),
)
for path in files:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    print(f"{digest.hexdigest()}  {path.relative_to(source_root).as_posix()}")
PY
    fi
done < <(find "$source_root" -mindepth 1 -maxdepth 1 -print0 | sort -z)

cp "$resolved_sources" "$resolved_sources_copy"

{
    echo "captured_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "source_date_epoch=${SOURCE_DATE_EPOCH:-unset}"
    echo "uname=$(uname -a)"
    echo "build_scripts_commit=$(git -C "$build_script_root" rev-parse HEAD)"
    echo "git_committer_name=${GIT_COMMITTER_NAME:-unset}"
    echo "git_committer_email=${GIT_COMMITTER_EMAIL:-unset}"
    for tool in git cmake ninja meson python3 gcc g++ 7z zstd; do
        echo
        echo "[$tool]"
        "$tool" --version 2>&1 | head -n 3 || true
    done
} > "$metadata_root/BUILD-ENVIRONMENT.txt"

source_parent=$(dirname "$source_root")
source_name=$(basename "$source_root")
archive_epoch=${SOURCE_DATE_EPOCH:-0}

tar --sort=name \
    --mtime="@$archive_epoch" \
    --owner=0 --group=0 --numeric-owner \
    -cf - \
    -C "$stage_root" "$bundle_name" \
    -C "$source_parent" \
    --exclude="$source_name/mpv-packaging" \
    --transform="s|^$source_name|$bundle_name/src_packages|" \
    "$source_name" |
    zstd -15 -T0 -o "$output_archive"

echo "Created corresponding-source bundle: $output_archive"
