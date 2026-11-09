# Console Link Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans for integration, with explicitly owned independent platform adapters. Steps use checkbox syntax for tracking.

**Goal:** Build and publish a compact native Rust capture-card application for macOS and Windows 11.

**Architecture:** One process owns two egui windows, native video capture and native audio input/output. Bounded queues protect latency; optional local Discord IPC supplies manual game activity.

**Tech Stack:** Rust, eframe/wgpu, objc2 AVFoundation, windows-rs Media Foundation, CPAL native CoreAudio/WASAPI, serde JSON, PNG encoding.

**Spec:** ../../architecture.md

## Global Constraints

- macOS 13+ Apple Silicon and Windows 11 x64; no GStreamer/FFmpeg/WebView runtime.
- 1080p60 only when enumerated by the device; preserve rational FPS.
- Separate clean game window, output/volume/mute/meter/sync, screenshots, optional Discord presence.
- Never claim hardware acceptance or Discord receiving audio from a successful build.
- No secrets in repository. Public repository Potanull/console-link, MIT.

## Review Focus

- Missing/busy capture device must yield a useful error and leave UI responsive.
- Missing audio output must stop audio rather than select unexpected speakers.
- Capture callbacks from a previous generation must not enter the current session.
- Video correction -500 ms must retain timestamped history, bounded by bytes and time.
- Malformed settings or IPC must not crash playback or overwrite unknown settings schemas.

## File responsibilities and shared interfaces

`src/types.rs`: `VideoDevice`, rational `VideoMode`, owned RGBA `VideoFrame`, `FrameSink`, `CaptureSession`, `monotonic_ns()`.
`src/native/macos.rs` and `windows.rs`: `enumerate() -> Result<Vec<VideoDevice>, String>`; `start(&str, &VideoMode, FrameSink) -> Result<Box<dyn CaptureSession>, String>`; RAII stop.
`src/audio.rs`: native CPAL input/output, bounded buffers, controls, stats, drift correction.
`src/scheduler.rs`: bounded frame history and signed offset scheduling.
`src/settings.rs`: validated schema and atomic persistence.
`src/discord.rs`: bounded IPC presence worker.
`src/app.rs`: control UI, clean game viewport, generation and connection lifecycle.
`src/main.rs`: entry point and diagnostics flags. `scripts/`: package builds. `.github/workflows/`: native build matrix.

### Task 1: Native video adapters

- [ ] macOS: enumerate real formats and rational rates; open AVFoundation with BGRA, copy respecting stride into owned RGBA; stop via RAII. Native callback timestamps use shared monotonic delivery time and are explicitly not presented as source clock synchronization.
- [ ] Windows: Media Foundation enumerate native media types; Source Reader decoded RGB32, stride-aware BGRA conversion; capture on owned COM thread, stop and cleanup on Drop.
- [ ] Compile-check each target, including Windows cross-target where possible; runtime device listing on Mac. Device tests remain pending if card absent.

### Task 2: Audio and frame scheduling

- [ ] Before implementation, run tests for bounded audio ring, volume/mute/underrun, -500ms continuous video and schema validation; observe failure.
- [ ] Implement CPAL native input/output with preallocated lock-free sample ring. Select explicit devices; gain/mute/peak atomics; finite sample sanitization; adjustable audio delay and slow drift feedback with bounded ratio.
- [ ] Implement `FrameHistory::push(Arc<VideoFrame>)` and `select(now_ns, video_delay_ns)` with limits of time/bytes; return latest due frame. Check max delay against budget.
- [ ] Tests pass. Hardware timestamps and actual device latency are separately measured acceptance work.

### Task 3: Desktop flow and persistence

- [ ] Validated local schema, unknown future schema preserved, atomic writes. Test corrupt/invalid/future values.
- [ ] egui app: empty/start/error/running states, explicit video and audio device selections, separate viewport, mute/gain/peak/sync, reconnect only selected device, output loss handling.
- [ ] PNG screenshot worker copies displayed frame; no capture buffer retained, write failure reported without stopping capture.
- [ ] Optional Discord IPC off UI thread: manual game, details/elapsed time, clear/close, bounded lengths and reconnect. Missing Discord does not affect media.
- [ ] Unit tests plus `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, release build.

### Task 4: Delivery

- [ ] Native CI macOS/Windows; packaging `.app` with camera/microphone usage descriptions and Windows portable archive.
- [ ] README with exact implemented behavior, prerequisites, run/package instructions, manual hardware acceptance checklist and unresolved limits.
- [ ] Independent whole-repo review; fix material defects; repeat impacted checks.
- [ ] Publish public GitHub repository; read back commit/files and inspect CI completion. Never equate build success with real-call/hardware acceptance.

## Integration rulings

- User explicitly approved implementation; routine plan review does not add another permission gate. Independent adapter work uses authorized subagents, root owns integration.
- CPAL is a thin native audio abstraction (CoreAudio/WASAPI), not an external media runtime; use it to avoid duplicating risky realtime FFI. Its exact implementation is checked before final integration.
- First video slice may use measured CPU BGRA conversion and texture upload, explicitly permitted fallback in design; native interop/YUV shader is deferred until profiling.
- PNG crate replaces duplicate ImageIO/WIC glue; small encoder, no external codec runtime. Verify its license and footprint in Cargo.lock.
- Source callback host timestamps are an initial conservative baseline; do not claim full hardware source-clock synchronization until measured and implemented.
