//! DX12 RTX Video SDK 1.1 interop. The bridge exports the SDK's documented C ABI.
//! Native command lists use wgpu's device and queue, never a second queue or CPU pixels.
use std::{
    ffi::c_void,
    path::Path,
    sync::{Arc, Mutex, OnceLock, Weak},
};
use windows58::{
    Win32::{
        Foundation::{CloseHandle, FreeLibrary, HMODULE, WAIT_OBJECT_0},
        Graphics::Direct3D12::*,
        System::{
            LibraryLoader::{
                GetProcAddress, LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR,
                LoadLibraryExW,
            },
            Threading::{CreateEventW, WaitForSingleObject},
        },
    },
    core::{Interface, PCSTR, PCWSTR},
};

type Ptr = *mut c_void;
type Init = unsafe extern "C" fn(u64, *const u16, Ptr, Ptr, u32) -> u32;
type Unary = unsafe extern "C" fn(Ptr) -> u32;
type Params = unsafe extern "C" fn(*mut Ptr) -> u32;
type Create = unsafe extern "C" fn(Ptr, u32, Ptr, *mut Ptr) -> u32;
type Evaluate = unsafe extern "C" fn(Ptr, Ptr, Ptr, Ptr) -> u32;
type SetUi = unsafe extern "C" fn(Ptr, *const u8, u32);
type SetResource = unsafe extern "C" fn(Ptr, *const u8, Ptr);
type GetInt = unsafe extern "C" fn(Ptr, *const u8, *mut i32) -> u32;
struct Api {
    module: HMODULE,
    init: Init,
    shutdown: Unary,
    parameters: Params,
    destroy_parameters: Unary,
    create: Create,
    evaluate: Evaluate,
    release: Unary,
    set_ui: SetUi,
    set_resource: SetResource,
    get_int: GetInt,
}
impl Api {
    fn load(directory: &Path) -> Result<Self, String> {
        let runtime = directory.join("nvngx_vsr.dll");
        if !runtime.is_file() {
            return Err(
                "RTX Video runtime missing: install the release nvngx_vsr.dll beside Console Link"
                    .into(),
            );
        }
        let bridge = directory.join("console_link_ngx.dll");
        if !bridge.is_file() {
            return Err(
                "RTX Video SDK bridge missing: install console_link_ngx.dll beside Console Link"
                    .into(),
            );
        }
        let path: Vec<u16> = bridge
            .as_os_str()
            .to_string_lossy()
            .encode_utf16()
            .chain(Some(0))
            .collect();
        // Absolute path plus restricted search prevents loading a bridge from the working directory.
        let module = unsafe {
            LoadLibraryExW(
                PCWSTR(path.as_ptr()),
                None,
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
            )
        }
        .map_err(|e| format!("Cannot load RTX Video bridge: {e}"))?;
        macro_rules! symbol {
            ($name:literal, $ty:ty) => {{
                let symbol =
                    unsafe { GetProcAddress(module, PCSTR(concat!($name, "\0").as_ptr())) };
                match symbol {
                    Some(f) => unsafe {
                        std::mem::transmute::<unsafe extern "system" fn() -> isize, $ty>(f)
                    },
                    None => {
                        unsafe {
                            let _ = FreeLibrary(module);
                        }
                        return Err(concat!("RTX Video bridge missing export ", $name).into());
                    }
                }
            }};
        }
        Ok(Self {
            module,
            init: symbol!("NVSDK_NGX_D3D12_Init", Init),
            shutdown: symbol!("NVSDK_NGX_D3D12_Shutdown1", Unary),
            parameters: symbol!("NVSDK_NGX_D3D12_GetCapabilityParameters", Params),
            destroy_parameters: symbol!("NVSDK_NGX_D3D12_DestroyParameters", Unary),
            create: symbol!("NVSDK_NGX_D3D12_CreateFeature", Create),
            evaluate: symbol!("NVSDK_NGX_D3D12_EvaluateFeature_C", Evaluate),
            release: symbol!("NVSDK_NGX_D3D12_ReleaseFeature", Unary),
            set_ui: symbol!("NVSDK_NGX_Parameter_SetUI", SetUi),
            set_resource: symbol!("NVSDK_NGX_Parameter_SetD3d12Resource", SetResource),
            get_int: symbol!("NVSDK_NGX_Parameter_GetI", GetInt),
        })
    }
}
impl Drop for Api {
    fn drop(&mut self) {
        unsafe {
            let _ = FreeLibrary(self.module);
        }
    }
}
fn ngx(result: u32, stage: &str) -> Result<(), String> {
    if result & 0xfff00000 == 0xbad00000 {
        Err(format!("RTX Video {stage} failed (NGX 0x{result:08X})"))
    } else {
        Ok(())
    }
}
fn dx<T>(result: windows58::core::Result<T>) -> Result<T, String> {
    result.map_err(|e| format!("RTX Video DX12: {e}"))
}
static SDK: Mutex<()> = Mutex::new(());
type Shared = Arc<Mutex<Session>>;
static SESSIONS: OnceLock<Mutex<Vec<Weak<Mutex<Session>>>>> = OnceLock::new();
struct Slot {
    allocator: ID3D12CommandAllocator,
    list: ID3D12GraphicsCommandList,
    done: u64,
    // wgpu must retain its allocation as well as the raw COM resource while NGX references it.
    textures: Option<[wgpu::Texture; 2]>,
    #[cfg(test)]
    timer: Timer,
}
#[cfg(test)]
struct Timer {
    heap: ID3D12QueryHeap,
    readback: ID3D12Resource,
    pending: bool,
}
#[cfg(test)]
impl Timer {
    fn new(device: &ID3D12Device) -> Result<Self, String> {
        unsafe {
            let mut heap = None;
            dx(device.CreateQueryHeap(
                &D3D12_QUERY_HEAP_DESC {
                    Type: D3D12_QUERY_HEAP_TYPE_TIMESTAMP,
                    Count: 2,
                    NodeMask: 0,
                },
                &mut heap,
            ))?;
            let mut readback = None;
            dx(device.CreateCommittedResource(
                &D3D12_HEAP_PROPERTIES {
                    Type: D3D12_HEAP_TYPE_READBACK,
                    ..Default::default()
                },
                D3D12_HEAP_FLAG_NONE,
                &D3D12_RESOURCE_DESC {
                    Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
                    Width: 16,
                    Height: 1,
                    DepthOrArraySize: 1,
                    MipLevels: 1,
                    SampleDesc: windows58::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
                    ..Default::default()
                },
                D3D12_RESOURCE_STATE_COPY_DEST,
                None,
                &mut readback,
            ))?;
            Ok(Self {
                heap: heap.unwrap(),
                readback: readback.unwrap(),
                pending: false,
            })
        }
    }
    fn read(&mut self, frequency: u64) -> Result<Option<f64>, String> {
        if !self.pending {
            return Ok(None);
        }
        unsafe {
            let mut data = std::ptr::null_mut();
            dx(self
                .readback
                .Map(0, Some(&D3D12_RANGE { Begin: 0, End: 16 }), Some(&mut data)))?;
            let times = std::slice::from_raw_parts(data.cast::<u64>(), 2);
            let elapsed = (times[1] - times[0]) as f64 * 1000.0 / frequency as f64;
            self.readback
                .Unmap(0, Some(&D3D12_RANGE { Begin: 0, End: 0 }));
            self.pending = false;
            Ok(Some(elapsed))
        }
    }
}
pub(super) struct Session {
    api: std::mem::ManuallyDrop<Api>,
    device: ID3D12Device,
    queue: ID3D12CommandQueue,
    _wgpu_device: wgpu::Device,
    parameters: Ptr,
    feature: Ptr,
    initialized: bool,
    fence: ID3D12Fence,
    serial: u64,
    slots: Vec<Slot>,
    failure: Option<String>,
    #[cfg(test)]
    gpu_timings: Vec<f64>,
}
// All calls and destruction are serialized by the session and process-wide SDK mutexes.
unsafe impl Send for Session {}
impl Session {
    pub(super) fn get(device: &wgpu::Device) -> Result<Shared, String> {
        let raw = unsafe { device.as_hal::<wgpu::hal::api::Dx12>() }
            .ok_or("RTX Video requires the DX12 graphics backend")?;
        let native_device = raw.raw_device().clone();
        let native_queue = raw.raw_queue().clone();
        drop(raw);
        let mut sessions = SESSIONS
            .get_or_init(Default::default)
            .lock()
            .map_err(|_| "RTX Video session lock failed")?;
        sessions.retain(|s| s.strong_count() != 0);
        for session in sessions.iter().filter_map(Weak::upgrade) {
            if session
                .lock()
                .map_err(|_| "RTX Video session lock failed")?
                .device
                == native_device
            {
                return Ok(session);
            }
        }
        let _sdk = SDK.lock().map_err(|_| "RTX Video SDK lock failed")?;
        let directory = std::env::current_exe()
            .map_err(|e| e.to_string())?
            .parent()
            .ok_or("Missing executable directory")?
            .to_path_buf();
        let api = Api::load(&directory)?;
        let fence = dx(unsafe { native_device.CreateFence(0, D3D12_FENCE_FLAG_NONE) })?;
        let mut session = Self {
            api: std::mem::ManuallyDrop::new(api),
            device: native_device,
            queue: native_queue,
            _wgpu_device: device.clone(),
            parameters: std::ptr::null_mut(),
            feature: std::ptr::null_mut(),
            initialized: false,
            fence,
            serial: 0,
            slots: Vec::new(),
            failure: None,
            #[cfg(test)]
            gpu_timings: Vec::new(),
        };
        // Release the global lock before any fallible work whose cleanup reacquires it.
        drop(_sdk);
        session.initialize()?;
        let session = Arc::new(Mutex::new(session));
        sessions.push(Arc::downgrade(&session));
        Ok(session)
    }
    fn initialize(&mut self) -> Result<(), String> {
        let _sdk = SDK.lock().map_err(|_| "RTX Video SDK lock failed")?;
        let log_dir = std::env::temp_dir().join("console-link-ngx");
        std::fs::create_dir_all(&log_dir).map_err(|e| e.to_string())?;
        let path: Vec<u16> = log_dir
            .as_os_str()
            .to_string_lossy()
            .encode_utf16()
            .chain(Some(0))
            .collect();
        unsafe {
            ngx(
                (self.api.init)(
                    0,
                    path.as_ptr(),
                    self.device.as_raw(),
                    std::ptr::null_mut(),
                    0x15,
                ),
                "initialization",
            )?;
            self.initialized = true;
            ngx(
                (self.api.parameters)(&mut self.parameters),
                "capability query",
            )?;
            let mut available = 0;
            ngx(
                (self.api.get_int)(
                    self.parameters,
                    c"VSR.Available".as_ptr().cast(),
                    &mut available,
                ),
                "VSR capability query",
            )?;
            if available == 0 {
                let mut driver = 0;
                (self.api.get_int)(
                    self.parameters,
                    c"VSR.NeedsUpdatedDriver".as_ptr().cast(),
                    &mut driver,
                );
                return Err(if driver != 0 {
                    "RTX Video needs a newer NVIDIA driver"
                } else {
                    "RTX Video is unsupported by this GPU or driver"
                }
                .into());
            }
            for _ in 0..3 {
                let allocator: ID3D12CommandAllocator = dx(self
                    .device
                    .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT))?;
                let list: ID3D12GraphicsCommandList = dx(self.device.CreateCommandList(
                    0,
                    D3D12_COMMAND_LIST_TYPE_DIRECT,
                    &allocator,
                    None,
                ))?;
                dx(list.Close())?;
                self.slots.push(Slot {
                    allocator,
                    list,
                    done: 0,
                    textures: None,
                    #[cfg(test)]
                    timer: Timer::new(&self.device)?,
                });
            }
            let slot = &self.slots[0];
            dx(slot.allocator.Reset())?;
            dx(slot.list.Reset(&slot.allocator, None))?;
            (self.api.set_ui)(self.parameters, c"CreationNodeMask".as_ptr().cast(), 1);
            (self.api.set_ui)(self.parameters, c"VisibilityNodeMask".as_ptr().cast(), 1);
            ngx(
                (self.api.create)(slot.list.as_raw(), 16, self.parameters, &mut self.feature),
                "feature creation",
            )?;
            dx(slot.list.Close())?;
            self.queue
                .ExecuteCommandLists(&[Some(dx(slot.list.cast())?)]);
            self.serial += 1;
            dx(self.queue.Signal(&self.fence, self.serial))?;
            self.slots[0].done = self.serial;
            self.wait(self.serial)?;
        }
        Ok(())
    }
    #[cfg(test)]
    pub(super) fn take_gpu_timings(&mut self) -> Result<Vec<f64>, String> {
        self.wait(self.serial)?;
        let frequency = dx(unsafe { self.queue.GetTimestampFrequency() })?;
        for slot in &mut self.slots {
            if let Some(time) = slot.timer.read(frequency)? {
                self.gpu_timings.push(time);
            }
        }
        Ok(std::mem::take(&mut self.gpu_timings))
    }
    pub(super) fn status(&self) -> Result<(), String> {
        self.failure.clone().map_or(Ok(()), Err)
    }
    fn wait(&self, value: u64) -> Result<(), String> {
        unsafe {
            if self.fence.GetCompletedValue() >= value {
                return Ok(());
            }
            let event = dx(CreateEventW(None, false, false, None))?;
            let result = dx(self.fence.SetEventOnCompletion(value, event)).and_then(|()| {
                if WaitForSingleObject(event, 10_000) == WAIT_OBJECT_0 {
                    Ok(())
                } else {
                    Err("RTX Video GPU completion timed out".into())
                }
            });
            let _ = CloseHandle(event);
            result
        }
    }
    pub(super) fn evaluate(
        &mut self,
        queue: &wgpu::Queue,
        preparation: wgpu::CommandBuffer,
        input: &wgpu::Texture,
        output: &wgpu::Texture,
        quality: u32,
    ) -> Result<(), String> {
        self.status()?;
        let result = self.evaluate_inner(queue, preparation, input, output, quality);
        if let Err(error) = &result {
            self.failure = Some(error.clone());
        }
        result
    }
    fn evaluate_inner(
        &mut self,
        queue: &wgpu::Queue,
        preparation: wgpu::CommandBuffer,
        input: &wgpu::Texture,
        output: &wgpu::Texture,
        quality: u32,
    ) -> Result<(), String> {
        let _sdk = SDK.lock().map_err(|_| "RTX Video SDK lock failed")?;
        let index = self.serial as usize % self.slots.len();
        self.wait(self.slots[index].done)?;
        let slot = &mut self.slots[index];
        #[cfg(test)]
        if let Some(time) = slot
            .timer
            .read(dx(unsafe { self.queue.GetTimestampFrequency() })?)?
        {
            self.gpu_timings.push(time);
        }
        unsafe {
            dx(slot.allocator.Reset())?;
            dx(slot.list.Reset(&slot.allocator, None))?;
            queue.submit([preparation]);
            let source = input
                .as_hal::<wgpu::hal::api::Dx12>()
                .ok_or("RTX source is not DX12")?
                .raw_resource()
                .clone();
            let destination = output
                .as_hal::<wgpu::hal::api::Dx12>()
                .ok_or("RTX output is not DX12")?
                .raw_resource()
                .clone();
            (self.api.set_resource)(self.parameters, c"Input1".as_ptr().cast(), source.as_raw());
            (self.api.set_resource)(
                self.parameters,
                c"Output".as_ptr().cast(),
                destination.as_raw(),
            );
            for (key, value) in [
                (c"Rect.X", 0),
                (c"Rect.Y", 0),
                (c"Rect.W", input.width()),
                (c"Rect.H", input.height()),
                (c"OutRect.X", 0),
                (c"OutRect.Y", 0),
                (c"OutRect.W", output.width()),
                (c"OutRect.H", output.height()),
                (c"VSR.QualityLevel", quality.clamp(1, 4)),
            ] {
                (self.api.set_ui)(self.parameters, key.as_ptr().cast(), value);
            }
            #[cfg(test)]
            slot.list
                .EndQuery(&slot.timer.heap, D3D12_QUERY_TYPE_TIMESTAMP, 0);
            // SDK 1.1 DX12 samples pass COMMON resources; NGX handles its internal UAV barriers.
            ngx(
                (self.api.evaluate)(
                    slot.list.as_raw(),
                    self.feature,
                    self.parameters,
                    std::ptr::null_mut(),
                ),
                "evaluation",
            )?;
            #[cfg(test)]
            {
                slot.list
                    .EndQuery(&slot.timer.heap, D3D12_QUERY_TYPE_TIMESTAMP, 1);
                slot.list.ResolveQueryData(
                    &slot.timer.heap,
                    D3D12_QUERY_TYPE_TIMESTAMP,
                    0,
                    2,
                    &slot.timer.readback,
                    0,
                );
                slot.timer.pending = true;
            }
            dx(slot.list.Close())?;
            slot.textures = Some([input.clone(), output.clone()]);
            self.queue
                .ExecuteCommandLists(&[Some(dx(slot.list.cast())?)]);
            self.serial += 1;
            dx(self.queue.Signal(&self.fence, self.serial))?;
            slot.done = self.serial;
        }
        Ok(())
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        // No SDK resources are freed while previously submitted native work is still running.
        if self.wait(self.serial).is_err() {
            // Device loss/timeout: retain SDK + texture allocations until process exit rather than free in-flight memory.
            // The normal shutdown path below releases everything after its fence completes.
            for slot in self.slots.drain(..) {
                std::mem::forget(slot);
            }
            return;
        }
        if let Ok(_sdk) = SDK.lock() {
            unsafe {
                if !self.feature.is_null() {
                    (self.api.release)(self.feature);
                }
                if !self.parameters.is_null() {
                    (self.api.destroy_parameters)(self.parameters);
                }
                if self.initialized {
                    (self.api.shutdown)(self.device.as_raw());
                }
                std::mem::ManuallyDrop::drop(&mut self.api);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn missing_runtime_has_explicit_error() {
        let path = std::env::temp_dir().join(format!(
            "console-link-no-rtx-runtime-{}",
            std::process::id()
        ));
        let error = super::Api::load(&path)
            .err()
            .expect("Missing runtime rejected");
        assert!(error.contains("runtime missing"), "{error}");
    }
    #[test]
    fn sdk_failure_codes_are_not_success() {
        assert!(super::ngx(1, "test").is_ok());
        assert!(
            super::ngx(0xbad00001, "test")
                .unwrap_err()
                .contains("0xBAD00001")
        );
    }
}
