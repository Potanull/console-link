//! A native video surface underneath GPUI, composed in the same OS window.
//!
//! GPUI owns input and its transparent overlay. Video uses the existing wgpu
//! device path, so ordinary playback never reads GPU pixels back into the CPU.
use gpui::Window;
use std::{
    cell::RefCell,
    future::Future,
    rc::Rc,
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
};

type SurfaceSlot = Rc<RefCell<Option<wgpu::Surface<'static>>>>;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos::NativeHost;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows::NativeHost;

/// Main-thread owned. Fields intentionally destroy the surface before its view.
pub struct Presenter {
    surface: SurfaceSlot,
    host: NativeHost,
    _instance: wgpu::Instance,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: [u32; 2],
}

impl Presenter {
    pub fn new(window: &mut Window) -> Result<Self, String> {
        let surface_slot = Rc::new(RefCell::new(None));
        let host = NativeHost::attach(window, Rc::clone(&surface_slot))?;
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: if cfg!(target_os = "macos") {
                wgpu::Backends::METAL
            } else {
                wgpu::Backends::DX12
            },
            ..Default::default()
        });
        let (raw_display_handle, raw_window_handle) = host.handles();
        // SAFETY: host owns the native view/HWND. The surface field is dropped
        // before host, and neither can leave this main-thread-only presenter.
        let surface = unsafe {
            instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle,
                raw_window_handle,
            })
        }
        .map_err(|error| error.to_string())?;
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .map_err(|error| format!("Не удалось выбрать GPU для видео: {error}"))?;
        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("Console Link video GPU"),
            required_limits: wgpu::Limits::default().using_resolution(adapter.limits()),
            ..Default::default()
        }))
        .map_err(|error| format!("Не удалось открыть GPU для видео: {error}"))?;
        let size = physical_size(window)?;
        if size
            .iter()
            .any(|&dimension| dimension > device.limits().max_texture_dimension_2d)
        {
            return Err("Размер окна превышает возможности GPU.".into());
        }
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .or_else(|| caps.formats.first().copied())
            .ok_or("У видеоповерхности нет формата изображения.")?;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size[0].max(1),
            height: size[1].max(1),
            present_mode: wgpu::PresentMode::AutoNoVsync,
            desired_maximum_frame_latency: 1,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
        };
        surface.configure(&device, &config);
        *surface_slot.borrow_mut() = Some(surface);
        Ok(Self {
            surface: surface_slot,
            host,
            _instance: instance,
            device,
            queue,
            config,
            size,
        })
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }
    pub fn format(&self) -> wgpu::TextureFormat {
        self.config.format
    }
    pub fn size(&self) -> [u32; 2] {
        self.size
    }

    pub fn resize(&mut self, window: &Window) -> Result<(), String> {
        self.host.resize()?;
        let size = physical_size(window)?;
        if size == self.size {
            return Ok(());
        }
        if size.contains(&0) {
            self.size = size;
            return Ok(());
        }
        if size
            .iter()
            .any(|&dimension| dimension > self.device.limits().max_texture_dimension_2d)
        {
            return Err("Размер окна превышает возможности GPU.".into());
        }
        self.config.width = size[0];
        self.config.height = size[1];
        if let Some(surface) = self.surface.borrow().as_ref() {
            surface.configure(&self.device, &self.config);
        }
        self.size = size;
        Ok(())
    }

    /// False means the compositor cannot accept a frame yet; keep the newest
    /// capture frame for the next UI tick rather than building a frame backlog.
    /// The draw closure must only encode GPU work, without dispatching UI/OS
    /// events or closing the parent window while a surface frame is acquired.
    pub fn present(
        &mut self,
        draw: impl FnOnce(
            &wgpu::Device,
            &wgpu::Queue,
            &wgpu::TextureView,
            &mut wgpu::CommandEncoder,
        ) -> Result<(), String>,
    ) -> Result<bool, String> {
        if self.size.contains(&0) {
            return Ok(false);
        }
        let current = self
            .surface
            .borrow()
            .as_ref()
            .ok_or("Видеоповерхность уже закрыта.")?
            .get_current_texture();
        let frame = match current {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Timeout) => return Ok(false),
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                if let Some(surface) = self.surface.borrow().as_ref() {
                    surface.configure(&self.device, &self.config);
                }
                return Ok(false);
            }
            Err(error) => return Err(format!("Не удалось показать видеокадр: {error}")),
        };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Console Link video frame"),
            });
        draw(&self.device, &self.queue, &view, &mut encoder)?;
        self.queue.submit([encoder.finish()]);
        frame.present();
        Ok(true)
    }
}

fn physical_size(window: &Window) -> Result<[u32; 2], String> {
    let size = window.viewport_size();
    scaled_size(
        f32::from(size.width),
        f32::from(size.height),
        window.scale_factor(),
    )
}

fn scaled_size(width: f32, height: f32, scale: f32) -> Result<[u32; 2], String> {
    if ![width, height, scale].into_iter().all(f32::is_finite)
        || width < 0.0
        || height < 0.0
        || scale <= 0.0
    {
        return Err("Некорректный размер видеоповерхности.".into());
    }
    let width = (f64::from(width) * f64::from(scale)).round();
    let height = (f64::from(height) * f64::from(scale)).round();
    if width > u32::MAX as f64 || height > u32::MAX as f64 {
        return Err("Размер видеоповерхности выходит за допустимые границы.".into());
    }
    Ok([width as u32, height as u32])
}

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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn surface_dimensions_follow_dpi_and_allow_minimized_zero_area() {
        assert_eq!(scaled_size(1240.5, 780.0, 2.0).unwrap(), [2481, 1560]);
        assert_eq!(scaled_size(0.0, 0.0, 2.0).unwrap(), [0, 0]);
        for dimensions in [
            (f32::NAN, 1., 1.),
            (1., 1., 0.),
            (-1., 1., 2.),
            (f32::MAX, 1., 2.),
        ] {
            assert!(scaled_size(dimensions.0, dimensions.1, dimensions.2).is_err());
        }
    }
}
