# FFmpeg source archive

This archive contains the sources for the FFmpeg tools built for Stitch.
The runtime uses FFmpeg 9.0.2 and GPL-3.0-or-later.
Each dependency keeps its license files.

`src_packages/` contains the resolved dependency sources and their Git data.
`mpv-winbuild-cmake/` contains the original build scripts.
`metadata/LOCAL-CHANGES/` contains the changes to those scripts and sources.

`bootstrap/` contains the CMake bootstrap inputs.
`metadata/CONFIGURED-BUILD/` contains the target configuration.
`metadata/BUILD-COMMANDS.txt` gives the build commands and container pin.
`metadata/RESOLVED-SOURCES.tsv` gives the source revisions.

Use the specified container to make a new build.
Apply the recorded changes to the original build scripts before configuration.
Keep the captured dependency sources at the specified source location.
Do not execute the upstream update target.

The build uses GCC for generic Windows x64 processors.
The archive contains sources for both `ffmpeg.exe` and `ffprobe.exe`.
The application runs these tools as separate processes.
The public [Stitch repository](https://github.com/MadMatt341/Stitch) contains the separate MIT-licensed application.
The source archive retains each dependency's license.
The source capture scripts use GPL-2.0-or-later, as specified in their headers and `metadata/COPYING`.
