# Capture upscaling design — 2026-11-11

Status: approved by user in chat on 2026-11-11; implementation in progress.

## Approved intent
Improve Full HD console capture on a 2560x1440 monitor. Prioritize smooth gameplay and low added latency; Windows NVIDIA RTX is the current target. Implement selectable NVIDIA Image Scaling (NIS), AMD FSR 1 (EASU + RCAS), and actual NVIDIA RTX Video Super Resolution. Include processed/upscaled screenshots and original-frame export. Do not chain upscalers. No game-engine DLSS, temporal FSR, frame generation or claims of recovered native detail.

## Approaches
Bilinear sampling is today's baseline. NIS combines spatial reconstruction and sharpening; FSR 1 reconstructs edges then applies RCAS. Both operate on completed captured images without game metadata. RTX Video uses NVIDIA's video model and requires native SDK/runtime integration. User chose all three instead of selecting only one spatial algorithm. Exact upstream algorithms must be used with licenses preserved; a generic sharpening shader must never be labelled NIS, FSR or RTX Video.

## Proposed product behavior
Add an Upscaling group to Image settings and the F1 overlay: Standard (bilinear), NIS, FSR 1, RTX Video. Standard remains the migration/default mode until the user enables enhancement. Persist selected mode and per-mode supported parameters. NIS/FSR expose a bounded sharpness control with moderate defaults; RTX exposes only controls verified in the actual SDK. No automatic algorithm switching. Show requested and effective mode plus actionable availability errors; on runtime failure use Standard with an explicit status, without stopping capture.

Use the actual video viewport in physical pixels, including DPI and aspect-fit letterboxing, rather than hardcoding 1440p. Apply enhancement only when both viewport dimensions exceed the source; otherwise retain ordinary display sampling and show why upscaling is inactive. Source dimensions, target dimensions and active mode are visible. Preserve aspect ratio and existing overlay bounds. Resizing or mode changes recreate only affected resources. Main preview can use a labelled test pattern; gameplay uses the live image. A temporary comparison switch shows the normal bilinear image at the same size, without overwriting saved settings or disabling unrelated color correction.

## Pipeline and ownership
Keep existing capture/pacing/newest-frame policy and one upload per changed Arc frame. No CPU processing or GPU-to-CPU readback in routine playback, no new history queue. Color-grade SDR at source resolution, then run the selected algorithm in its required display-referred color space; final presentation handles sRGB conversion once. Existing artistic HDR effect remains SDR. Do not enable RTX Video HDR as part of this feature.

Spatial algorithms integrate with wgpu 27 via vetted WGSL ports of upstream shaders, reference constants and coefficient tables. Preserve vendor attribution and test against upstream reference results; algorithm-specific intermediate textures are reused. SDK-backed RTX Video requires access to the same DX12 device/resources or a documented supported interop path, explicit barriers/lifetimes, and completion synchronization without CPU stalls. Do not guess ABI signatures or use undocumented driver hooks. Implementation choices (module split, shader dispatch layout, bounded parameter defaults) belong to the builder and must be supported by measurements.

## RTX dependency gate
Official RTX Video SDK 1.1 developer download redirects to NVIDIA Developer login. NVIDIA NGC also publicly lists the SDK archive, but both anonymous API access and the catalog download route returned HTTP 403 from this environment; public listing alone does not prove a login is universally required. Published requirements: Windows 10 64-bit+, GeForce RTX 20-series or newer; APIs DX11/DX12/Vulkan/CUDA. Obtain genuine headers/libraries/sample/docs using the user's authorized download; inspect runtime redistribution terms before packaging. No credentials in code/logs. A disabled option is not completion of the requested RTX feature. Until SDK access is resolved, spatial work can proceed but RTX remains explicitly incomplete. Reassess the native integration once the real SDK contract is available; surface any material design change before implementation of that part.

## Screenshots
Keep original export at capture resolution and without grading/upscaling. Default processed export matches the video-only frame/settings/output-size pair most recently submitted for display; when comparison is active, export its effective Standard image. Export no UI or letterbox bars. Use the same GPU processing path, with readback only on explicit export, asynchronous completion and background PNG encoding. Bound pending exports (one at a time) and report busy/error state. Snapshot source Arc, grade, mode, parameters and physical target size together. Preserve existing exclusive filenames and sRGB PNG tagging; no overwrites. Handle resize/mode change while export is pending without reusing overwritten resources.

## Verification and acceptance
Tests cover legacy settings/defaults, enum/parameter validation, mode availability, DPI/viewport sizing, aspect ratios, resize, 1:1/downscale bypass, source changes and comparison. Validate shaders on actual DX12 GPU. Compare NIS/FSR output against upstream references on edges, fine textures, gradients and flat colors, with documented tolerance. Test real PNG dimensions/color/selected-mode parity and original preservation. Check runtime failure fallback and resource lifetime while changing modes/capture resolution/exporting.

Measure warm GPU processing and frame delivery consistency for 1080p to 1440p with grading on/off for every available mode; report median/tail timings and adapter/driver, not invented HDMI latency. Aim for spatial processing below 2 ms on the previously tested RTX 4080, investigate regressions rather than guarantee an unmeasured threshold. RTX needs measured sustainable 60 FPS and explicit quality/latency reporting; slower modes remain user-selectable with honest metrics. Physical HDMI playtesting is distinct from synthetic GPU checks. Run fmt, strict Clippy, all tests, release build and narrow-overlay UI checks. No automatic release/commit is included in this request.

## Sources
- https://github.com/NVIDIAGameWorks/NVIDIAImageScaling
- https://gpuopen.com/fidelityfx-superresolution/
- https://gpuopen.com/manuals/fidelityfx_sdk/techniques/super-resolution-spatial/
- https://developer.nvidia.com/rtx-video-sdk/getting-started
- https://developer.nvidia.com/downloads/rtx/sdk/rtx_video_sdk_v1.1.0.zip (login required)
