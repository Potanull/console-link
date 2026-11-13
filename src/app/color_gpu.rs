use std::sync::Arc;

use console_link::{capture::VideoFrame, color::ColorSettings};
use eframe::{egui, egui_wgpu};
use egui_wgpu::{Callback, CallbackResources, CallbackTrait, ScreenDescriptor};

pub(super) fn install(state: &egui_wgpu::RenderState) {
    let resources = Resources::new(&state.device, state.target_format);
    state.renderer.write().callback_resources.insert(resources);
}

pub(super) fn paint(
    ui: &egui::Ui,
    rect: egui::Rect,
    frame: Arc<VideoFrame>,
    color: ColorSettings,
    original: bool,
) {
    if frame.width == 0
        || frame.height == 0
        || frame.rgba.len() != frame.width as usize * frame.height as usize * 4
    {
        return;
    }
    ui.painter().add(Callback::new_paint_callback(
        rect,
        VideoCallback {
            frame,
            color,
            original,
        },
    ));
}

struct VideoCallback {
    frame: Arc<VideoFrame>,
    color: ColorSettings,
    original: bool,
}
impl CallbackTrait for VideoCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _: &ScreenDescriptor,
        _: &mut wgpu::CommandEncoder,
        resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if let Some(resources) = resources.get_mut::<Resources>() {
            resources.update(device, queue, &self.frame, self.color, self.original);
        }
        Vec::new()
    }
    fn paint(
        &self,
        _: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &CallbackResources,
    ) {
        if let Some(resources) = resources.get::<Resources>() {
            resources.draw(pass);
        }
    }
}

pub(super) use console_link::gpu::color::Resources;
