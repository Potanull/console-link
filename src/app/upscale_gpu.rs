//! Persistent spatial passes. Export creates an isolated instance on the same GPU.
use console_link::gpu::upscale::{Resources, checked_gpu_creation, validate};
use console_link::{
    capture::VideoFrame,
    color::ColorSettings,
    upscale::{UpscaleMode, UpscaleSettings},
};
use eframe::{egui, egui_wgpu};
use egui_wgpu::{Callback, CallbackResources, CallbackTrait, ScreenDescriptor};
use std::sync::Arc;

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

pub(super) use console_link::gpu::upscale::render_export;

#[cfg(test)]
mod tests {
    use super::*;
    fn gpu() -> (wgpu::Device, wgpu::Queue) {
        console_link::gpu::request_device().unwrap()
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
}
