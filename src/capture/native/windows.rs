//! Media Foundation capture. COM and media resources stay on one worker thread.

use super::super::pixels;
use crate::capture::{CaptureSession, FrameSink, VideoDevice, VideoFrame, VideoMode, monotonic_ns};
use ::windows::Win32::Media::MediaFoundation::*;
use ::windows::Win32::System::Com::{
    COINIT_MULTITHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize,
};
use ::windows::core::{HRESULT, Interface, PWSTR, Ref, implement};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const VIDEO: u32 = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;

struct Runtime;

impl Runtime {
    unsafe fn new() -> Result<Self, String> {
        // Every caller owns a dedicated MTA worker, never the UI apartment.
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .map_err(describe)?
        };
        if let Err(error) = unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) } {
            unsafe { CoUninitialize() };
            return Err(describe(error));
        }
        Ok(Self)
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        unsafe {
            let _ = MFShutdown();
            CoUninitialize();
        }
    }
}

struct Source(IMFMediaSource);

impl Drop for Source {
    fn drop(&mut self) {
        // Shutdown explicitly releases the camera even if another COM reference survives.
        unsafe {
            let _ = self.0.Shutdown();
        }
    }
}

fn describe(error: ::windows::core::Error) -> String {
    format!(
        "Media Foundation: {error}. Check camera permissions and close other apps using this card."
    )
}

unsafe fn attributes(capacity: u32) -> Result<IMFAttributes, String> {
    let mut result = None;
    unsafe { MFCreateAttributes(&mut result, capacity).map_err(describe)? };
    result.ok_or_else(|| "Media Foundation returned no attributes".into())
}

unsafe fn string_attribute(
    attributes: &IMFAttributes,
    key: &::windows::core::GUID,
) -> Result<String, String> {
    let mut value = PWSTR::null();
    let mut len = 0;
    unsafe {
        attributes
            .GetAllocatedString(key, &mut value, &mut len)
            .map_err(describe)?
    };
    if value.is_null() {
        return Err("Capture device returned an empty identifier".into());
    }
    let result =
        unsafe { String::from_utf16_lossy(std::slice::from_raw_parts(value.0, len as usize)) };
    unsafe { CoTaskMemFree(Some(value.0.cast())) };
    Ok(result)
}

unsafe fn activations() -> Result<Vec<IMFActivate>, String> {
    let attrs = unsafe { attributes(1)? };
    unsafe {
        attrs
            .SetGUID(
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
            )
            .map_err(describe)?
    };
    let mut array = std::ptr::null_mut();
    let mut count = 0;
    unsafe { MFEnumDeviceSources(&attrs, &mut array, &mut count).map_err(describe)? };
    if array.is_null() {
        return if count == 0 {
            Ok(Vec::new())
        } else {
            Err("Invalid capture-device list".into())
        };
    }
    let mut devices = Vec::with_capacity(count as usize);
    // MF allocates an array of owned interface pointers. Move each Option out before freeing
    // the array, so each COM reference is released exactly once by the Vec.
    for slot in unsafe { std::slice::from_raw_parts_mut(array, count as usize) } {
        if let Some(device) = slot.take() {
            devices.push(device);
        }
    }
    unsafe { CoTaskMemFree(Some(array.cast())) };
    Ok(devices)
}

unsafe fn mode_from_type(media_type: &IMFMediaType) -> Option<VideoMode> {
    let size = unsafe { media_type.GetUINT64(&MF_MT_FRAME_SIZE).ok()? };
    let rate = unsafe { media_type.GetUINT64(&MF_MT_FRAME_RATE).ok()? };
    let mode = VideoMode {
        width: (size >> 32) as u32,
        height: size as u32,
        fps_num: (rate >> 32) as u32,
        fps_den: rate as u32,
    };
    (mode.width > 0 && mode.height > 0 && mode.fps_num > 0 && mode.fps_den > 0).then_some(mode)
}

pub fn enumerate() -> Result<Vec<VideoDevice>, String> {
    thread::Builder::new()
        .name("mf-enumerate".into())
        .spawn(|| unsafe {
            let _runtime = Runtime::new()?;
            let mut result = Vec::new();
            for activation in activations()? {
                let id = string_attribute(
                    &activation,
                    &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
                )?;
                let name = string_attribute(&activation, &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME)?;
                let mut modes = Vec::new();
                // One unavailable device must not hide other capture cards. It remains visible
                // with an empty mode list and can be retried after the other application exits.
                if let Ok(source) = activation.ActivateObject::<IMFMediaSource>() {
                    let source = Source(source);
                    if let Ok(reader) = MFCreateSourceReaderFromMediaSource(&source.0, None) {
                        for index in 0..4096 {
                            match reader.GetNativeMediaType(VIDEO, index) {
                                Ok(media_type) => {
                                    if let Some(mode) = mode_from_type(&media_type)
                                        && !modes.contains(&mode)
                                    {
                                        modes.push(mode);
                                    }
                                }
                                Err(_) => break,
                            }
                        }
                    }
                }
                modes.sort_by_key(|m| {
                    (
                        m.width,
                        m.height,
                        (m.fps_num as u64 * 1_000_000) / m.fps_den as u64,
                    )
                });
                result.push(VideoDevice { id, name, modes });
            }
            Ok(result)
        })
        .map_err(|e| e.to_string())?
        .join()
        .map_err(|_| "Device enumeration worker failed".to_string())?
}

enum Completion {
    Frame,
    Failed(String),
}

#[implement(IMFSourceReaderCallback)]
struct ReaderCallback {
    events: mpsc::Sender<Completion>,
    sink: FrameSink,
    stop: Arc<AtomicBool>,
    width: u32,
    height: u32,
    stride: Arc<AtomicI32>,
}

impl IMFSourceReaderCallback_Impl for ReaderCallback_Impl {
    fn OnReadSample(
        &self,
        status: HRESULT,
        _stream: u32,
        flags: u32,
        _timestamp: i64,
        sample: Ref<IMFSample>,
    ) -> ::windows::core::Result<()> {
        if self.stop.load(Ordering::Acquire) {
            return Ok(());
        }
        let fatal = MF_SOURCE_READERF_ERROR.0
            | MF_SOURCE_READERF_ENDOFSTREAM.0
            | MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0
            | MF_SOURCE_READERF_NATIVEMEDIATYPECHANGED.0;
        let completion = if let Err(error) = status.ok() {
            Completion::Failed(describe(error))
        } else if flags & fatal as u32 != 0 {
            Completion::Failed(
                "Capture device stopped or changed its video format. Reconnect the card.".into(),
            )
        } else if let Some(sample) = sample.as_ref() {
            let timestamp_ns = monotonic_ns();
            match unsafe {
                copy_sample(
                    sample,
                    self.width,
                    self.height,
                    self.stride.load(Ordering::Acquire),
                )
            } {
                Ok(rgba) => {
                    if !self.stop.load(Ordering::Acquire) {
                        match delivery::deliver_frame(
                            &self.sink,
                            VideoFrame {
                                width: self.width,
                                height: self.height,
                                rgba,
                                timestamp_ns,
                            },
                        ) {
                            Ok(()) => Completion::Frame,
                            Err(error) => Completion::Failed(error),
                        }
                    } else {
                        Completion::Frame
                    }
                }
                Err(error) => Completion::Failed(error),
            }
        } else {
            // Stream ticks have no frame; continue issuing requests without reusing pixels.
            Completion::Frame
        };
        let _ = self.events.send(completion);
        Ok(())
    }

    fn OnFlush(&self, _stream: u32) -> ::windows::core::Result<()> {
        Ok(())
    }
    fn OnEvent(&self, _stream: u32, _event: Ref<IMFMediaEvent>) -> ::windows::core::Result<()> {
        Ok(())
    }
}

unsafe fn copy_sample(
    sample: &IMFSample,
    width: u32,
    height: u32,
    stride: i32,
) -> Result<Vec<u8>, String> {
    let buffer = unsafe { sample.ConvertToContiguousBuffer().map_err(describe)? };
    // 2D buffers can have padding and negative pitch. Lock2DSize exposes allocation
    // bounds as well as the address of the first displayed scanline.
    if let Ok(two_d) = buffer.cast::<IMF2DBuffer2>() {
        let mut scanline = std::ptr::null_mut();
        let mut pitch = 0;
        let mut start = std::ptr::null_mut();
        let mut length = 0;
        if unsafe {
            two_d.Lock2DSize(
                MF2DBuffer_LockFlags_Read,
                &mut scanline,
                &mut pitch,
                &mut start,
                &mut length,
            )
        }
        .is_ok()
        {
            let result =
                if start.is_null() || scanline.is_null() || (scanline as usize) < start as usize {
                    Err("Invalid native frame buffer".into())
                } else {
                    let data = unsafe { std::slice::from_raw_parts(start, length as usize) };
                    pixels::rgb32_to_rgba(
                        data,
                        scanline as usize - start as usize,
                        width,
                        height,
                        pitch,
                    )
                };
            unsafe {
                let _ = two_d.Unlock2D();
            }
            return result;
        }
    }
    let mut pointer = std::ptr::null_mut();
    let mut length = 0;
    unsafe {
        buffer
            .Lock(&mut pointer, None, Some(&mut length))
            .map_err(describe)?
    };
    let result = if pointer.is_null() {
        Err("Capture returned an empty frame buffer".into())
    } else {
        let data = unsafe { std::slice::from_raw_parts(pointer, length as usize) };
        let row_zero = if stride < 0 {
            (height.saturating_sub(1) as usize)
                .checked_mul(stride.unsigned_abs() as usize)
                .ok_or_else(|| "Frame offset overflow".to_string())
        } else {
            Ok(0)
        };
        row_zero.and_then(|offset| pixels::rgb32_to_rgba(data, offset, width, height, stride))
    };
    unsafe {
        let _ = buffer.Unlock();
    }
    result
}

struct Session {
    stop: Arc<AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
    worker: Option<JoinHandle<()>>,
}

impl CaptureSession for Session {
    fn error(&self) -> Option<String> {
        self.error.lock().ok().and_then(|error| error.clone())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        // Never block the UI on a device driver. The worker wakes within 100ms,
        // flushes pending requests, and shuts down the source on its own apartment.
        // The callback checks this flag before delivering any further frame.
        if let Some(worker) = self.worker.take()
            && worker.is_finished()
        {
            let _ = worker.join();
        }
    }
}

pub fn start(
    device_id: &str,
    mode: &VideoMode,
    sink: FrameSink,
) -> Result<Box<dyn CaptureSession>, String> {
    if mode.width == 0 || mode.height == 0 || mode.fps_num == 0 || mode.fps_den == 0 {
        return Err("Select a valid device video mode".into());
    }
    let device_id = device_id.to_owned();
    let mode = mode.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = stop.clone();
    let error = Arc::new(Mutex::new(None));
    let worker_error = error.clone();
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let worker = thread::Builder::new()
        .name("mf-capture".into())
        .spawn(move || unsafe {
            let outcome = run_capture(&device_id, &mode, sink, worker_stop, &ready_tx);
            if let Err(error) = outcome {
                if let Ok(mut last_error) = worker_error.lock() {
                    *last_error = Some(error.clone());
                }
                let _ = ready_tx.send(Err(error));
            }
        })
        .map_err(|e| e.to_string())?;
    match ready_rx.recv_timeout(Duration::from_secs(8)) {
        Ok(Ok(())) => Ok(Box::new(Session {
            stop,
            error,
            worker: Some(worker),
        })),
        Ok(Err(error)) => {
            stop.store(true, Ordering::Release);
            Err(error)
        }
        Err(_) => {
            stop.store(true, Ordering::Release);
            Err(
                "Capture card did not open within 8 seconds. Check permissions and USB connection."
                    .into(),
            )
        }
    }
}

unsafe fn run_capture(
    device_id: &str,
    mode: &VideoMode,
    sink: FrameSink,
    stop: Arc<AtomicBool>,
    ready: &mpsc::SyncSender<Result<(), String>>,
) -> Result<(), String> {
    let _runtime = unsafe { Runtime::new()? };
    let mut selected = None;
    for activation in unsafe { activations()? } {
        if unsafe {
            string_attribute(
                &activation,
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
            )?
        } == device_id
        {
            selected = Some(activation);
            break;
        }
    }
    let activation = selected.ok_or("Selected capture device is not connected")?;
    let source = Source(unsafe {
        activation
            .ActivateObject::<IMFMediaSource>()
            .map_err(describe)?
    });
    let (events_tx, events_rx) = mpsc::channel();
    let stride = Arc::new(AtomicI32::new(
        (mode.width.checked_mul(4).ok_or("Frame width overflow")?) as i32,
    ));
    let callback: IMFSourceReaderCallback = ReaderCallback {
        events: events_tx,
        sink,
        stop: stop.clone(),
        width: mode.width,
        height: mode.height,
        stride: stride.clone(),
    }
    .into();
    let attrs = unsafe { attributes(3)? };
    unsafe {
        attrs
            .SetUnknown(&MF_SOURCE_READER_ASYNC_CALLBACK, &callback)
            .map_err(describe)?;
        attrs
            .SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)
            .map_err(describe)?;
        attrs.SetUINT32(&MF_LOW_LATENCY, 1).map_err(describe)?;
    }
    let reader =
        unsafe { MFCreateSourceReaderFromMediaSource(&source.0, &attrs).map_err(describe)? };
    let mut native_type = None;
    for index in 0..4096 {
        match unsafe { reader.GetNativeMediaType(VIDEO, index) } {
            Ok(media_type) if unsafe { mode_from_type(&media_type) }.as_ref() == Some(mode) => {
                native_type = Some(media_type);
                break;
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    let native_type = native_type
        .ok_or("This exact resolution and frame rate are not available on the selected card")?;
    unsafe {
        reader
            .SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)
            .map_err(describe)?;
        reader.SetStreamSelection(VIDEO, true).map_err(describe)?;
        reader
            .SetCurrentMediaType(VIDEO, None, &native_type)
            .map_err(describe)?;
    }
    let output = unsafe { MFCreateMediaType().map_err(describe)? };
    unsafe {
        output
            .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
            .map_err(describe)?;
        output
            .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)
            .map_err(describe)?;
        output
            .SetUINT64(
                &MF_MT_FRAME_SIZE,
                ((mode.width as u64) << 32) | mode.height as u64,
            )
            .map_err(describe)?;
        output
            .SetUINT64(
                &MF_MT_FRAME_RATE,
                ((mode.fps_num as u64) << 32) | mode.fps_den as u64,
            )
            .map_err(describe)?;
        reader
            .SetCurrentMediaType(VIDEO, None, &output)
            .map_err(describe)?;
    }
    let actual = unsafe { reader.GetCurrentMediaType(VIDEO).map_err(describe)? };
    if unsafe { mode_from_type(&actual) }.as_ref() != Some(mode) {
        return Err("The card negotiated a different resolution or frame rate; select a supported mode explicitly".into());
    }
    let actual_stride = unsafe { actual.GetUINT32(&MF_MT_DEFAULT_STRIDE) }
        .map(|v| v as i32)
        .or_else(|_| unsafe {
            MFGetStrideForBitmapInfoHeader(MFVideoFormat_RGB32.data1, mode.width)
        })
        .map_err(describe)?;
    stride.store(actual_stride, Ordering::Release);
    if stop.load(Ordering::Acquire) {
        return Ok(());
    }
    unsafe {
        reader
            .ReadSample(VIDEO, 0, None, None, None, None)
            .map_err(describe)?
    };
    let _ = ready.send(Ok(()));
    while !stop.load(Ordering::Acquire) {
        match events_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Completion::Frame) => {
                if !stop.load(Ordering::Acquire) {
                    unsafe {
                        reader
                            .ReadSample(VIDEO, 0, None, None, None, None)
                            .map_err(describe)?
                    };
                }
            }
            Ok(Completion::Failed(error)) => return Err(error),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    unsafe {
        let _ = reader.Flush(VIDEO);
    }
    Ok(())
}

mod delivery {
    use crate::capture::{FrameSink, VideoFrame};

    pub fn deliver_frame(sink: &FrameSink, frame: VideoFrame) -> Result<(), String> {
        // Rust panics must never unwind through the COM callback ABI. A failed
        // consumer ends the request loop through Completion::Failed.
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| sink(frame)))
            .map_err(|_| "Video frame consumer panicked; capture stopped safely".to_string())
    }

    #[cfg(test)]
    mod tests {
        use super::deliver_frame;
        use crate::capture::{FrameSink, VideoFrame};
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        fn frame() -> VideoFrame {
            VideoFrame {
                width: 1,
                height: 1,
                rgba: vec![0, 0, 0, 255],
                timestamp_ns: 1,
            }
        }

        #[test]
        fn sink_panic_becomes_a_capture_failure_instead_of_crossing_com_abi() {
            let sink: FrameSink = Arc::new(|_| panic!("sink failed"));
            assert_eq!(
                deliver_frame(&sink, frame()).unwrap_err(),
                "Video frame consumer panicked; capture stopped safely"
            );
        }

        #[test]
        fn healthy_sink_receives_one_owned_frame() {
            let count = Arc::new(AtomicUsize::new(0));
            let observed = count.clone();
            let sink: FrameSink = Arc::new(move |frame| {
                assert_eq!(frame.rgba, [0, 0, 0, 255]);
                observed.fetch_add(1, Ordering::Relaxed);
            });
            deliver_frame(&sink, frame()).unwrap();
            assert_eq!(count.load(Ordering::Relaxed), 1);
        }
    }
}
