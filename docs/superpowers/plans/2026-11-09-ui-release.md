# Console Link UI and Release Implementation Plan

> **For agentic workers:** Execute with explicitly owned UI and packaging work, then independent verification. Track steps using checkboxes.

**Goal:** Deliver a professional UI and public downloadable release with Windows installer.

**Architecture:** Preserve native capture/audio and egui/wgpu. Isolate visual components; use per-user Inno Setup and manually dispatched GitHub release workflow.

**Tech Stack:** Existing Rust/egui stack, Inno Setup, GitHub Actions and Releases.

**Spec:** ../specs/2026-11-09-ui-release-design.md

## Global Constraints

- Preserve macOS arm64 and Windows 11 x64 support, all current media controls and clean game window.
- Black/white gamepad icon remains. UI palette is author's monochrome proposal.
- No credentials in source/logs. No compulsory Windows admin install.
- Physical capture and Discord acceptance remain explicitly unverified.

## Review Focus

- Narrow windows must not hide primary action or make required fields inaccessible.
- Restyling must preserve audio controls and state changes during startup.
- Duplicate/unavailable devices and reconnect must remain explicit.
- Installer upgrades use a stable AppId; uninstaller removes only installed app files.
- Release tag/version/commit must match the files being published.

### Task 1: UI

Owner: src/app.rs, src/main.rs, new presentation modules/fonts. Preserve media lifecycle. Build cards, navigation and typography, visually inspect actual renderer output, and verify empty/connected/error/compact states where practical. Check compile, Clippy and smoke launch.

- [x] Redesign controls and theme.
- [x] Produce and inspect UI snapshots.
- [x] Check behavior against existing controls and startup/stop rules.

### Task 2: Packaging

Owner: packaging/windows.iss, scripts/package-windows.ps1, .github/workflows, release notes. Build per-user installer, portable archive and CI install/uninstall smoke. Workflow validates requested tag against Cargo version; assets are published only after successful builds.

- [x] Implement Windows installer and package script.
- [x] Implement manual release workflow and asset checksums.
- [ ] Validate installer on Windows CI.

### Task 3: Integration and delivery

Owner: root, Cargo version, Info.plist, README and validation docs. Bump0.2.0; independent reviewer checks presentation/state and packaging/version consistency. Run existing suite, fix material defects, publish source, dispatch release and follow to completion. Read back release assets and checksums.

- [x] Bump versions, update docs, integrate and independently review.
- [ ] Run tests/lints/native smoke and publish source.
- [ ] Dispatch release, inspect native CI results and public release assets.
