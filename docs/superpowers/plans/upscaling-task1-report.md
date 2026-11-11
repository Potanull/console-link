# Spatial GPU core — Task 1 evidence

Implemented 2026-11-11. No commits or releases.

## Integration contract

`console_link::upscale` exports serde/default/Copy/PartialEq `UpscaleMode::{Standard,Nis,Fsr1,RtxVideo}` and `UpscaleSettings { mode, nis_sharpness, fsr_sharpness }`. Defaults are Standard, 0.3, 0.3. `normalize()` clamps finite sharpness to 0..1 and replaces nonfinite values with 0.3. `effective_mode(source:[u32;2],target:[u32;2])` bypasses unless both target dimensions exceed nonzero source dimensions.

`app::upscale_gpu` is registered. Its sibling-visible interfaces are:

```rust
const MAX_TARGET_DIMENSION: u32 = 8192;
fn paint_enhanced(
    ui: &egui::Ui, rect: egui::Rect, frame: Arc<VideoFrame>,
    color: ColorSettings, original: bool,
    settings: UpscaleSettings, target: [u32; 2],
) -> Result<(), String>;
fn render_export(
    device: &wgpu::Device, queue: &wgpu::Queue,
    frame: Arc<VideoFrame>, color: ColorSettings,
    settings: UpscaleSettings, target: [u32; 2],
) -> Result<Vec<u8>, String>;
```

Keep the existing `color_gpu::paint` interface. `color_gpu::install` installs spatial resources on the existing wgpu device. `paint_enhanced` validates frame/target and rejects unavailable effective RTX or NIS beyond upstream's 2x-per-axis limit. UI must report the error and deliberately submit Standard fallback. Native/downscale/mixed-axis resizing directly calls existing single-pass color presentation. The conservative playback ceiling matches wgpu's requested default limit; export checks the actual device limit. The caller supplies physical video-only target pixels, including DPI/aspect-fit decisions.

Enhanced playback uses source-resolution color grade -> NIS or EASU -> optional RCAS -> presentation. Intermediates are gamma-encoded RGBA8 Unorm; final presentation decodes only for an sRGB surface. No implicit algorithm chaining. NIS uses its directional 6-tap filter banks, edge map, contrast-limited sharpening, luma reconstruction and bilinear chroma. FSR uses the 12-tap edge-adaptive EASU kernel, vendor bit-level reciprocal approximations and separate RCAS. Sharpness maps to upstream NIS slider; FSR maps 0..1 to RCAS 2..0 attenuation stops.

Textures resize only with the corresponding dimensions. Source upload remains guarded by retained owning Arc identity in color resources. Identical normalized settings, color, original flag, target and Arc reuse the completed enhanced output on overlay redraws. NIS coefficients live in one persistent 4096-byte uniform bank. No playback readback or CPU pixel transform.

`render_export` must run on the screenshot worker. It creates isolated resources on the same device/queue, uses the identical reconstruction shaders, copies only on explicit export, waits for its own submission/map on that worker, strips row padding and returns gamma RGBA8 bytes. The caller owns the frozen frame/settings/target recipe and PNG encoding. Resize/mode changes in playback cannot overwrite export resources. The result must be saved with neutral CPU color settings to avoid grading twice. Standard export uses the same existing single-pass color shader as Standard playback.

Ruling: Standard/native/downscale retain the existing single pass and color-before-display behavior instead of adding source-resolution intermediate passes. This preserves default playback cost and existing output; source-resolution grading is used for enhanced spatial modes. Requested by coordinating implementer after profiling review.

## Provenance and reference generation

- NVIDIA Image Scaling v1.0.3: [official repository](https://github.com/NVIDIAGameWorks/NVIDIAImageScaling/tree/35e13ba316c98eeecf16f37eae70ce88019911f6), pinned `35e13ba316c98eeecf16f37eae70ce88019911f6`.
- FidelityFX FSR 1: [official repository](https://github.com/GPUOpen-Effects/FidelityFX-FSR/tree/a21ffb8f6c13233ba336352bdff293894c706575), pinned `a21ffb8f6c13233ba336352bdff293894c706575`.

Original NIS_Scaler.h, NIS_Config.h, ffx_fsr1.h, ffx_a.h and MIT licenses are retained under `src/app/vendor`, with THIRD_PARTY attribution. `generate_reference.py` extracts the original upstream function bodies into `reference.cpp`, adapts shader vector/texture intrinsics and out/inout parameter syntax for C++, and runs that CPU harness. It does not use WGSL or the Rust GPU implementation. `CXX=g++ python src/app/vendor/generate_reference.py` regenerates reference.cpp, coefficient bytes and fixtures; g++ is required only for regenerating fixtures, never app builds. NIS's original GetEdgeMap array stride is adapted from 4 to 6 solely to accept the same 6x6 support array; indexing/equations remain upstream.

Reference input is 17x13 composite grayscale ramps, horizontal/vertical/diagonal colored boundaries, checker texture and varied RGB details, output 29x21. Sharpness 0, 0.3 and 1 are tested. EASU intermediate is quantized to RGBA8 before upstream RCAS, matching playback. Flat black/white and saturated-channel tests separately cover zero peak denominators: the WGSL RCAS limiter explicitly defines those channel contributions as zero to prevent NaN; no denoise option is enabled.

## Actual GPU checks

Adapter: NVIDIA GeForce RTX 4080, DX12, driver `32.0.16.1088`.

`cargo test --target x86_64-pc-windows-gnu --offline real_gpu_ -- --nocapture` passed all three GPU tests (including existing native color parity). Spatial tests assert actual GPU adapter availability rather than silently skip.

| Reference | Sharpness | Maximum byte error | Mean byte error |
|---|---:|---:|---:|
| NIS | 0 | 1 | 0.0489 |
| NIS | 0.3 | 1 | 0.0468 |
| NIS | 1 | 1 | 0.0484 |
| FSR EASU + RCAS | 0 | 1 | 0.0291 |
| FSR EASU + RCAS | 0.3 | 2 | 0.0431 |
| FSR EASU + RCAS | 1 | 4 | 0.0673 |

EASU independently differs by at most 1/255. RGBA8 roundoff can amplify through maximum RCAS up to gain 4; accepted full-sharpness tolerance is 4/255 with mean below 0.1/255. Other final comparisons use max 2/255 and mean below 0.1/255. Initial max-2 full-sharpness reference assertion failed; independent EASU validation established the quantization amplification and justified the explicit final bound. This is a CPU upstream-reference comparison, not a claim of bit-identical output to every vendor GPU compiler.

Additional assertions: flat RGB constants within 1 byte, finite black/white/saturated output, native/downscale/mixed-axis Standard bypass, intermediate texture reuse, source size/target validation, unsupported RTX rejection, and nonneutral color + each spatial mode -> odd-width 29x21 PNG -> exact RGBA bytes/dimensions/sRGB tag. Settings/default/geometry tests live in `src/upscale.rs`. Strict `cargo clippy --target x86_64-pc-windows-gnu --offline --all-targets -- -D warnings` passed; touched Rust files formatted with Rust 1.99 rustfmt.

## Warm synthetic component benchmark

Command: `cargo test --target x86_64-pc-windows-gnu --offline benchmark_spatial_1080p -- --ignored --nocapture`.

1920x1080 ramp/checker source -> 2560x1440; three distinct preallocated frame Arcs rotate so unchanged-frame caching cannot bypass processing. Persistent pipelines/textures; five warm batches then thirty measured batches of four submissions, completion wait after each batch, no readback. Grade-on uses exposure/contrast/saturation adjustments. Values are per-frame batch component wall time, including source upload, CPU command preparation, submission and completion. They are neither isolated timestamp-query GPU duration nor HDMI/display latency.

| Mode | Grade | Median ms/frame | p95 ms/frame |
|---|---|---:|---:|
| Standard | off | 0.625 | 0.757 |
| Standard | on | 0.637 | 0.777 |
| NIS | off | 0.840 | 0.973 |
| NIS | on | 0.852 | 0.989 |
| FSR 1 | off | 0.863 | 0.961 |
| FSR 1 | on | 0.841 | 0.973 |

The first NIS prototype was ~24 ms because dynamically indexing a shader const coefficient array materialized excessive per-fragment state. Replacing that storage with a persistent uniform coefficient bank retained identical reference agreement and removed the regression. No threshold guarantee is inferred for other hardware or actual capture delivery.

## Limits and handoff

RTX is intentionally rejected pending genuine SDK/runtime integration. No RTX implementation, GPU device-loss recovery, physical HDMI playtesting or isolated GPU timestamp benchmark is claimed. UI/settings/frozen-recipe integration and whole-change release checks are the coordinating implementer's tasks. No new runtime dependencies, history queue, automatic commit or publishing.
