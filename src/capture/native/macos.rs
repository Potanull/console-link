//! AVFoundation capture. Native objects remain on one owner thread; the serial
//! delegate queue copies pixels before returning the driver's sample buffer.

use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use block2::RcBlock;
use dispatch2::{DispatchQueue, DispatchQueueAttr, DispatchRetained};
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{AnyThread, DefinedClass, define_class, msg_send};
use objc2_av_foundation::{
    AVAuthorizationStatus, AVCaptureConnection, AVCaptureDevice, AVCaptureDeviceFormat,
    AVCaptureDeviceInput, AVCaptureOutput, AVCaptureSession, AVCaptureVideoDataOutput,
    AVCaptureVideoDataOutputSampleBufferDelegate, AVMediaTypeVideo,
};
use objc2_core_media::{CMSampleBuffer, CMTime, CMVideoFormatDescriptionGetDimensions};
use objc2_core_video::{
    CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow, CVPixelBufferGetHeight,
    CVPixelBufferGetPixelFormatType, CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress,
    CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress, kCVPixelBufferPixelFormatTypeKey,
    kCVPixelFormatType_32BGRA,
};
use objc2_foundation::{NSDictionary, NSNumber, NSObject, NSObjectProtocol, NSString, ns_string};

use crate::capture::{CaptureSession, FrameSink, VideoDevice, VideoFrame, VideoMode, monotonic_ns};

struct DelegateState {
    sink: FrameSink,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements. The only ivar is a
    // thread-safe callback; native capture invokes it on a serial dispatch queue.
    #[unsafe(super(NSObject))]
    #[ivars = DelegateState]
    struct FrameDelegate;

    unsafe impl NSObjectProtocol for FrameDelegate {}

    unsafe impl AVCaptureVideoDataOutputSampleBufferDelegate for FrameDelegate {
        #[unsafe(method(captureOutput:didOutputSampleBuffer:fromConnection:))]
        unsafe fn captured(
            &self,
            _output: &AVCaptureOutput,
            sample: &CMSampleBuffer,
            _connection: &AVCaptureConnection,
        ) {
            let received_ns = monotonic_ns();
            // A Rust consumer panic must never unwind through an Objective-C
            // framework callback. Pixel storage is owned before invoking it.
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                if let Some(frame) = copy_sample(sample, received_ns) {
                    (self.ivars().sink)(frame);
                }
            }));
        }
    }
);

impl FrameDelegate {
    fn new(sink: FrameSink) -> Retained<Self> {
        let this = Self::alloc().set_ivars(DelegateState { sink });
        // SAFETY: NSObject's init has the standard initializer signature.
        unsafe { msg_send![super(this), init] }
    }
}

fn copy_sample(sample: &CMSampleBuffer, timestamp_ns: u64) -> Option<VideoFrame> {
    // SAFETY: AVFoundation owns a valid CMSampleBuffer throughout this callback.
    // image_buffer retains the image until this function returns.
    let image = unsafe { sample.image_buffer() }?;
    if CVPixelBufferGetPixelFormatType(&image) != kCVPixelFormatType_32BGRA {
        return None;
    }
    let width = CVPixelBufferGetWidth(&image);
    let height = CVPixelBufferGetHeight(&image);
    let stride = CVPixelBufferGetBytesPerRow(&image);
    let row_bytes = width.checked_mul(4)?;
    let byte_count = row_bytes.checked_mul(height)?;
    if width == 0
        || height == 0
        || stride < row_bytes
        || byte_count > crate::config::capture::MAX_FRAME_BYTES
    {
        return None;
    }
    // SAFETY: The retained image is valid. ReadOnly is paired with the same flag
    // on unlock; the guard also unlocks when conversion returns early.
    if unsafe { CVPixelBufferLockBaseAddress(&image, CVPixelBufferLockFlags::ReadOnly) } != 0 {
        return None;
    }
    struct Unlock<'a>(&'a objc2_core_video::CVPixelBuffer);
    impl Drop for Unlock<'_> {
        fn drop(&mut self) {
            // SAFETY: This guard exists only after a successful matching lock.
            unsafe { CVPixelBufferUnlockBaseAddress(self.0, CVPixelBufferLockFlags::ReadOnly) };
        }
    }
    let _unlock = Unlock(&image);
    let base = CVPixelBufferGetBaseAddress(&image).cast::<u8>();
    if base.is_null() || stride.checked_mul(height)? > isize::MAX as usize {
        return None;
    }
    let source_len = (height - 1).checked_mul(stride)?.checked_add(row_bytes)?;
    // SAFETY: BGRA is non-planar, base is locked and non-null, and CoreVideo
    // exposes height rows with at least width*4 accessible bytes per row.
    let source = unsafe { std::slice::from_raw_parts(base, source_len) };
    let rgba = bgra_to_rgba(source, stride, width, height)?;
    Some(VideoFrame {
        width: width as u32,
        height: height as u32,
        rgba,
        timestamp_ns,
    })
}

fn bgra_to_rgba(source: &[u8], stride: usize, width: usize, height: usize) -> Option<Vec<u8>> {
    let row_bytes = width.checked_mul(4)?;
    if width == 0 || height == 0 || stride < row_bytes {
        return None;
    }
    let needed = (height - 1).checked_mul(stride)?.checked_add(row_bytes)?;
    let size = row_bytes.checked_mul(height)?;
    if source.len() < needed || size > crate::config::capture::MAX_FRAME_BYTES {
        return None;
    }
    let mut rgba = vec![0; size];
    for y in 0..height {
        let row = &source[y * stride..y * stride + row_bytes];
        super::super::pixels::copy_bgra_row(row, &mut rgba[y * row_bytes..(y + 1) * row_bytes]);
    }
    Some(rgba)
}

// This legacy discovery method includes generic USB capture devices on macOS
// 13 without referencing newer external-camera device-type symbols.
#[allow(deprecated)]
fn devices() -> Retained<objc2_foundation::NSArray<AVCaptureDevice>> {
    // SAFETY: AVMediaTypeVideo is a framework constant and valid for discovery.
    unsafe { AVCaptureDevice::devicesWithMediaType(video_media_type()) }
}

fn video_media_type() -> &'static NSString {
    // AVMediaTypeVideo is weak-linked in generated bindings but is available on
    // all supported macOS versions. "vide" is its documented FourCC value.
    unsafe { AVMediaTypeVideo.unwrap_or(ns_string!("vide")) }
}

fn rate_from_duration(duration: CMTime) -> Option<(u32, u32)> {
    if duration.value <= 0
        || duration.timescale <= 0
        || !duration
            .flags
            .contains(objc2_core_media::CMTimeFlags::Valid)
    {
        return None;
    }
    let mut a = duration.timescale as u64;
    let mut b = duration.value as u64;
    let (num, den) = (a, b);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    Some((u32::try_from(num / a).ok()?, u32::try_from(den / a).ok()?))
}

fn mode_rates(format: &AVCaptureDeviceFormat) -> Vec<(u32, u32)> {
    // SAFETY: The immutable format object comes from AVCaptureDevice.formats.
    let ranges = unsafe { format.videoSupportedFrameRateRanges() };
    let mut rates = Vec::new();
    for range in &ranges {
        // SAFETY: These read-only properties are valid on an AVFrameRateRange.
        let (min, max, min_duration, max_duration) = unsafe {
            (
                range.minFrameRate(),
                range.maxFrameRate(),
                range.minFrameDuration(),
                range.maxFrameDuration(),
            )
        };
        for duration in [min_duration, max_duration] {
            if let Some(rate) = rate_from_duration(duration) {
                rates.push(rate);
            }
        }
        // A frame-rate range also supports common values between its bounds.
        for (num, den) in [
            (24, 1),
            (25, 1),
            (30, 1),
            (50, 1),
            (60, 1),
            (24000, 1001),
            (30000, 1001),
            (60000, 1001),
        ] {
            let rate = num as f64 / den as f64;
            if rate >= min - 0.00001 && rate <= max + 0.00001 {
                rates.push((num, den));
            }
        }
    }
    rates.retain(|&(num, den)| den > 0 && num > 0 && num as f64 / den as f64 <= 60.00001);
    rates.sort_unstable();
    rates.dedup();
    rates
}

pub fn enumerate() -> Result<Vec<VideoDevice>, String> {
    autoreleasepool(|_| {
        let mut result = Vec::new();
        for device in &devices() {
            let mut modes = Vec::new();
            // SAFETY: Enumerated capture devices expose immutable format arrays.
            for format in &unsafe { device.formats() } {
                let description = unsafe { format.formatDescription() };
                let size = unsafe { CMVideoFormatDescriptionGetDimensions(&description) };
                if size.width <= 0 || size.height <= 0 || size.width > 1920 || size.height > 1080 {
                    continue;
                }
                for (fps_num, fps_den) in mode_rates(&format) {
                    modes.push(VideoMode {
                        width: size.width as u32,
                        height: size.height as u32,
                        fps_num,
                        fps_den,
                    });
                }
            }
            modes.sort_unstable_by(|a, b| {
                (b.width * b.height)
                    .cmp(&(a.width * a.height))
                    .then_with(|| {
                        ((b.fps_num as u64) * a.fps_den as u64)
                            .cmp(&((a.fps_num as u64) * b.fps_den as u64))
                    })
            });
            modes.dedup();
            result.push(VideoDevice {
                id: unsafe { device.uniqueID() }.to_string(),
                name: unsafe { device.localizedName() }.to_string(),
                modes,
            });
        }
        Ok(result)
    })
}

fn check_permission() -> Result<(), String> {
    // SAFETY: Video is an allowed media type for the authorization API.
    let status = unsafe { AVCaptureDevice::authorizationStatusForMediaType(video_media_type()) };
    if status == AVAuthorizationStatus::Authorized {
        return Ok(());
    }
    if status == AVAuthorizationStatus::NotDetermined {
        let completion = RcBlock::new(|_: objc2::runtime::Bool| {});
        // SAFETY: The block has the documented Bool signature; AVFoundation
        // copies it for the asynchronous permission response.
        unsafe {
            AVCaptureDevice::requestAccessForMediaType_completionHandler(
                video_media_type(),
                &completion,
            )
        };
        return Err("Approve camera access for Console Link, then connect again. Capture cards require the macOS Camera permission.".into());
    }
    Err("Camera access is denied. Enable Console Link in System Settings > Privacy & Security > Camera, then connect again.".into())
}

struct NativeSession {
    session: Retained<AVCaptureSession>,
    output: Retained<AVCaptureVideoDataOutput>,
    _delegate: Retained<FrameDelegate>,
    queue: DispatchRetained<DispatchQueue>,
}

impl Drop for NativeSession {
    fn drop(&mut self) {
        // SAFETY: Session and output belong to this worker. Stop delivery, detach
        // the delegate, then drain the serial callback queue before releasing it.
        unsafe {
            self.session.stopRunning();
            self.output.setSampleBufferDelegate_queue(None, None);
        }
        self.queue.exec_sync(|| {});
    }
}

fn open(device_id: &str, mode: &VideoMode, sink: FrameSink) -> Result<NativeSession, String> {
    let device = devices()
        .into_iter()
        .find(|d| unsafe { d.uniqueID() }.to_string() == device_id)
        .ok_or("The selected capture device is disconnected. Refresh devices and reconnect.")?;
    let selected_format = unsafe { device.formats() }.into_iter().find(|format| {
        let description = unsafe { format.formatDescription() };
        let size = unsafe { CMVideoFormatDescriptionGetDimensions(&description) };
        size.width == mode.width as i32 && size.height == mode.height as i32
            && mode_rates(format).contains(&(mode.fps_num, mode.fps_den))
    }).ok_or("This capture mode is no longer available. Refresh devices and choose an advertised mode.")?;

    // SAFETY: Every method receives the corresponding AVFoundation object;
    // canAdd checks prevent invalid session graph changes.
    let session = unsafe { AVCaptureSession::new() };
    let output = unsafe { AVCaptureVideoDataOutput::new() };
    let input =
        unsafe { AVCaptureDeviceInput::deviceInputWithDevice_error(&device) }.map_err(|e| {
            format!(
                "Cannot open capture device (it may be busy): {}",
                e.localizedDescription()
            )
        })?;
    unsafe { session.beginConfiguration() };
    if !unsafe { session.canAddInput(&input) } || !unsafe { session.canAddOutput(&output) } {
        unsafe { session.commitConfiguration() };
        return Err("The device cannot supply video to an AVFoundation capture session.".into());
    }
    unsafe {
        session.addInput(&input);
        session.addOutput(&output);
    }
    // InputPriority is an iOS/Catalyst preset, not a macOS capability gate.
    // Configure the advertised macOS device format and frame durations directly.
    unsafe { device.lockForConfiguration() }.map_err(|e| {
        unsafe { session.commitConfiguration() };
        format!(
            "Cannot configure capture device: {}",
            e.localizedDescription()
        )
    })?;
    // SAFETY: selected_format and the rational duration were enumerated from
    // this same device; the configuration lock is held for every mutation.
    unsafe {
        device.setActiveFormat(&selected_format);
        let duration = CMTime {
            value: mode.fps_den as i64,
            timescale: mode.fps_num as i32,
            flags: objc2_core_media::CMTimeFlags::Valid,
            epoch: 0,
        };
        device.setActiveVideoMinFrameDuration(duration);
        device.setActiveVideoMaxFrameDuration(duration);
        device.unlockForConfiguration();
    }

    // SAFETY: CoreVideo's CFString key is toll-free bridged to NSString.
    let key = unsafe { &*(kCVPixelBufferPixelFormatTypeKey as *const _ as *const NSString) };
    let pixel_format = NSNumber::numberWithUnsignedInt(kCVPixelFormatType_32BGRA);
    if !unsafe { output.availableVideoCVPixelFormatTypes() }
        .into_iter()
        .any(|format| format.unsignedIntValue() == kCVPixelFormatType_32BGRA)
    {
        unsafe { session.commitConfiguration() };
        return Err(
            "The system cannot convert this capture mode to BGRA. Choose another advertised mode."
                .into(),
        );
    }
    let settings = NSDictionary::from_slices(&[key], &[&*pixel_format as &AnyObject]);
    let delegate = FrameDelegate::new(sink);
    let queue = DispatchQueue::new("dev.console-link.capture", DispatchQueueAttr::SERIAL);
    unsafe {
        output.setVideoSettings(Some(&settings));
        output.setAlwaysDiscardsLateVideoFrames(true);
        output.setSampleBufferDelegate_queue(
            Some(ProtocolObject::from_ref(&*delegate)),
            Some(&queue),
        );
        session.commitConfiguration();
        session.startRunning();
    }
    let owned = NativeSession {
        session,
        output,
        _delegate: delegate,
        queue,
    };
    if !unsafe { owned.session.isRunning() } {
        return Err("Capture could not start. Check the card, Camera permission, and other apps using the device.".into());
    }
    // Preset-managed drivers may renegotiate at startRunning. Never report a
    // different input size as the mode the user selected.
    let actual_format = unsafe { device.activeFormat() };
    let actual_description = unsafe { actual_format.formatDescription() };
    let actual_size = unsafe { CMVideoFormatDescriptionGetDimensions(&actual_description) };
    if actual_size.width != mode.width as i32 || actual_size.height != mode.height as i32 {
        return Err(format!(
            "The card selected {}x{} instead of {}x{}. Choose another supported mode.",
            actual_size.width, actual_size.height, mode.width, mode.height
        ));
    }
    // UVC drivers may expose a frame-duration range after starting. It is
    // not a delivery-rate guarantee: keep receiving frames and measure their
    // actual rate in the UI instead of rejecting a usable video stream.
    Ok(owned)
}

struct Capture {
    stop: Option<mpsc::Sender<()>>,
    worker: Option<JoinHandle<()>>,
}

impl CaptureSession for Capture {}

impl Drop for Capture {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub fn start(
    device_id: &str,
    mode: &VideoMode,
    sink: FrameSink,
) -> Result<Box<dyn CaptureSession>, String> {
    if mode.fps_num == 0
        || mode.fps_num > i32::MAX as u32
        || mode.fps_den == 0
        || mode.width == 0
        || mode.height == 0
    {
        return Err("Invalid capture mode.".into());
    }
    check_permission()?;
    let device_id = device_id.to_owned();
    let mode = mode.clone();
    let (stop_tx, stop_rx) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let worker = thread::Builder::new()
        .name("capture-macos".into())
        .spawn(move || {
            autoreleasepool(|_| match open(&device_id, &mode, sink) {
                Ok(session) => {
                    if ready_tx.send(Ok(())).is_ok() {
                        let _ = stop_rx.recv();
                    }
                    drop(session);
                }
                Err(error) => {
                    let _ = ready_tx.send(Err(error));
                }
            });
        })
        .map_err(|e| format!("Cannot start capture worker: {e}"))?;
    match ready_rx.recv_timeout(Duration::from_secs(20)) {
        Ok(Ok(())) => Ok(Box::new(Capture {
            stop: Some(stop_tx),
            worker: Some(worker),
        })),
        Ok(Err(error)) => {
            let _ = worker.join();
            Err(error)
        }
        Err(_) => {
            // The worker owns all native objects. Closing stop guarantees it
            // will stop even when a slow driver eventually finishes opening.
            drop(stop_tx);
            Err("Capture initialization timed out. Check the USB connection and device permissions.".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rational_rate_preserves_ntsc_and_reduces_fractions() {
        let duration = CMTime {
            value: 1001,
            timescale: 60000,
            flags: objc2_core_media::CMTimeFlags::Valid,
            epoch: 0,
        };
        assert_eq!(rate_from_duration(duration), Some((60000, 1001)));
        assert_eq!(
            rate_from_duration(CMTime {
                value: 2,
                timescale: 120,
                ..duration
            }),
            Some((60, 1))
        );
        assert_eq!(
            rate_from_duration(CMTime {
                value: 0,
                ..duration
            }),
            None
        );
    }

    #[test]
    fn bgra_conversion_respects_row_padding_and_forces_opaque_alpha() {
        let source = [
            1, 2, 3, 0, 7, 8, 9, 4, 99, 99, 99, 99, 10, 11, 12, 0, 20, 21, 22, 0,
        ];
        assert_eq!(
            bgra_to_rgba(&source, 12, 2, 2),
            Some(vec![
                3, 2, 1, 255, 9, 8, 7, 255, 12, 11, 10, 255, 22, 21, 20, 255
            ])
        );
        assert!(bgra_to_rgba(&source[..19], 12, 2, 2).is_none());
        assert!(bgra_to_rgba(&source, 7, 2, 2).is_none());
    }
}
