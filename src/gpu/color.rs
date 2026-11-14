use crate::{capture::VideoFrame, color::ColorSettings};
use std::sync::Arc;

struct Source {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    size: [u32; 2],
}
pub struct Resources {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform: wgpu::Buffer,
    last_uniform: Option<[f32; 12]>,
    target_is_srgb: bool,
    source: Option<Source>,
    // Retaining ownership makes pointer equality safe even when capture recycles allocations.
    last_frame: Option<Arc<VideoFrame>>,
}
impl Resources {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Video color bindings"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(48),
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Video color pipeline layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SDR color shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("color.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Video color pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Video linear clamp sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Video color uniform"),
            size: 48,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            layout,
            sampler,
            uniform,
            last_uniform: None,
            target_is_srgb: format.is_srgb(),
            source: None,
            last_frame: None,
        }
    }
    pub fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &Arc<VideoFrame>,
        mut color: ColorSettings,
        original: bool,
    ) {
        let size = [frame.width, frame.height];
        if self
            .source
            .as_ref()
            .is_none_or(|source| source.size != size)
        {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Video gamma RGBA source"),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Video source bindings"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.uniform.as_entire_binding(),
                    },
                ],
            });
            self.source = Some(Source {
                texture,
                bind_group,
                size,
            });
            self.last_frame = None;
        }
        if self
            .last_frame
            .as_ref()
            .is_none_or(|previous| !Arc::ptr_eq(previous, frame))
        {
            let source = self.source.as_ref().unwrap();
            queue.write_texture(
                source.texture.as_image_copy(),
                &frame.rgba,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(frame.width * 4),
                    rows_per_image: Some(frame.height),
                },
                source.texture.size(),
            );
            self.last_frame = Some(Arc::clone(frame));
        }
        color.normalize();
        let values = color.uniform_values(original, self.target_is_srgb);
        if self.last_uniform != Some(values) {
            let mut bytes = [0u8; 48];
            for (value, chunk) in values.iter().zip(bytes.as_chunks_mut::<4>().0) {
                chunk.copy_from_slice(&value.to_ne_bytes());
            }
            queue.write_buffer(&self.uniform, 0, &bytes);
            self.last_uniform = Some(values);
        }
    }
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if let Some(source) = &self.source {
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &source.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{FACTORY_PRESETS, apply_rgba};
    use std::{
        future::Future,
        task::{Context, Poll, Wake, Waker},
        time::Duration,
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
                Poll::Ready(value) => return value,
                Poll::Pending => std::thread::park(),
            }
        }
    }

    fn pattern(width: u32, height: u32) -> Arc<VideoFrame> {
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        let swatches = [
            [0, 0, 0],
            [255, 255, 255],
            [255, 0, 0],
            [0, 255, 0],
            [0, 0, 255],
            [255, 0, 255],
            [11, 12, 13],
            [128, 128, 128],
        ];
        for y in 0..height {
            for x in 0..width {
                let ramp = (x * 255 / (width - 1)) as u8;
                let rgb = match y % 5 {
                    0 => [ramp; 3],
                    1 => [ramp, 255 - ramp, (x % 256) as u8],
                    2 => [ramp, 0, 0],
                    3 => [0, ramp, 255],
                    _ => {
                        swatches
                            [(x as usize * swatches.len() / width as usize).min(swatches.len() - 1)]
                    }
                };
                rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], ((x + y) % 256) as u8]);
            }
        }
        Arc::new(VideoFrame {
            width,
            height,
            rgba,
            timestamp_ns: 0,
        })
    }

    fn readback(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        resources: &Resources,
        frame: &VideoFrame,
        format: wgpu::TextureFormat,
    ) -> Vec<u8> {
        let output = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Color parity target"),
            size: wgpu::Extent3d {
                width: frame.width,
                height: frame.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let row = (frame.width * 4).div_ceil(256) * 256;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Color parity readback"),
            size: row as u64 * frame.height as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let view = output.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Color parity draw"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            resources.draw(&mut pass);
        }
        encoder.copy_texture_to_buffer(
            output.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(frame.height),
                },
            },
            output.size(),
        );
        queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                tx.send(result).unwrap();
            });
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(30)),
            })
            .unwrap();
        rx.recv_timeout(Duration::from_secs(30)).unwrap().unwrap();
        let mapped = buffer.slice(..).get_mapped_range();
        let bytes = mapped
            .chunks_exact(row as usize)
            .flat_map(|line| line[..frame.width as usize * 4].iter().copied())
            .collect();
        drop(mapped);
        buffer.unmap();
        bytes
    }

    #[test]
    fn real_gpu_native_size_parity_and_resource_reuse() {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter =
            match block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) {
                Ok(adapter) => adapter,
                Err(error) => {
                    eprintln!("GPU color parity unavailable: {error}");
                    return;
                }
            };
        eprintln!("GPU color parity adapter: {:?}", adapter.get_info());
        let (device, queue) =
            block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let frame = pattern(257, 35); // Deliberately unaligned rows; rendering is one source pixel per output pixel.
        let enabled = ColorSettings {
            enabled: true,
            ..Default::default()
        };
        let mut cases = vec![
            (ColorSettings::default(), false),
            (enabled, false),
            (
                ColorSettings {
                    exposure: 2.0,
                    ..enabled
                },
                true,
            ),
        ];
        cases.extend(FACTORY_PRESETS.map(|(_, settings)| (settings, false)));
        cases.extend([
            (
                ColorSettings {
                    exposure: 2.0,
                    contrast: 1.5,
                    saturation: 2.0,
                    temperature: 1.0,
                    shadows: 1.0,
                    highlights: 1.0,
                    hdr: 1.0,
                    ..enabled
                },
                false,
            ),
            (
                ColorSettings {
                    exposure: -2.0,
                    contrast: 0.5,
                    saturation: 0.0,
                    temperature: -1.0,
                    shadows: -1.0,
                    highlights: -1.0,
                    hdr: 1.0,
                    ..enabled
                },
                false,
            ),
            (
                ColorSettings {
                    exposure: f32::NAN,
                    contrast: f32::INFINITY,
                    saturation: -100.0,
                    ..enabled
                },
                false,
            ),
        ]);
        for format in [
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        ] {
            let mut resources = Resources::new(&device, format);
            let validation_error = block_on(device.pop_error_scope());
            assert!(
                validation_error.is_none(),
                "GPU pipeline validation error: {validation_error:?}"
            );
            device.push_error_scope(wgpu::ErrorFilter::Validation);
            for &(settings, original) in &cases {
                resources.update(&device, &queue, &frame, settings, original);
                let texture = resources.source.as_ref().unwrap().texture.clone();
                resources.update(&device, &queue, &frame, settings, original);
                assert_eq!(texture, resources.source.as_ref().unwrap().texture);
                assert!(Arc::ptr_eq(resources.last_frame.as_ref().unwrap(), &frame));
                let output = readback(&device, &queue, &resources, &frame, format);
                let mut expected = frame.rgba.clone();
                if !original {
                    apply_rgba(&mut expected, &settings).unwrap();
                }
                for (index, (&actual, &expected)) in output.iter().zip(&expected).enumerate() {
                    let tolerance = if index % 4 == 3 || original || settings.is_identity() {
                        0
                    } else {
                        2
                    };
                    assert!(
                        actual.abs_diff(expected) <= tolerance,
                        "{format:?}, {settings:?}, original={original}, byte {index}: GPU={actual}, CPU={expected}"
                    );
                }
            }
            let full_hd = pattern(1920, 1080);
            let settings = ColorSettings {
                shadows: 0.6,
                hdr: 0.8,
                saturation: 1.2,
                ..enabled
            };
            resources.update(&device, &queue, &full_hd, settings, false);
            let actual = readback(&device, &queue, &resources, &full_hd, format);
            let mut expected = full_hd.rgba.clone();
            apply_rgba(&mut expected, &settings).unwrap();
            assert!(
                actual
                    .iter()
                    .zip(&expected)
                    .enumerate()
                    .all(|(i, (a, b))| a.abs_diff(*b) <= if i % 4 == 3 { 0 } else { 2 }),
                "1080p parity failed for {format:?}"
            );
        }
        assert!(
            block_on(device.pop_error_scope()).is_none(),
            "GPU pipeline validation error"
        );
    }
    #[test]
    #[ignore = "Synthetic GPU upload/draw timing; run explicitly with --ignored --nocapture"]
    fn synthetic_1080p_upload_and_grade_timing() {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("A real GPU adapter is required for this explicit performance exercise");
        eprintln!("Synthetic timing adapter: {:?}", adapter.get_info());
        let (device, queue) =
            block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let mut resources = Resources::new(&device, format);
        let validation = block_on(device.pop_error_scope());
        assert!(
            validation.is_none(),
            "GPU pipeline validation: {validation:?}"
        );
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let output = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Synthetic 1080p timing target"),
            size: wgpu::Extent3d {
                width: 1920,
                height: 1080,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = output.create_view(&Default::default());
        let frames = [
            pattern(1920, 1080),
            pattern(1920, 1080),
            pattern(1920, 1080),
        ];
        let hdr = FACTORY_PRESETS[4].1;
        assert!(hdr.enabled && hdr.hdr > 0.0);
        let mut submit = |index: usize, original: bool| {
            // Same update/draw implementation as the egui callback. Cycling owning Arcs forces
            // an upload every iteration without allocating or grading any CPU pixels here.
            resources.update(
                &device,
                &queue,
                &frames[index % frames.len()],
                hdr,
                original,
            );
            let mut encoder = device.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Synthetic color timing pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                resources.draw(&mut pass);
            }
            queue.submit([encoder.finish()]);
        };
        let wait = || {
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: Some(Duration::from_secs(30)),
                })
                .unwrap()
        };
        const FRAMES: usize = 120;
        let mut averages = [0.0; 2];
        for (case, original) in [true, false].into_iter().enumerate() {
            for frame in 0..12 {
                submit(frame, original);
            }
            wait();
            let start = std::time::Instant::now();
            for frame in 0..FRAMES {
                submit(frame, original);
            }
            wait();
            averages[case] = start.elapsed().as_secs_f64() * 1000.0 / FRAMES as f64;
        }
        let validation = block_on(device.pop_error_scope());
        assert!(
            validation.is_none(),
            "GPU timing validation: {validation:?}"
        );
        eprintln!(
            "Synthetic 1920x1080 Rgba8Unorm upload+draw+batch-completion wall time ({FRAMES} frames/case): original bypass {:.3} ms/frame; HDR preset {:.3} ms/frame; delta {:+.3} ms/frame. No readback or CPU grading in timed batches. This is NOT HDMI-to-display latency.",
            averages[0],
            averages[1],
            averages[1] - averages[0]
        );
    }
}
