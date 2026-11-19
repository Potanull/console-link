//! Numerical checks against the original pretrained GLSL, independent of the WGSL port.
use super::*;

const MODEL: &str = include_str!("../../assets/models/anime4k-3d-x2-us.glsl");

fn neural_settings() -> UpscaleSettings {
    UpscaleSettings {
        mode: UpscaleMode::Anime4k,
        ..Default::default()
    }
}

fn patterned_frame(width: u32, height: u32) -> Arc<VideoFrame> {
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            rgba.extend_from_slice(&[
                if x > y { 217 } else { 31 },
                ((x * 37 + y * 11) % 256) as u8,
                ((x * 7 + y * 59) % 256) as u8,
                ((x * 13 + y * 17) % 128 + 128) as u8,
            ]);
        }
    }
    Arc::new(VideoFrame {
        width,
        height,
        rgba,
        timestamp_ns: 0,
    })
}

fn numbers(text: &str) -> Vec<f32> {
    text.split(',')
        .map(|v| v.trim().parse().expect("Original model numeric literal"))
        .collect()
}

fn load_cpu(pixels: &[[f32; 4]], size: [usize; 2], x: i32, y: i32) -> [f32; 4] {
    let x = x.clamp(0, size[0] as i32 - 1) as usize;
    let y = y.clamp(0, size[1] as i32 - 1) as usize;
    pixels[y * size[0] + x]
}

fn bilinear_cpu(pixels: &[[f32; 4]], size: [usize; 2], x: f32, y: f32) -> [f32; 4] {
    let base_x = x.floor() as i32;
    let base_y = y.floor() as i32;
    let dx = x - x.floor();
    let dy = y - y.floor();
    let mut result = [0.; 4];
    for (ox, wx) in [(0, 1. - dx), (1, dx)] {
        for (oy, wy) in [(0, 1. - dy), (1, dy)] {
            let sample = load_cpu(pixels, size, base_x + ox, base_y + oy);
            for (out, value) in result.iter_mut().zip(sample) {
                *out += value * wx * wy;
            }
        }
    }
    result
}

/// Interpret the pinned GLSL weights directly. Intermediate CPU activations stay
/// f32; GPU uses f16 render targets, so comparison allows 2/255 rounding error.
fn original_model_reference(frame: &VideoFrame) -> Vec<u8> {
    let size = [frame.width as usize, frame.height as usize];
    let original: Vec<[f32; 4]> = frame
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| std::array::from_fn(|c| f32::from(p[c]) / 255.))
        .collect();
    let mut input = original.clone();
    let hooks: Vec<_> = MODEL.split("vec4 hook() {").skip(1).collect();
    assert_eq!(hooks.len(), 4, "Three convolutions and depth-to-space");
    for (layer, hook) in hooks[..3].iter().enumerate() {
        let body = hook.split("return result;").next().unwrap();
        let bias = numbers(
            body.split("result += vec4(")
                .nth(1)
                .unwrap()
                .split(')')
                .next()
                .unwrap(),
        );
        assert_eq!(bias.len(), 4);
        let mut output = vec![[0.; 4]; input.len()];
        for matrix in body.split("mat4(").skip(1) {
            let (weights, operation) = matrix.split_once(')').unwrap();
            let weights = numbers(weights);
            assert_eq!(weights.len(), 16);
            let activation = if operation.contains("* go_1(") { 1 } else { 0 };
            let offsets = numbers(
                operation
                    .split("* go_")
                    .nth(1)
                    .unwrap()
                    .split_once('(')
                    .unwrap()
                    .1
                    .split(')')
                    .next()
                    .unwrap(),
            );
            for y in 0..size[1] {
                for x in 0..size[0] {
                    let sample = load_cpu(
                        &input,
                        size,
                        x as i32 + offsets[0] as i32,
                        y as i32 + offsets[1] as i32,
                    );
                    for (channel, value) in sample.into_iter().enumerate() {
                        let value = if layer == 0 {
                            value
                        } else if activation == 1 {
                            (-value).max(0.)
                        } else {
                            value.max(0.)
                        };
                        for row in 0..4 {
                            output[y * size[0] + x][row] += weights[channel * 4 + row] * value;
                        }
                    }
                }
            }
        }
        for pixel in &mut output {
            for (value, bias) in pixel.iter_mut().zip(&bias) {
                *value += bias;
            }
        }
        input = output;
    }
    let mut result = Vec::with_capacity(frame.rgba.len() * 4);
    for y in 0..size[1] * 2 {
        for x in 0..size[0] * 2 {
            let mut pixel = bilinear_cpu(
                &original,
                size,
                x as f32 * 0.5 - 0.25,
                y as f32 * 0.5 - 0.25,
            );
            let residual = input[(y / 2) * size[0] + x / 2][(y % 2) * 2 + x % 2];
            // The application's intentional deviation from upstream: a learned
            // brightness residual changes RGB, never the source alpha channel.
            for channel in &mut pixel[..3] {
                *channel += residual;
            }
            result.extend(pixel.map(|v| (v.clamp(0., 1.) * 255.).round() as u8));
        }
    }
    result
}

#[test]
fn neural_gpu_matches_original_model_on_edges_and_odd_dimensions() {
    let (device, queue) = gpu();
    for size in [[9, 7], [3, 5], [1, 1]] {
        let frame = patterned_frame(size[0], size[1]);
        let doubled = [size[0] * 2, size[1] * 2];
        let reference = original_model_reference(&frame);
        let targets = if size == [9, 7] {
            vec![doubled, [13, 11]]
        } else {
            vec![doubled]
        };
        for target in targets {
            let expected = if target == doubled {
                reference.clone()
            } else {
                // Independently resample the quantized 2x model output. This
                // checks subpixel placement in the final non-integer resize.
                let pixels: Vec<[f32; 4]> = reference
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|p| p.map(|v| f32::from(v) / 255.))
                    .collect();
                let mut resized = Vec::with_capacity((target[0] * target[1] * 4) as usize);
                for y in 0..target[1] {
                    for x in 0..target[0] {
                        let sample = bilinear_cpu(
                            &pixels,
                            [doubled[0] as usize, doubled[1] as usize],
                            (x as f32 + 0.5) * doubled[0] as f32 / target[0] as f32 - 0.5,
                            (y as f32 + 0.5) * doubled[1] as f32 / target[1] as f32 - 0.5,
                        );
                        resized.extend(sample.map(|v| (v.clamp(0., 1.) * 255.).round() as u8));
                    }
                }
                resized
            };
            let actual = render_export(
                &device,
                &queue,
                frame.clone(),
                ColorSettings::default(),
                neural_settings(),
                target,
            )
            .unwrap();
            assert_eq!(actual.len(), expected.len());
            for (index, (&actual, &expected)) in actual.iter().zip(&expected).enumerate() {
                assert!(
                    actual.abs_diff(expected) <= 2,
                    "{size:?} -> {target:?}, channel {index}: GPU {actual}, original model {expected}"
                );
            }
        }
    }
}

#[test]
fn neural_gpu_preserves_flat_color_and_changes_patterned_detail() {
    let (device, queue) = gpu();
    for color in [[0, 0, 0, 255], [73, 113, 193, 255], [255, 255, 255, 255]] {
        let frame = Arc::new(VideoFrame {
            width: 7,
            height: 5,
            rgba: color.repeat(35),
            timestamp_ns: 0,
        });
        let actual = render_export(
            &device,
            &queue,
            frame,
            ColorSettings::default(),
            neural_settings(),
            [14, 10],
        )
        .unwrap();
        for pixel in actual.as_chunks::<4>().0 {
            assert!(
                pixel[..3]
                    .iter()
                    .zip(color)
                    .all(|(a, b)| a.abs_diff(b) <= 3)
            );
            assert_eq!(pixel[3], 255);
        }
    }
    let frame = patterned_frame(17, 13);
    let upscale = |settings| {
        render_export(
            &device,
            &queue,
            frame.clone(),
            ColorSettings::default(),
            settings,
            [34, 26],
        )
        .unwrap()
    };
    let neural = upscale(neural_settings());
    let standard = upscale(UpscaleSettings::default());
    let changed = neural
        .iter()
        .zip(&standard)
        .filter(|(a, b)| a.abs_diff(**b) > 2)
        .count();
    assert!(
        changed > neural.len() / 100,
        "The CNN must actually reconstruct patterned detail"
    );
    assert!(
        neural
            .as_chunks::<4>()
            .0
            .iter()
            .zip(standard.as_chunks::<4>().0)
            .all(|(a, b)| a[3].abs_diff(b[3]) <= 1)
    );
}

#[test]
fn neural_gpu_resizing_mode_switches_and_export_stay_consistent() {
    let (device, queue) = gpu();
    let mut resources = Resources::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    for (source, target, mode) in [
        ([9, 7], [18, 14], UpscaleMode::Anime4k),
        ([9, 7], [13, 11], UpscaleMode::Anime4k),
        ([9, 7], [13, 11], UpscaleMode::Standard),
        ([7, 5], [11, 9], UpscaleMode::Anime4k),
        ([9, 7], [18, 14], UpscaleMode::Anime4k),
    ] {
        let frame = patterned_frame(source[0], source[1]);
        let settings = UpscaleSettings {
            mode,
            ..Default::default()
        };
        let mut encoder = device.create_command_encoder(&Default::default());
        resources.update(
            &device,
            &queue,
            &mut encoder,
            &frame,
            ColorSettings::default(),
            false,
            settings,
            target,
        );
        assert_eq!(resources.last_mode, mode);
        let actual = readback(
            &device,
            &queue,
            encoder,
            &resources.output().texture,
            target,
        )
        .unwrap();
        let expected = render_export(
            &device,
            &queue,
            frame,
            ColorSettings::default(),
            settings,
            target,
        )
        .unwrap();
        assert_eq!(actual.len(), (target[0] * target[1] * 4) as usize);
        assert!(
            actual
                .iter()
                .zip(&expected)
                .all(|(a, b)| a.abs_diff(*b) <= 1)
        );
    }
}

#[test]
fn neural_validation_limits_cost_without_rejecting_ordinary_sampling() {
    let small = patterned_frame(9, 7);
    assert!(validate(&small, [19, 14], neural_settings(), 8192).is_err());
    assert!(validate(&small, [18, 15], neural_settings(), 8192).is_err());
    assert_eq!(
        validate(&small, [18, 14], neural_settings(), 8192).unwrap(),
        UpscaleMode::Anime4k
    );
    let wide = patterned_frame(17, 7);
    assert!(
        validate(&wide, [20, 10], neural_settings(), 32).is_err(),
        "The hidden 2x intermediate must fit the texture limit"
    );
    let large = VideoFrame {
        width: 1921,
        height: 1080,
        rgba: vec![128; 1921 * 1080 * 4],
        timestamp_ns: 0,
    };
    assert!(validate(&large, [3842, 2160], neural_settings(), 8192).is_err());
    for target in [[1921, 1080], [1280, 720], [1921, 1440]] {
        assert_eq!(
            validate(&large, target, neural_settings(), 8192).unwrap(),
            UpscaleMode::Standard
        );
    }
}

#[test]
fn neural_renderer_fallback_and_original_keep_ordinary_video() {
    let (device, queue) = gpu();
    let mut renderer =
        crate::gpu::VideoRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm).unwrap();
    let frame = patterned_frame(7, 5);
    for (target, original) in [([22, 16], false), ([14, 10], true)] {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Neural fallback and original regression"),
            size: wgpu::Extent3d {
                width: target[0],
                height: target[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        let color = ColorSettings {
            enabled: true,
            exposure: 0.5,
            ..Default::default()
        };
        renderer
            .draw(
                &device,
                &queue,
                &view,
                &mut encoder,
                Some(&frame),
                color,
                original,
                neural_settings(),
                [0., 0., target[0] as f32, target[1] as f32],
            )
            .unwrap();
        assert_eq!(renderer.effective_settings().mode, UpscaleMode::Standard);
        assert_eq!(renderer.effective_target(), target);
        if original {
            assert!(renderer.warning().is_none());
        } else {
            assert!(renderer.warning().is_some());
        }
        let actual = readback(&device, &queue, encoder, &texture, target).unwrap();
        let expected = render_export(
            &device,
            &queue,
            frame.clone(),
            if original {
                ColorSettings::default()
            } else {
                color
            },
            UpscaleSettings::default(),
            target,
        )
        .unwrap();
        assert!(
            actual
                .iter()
                .zip(&expected)
                .all(|(a, b)| a.abs_diff(*b) <= 1)
        );
    }
}

#[test]
#[ignore = "Manual GPU performance measurement; includes upload, encoding and GPU completion"]
fn neural_gpu_1080p_to_4k_benchmark() {
    let (device, queue) = gpu();
    let mut resources = Resources::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    let template = patterned_frame(1920, 1080);
    let mut measurements = Vec::new();
    for index in 0..44 {
        let frame = Arc::new(VideoFrame {
            rgba: template.rgba.clone(),
            timestamp_ns: index,
            width: 1920,
            height: 1080,
        });
        let started = std::time::Instant::now();
        let mut encoder = device.create_command_encoder(&Default::default());
        resources.update(
            &device,
            &queue,
            &mut encoder,
            &frame,
            ColorSettings::default(),
            false,
            neural_settings(),
            [3840, 2160],
        );
        assert_eq!(resources.last_mode, UpscaleMode::Anime4k);
        let submission = queue.submit([encoder.finish()]);
        device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(std::time::Duration::from_secs(30)),
            })
            .unwrap();
        if index >= 4 {
            measurements.push(started.elapsed().as_secs_f64() * 1000.);
        }
    }
    measurements.sort_by(f64::total_cmp);
    eprintln!(
        "Anime4K CNN 1080p -> 4K, 40 frames, upload + encode + GPU completion, no readback: median {:.2} ms, p95 {:.2} ms",
        measurements[20], measurements[37]
    );
}
