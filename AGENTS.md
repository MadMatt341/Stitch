# Contributor guide

Stitch is a native Windows app that makes one MP4 video from an ordered list of local video files.

## Scope and source protection

Keep source files unchanged.
Keep export output temporary until output validation passes.
Replace the selected destination only after success.
Keep a previous output file after failure or cancellation.
Read the full [media contract](docs/media.md) before changes to export, input inspection, or process lifetime.

Keep the app small and local.
Get a scope decision before you add network media, export profiles, or a different media engine.
The FFmpeg runtime stays separate from application source.
Keep specified dependency versions in the manifests and build script.

## Local execution policy

The user requires this project to operate without GitHub Actions.
Run builds, runtime compilation, checks, packaging, and release uploads locally.
Do not create, enable, or dispatch GitHub Actions workflows for this project.
Keep Actions disabled in the active `MadMatt341/Stitch` source repository.
GitHub remains the host for Git repositories and public release files.

## Repository contract

| Path | Owner |
| --- | --- |
| `src/` | Active Rust application and library source. |
| `tests/` | Active integration tests. |
| `scripts/` | Build and native UI check procedures. |
| `docs/` | Full contracts and contributor procedures. |
| `build/` | Generated application, runtime, Cargo output, and test output. |
| `.reader-data/` | Ignored download cache and temporary reference files. |
| Root files | Manifests, configuration, contributor guidance, and the user introduction. |

Do not commit generated output, runtime binaries, or test video files.
Keep the standard Rust `src/` and `tests/` folders.
Do not add project-name folders for this one application.
Add a nested `AGENTS.md` only when a directory has different rules.
Do not duplicate the parent rules.
The [development reference](docs/development.md#source-and-output) owns the full folder contract.

## Read for the task

Start with the applicable reference in this task map:

| Task | Owning reference | Source and check entry points |
| --- | --- | --- |
| List order, selection, and repeated files | [List state](docs/ui.md#list-state) | `src/queue.rs`, `src/ui.rs`, queue unit tests. |
| Mouse, keyboard, row drag, painting, and window appearance | [Input and window](docs/ui.md#input-and-window) | `src/ui.rs`, `scripts/test-ui.ps1`. |
| File picker and focus | [File dialogs](docs/ui.md#file-dialogs) | `src/picker.rs`, `src/ui.rs`, interactive UI checks. |
| Saved state and name migration | [Saved state](docs/ui.md#saved-state) | `src/ui.rs`, `scripts/test-ui.ps1`. |
| Metadata, thumbnails, and export compatibility | [Media selection](docs/media.md#media-selection) | `src/media.rs`, media unit tests, `tests/media_flow.rs`. |
| Source locks, output publication, and cancellation | [Output protection](docs/media.md#output-protection) | `src/media.rs`, `src/ui.rs`, `tests/media_flow.rs`. |
| Builds, dependency pins, and runtime packaging | [Build](docs/development.md#build) | `Cargo.toml`, `Cargo.lock`, `.cargo/config.toml`, `scripts/build.ps1`. |
| Public source, licenses, and downloads | [Public downloads](docs/releases.md) | `LICENSE`, `.github/runtime/`, `scripts/check-runtime-candidate.ps1`, `scripts/package-release.ps1`. |
| Checks and their environment limits | [Checks](docs/development.md#checks) | `tests/media_flow.rs`, `scripts/test-ui.ps1`, source unit tests. |
| Documentation changes or reference moves | [Documentation rules](docs/documentation.md) | `AGENTS.md`, the applicable owning reference. |

Read the full applicable sections, their conditions, and their source and test entry points.
For changes across components, read each applicable owner.
For a small text correction, read only its related context and the writing rules.
When the necessary implementation and check information is available, stop reading more references.

## Change and verification

Keep queue identity separate from filenames.
Repeated files are separate list entries.
Keep state changes on the UI thread.
Send media results from worker threads through the existing event channel.
Show pointer feedback immediately.
Keep drag display separate from the committed list order until mouse release.

After source changes, use the [development checks](docs/development.md#checks) from the project root.
Use the media acceptance check for media changes.
Use the native UI check for UI changes.
Do the applicable interactive checks for physical input and file-dialog appearance.
Record checks that you did not complete.
Native messages do not prove physical input or each displayed file-picker frame.

Change the owning reference when you change its contract.
Correct related README text in the same change.
Keep dated results separate from implementation guidance.
Keep raw logs and generated reports in ignored output folders.

## Documentation rules

Policy version: `DocumentationRules` 0.1.1.
Write all new and changed documentation in ASD-STE100 Simplified Technical English, Issue 9.
Use the installed `documentation-rules` skill when a task uses or changes project documentation.
Obey the [reading and writing rules](docs/documentation.md).

Keep contracts, conditions, and checks at their owners.
Keep long dated evidence in linked references.
Keep the user's selections and approval scope.
