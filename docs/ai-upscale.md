# Experimental AI upscaling

## Decision

Add an opt-in `Anime4k` upscale mode using the pretrained Anime4K v4.0
3DGraphics Upscale x2 US CNN. Standard remains the default. No frame generation,
network access, Python, model installer, CUDA or external inference runtime.

The user requested sharper console gameplay while preserving the game's style,
on macOS and Windows 11. This is spatial reconstruction, not generative restyling.
It can still strengthen edges and capture artifacts; quality needs comparison on
real games. The experimental label remains until hardware and gameplay validation.

## Alternatives

- [dlss5-for-all](https://github.com/pcdofafa/dlss5-for-all): Windows-only
  integration; source redistribution restrictions and external NVIDIA components.
  No source or binary from this project is incorporated.
- [Real-ESRGAN](https://github.com/xinntao/Real-ESRGAN): permissive pretrained
  restoration models, but substantially heavier runtime and inference. The
  portable CLI is not a suitable per-frame process for low-latency gameplay.
- [FSRCNNX](https://github.com/igv/FSRCNN-TensorFlow): compact pretrained shader
  networks; mixed GPL/MIT source licensing needs further review before bundling.
- [Anime4K](https://github.com/bloc97/Anime4K): MIT, tiny pretrained 3D CNN,
  direct GPU execution. Selected for this bounded first experiment. This is not
  a claim that it matches DLSS, Real-ESRGAN or game-engine temporal upscaling.

## Provenance and implementation

Pinned commit: `7684e9586f8dcc738af08a1cdceb024cc184f426`.

[Exact source](https://github.com/bloc97/Anime4K/blob/7684e9586f8dcc738af08a1cdceb024cc184f426/glsl/Upscale/Anime4K_3DGraphics_Upscale_x2_US.glsl)
is retained in `assets/models/anime4k-3d-x2-us.glsl`, including its MIT notice.
A deterministic development-time generator ports the learned coefficients to
WGSL. The compiled app embeds the WGSL, so inference is available offline.

Capture and A/V pacing remain unchanged. The existing GPU renderer grades the
source, runs three convolution passes at source resolution with signed floating
point feature textures, reconstructs a 2x image by depth-to-space, then samples
that image to the selected output size. RGB receives the learned scalar residual;
alpha is preserved. Metal and DX12 execute the same WGSL through wgpu.

No worker or growing frame queue is introduced: this small CNN is part of the
existing GPU command stream. Pipelines initialize lazily when selected. Repeated
draws of the same frame and recipe reuse the existing output. A failed
initialization or unsupported configuration keeps the standard video path and
shows a warning. Surface loss retains the existing surface reconfiguration;
GPU device-loss recovery is unchanged and has not been validated here.

## Bounds and controls

- Source area at most 1920 × 1080 pixels; intermediate sides must fit GPU limits.
- Neural reconstruction is fixed at 2x; requested output must increase both axes
  and may not exceed 2x on either axis.
- Native-sized, downscaled and mixed-axis output uses the standard path.
- Auto uses physical viewport dimensions. QHD/UHD explicitly request processing
  sizes while preserving source aspect ratio.
- Comparison and original controls bypass the CNN immediately.
- Screenshot export uses the same deterministic model and effective recipe.
- No extra FPS is generated and 60 FPS is not guaranteed.

## Delivery and validation

1. Add the mode and UI explanation without changing old settings defaults.
2. Port the pinned model and integrate lazy GPU resources and fallback.
3. Compare GPU output against an independent CPU interpretation of upstream
   coefficients; cover borders, odd dimensions, flat fields and color edges.
4. Check resize/mode changes and live/export parity; compile Windows library.
5. Benchmark persistent 1080p to 4K GPU processing and run native window smoke.
6. Keep real capture/gameplay and Windows hardware performance as separate
   validation, not inferred from compilation or synthetic input.

## Validation results (2026-11-19)

- Independent review matched all 576 matrix coefficients and 12 biases to the
  pinned GLSL source. No blocking code defects found.
- Metal on Apple M2 Pro: numerical reference, flat fields, patterned detail,
  alpha, odd dimensions, fractional scaling, resize/mode changes, export,
  limits, fallback and original bypass passed.
- Full checks: formatting, reproducible shader, strict Clippy, 149 Rust tests
  and 6 Node tests passed. Seven hardware/manual Rust tests remain ignored by
  default; the new manual benchmark was run explicitly.
- Benchmark: 40 warmed 1080p to 4K frames, median 5.20 ms, p95 6.38 ms.
  Measures upload + command encoding + GPU completion; no readback. It excludes
  capture, presentation, display latency and shader initialization. Test-profile
  measurement on one Mac, not a universal FPS guarantee.
- Optimized native preview: 599 presented synthetic frames, effective Anime4k
  at 3840 × 2160, no fallback warning or playback readback. Borderless,
  fullscreen and windowed transitions passed.
- Windows x86_64 GNU library cross-check passed. This is not Windows hardware
  validation or an MSVC installer build.
- Local ad-hoc signed Mac preview packaged separately from the installed app.
  Real console/gameplay quality and Windows GPU speed remain unvalidated.
  Release publication requires a successful Native builds run for both platforms.

