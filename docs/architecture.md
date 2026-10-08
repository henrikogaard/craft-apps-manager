# Architecture and repository layout

Craft Library is a macOS-only Rust application. `src/main.rs` dispatches headless operations or starts the UI. `src/web_ui.rs` owns the Tao window, local Wry webview and validated IPC. `src/native_menu.rs` provides the macOS menus and About panel. `src/web/index.html` contains the bundled interface; it uses no frontend server or downloaded scripts.

## Backend boundaries

- `model.rs`: official catalog, settings, library paths and installed records.
- `network.rs`, `updates.rs`, `hourly.rs`: GitHub downloads, source archives and optional availability checks.
- `macos_build.rs`, `builder.rs`, `tools.rs`: native bundle recipes, build provenance, history and prerequisites.
- `installers.rs`, `apps.rs`, `backups.rs`: DMG verification, app operations and managed backups.
- `files.rs`, `platform.rs`, `jobs.rs`: bounded filesystem operations, macOS services and cancellable child processes.
- `scheduler.rs`: optional launch agents. `self_update.rs` and `native/sparkle.m`: main-thread Sparkle controller and busy-operation relaunch deferral.

## Source tree

```text
src/                 Rust application and backend modules
src/web/             Embedded HTML, CSS and JavaScript
assets/              Manager icon, upstream app icons, fonts and notices
tests/               Backend workflow tests
docs/                macOS setup, architecture and review evidence
scripts/             macOS bundle packaging
.github/workflows/   macOS checks and universal package artifact
```

Generated packages live in ignored `dist/`. User libraries, archives and caches are outside the checkout. GitHub app repositories are fixed by the catalog; the manager repository override applies only to manager updates.

## Verification boundaries

Release installation verifies the GitHub asset digest, bundle identity, code signature and Gatekeeper result. Source builds verify the archive checksum and exact app commit, run the upstream recipe, inspect the resulting bundle and record provenance. Build prerequisites and upstream compilation can still fail. Source builds execute official upstream build code and are locally ad-hoc signed.

The webview accepts only bundled content, blocks remote navigation and sends a bounded JSON message to Rust. Backend handlers validate app IDs and operation names; download URLs come from GitHub metadata, not frontend input. Managed filesystem operations reject traversal and symlink boundaries. Native updates retain rollback and provenance metadata.
