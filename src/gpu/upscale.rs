//! Shared persistent GPU passes for native presentation and screenshots.
use crate::{
    capture::VideoFrame,
    color::ColorSettings,
    upscale::{UpscaleMode, UpscaleSettings},
};
use std::sync::Arc;
#[cfg(target_os = "windows")]
#[path = "rtx_video.rs"]
mod rtx_video;
pub const MAX_TARGET_DIMENSION: u32 = 8192;
pub fn checked_gpu_creation<T>(
    device: &wgpu::Device,
    create: impl FnOnce() -> T,
) -> Result<T, String> {
    device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    device.push_error_scope(wgpu::ErrorFilter::Internal);
    device.push_error_scope(wgpu::ErrorFilter::Validation);
    let created = create();
    let mut failure = None;
    for _ in 0..3 {
        if let Some(error) = wait_for_gpu(device.pop_error_scope()) {
            failure.get_or_insert_with(|| error.to_string());
        }
    }
    match failure {
        Some(error) => Err(error),
        None => Ok(created),
    }
}

pub(super) fn wait_for_gpu<T>(future: impl std::future::Future<Output = T>) -> T {
    use std::task::{Context, Poll, Wake, Waker};
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
            Poll::Ready(result) => return result,
            Poll::Pending => std::thread::park(),
        }
    }
}
pub fn validate(
    frame: &VideoFrame,
    target: [u32; 2],
    settings: UpscaleSettings,
    limit: u32,
) -> Result<UpscaleMode, String> {
    if frame.width == 0
        || frame.height == 0
        || target.contains(&0)
        || frame.rgba.len() as u64 != u64::from(frame.width) * u64::from(frame.height) * 4
    {
        return Err("Invalid source or output dimensions".into());
    }
    if [frame.width, frame.height, target[0], target[1]]
        .iter()
        .any(|&n| n > limit)
    {
        return Err(format!("GPU texture limit is {limit} pixels per dimension"));
    }
    let mode = settings.effective_mode([frame.width, frame.height], target);
    if mode == UpscaleMode::Nis
        && (u64::from(target[0]) > u64::from(frame.width) * 2
            || u64::from(target[1]) > u64::from(frame.height) * 2)
    {
        return Err("NIS supports up to 2x scaling per dimension".into());
    }
    Ok(mode)
}
struct Target {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    bind: wgpu::BindGroup,
    size: [u32; 2],
}
struct Recipe {
    frame: Arc<VideoFrame>,
    color: ColorSettings,
    original: bool,
    settings: UpscaleSettings,
    target: [u32; 2],
}
pub struct Resources {
    grade: super::color::Resources,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    coefficients: wgpu::Buffer,
    spatial_uniform: wgpu::Buffer,
    present_uniform: wgpu::Buffer,
    standard: wgpu::RenderPipeline,
    nis: wgpu::RenderPipeline,
    easu: wgpu::RenderPipeline,
    rcas: wgpu::RenderPipeline,
    present: wgpu::RenderPipeline,
    target_is_srgb: bool,
    graded: Option<Target>,
    reconstructed: Option<Target>,
    sharpened: Option<Target>,
    last_mode: UpscaleMode,
    last_recipe: Option<Recipe>,
    #[cfg(target_os = "windows")]
    rtx: Option<Result<Arc<std::sync::Mutex<rtx_video::Session>>, String>>,
    rtx_error: Option<String>,
}
impl Resources {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Spatial bindings"),
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
                        min_binding_size: wgpu::BufferSize::new(32),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(4096),
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Spatial layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let fsr = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("FSR 1 EASU RCAS"),
            source: wgpu::ShaderSource::Wgsl(include_str!("fsr.wgsl").into()),
        });
        let nis = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("NVIDIA Image Scaling"),
            source: wgpu::ShaderSource::Wgsl(include_str!("nis.wgsl").into()),
        });
        let make = |shader: &wgpu::ShaderModule, entry: &str, format| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview: None,
                cache: None,
            })
        };
        let uniform = || {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Spatial parameters"),
                size: 32,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Spatial clamp sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let coefficients = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("NIS coefficient bank"),
            size: 4096,
            usage: wgpu::BufferUsages::UNIFORM,
            mapped_at_creation: true,
        });
        coefficients
            .slice(..)
            .get_mapped_range_mut()
            .copy_from_slice(include_bytes!("../app/vendor/nis-coefficients.bin"));
        coefficients.unmap();
        Self {
            grade: super::color::Resources::new(device, wgpu::TextureFormat::Rgba8Unorm),
            layout,
            sampler,
            coefficients,
            spatial_uniform: uniform(),
            present_uniform: uniform(),
            standard: make(&fsr, "standard", wgpu::TextureFormat::Rgba8Unorm),
            nis: make(&nis, "nis", wgpu::TextureFormat::Rgba8Unorm),
            easu: make(&fsr, "easu", wgpu::TextureFormat::Rgba8Unorm),
            rcas: make(&fsr, "rcas", wgpu::TextureFormat::Rgba8Unorm),
            present: make(&fsr, "present", format),
            target_is_srgb: format.is_srgb(),
            graded: None,
            reconstructed: None,
            sharpened: None,
            last_mode: UpscaleMode::Standard,
            last_recipe: None,
            #[cfg(target_os = "windows")]
            rtx: None,
            rtx_error: None,
        }
    }
    pub fn rtx_status(&mut self, device: &wgpu::Device) -> Result<(), String> {
        if let Some(error) = &self.rtx_error {
            return Err(error.clone());
        }
        #[cfg(target_os = "windows")]
        {
            let session = self
                .rtx
                .get_or_insert_with(|| rtx_video::Session::get(device));
            session
                .as_ref()
                .map_err(Clone::clone)?
                .lock()
                .map_err(|_| "RTX Video session lock failed")?
                .status()
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = device;
            Err("RTX Video requires Windows and an NVIDIA RTX GPU".into())
        }
    }
    fn target(&self, device: &wgpu::Device, size: [u32; 2], uniform: &wgpu::Buffer) -> Target {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Spatial intermediate"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Spatial texture bindings"),
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
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.coefficients.as_entire_binding(),
                },
            ],
        });
        Target {
            texture,
            view,
            bind,
            size,
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        frame: &Arc<VideoFrame>,
        color: ColorSettings,
        original: bool,
        mut settings: UpscaleSettings,
        target: [u32; 2],
    ) {
        settings.normalize();
        if self.last_recipe.as_ref().is_some_and(|r| {
            Arc::ptr_eq(&r.frame, frame)
                && r.color == color
                && r.original == original
                && r.settings == settings
                && r.target == target
        }) {
            return;
        }
        let size = [frame.width, frame.height];
        let mode = settings.effective_mode(size, target);
        if self.graded.as_ref().is_none_or(|t| t.size != size) {
            self.graded = Some(self.target(device, size, &self.spatial_uniform));
        }
        if self.reconstructed.as_ref().is_none_or(|t| t.size != target) {
            self.reconstructed = Some(self.target(device, target, &self.present_uniform));
            self.sharpened = None;
        }
        if mode == UpscaleMode::Fsr1 && self.sharpened.as_ref().is_none_or(|t| t.size != target) {
            self.sharpened = Some(self.target(device, target, &self.present_uniform));
        }
        self.grade.update(device, queue, frame, color, original);
        let sharp = if mode == UpscaleMode::Nis {
            settings.nis_sharpness
        } else {
            settings.fsr_sharpness
        };
        write_uniform(
            queue,
            &self.spatial_uniform,
            [
                size[0] as f32,
                size[1] as f32,
                target[0] as f32,
                target[1] as f32,
                sharp,
                0.0,
                0.0,
                0.0,
            ],
        );
        // Reconstructed input is target-sized for RCAS. Separate uniforms prevent queued write aliasing.
        write_uniform(
            queue,
            &self.present_uniform,
            [
                target[0] as f32,
                target[1] as f32,
                target[0] as f32,
                target[1] as f32,
                sharp,
                if self.target_is_srgb { 1.0 } else { 0.0 },
                0.0,
                0.0,
            ],
        );
        if mode == UpscaleMode::RtxVideo {
            let result = self
                .rtx_status(device)
                .and_then(|()| self.evaluate_rtx(device, queue, settings.rtx_quality));
            match result {
                Ok(()) => {
                    self.last_mode = UpscaleMode::RtxVideo;
                    self.last_recipe = Some(Recipe {
                        frame: frame.clone(),
                        color,
                        original,
                        settings,
                        target,
                    });
                    return;
                }
                Err(error) => {
                    self.rtx_error = Some(error);
                }
            }
        }
        let graded = self.graded.as_ref().unwrap();
        {
            let mut pass = begin_pass(encoder, &graded.view, "Source resolution color grade");
            self.grade.draw(&mut pass);
        }
        let pipeline = match mode {
            UpscaleMode::Nis => &self.nis,
            UpscaleMode::Fsr1 => &self.easu,
            _ => &self.standard,
        };
        render(
            encoder,
            &self.reconstructed.as_ref().unwrap().view,
            pipeline,
            &graded.bind,
            "Spatial reconstruction",
        );
        if mode == UpscaleMode::Fsr1 {
            render(
                encoder,
                &self.sharpened.as_ref().unwrap().view,
                &self.rcas,
                &self.reconstructed.as_ref().unwrap().bind,
                "FSR RCAS",
            );
        }
        self.last_mode = if mode == UpscaleMode::RtxVideo {
            UpscaleMode::Standard
        } else {
            mode
        };
        self.last_recipe = Some(Recipe {
            frame: Arc::clone(frame),
            color,
            original,
            settings,
            target,
        });
    }
    fn evaluate_rtx(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        quality: u32,
    ) -> Result<(), String> {
        #[cfg(target_os = "windows")]
        {
            let graded = self.graded.as_ref().unwrap();
            let output = self.reconstructed.as_ref().unwrap();
            let mut preparation = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("RTX grade and handoff"),
            });
            {
                let mut pass = begin_pass(&mut preparation, &graded.view, "RTX source grade");
                self.grade.draw(&mut pass);
            }
            // Mark the output initialized in wgpu before the native writer, preventing a later lazy clear.
            drop(begin_pass(
                &mut preparation,
                &output.view,
                "RTX output initialization",
            ));
            preparation.transition_resources(
                std::iter::empty(),
                [&graded.texture, &output.texture]
                    .into_iter()
                    .map(|texture| wgpu::TextureTransition {
                        texture,
                        selector: None,
                        state: wgpu::TextureUses::PRESENT,
                    }),
            );
            self.rtx
                .as_ref()
                .unwrap()
                .as_ref()
                .map_err(Clone::clone)?
                .lock()
                .map_err(|_| "RTX Video session lock failed")?
                .evaluate(
                    queue,
                    preparation.finish(),
                    &graded.texture,
                    &output.texture,
                    quality,
                )
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (device, queue, quality);
            Err("RTX Video requires Windows and an NVIDIA RTX GPU".into())
        }
    }
    pub fn error(&self) -> Option<&str> {
        self.rtx_error.as_deref()
    }
    fn output(&self) -> &Target {
        if self.last_mode == UpscaleMode::Fsr1 {
            self.sharpened.as_ref().unwrap()
        } else {
            self.reconstructed.as_ref().unwrap()
        }
    }
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.reconstructed.is_some() {
            pass.set_pipeline(&self.present);
            pass.set_bind_group(0, &self.output().bind, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}
fn write_uniform(queue: &wgpu::Queue, buffer: &wgpu::Buffer, values: [f32; 8]) {
    let mut bytes = [0u8; 32];
    for (value, chunk) in values.iter().zip(bytes.as_chunks_mut::<4>().0) {
        chunk.copy_from_slice(&value.to_ne_bytes());
    }
    queue.write_buffer(buffer, 0, &bytes);
}
fn begin_pass<'a>(
    encoder: &'a mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    label: &str,
) -> wgpu::RenderPass<'a> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
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
    })
}
fn render(
    encoder: &mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    pipeline: &wgpu::RenderPipeline,
    bind: &wgpu::BindGroup,
    label: &str,
) {
    let mut pass = begin_pass(encoder, view, label);
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, bind, &[]);
    pass.draw(0..3, 0..1);
}

/// Run only on the screenshot worker: this waits for its own readback, never during playback.
pub fn render_export(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    frame: Arc<VideoFrame>,
    color: ColorSettings,
    settings: UpscaleSettings,
    target: [u32; 2],
) -> Result<Vec<u8>, String> {
    let mode = validate(
        &frame,
        target,
        settings,
        device.limits().max_texture_dimension_2d,
    )?;
    if mode == UpscaleMode::Standard {
        let mut grade = super::color::Resources::new(device, wgpu::TextureFormat::Rgba8Unorm);
        grade.update(device, queue, &frame, color, false);
        let output = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Standard export"),
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
        let view = output.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = begin_pass(&mut encoder, &view, "Standard export");
            grade.draw(&mut pass);
        }
        return readback(device, queue, encoder, &output, target);
    }
    let mut resources = checked_gpu_creation(device, || {
        Resources::new(device, wgpu::TextureFormat::Rgba8Unorm)
    })?;
    let mut encoder = device.create_command_encoder(&Default::default());
    resources.update(
        device,
        queue,
        &mut encoder,
        &frame,
        color,
        false,
        settings,
        target,
    );
    if let Some(error) = &resources.rtx_error {
        return Err(error.clone());
    }
    readback(device, queue, encoder, &resources.output().texture, target)
}
fn readback(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    mut encoder: wgpu::CommandEncoder,
    texture: &wgpu::Texture,
    target: [u32; 2],
) -> Result<Vec<u8>, String> {
    let row = (target[0] * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Screenshot readback"),
        size: u64::from(row) * u64::from(target[1]),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(target[1]),
            },
        },
        texture.size(),
    );
    let submission = queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(std::time::Duration::from_secs(30)),
        })
        .map_err(|e| e.to_string())?;
    rx.recv_timeout(std::time::Duration::from_secs(30))
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let mapped = buffer.slice(..).get_mapped_range();
    let bytes = mapped
        .chunks_exact(row as usize)
        .flat_map(|line| line[..target[0] as usize * 4].iter().copied())
        .collect();
    drop(mapped);
    buffer.unmap();
    Ok(bytes)
}

#[cfg(test)]
#[path = "upscale_tests.rs"]
mod tests;
