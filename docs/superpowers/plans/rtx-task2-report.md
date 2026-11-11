# RTX Video Task 2 implementation evidence

Date: 2026-11-12. SDK: official NVIDIA RTX Video SDK 1.1 supplied by the user. Target hardware: NVIDIA GeForce RTX 4080, DX12, driver 32.0.16.1088. No commit or release was created by the implementation worker.

## Implemented interface and behavior

- `src/app/upscale_gpu.rs::rtx_status(&egui_wgpu::RenderState) -> Result<(), String>` lazily loads the runtime, queries actual `VSR.Available`, and creates the SDK feature. The UI can call this only when RTX is requested. Missing runtime, unsupported GPU/driver, initialization and subsequent evaluation failures return actionable status.
- `src/app/rtx_video.rs` is Windows-only. It uses the SDK's documented C API through the small `console_link_ngx.dll` bridge. The bridge statically links the official SDK dispatch library; it does not call private exports of `nvngx_vsr.dll`.
- `UpscaleSettings.rtx_quality` reaches `VSR.QualityLevel`, clamped to 1–4. Default 2 is the SDK's Medium level. No NIS/FSR sharpening is chained onto RTX.
- Playback evaluates the source-resolution graded texture into the target-sized RGBA8 UAV texture. Export uses this same path on isolated textures and returns an error if RTX failed; it never labels a fallback export as RTX.
- If runtime evaluation fails, the current playback callback renders graded Standard output and stores the error. Subsequent status queries expose that error to the UI. Ordinary Standard/NIS/FSR paths do not initialize the SDK.

## ABI and build verification

The worker read the actual supplied `nvsdk_ngx.h`, `nvsdk_ngx_params.h`, `nvsdk_ngx_defs.h`, `nvsdk_ngx_defs_vsr.h`, `nvsdk_ngx_helpers_vsr.h`, `CDx12NGXVSR.cpp`, and programming guide. It also read the installed wgpu 27.0.1 / wgpu-hal 27.0.4 source, rather than assuming current online APIs.

The public SDK `NVSDK_NGX_D3D12_Init` has five arguments: application ID, writable log path, device, optional feature-common-info pointer, SDK version. The different four-argument `NGX_SNIPPET_BUILD` declaration is not the public client API. Native execution caught an initial branch-selection ABI error; the corrected five-argument call passes null feature info and version 0x15. App ID 0 follows the guide until NVIDIA assigns an application-specific ID.

The SDK ships MSVC libraries. GNU app linkage remains unchanged except for an alias of the already resolved windows 0.58 dependency, matching wgpu-hal's COM types. Root built the bridge using official Microsoft linker/CRT packages in a temporary directory; no system installer or proprietary SDK source was added to the repository. Root owns the reproducible bridge/package script and license documentation.

Only the release SDK VSR DLL was used. SHA-256 of `bin/Windows/x64/rel/nvngx_vsr.dll`:

`c3d88eea5ff7a548edefa66414cf6e77464d0947277c904f324dd23abf58a1ed`

The bridge and release DLL were placed adjacent to test executables. DLL loading uses an absolute executable-directory bridge path with restricted Windows loader search flags. The normal missing-runtime path is an explicit error; there is no automatic SDK download.

## Queue, state, lifetime and concurrency

Ruling: wgpu-hal 27 DX12 exposes raw device, queue and resource access, but no public raw command-list getter. A separate native command list on the **existing wgpu command queue** avoids a wgpu fork and follows the supplied SDK's DX12 model.

1. A dedicated wgpu command encoder grades the source, initializes the output in wgpu's tracking, and transitions both textures using `TextureUses::PRESENT`, which wgpu-hal DX12 maps to `D3D12_RESOURCE_STATE_COMMON`.
2. That encoder is submitted through wgpu. The native list evaluates VSR on the same queue, after the grading submission. The SDK 1.1 DX12 sample uses COMMON input/output resources and manages internal UAV transitions. The output has `STORAGE_BINDING`, providing the required UAV allocation flag.
3. The later egui/render/export command buffer transitions from the tracked COMMON state to shader sampling or copy source. Native list state cannot invalidate an existing wgpu render list because it is a separate list.
4. COM resource references are cloned and the wgpu HAL guard is released before acquiring another guard. This avoids wgpu's device-wide recursive snatch-lock restriction.
5. All NGX calls are serialized by a process-wide SDK mutex. A weak cache shares a session per native device between playback and export; each call additionally holds that session's mutex. The three native allocator/list slots are reset only after their fences complete.
6. Each submitted slot retains full wgpu texture handles, not only COM pointers, preserving the allocator allocation through native execution and resize. Session shutdown waits for native completion, then releases feature, parameters and SDK state. On a GPU timeout it retains in-flight slot allocations and the loaded SDK module until process exit instead of freeing memory still in use.

There is no new capture queue, CPU pixel transform, playback pixel readback, or added frame accumulation. Slot fences concern native command allocator reuse. Normal scheduling does not wait per frame; an occupied slot waits only when its previous work has not completed. Initialization and shutdown wait for their own native work.

## Tests actually run

The baseline `real_gpu_rtx_evaluation` was first run and failed with `RTX Video SDK is unavailable` on the real RTX 4080. After implementation:

- `real_gpu_rtx_evaluation`: passed genuine SDK evaluation for 1920×1080 → 2560×1440 constant input, RGB values within 8/255 and every alpha byte 255.
- `real_gpu_rtx_pattern_resize_quality_and_concurrent_export`: passed all SDK qualities 1–4, alternating 640×360 → 1280×720 / 960×540 output, then 12 alternating-size submissions while a separate thread exports a frozen frame. Concurrent export was byte-identical to a subsequent isolated export.
- Pattern output differed from Standard by more than 2/255 in 2,297,648 channels at quality 1; 1,174,349 at quality 2; 2,129,040 at quality 3; 1,119,085 at quality 4. This verifies that the output is actually processed and is not a bilinear placeholder.
- `rtx_error_falls_back_to_graded_standard`: passed injected runtime failure, explicit error status, effective Standard mode, and pixel agreement with the graded Standard export within 1/255.
- `missing_runtime_has_explicit_error`: passed independent of installed SDK.
- `sdk_failure_codes_are_not_success`: passed actual SDK failure-bit classification.
- Updated the older spatial regression's obsolete assertion that all RTX validation must fail; geometry validation now accepts RTX and capability probing happens separately.

Vendor/hardware tests are explicitly ignored in normal CI and were run here with `--ignored --nocapture --test-threads=1`. Actual unsupported hardware and physical GPU removal/device loss were not available to test. No claim is made that an injected failure is an unsupported-GPU hardware test. Root owns the final whole-application suite, Clippy, formatting and release checks after integration.

## 1080p → 1440p measurements

Synthetic warmed benchmark: five warmup batches plus 30 measured batches of four frames, source upload and actual GPU execution, no pixel readback. Native timestamp queries bracket only the SDK VSR evaluation; timestamp collection exists only in test builds and reads 16 bytes of timing metadata after fences. These are component measurements, not capture-card or HDMI-to-display latency.

| Mode | Grading | Component wall median / p95 (ms/frame) | VSR-only GPU median / p95 (ms) |
|---|---|---|---|
| Standard | Off | 0.648 / 0.872 | — |
| Standard | On | 0.654 / 0.729 | — |
| NIS | Off | 0.824 / 0.971 | — |
| NIS | On | 0.828 / 0.938 | — |
| FSR 1 | Off | 0.837 / 0.976 | — |
| FSR 1 | On | 0.876 / 1.014 | — |
| RTX Medium (2) | Off | 1.902 / 1.982 | 1.232 / 1.675 |
| RTX Medium (2) | On | 1.960 / 2.214 | 1.267 / 1.708 |

Each VSR GPU distribution contains 120 warmed samples. Component wall includes upload, CPU recording, submission and completion wait; it must not be interpreted as pure GPU latency. No extrapolation to other GPUs or input videos is implied.

## Review state

Root dispatched an independent reviewer. Initial native review found no queue/lifetime blocker and requested opaque-alpha validation; that assertion was added and passed. Final reviewer conclusions and full integration gates belong in the root execution ledger.

## Reproduction commands and final integration gates

The portable-toolchain wrapper used for compilation was:

```powershell
& 'C:/Users/gamos/.codex/visualizations/2026/11/11/01a1cbcc-4c7f-7921-a2a9-092befd122a5/console-link/upscale/check.ps1' test rtx
```

The actual native GPU checks and benchmark were explicitly executed from the newly built test binary (the hash may change in a subsequent build):

```powershell
& 'D:/Code/console-link/target/debug/deps/console_link-307c8c79560faebb.exe' real_gpu_rtx --ignored --nocapture --test-threads=1
& 'D:/Code/console-link/target/debug/deps/console_link-307c8c79560faebb.exe' benchmark_spatial_1080p_to_1440p --ignored --nocapture --test-threads=1
```

Root subsequently reported all 120 normal tests passing, strict Clippy clean, release build success, and an inspected actual RTX overlay screenshot showing effective RTX and quality controls. Independent review was clean after the alpha test addition. Physical HDMI latency and unsupported-device/device-loss hardware testing remain outside this evidence.
