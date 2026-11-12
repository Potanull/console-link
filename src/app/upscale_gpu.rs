//! Persistent spatial passes. Export creates an isolated instance on the same GPU.
use console_link::{
    capture::VideoFrame,
    color::ColorSettings,
    upscale::{UpscaleMode, UpscaleSettings},
};
use eframe::{egui, egui_wgpu};
use egui_wgpu::{Callback, CallbackResources, CallbackTrait, ScreenDescriptor};
use std::sync::Arc;
#[cfg(target_os = "windows")]
#[path = "rtx_video.rs"]
mod rtx_video;

/// Lazily query the actual SDK and retain its feature on the presentation device.
pub(super) fn rtx_status(state: &egui_wgpu::RenderState) -> Result<(), String> {
    let mut renderer = state.renderer.write();
    let resources = renderer
        .callback_resources
        .get_mut::<Resources>()
        .ok_or("GPU processing is unavailable")?;
    resources.rtx_status(&state.device)
}

pub(super) const MAX_TARGET_DIMENSION: u32 = 8192;
/// Compile spatial shaders only when an enhanced mode is requested. A driver
/// validation failure must leave the ordinary video renderer available.
pub(super) fn ensure(state: &egui_wgpu::RenderState, mode: UpscaleMode) -> Result<(), String> {
    ensure_resources(
        &state.device,
        state.target_format,
        &mut state.renderer.write().callback_resources,
        mode,
    )
}

struct InitializationError(String);

fn ensure_resources(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    resources: &mut CallbackResources,
    mode: UpscaleMode,
) -> Result<(), String> {
    if mode == UpscaleMode::Standard || resources.get::<Resources>().is_some() {
        return Ok(());
    }
    if let Some(error) = resources.get::<InitializationError>() {
        return Err(error.0.clone());
    }
    match checked_gpu_creation(device, || Resources::new(device, format)) {
        Ok(created) => {
            resources.insert(created);
            Ok(())
        }
        Err(error) => {
            resources.insert(InitializationError(error.clone()));
            Err(error)
        }
    }
}

fn checked_gpu_creation<T>(device: &wgpu::Device, create: impl FnOnce() -> T) -> Result<T, String> {
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

fn wait_for_gpu<T>(future: impl std::future::Future<Output = T>) -> T {
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
fn validate(
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
pub(super) fn paint_enhanced(
    ui: &egui::Ui,
    rect: egui::Rect,
    frame: Arc<VideoFrame>,
    color: ColorSettings,
    original: bool,
    settings: UpscaleSettings,
    target: [u32; 2],
) -> Result<(), String> {
    let mode = validate(&frame, target, settings, MAX_TARGET_DIMENSION)?;
    if mode == UpscaleMode::Standard {
        super::color_gpu::paint(ui, rect, frame, color, original);
        return Ok(());
    }
    ui.painter().add(Callback::new_paint_callback(
        rect,
        VideoCallback {
            frame,
            color,
            original,
            settings,
            target,
        },
    ));
    Ok(())
}
struct VideoCallback {
    frame: Arc<VideoFrame>,
    color: ColorSettings,
    original: bool,
    settings: UpscaleSettings,
    target: [u32; 2],
}
impl CallbackTrait for VideoCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _: &ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if let Some(r) = resources.get_mut::<Resources>() {
            r.update(
                device,
                queue,
                encoder,
                &self.frame,
                self.color,
                self.original,
                self.settings,
                self.target,
            );
        }
        Vec::new()
    }
    fn paint(
        &self,
        _: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &CallbackResources,
    ) {
        if let Some(r) = resources.get::<Resources>() {
            r.draw(pass);
        }
    }
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
struct Resources {
    grade: super::color_gpu::Resources,
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
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
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
            .copy_from_slice(include_bytes!("vendor/nis-coefficients.bin"));
        coefficients.unmap();
        Self {
            grade: super::color_gpu::Resources::new(device, wgpu::TextureFormat::Rgba8Unorm),
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
    fn rtx_status(&mut self, device: &wgpu::Device) -> Result<(), String> {
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
    fn update(
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
    fn output(&self) -> &Target {
        if self.last_mode == UpscaleMode::Fsr1 {
            self.sharpened.as_ref().unwrap()
        } else {
            self.reconstructed.as_ref().unwrap()
        }
    }
    fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
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
pub(super) fn render_export(
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
        let mut grade = super::color_gpu::Resources::new(device, wgpu::TextureFormat::Rgba8Unorm);
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
mod tests {
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
    fn lazy_spatial_initialization_preserves_standard_and_reuses_gpu_resources() {
        let (device, _) = gpu();
        let mut resources = CallbackResources::default();
        ensure_resources(
            &device,
            wgpu::TextureFormat::Rgba8Unorm,
            &mut resources,
            UpscaleMode::Standard,
        )
        .unwrap();
        assert!(resources.get::<Resources>().is_none());
        ensure_resources(
            &device,
            wgpu::TextureFormat::Rgba8Unorm,
            &mut resources,
            UpscaleMode::Nis,
        )
        .unwrap();
        let first = resources.get::<Resources>().unwrap() as *const Resources;
        ensure_resources(
            &device,
            wgpu::TextureFormat::Rgba8Unorm,
            &mut resources,
            UpscaleMode::Fsr1,
        )
        .unwrap();
        assert_eq!(
            first,
            resources.get::<Resources>().unwrap() as *const Resources
        );
    }

    #[test]
    fn shader_validation_failure_returns_error_and_keeps_standard_available() {
        let (device, _) = gpu();
        let error = checked_gpu_creation(&device, || {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Deliberately invalid spatial shader"),
                source: wgpu::ShaderSource::Wgsl("invalid shader".into()),
            })
        })
        .expect_err("Shader failure must return Result, not panic");
        let mut resources = CallbackResources::default();
        resources.insert(InitializationError(error.clone()));
        ensure_resources(
            &device,
            wgpu::TextureFormat::Rgba8Unorm,
            &mut resources,
            UpscaleMode::Standard,
        )
        .unwrap();
        assert_eq!(
            ensure_resources(
                &device,
                wgpu::TextureFormat::Rgba8Unorm,
                &mut resources,
                UpscaleMode::Fsr1
            )
            .unwrap_err(),
            error
        );
        assert!(resources.get::<Resources>().is_none());
        let _ordinary_video =
            super::super::color_gpu::Resources::new(&device, wgpu::TextureFormat::Rgba8Unorm);
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
            rgba: include_bytes!("vendor/reference-input.rgba").to_vec(),
            timestamp_ns: 0,
        });
        let fixtures: [(UpscaleMode, [&[u8]; 3]); 2] = [
            (
                UpscaleMode::Nis,
                [
                    include_bytes!("vendor/nis-0.rgba"),
                    include_bytes!("vendor/nis-1.rgba"),
                    include_bytes!("vendor/nis-2.rgba"),
                ],
            ),
            (
                UpscaleMode::Fsr1,
                [
                    include_bytes!("vendor/fsr-0.rgba"),
                    include_bytes!("vendor/fsr-1.rgba"),
                    include_bytes!("vendor/fsr-2.rgba"),
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
            .zip(include_bytes!("vendor/fsr-easu.rgba"))
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
            let path = super::super::color_integration::save_screenshot(
                &output,
                &Default::default(),
                false,
                &directory,
            )
            .unwrap();
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
                let mut standard = super::super::color_gpu::Resources::new(
                    &device,
                    wgpu::TextureFormat::Rgba8Unorm,
                );
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
}
