//! Shared GPU video processing. Presentation never maps a GPU buffer to the CPU.
pub mod color;
pub mod upscale;

use crate::{
    capture::VideoFrame,
    color::ColorSettings,
    upscale::{UpscaleMode, UpscaleSettings},
};
use std::sync::Arc;

pub fn request_device() -> Result<(wgpu::Device, wgpu::Queue), String> {
    let instance = wgpu::Instance::default();
    let adapter = upscale::wait_for_gpu(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    }))
    .map_err(|e| e.to_string())?;
    upscale::wait_for_gpu(adapter.request_device(&wgpu::DeviceDescriptor::default()))
        .map_err(|e| e.to_string())
}

pub struct VideoRenderer {
    grade: color::Resources,
    spatial: Option<Result<upscale::Resources, String>>,
    format: wgpu::TextureFormat,
    warning: Option<String>,
    effective: UpscaleSettings,
    effective_target: [u32; 2],
    background: [u8; 3],
}

impl VideoRenderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Result<Self, String> {
        let grade =
            upscale::checked_gpu_creation(device, || color::Resources::new(device, format))?;
        Ok(Self {
            grade,
            spatial: None,
            format,
            warning: None,
            effective: UpscaleSettings::default(),
            effective_target: [1, 1],
            background: [0, 0, 0],
        })
    }

    fn spatial(&mut self, device: &wgpu::Device) -> Result<&mut upscale::Resources, String> {
        self.spatial
            .get_or_insert_with(|| {
                upscale::checked_gpu_creation(device, || {
                    upscale::Resources::new(device, self.format)
                })
            })
            .as_mut()
            .map_err(|e| e.clone())
    }

    pub fn rtx_status(&mut self, device: &wgpu::Device) -> Result<(), String> {
        self.spatial(device)?.rtx_status(device)
    }

    pub fn set_background(&mut self, rgb: [u8; 3]) {
        self.background = rgb;
    }

    pub fn effective_settings(&self) -> UpscaleSettings {
        self.effective
    }

    pub fn effective_target(&self) -> [u32; 2] {
        self.effective_target
    }

    pub fn warning(&self) -> Option<&str> {
        self.warning.as_deref()
    }

    /// `viewport` uses physical surface pixels. Both passes use this one target
    /// rectangle, while reconstruction textures retain the exact image size.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
        frame: Option<&Arc<VideoFrame>>,
        color: ColorSettings,
        original: bool,
        settings: UpscaleSettings,
        viewport: [f32; 4],
    ) -> Result<(), String> {
        self.warning = None;
        let target = [
            viewport[2].round().max(1.0) as u32,
            viewport[3].round().max(1.0) as u32,
        ];
        let viewport_target = target;
        let target = frame.map_or(target, |frame| {
            settings
                .output_resolution
                .target(target, [frame.width, frame.height])
        });
        let mut effective = settings;
        effective.mode = UpscaleMode::Standard;
        let enhanced = if let Some(frame) = frame {
            // Invalid input remains an error. Unsupported enhancement must never
            // stop an otherwise valid ordinary video stream.
            upscale::validate(
                frame,
                target,
                UpscaleSettings::default(),
                device.limits().max_texture_dimension_2d,
            )?;
            let mode = match upscale::validate(
                frame,
                target,
                settings,
                device.limits().max_texture_dimension_2d,
            ) {
                Ok(mode) => mode,
                Err(error) => {
                    self.warning = Some(error);
                    UpscaleMode::Standard
                }
            };
            if mode == UpscaleMode::RtxVideo && !cfg!(target_os = "windows") {
                self.warning = Some("RTX Video требует Windows и NVIDIA RTX.".into());
                false
            } else if mode != UpscaleMode::Standard {
                match self.spatial(device) {
                    Ok(spatial) => {
                        spatial.update(
                            device, queue, encoder, frame, color, original, settings, target,
                        );
                        let failure = if mode == UpscaleMode::RtxVideo {
                            spatial.error().map(str::to_owned)
                        } else {
                            None
                        };
                        if let Some(error) = failure {
                            self.warning = Some(error);
                            false
                        } else {
                            effective.mode = mode;
                            true
                        }
                    }
                    Err(error) => {
                        self.warning = Some(error);
                        false
                    }
                }
            } else {
                false
            }
        } else {
            false
        };
        self.effective = effective;
        self.effective_target = if enhanced { target } else { viewport_target };
        if !enhanced && let Some(frame) = frame {
            self.grade.update(device, queue, frame, color, original);
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Console Link native video"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear({
                        let channel = |n: u8| {
                            let v = f64::from(n) / 255.;
                            if self.format.is_srgb() {
                                if v <= 0.04045 {
                                    v / 12.92
                                } else {
                                    ((v + 0.055) / 1.055).powf(2.4)
                                }
                            } else {
                                v
                            }
                        };
                        wgpu::Color {
                            r: channel(self.background[0]),
                            g: channel(self.background[1]),
                            b: channel(self.background[2]),
                            a: 1.,
                        }
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        if frame.is_some()
            && viewport.iter().all(|v| v.is_finite())
            && viewport[2] > 0.0
            && viewport[3] > 0.0
        {
            pass.set_viewport(
                viewport[0].max(0.0),
                viewport[1].max(0.0),
                viewport[2],
                viewport[3],
                0.0,
                1.0,
            );
            if enhanced {
                self.spatial
                    .as_ref()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .draw(&mut pass);
            } else {
                self.grade.draw(&mut pass);
            }
        }
        Ok(())
    }
}

pub fn save_screenshot(
    frame: &VideoFrame,
    settings: &ColorSettings,
    original: bool,
    directory: &std::path::Path,
) -> Result<String, String> {
    if original || settings.is_identity() {
        return crate::runtime::save_screenshot(frame, directory);
    }
    let mut rgba = frame.rgba.clone();
    crate::color::apply_rgba(&mut rgba, settings)?;
    crate::runtime::save_screenshot(
        &VideoFrame {
            width: frame.width,
            height: frame.height,
            rgba,
            timestamp_ns: frame.timestamp_ns,
        },
        directory,
    )
}

/// Physical pixels for a contained image, clipped to the available surface.
pub fn fit_viewport(rect: [f32; 4], surface: [u32; 2], source: [u32; 2]) -> Option<[f32; 4]> {
    if !rect.iter().all(|n| n.is_finite()) || source.contains(&0) || surface.contains(&0) {
        return None;
    }
    let x = rect[0].clamp(0.0, surface[0] as f32);
    let y = rect[1].clamp(0.0, surface[1] as f32);
    let w = rect[2].max(0.0).min(surface[0] as f32 - x);
    let h = rect[3].max(0.0).min(surface[1] as f32 - y);
    if w < 1.0 || h < 1.0 {
        return None;
    }
    // Preserve the limiting dimension directly instead of multiplying a rounded
    // f32 ratio back into it (1000 / 1920 * 1920 can otherwise become 999.99994).
    let (width, height) =
        if f64::from(w) / f64::from(source[0]) <= f64::from(h) / f64::from(source[1]) {
            (
                w.floor(),
                (f64::from(w) * f64::from(source[1]) / f64::from(source[0]))
                    .floor()
                    .max(1.0) as f32,
            )
        } else {
            (
                (f64::from(h) * f64::from(source[0]) / f64::from(source[1]))
                    .floor()
                    .max(1.0) as f32,
                h.floor(),
            )
        };
    Some([
        (x + (w - width) / 2.0).floor(),
        (y + (h - height) / 2.0).floor(),
        width,
        height,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn letterbox_respects_surface_edges_and_invalid_sizes() {
        assert_eq!(
            fit_viewport([0., 0., 1000., 1000.], [1000, 1000], [1920, 1080]),
            Some([0., 219., 1000., 562.])
        );
        let v = fit_viewport([100., 50., 2000., 2000.], [1200, 800], [1920, 1080]).unwrap();
        assert!(v[0] + v[2] <= 1200. && v[1] + v[3] <= 800.);
        assert!(fit_viewport([0., 0., f32::NAN, 800.], [1200, 800], [1920, 1080]).is_none());
    }

    #[test]
    fn unsupported_nis_keeps_video_and_exports_the_effective_standard_recipe() {
        let (device, queue) = request_device().unwrap();
        let mut renderer = VideoRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm).unwrap();
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Fallback regression"),
            size: wgpu::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let frame = Arc::new(VideoFrame {
            width: 1,
            height: 1,
            rgba: vec![80, 128, 200, 255],
            timestamp_ns: 0,
        });
        let view = texture.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        let requested = UpscaleSettings {
            mode: UpscaleMode::Nis,
            ..Default::default()
        };
        renderer
            .draw(
                &device,
                &queue,
                &view,
                &mut encoder,
                Some(&frame),
                ColorSettings::default(),
                false,
                requested,
                [0., 0., 4., 4.],
            )
            .unwrap();
        assert_eq!(renderer.effective_settings().mode, UpscaleMode::Standard);
        assert!(renderer.warning().unwrap().contains("2x"));
        assert!(
            renderer.spatial.is_none(),
            "ordinary fallback must not compile spatial shaders"
        );
        queue.submit([encoder.finish()]);
        let exported = upscale::render_export(
            &device,
            &queue,
            frame,
            ColorSettings::default(),
            renderer.effective_settings(),
            [4, 4],
        )
        .unwrap();
        assert!(
            exported
                .as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| *pixel == [80, 128, 200, 255])
        );
    }
}
