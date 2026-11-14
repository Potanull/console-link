use super::*;
use std::{
    future::Future,
    task::{Context, Poll, Wake, Waker},
};
fn block_on<T>(future: impl Future<Output = T>) -> T {
    struct ThreadWake(std::thread::Thread);
    impl Wake for ThreadWake {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(ThreadWake(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(v) => return v,
            Poll::Pending => std::thread::park(),
        }
    }
}
fn gpu() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        .expect("Spatial tests require actual GPU");
    eprintln!("Spatial adapter: {:?}", adapter.get_info());
    block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap()
}
#[test]
#[ignore = "Requires packaged RTX Video runtime and NVIDIA RTX hardware"]
fn real_gpu_rtx_evaluation() {
    let (device, queue) = gpu();
    let frame = Arc::new(VideoFrame {
        width: 1920,
        height: 1080,
        rgba: [73, 113, 193, 255].repeat(1920 * 1080),
        timestamp_ns: 0,
    });
    let rgba = render_export(
        &device,
        &queue,
        frame,
        ColorSettings::default(),
        UpscaleSettings {
            mode: UpscaleMode::RtxVideo,
            ..Default::default()
        },
        [2560, 1440],
    )
    .expect("Genuine RTX Video evaluation");
    assert_eq!(rgba.len(), 2560 * 1440 * 4);
    for px in rgba.as_chunks::<4>().0 {
        assert!(
            px[0].abs_diff(73) < 8
                && px[1].abs_diff(113) < 8
                && px[2].abs_diff(193) < 8
                && px[3] == 255,
            "{px:?}"
        );
    }
}
#[test]
fn rtx_error_falls_back_to_graded_standard() {
    let (device, queue) = gpu();
    let mut resources = Resources::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    resources.rtx_error = Some("RTX Video test failure".into());
    let frame = Arc::new(VideoFrame {
        width: 9,
        height: 7,
        rgba: [73, 113, 193, 255].repeat(63),
        timestamp_ns: 0,
    });
    let color = ColorSettings {
        enabled: true,
        exposure: 0.25,
        ..Default::default()
    };
    let mut encoder = device.create_command_encoder(&Default::default());
    resources.update(
        &device,
        &queue,
        &mut encoder,
        &frame,
        color,
        false,
        UpscaleSettings {
            mode: UpscaleMode::RtxVideo,
            ..Default::default()
        },
        [17, 13],
    );
    assert_eq!(resources.last_mode, UpscaleMode::Standard);
    assert_eq!(
        resources.rtx_status(&device).unwrap_err(),
        "RTX Video test failure"
    );
    let actual = readback(
        &device,
        &queue,
        encoder,
        &resources.output().texture,
        [17, 13],
    )
    .unwrap();
    let expected = render_export(
        &device,
        &queue,
        frame,
        color,
        UpscaleSettings::default(),
        [17, 13],
    )
    .unwrap();
    assert!(actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 1));
}
#[test]
#[ignore = "Requires packaged RTX Video runtime and NVIDIA RTX hardware"]
fn real_gpu_rtx_pattern_resize_quality_and_concurrent_export() {
    let (device, queue) = gpu();
    let mut resources = Resources::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    let mut rgba = Vec::with_capacity(640 * 360 * 4);
    for y in 0..360 {
        for x in 0..640 {
            let edge = if (x / 7 + y / 7) % 2 == 0 { 35 } else { 218 };
            rgba.extend_from_slice(&[edge, (x * 255 / 639) as u8, (y * 255 / 359) as u8, 255]);
        }
    }
    let frame = Arc::new(VideoFrame {
        width: 640,
        height: 360,
        rgba,
        timestamp_ns: 0,
    });
    let settings = UpscaleSettings {
        mode: UpscaleMode::RtxVideo,
        ..Default::default()
    };
    for quality in 1..=4 {
        let target = if quality % 2 == 0 {
            [960, 540]
        } else {
            [1280, 720]
        };
        let mut encoder = device.create_command_encoder(&Default::default());
        resources.update(
            &device,
            &queue,
            &mut encoder,
            &frame,
            ColorSettings::default(),
            false,
            UpscaleSettings {
                rtx_quality: quality,
                ..settings
            },
            target,
        );
        assert_eq!(
            resources.last_mode,
            UpscaleMode::RtxVideo,
            "{:?}",
            resources.rtx_error
        );
        let actual = readback(
            &device,
            &queue,
            encoder,
            &resources.output().texture,
            target,
        )
        .unwrap();
        let standard = render_export(
            &device,
            &queue,
            frame.clone(),
            ColorSettings::default(),
            UpscaleSettings::default(),
            target,
        )
        .unwrap();
        let changed = actual
            .iter()
            .zip(&standard)
            .filter(|(a, b)| a.abs_diff(**b) > 2)
            .count();
        eprintln!(
            "RTX quality {quality} {target:?}: {changed} channels differ >2/255 from Standard"
        );
        assert!(
            changed > actual.len() / 100,
            "VSR must actually process patterned pixels"
        );
    }
    let export_frame = frame.clone();
    let export_device = device.clone();
    let export_queue = queue.clone();
    let export = std::thread::spawn(move || {
        render_export(
            &export_device,
            &export_queue,
            export_frame,
            ColorSettings::default(),
            settings,
            [960, 540],
        )
        .unwrap()
    });
    for n in 0..12 {
        let mut encoder = device.create_command_encoder(&Default::default());
        resources.update(
            &device,
            &queue,
            &mut encoder,
            &frame,
            ColorSettings::default(),
            false,
            settings,
            if n % 2 == 0 { [1280, 720] } else { [960, 540] },
        );
        assert_eq!(
            resources.last_mode,
            UpscaleMode::RtxVideo,
            "{:?}",
            resources.rtx_error
        );
        queue.submit([encoder.finish()]);
    }
    let actual = export.join().unwrap();
    let expected = render_export(
        &device,
        &queue,
        frame,
        ColorSettings::default(),
        settings,
        [960, 540],
    )
    .unwrap();
    assert_eq!(
        actual, expected,
        "Frozen export must survive concurrent playback resize"
    );
}
#[test]
fn real_gpu_spatial_constant_patterns_and_reuse() {
    let (device, queue) = gpu();
    let mut resources = Resources::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    let frame = Arc::new(VideoFrame {
        width: 9,
        height: 7,
        rgba: [73, 113, 193, 255].repeat(63),
        timestamp_ns: 0,
    });
    for mode in [UpscaleMode::Standard, UpscaleMode::Nis, UpscaleMode::Fsr1] {
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
            [17, 13],
        );
        let output = readback(
            &device,
            &queue,
            encoder,
            &resources.output().texture,
            [17, 13],
        )
        .unwrap();
        for px in output.as_chunks::<4>().0 {
            for (actual, expected) in px.iter().zip([73, 113, 193, 255]) {
                assert!(
                    (i32::from(*actual) - expected).abs() <= 1,
                    "{mode:?}: {px:?}"
                );
            }
        }
        let texture = resources.graded.as_ref().unwrap().texture.clone();
        let target = resources.reconstructed.as_ref().unwrap().texture.clone();
        let mut encoder = device.create_command_encoder(&Default::default());
        resources.update(
            &device,
            &queue,
            &mut encoder,
            &frame,
            ColorSettings::default(),
            false,
            settings,
            [17, 13],
        );
        assert_eq!(texture, resources.graded.as_ref().unwrap().texture);
        assert_eq!(target, resources.reconstructed.as_ref().unwrap().texture);
        queue.submit([encoder.finish()]);
    }
    for rgb in [
        [0, 0, 0],
        [255, 255, 255],
        [255, 0, 0],
        [0, 255, 0],
        [0, 0, 255],
    ] {
        let flat = Arc::new(VideoFrame {
            width: 9,
            height: 7,
            rgba: [rgb[0], rgb[1], rgb[2], 255].repeat(63),
            timestamp_ns: 1,
        });
        for mode in [UpscaleMode::Nis, UpscaleMode::Fsr1] {
            let mut encoder = device.create_command_encoder(&Default::default());
            resources.update(
                &device,
                &queue,
                &mut encoder,
                &flat,
                ColorSettings::default(),
                false,
                UpscaleSettings {
                    mode,
                    ..Default::default()
                },
                [17, 13],
            );
            let output = readback(
                &device,
                &queue,
                encoder,
                &resources.output().texture,
                [17, 13],
            )
            .unwrap();
            for px in output.as_chunks::<4>().0 {
                for i in 0..3 {
                    assert!(px[i].abs_diff(rgb[i]) <= 1, "{mode:?} flat {rgb:?}: {px:?}");
                }
            }
        }
    }
    for mode in [UpscaleMode::Nis, UpscaleMode::Fsr1] {
        for target in [[9, 7], [5, 3], [17, 7]] {
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
            assert_eq!(resources.last_mode, UpscaleMode::Standard);
            queue.submit([encoder.finish()]);
        }
    }
    assert!(validate(&frame, [8193, 10], UpscaleSettings::default(), 8192).is_err());
    assert!(
        validate(
            &frame,
            [17, 13],
            UpscaleSettings {
                mode: UpscaleMode::RtxVideo,
                ..Default::default()
            },
            8192
        )
        .is_ok()
    );
}
#[test]
fn real_gpu_matches_pinned_upstream_references_and_png() {
    let (device, queue) = gpu();
    let frame = Arc::new(VideoFrame {
        width: 17,
        height: 13,
        rgba: include_bytes!("../app/vendor/reference-input.rgba").to_vec(),
        timestamp_ns: 0,
    });
    let fixtures: [(UpscaleMode, [&[u8]; 3]); 2] = [
        (
            UpscaleMode::Nis,
            [
                include_bytes!("../app/vendor/nis-0.rgba"),
                include_bytes!("../app/vendor/nis-1.rgba"),
                include_bytes!("../app/vendor/nis-2.rgba"),
            ],
        ),
        (
            UpscaleMode::Fsr1,
            [
                include_bytes!("../app/vendor/fsr-0.rgba"),
                include_bytes!("../app/vendor/fsr-1.rgba"),
                include_bytes!("../app/vendor/fsr-2.rgba"),
            ],
        ),
    ];
    // Check reconstruction separately so a sharpness tolerance cannot hide a wrong EASU kernel.
    let mut easu_resources = Resources::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    let mut encoder = device.create_command_encoder(&Default::default());
    easu_resources.update(
        &device,
        &queue,
        &mut encoder,
        &frame,
        ColorSettings::default(),
        false,
        UpscaleSettings {
            mode: UpscaleMode::Fsr1,
            ..Default::default()
        },
        [29, 21],
    );
    let easu = readback(
        &device,
        &queue,
        encoder,
        &easu_resources.reconstructed.as_ref().unwrap().texture,
        [29, 21],
    )
    .unwrap();
    let easu_error = easu
        .iter()
        .zip(include_bytes!("../app/vendor/fsr-easu.rgba"))
        .map(|(&a, &b)| a.abs_diff(b))
        .max()
        .unwrap();
    eprintln!("EASU reconstruction max difference: {easu_error}/255");
    assert!(easu_error <= 1);
    let mut reference_failures = Vec::new();
    for (mode, expected) in fixtures {
        for (index, sharpness) in [0.0, 0.3, 1.0].into_iter().enumerate() {
            let settings = UpscaleSettings {
                mode,
                nis_sharpness: sharpness,
                fsr_sharpness: sharpness,
                ..Default::default()
            };
            let output = render_export(
                &device,
                &queue,
                frame.clone(),
                ColorSettings::default(),
                settings,
                [29, 21],
            )
            .unwrap();
            let differences: Vec<_> = output
                .iter()
                .zip(expected[index])
                .map(|(&a, &b)| i32::from(a).abs_diff(i32::from(b)))
                .collect();
            let max = differences.iter().max().unwrap();
            let mean = differences.iter().sum::<u32>() as f64 / differences.len() as f64;
            eprintln!(
                "{mode:?} sharpness {sharpness}: reference max {max}/255, mean {mean:.4}/255"
            );
            // RGBA8 EASU roundoff of one unit can amplify through maximum RCAS (gain <=4).
            let tolerance = if mode == UpscaleMode::Fsr1 && sharpness == 1.0 {
                4
            } else {
                2
            };
            if *max > tolerance || mean >= 0.1 {
                let worst: Vec<_> = differences
                    .iter()
                    .enumerate()
                    .filter(|(_, difference)| **difference > tolerance)
                    .take(20)
                    .map(|(i, difference)| {
                        (
                            i / 4 % 29,
                            i / 4 / 29,
                            i % 4,
                            output[i],
                            expected[index][i],
                            difference,
                        )
                    })
                    .collect();
                reference_failures.push(format!("{mode:?} sharpness {sharpness}: max {max}, mean {mean}; (x,y,channel,actual,expected,delta): {worst:?}"));
            }
        }
        let color = ColorSettings {
            enabled: true,
            exposure: 0.25,
            contrast: 1.1,
            ..Default::default()
        };
        let settings = UpscaleSettings {
            mode,
            ..Default::default()
        };
        let rgba =
            render_export(&device, &queue, frame.clone(), color, settings, [29, 21]).unwrap();
        let output = VideoFrame {
            width: 29,
            height: 21,
            rgba: rgba.clone(),
            timestamp_ns: 0,
        };
        let directory = std::env::temp_dir().join(format!(
            "console-link-spatial-png-{:?}-{}",
            mode,
            std::process::id()
        ));
        let path =
            crate::gpu::save_screenshot(&output, &Default::default(), false, &directory).unwrap();
        let decoder =
            png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&path).unwrap()));
        let mut reader = decoder.read_info().unwrap();
        assert_eq!(
            reader.info().srgb,
            Some(png::SrgbRenderingIntent::Perceptual)
        );
        let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut bytes).unwrap();
        assert_eq!((info.width, info.height), (29, 21));
        assert_eq!(&bytes[..info.buffer_size()], rgba);
        drop(reader);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
    assert!(
        reference_failures.is_empty(),
        "{}",
        reference_failures.join("\n")
    );
}
#[test]
#[ignore = "Synthetic GPU component throughput benchmark; run explicitly on target hardware"]
fn benchmark_spatial_1080p_to_1440p() {
    let (device, queue) = gpu();
    let mut rgba = Vec::with_capacity(1920 * 1080 * 4);
    for y in 0..1080u32 {
        for x in 0..1920u32 {
            let checker = if (x / 8 + y / 8) % 2 == 0 { 32 } else { 192 };
            rgba.extend_from_slice(&[
                (16 + x * 223 / 1919) as u8,
                (16 + y * 223 / 1079) as u8,
                checker,
                255,
            ]);
        }
    }
    let frames: Vec<_> = (0..3)
        .map(|i| {
            Arc::new(VideoFrame {
                width: 1920,
                height: 1080,
                rgba: rgba.clone(),
                timestamp_ns: i,
            })
        })
        .collect();
    let output = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Benchmark presentation"),
        size: wgpu::Extent3d {
            width: 2560,
            height: 1440,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = output.create_view(&Default::default());
    for mode in [
        UpscaleMode::Standard,
        UpscaleMode::Nis,
        UpscaleMode::Fsr1,
        UpscaleMode::RtxVideo,
    ] {
        for graded in [false, true] {
            let color = ColorSettings {
                enabled: graded,
                exposure: 0.25,
                contrast: 1.1,
                saturation: 1.1,
                ..Default::default()
            };
            let settings = UpscaleSettings {
                mode,
                ..Default::default()
            };
            let mut spatial = Resources::new(&device, wgpu::TextureFormat::Rgba8Unorm);
            let mut standard =
                super::super::color::Resources::new(&device, wgpu::TextureFormat::Rgba8Unorm);
            let mut samples = Vec::new();
            for batch in 0..35 {
                let start = std::time::Instant::now();
                for n in 0..4 {
                    let frame = &frames[(batch * 4 + n) % 3];
                    let mut encoder = device.create_command_encoder(&Default::default());
                    if mode == UpscaleMode::Standard {
                        standard.update(&device, &queue, frame, color, false);
                        let mut pass = begin_pass(&mut encoder, &view, "Benchmark standard");
                        standard.draw(&mut pass);
                    } else {
                        spatial.update(
                            &device,
                            &queue,
                            &mut encoder,
                            frame,
                            color,
                            false,
                            settings,
                            [2560, 1440],
                        );
                        assert_eq!(spatial.last_mode, mode, "{:?}", spatial.rtx_error);
                        let mut pass = begin_pass(&mut encoder, &view, "Benchmark enhanced");
                        spatial.draw(&mut pass);
                    }
                    queue.submit([encoder.finish()]);
                }
                device
                    .poll(wgpu::PollType::Wait {
                        submission_index: None,
                        timeout: Some(std::time::Duration::from_secs(30)),
                    })
                    .unwrap();
                if batch >= 5 {
                    samples.push(start.elapsed().as_secs_f64() * 1000.0 / 4.0);
                }
            }
            #[cfg(target_os = "windows")]
            if mode == UpscaleMode::RtxVideo {
                let mut session = spatial
                    .rtx
                    .as_ref()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .lock()
                    .unwrap();
                let mut gpu = session.take_gpu_timings().unwrap();
                gpu.drain(..20);
                gpu.sort_by(f64::total_cmp);
                eprintln!(
                    "GPU-EVAL RTX quality2 grading={graded}: median {:.3}ms p95 {:.3}ms ({} warmed native timestamp samples; VSR only)",
                    gpu[gpu.len() / 2],
                    gpu[gpu.len() * 95 / 100],
                    gpu.len()
                );
            }
            samples.sort_by(f64::total_cmp);
            eprintln!(
                "COMPONENT-WALL {mode:?} grading={graded}: median {:.3}ms/frame p95 {:.3}ms/frame (30 batches x4, includes upload/CPU/submit/wait, no readback)",
                samples[15], samples[28]
            );
        }
    }
}
