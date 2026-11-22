#![cfg(target_os = "windows")]

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use clipforge_common::{ClipForgeError, Result};
use tokio::sync::mpsc;
use tracing::{debug, warn};
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{
    Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Win32::Foundation::{HMODULE, HWND, LPARAM, RECT, TRUE};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_11_0};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BOX, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ,
    D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Resource, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::{
    DXGI_ERROR_DEVICE_REMOVED, DXGI_ERROR_DEVICE_RESET, IDXGIDevice,
};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
};
use windows::Win32::System::WinRT::Direct3D11::{
    CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize};
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
use windows::core::{BOOL, Interface};

/// Primary monitor flag from `WinUser.h` (`MONITORINFOF_PRIMARY`).
const MONITORINFOF_PRIMARY: u32 = 1;

/// Raw BGRA8 video frame captured via Windows Graphics Capture.
#[derive(Debug, Clone)]
pub struct CapturedVideoFrame {
    pub width: u32,
    pub height: u32,
    pub pts_ns: i64,
    /// Tightly packed BGRA8 (`width * height * 4`) for `ffmpeg` rawvideo.
    pub bgra: Vec<u8>,
}

#[derive(Debug, Clone)]
struct EnumeratedMonitor {
    hmonitor: isize,
    name: String,
    width: u32,
    height: u32,
    is_primary: bool,
}

unsafe extern "system" fn enum_monitor_proc(
    hmonitor: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    lparam: LPARAM,
) -> BOOL {
    let list = unsafe { &mut *(lparam.0 as *mut Vec<EnumeratedMonitor>) };
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;

    if unsafe {
        GetMonitorInfoW(
            hmonitor,
            (&mut info as *mut MONITORINFOEXW).cast::<MONITORINFO>(),
        )
    }
    .as_bool()
    {
        let end = info
            .szDevice
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(info.szDevice.len());
        let name = String::from_utf16_lossy(&info.szDevice[..end]);
        let rc = info.monitorInfo.rcMonitor;
        let width = (rc.right - rc.left).max(0) as u32;
        let height = (rc.bottom - rc.top).max(0) as u32;
        let is_primary = (info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY) != 0;
        list.push(EnumeratedMonitor {
            hmonitor: hmonitor.0 as isize,
            name,
            width,
            height,
            is_primary,
        });
    }
    TRUE
}

fn enumerate_monitors_internal() -> Result<Vec<EnumeratedMonitor>> {
    let mut monitors: Vec<EnumeratedMonitor> = Vec::new();
    let ok = unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(enum_monitor_proc),
            LPARAM(&mut monitors as *mut Vec<EnumeratedMonitor> as isize),
        )
    };
    if !ok.as_bool() {
        return Err(ClipForgeError::CaptureFailed(
            "EnumDisplayMonitors failed".to_string(),
        ));
    }
    Ok(monitors)
}

/// Enumerates active Windows display monitors via `EnumDisplayMonitors`.
pub fn enumerate_windows_monitors() -> Result<Vec<crate::discovery::MonitorInfo>> {
    let monitors = enumerate_monitors_internal()?;
    Ok(monitors
        .into_iter()
        .map(|m| crate::discovery::MonitorInfo {
            name: m.name,
            resolution: (m.width > 0 && m.height > 0).then(|| format!("{}x{}", m.width, m.height)),
        })
        .collect())
}

#[derive(Clone, Copy)]
enum CaptureTarget {
    Monitor(isize),
    Window(isize),
}

struct RoInitGuard {
    initialized: bool,
}

impl RoInitGuard {
    fn new_multithreaded() -> Result<Self> {
        match unsafe { RoInitialize(RO_INIT_MULTITHREADED) } {
            Ok(()) => Ok(Self { initialized: true }),
            Err(err) => {
                // RPC_E_CHANGED_MODE (0x80010106) means apartment is already initialized on this thread.
                if err.code().0 as u32 == 0x8001_0106 {
                    Ok(Self { initialized: false })
                } else {
                    Err(ClipForgeError::CaptureFailed(format!(
                        "RoInitialize(RO_INIT_MULTITHREADED) failed: {err}"
                    )))
                }
            }
        }
    }
}

impl Drop for RoInitGuard {
    fn drop(&mut self) {
        if self.initialized {
            unsafe { RoUninitialize() };
        }
    }
}

struct D3dResources {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    winrt_device: IDirect3DDevice,
}

fn create_d3d_resources() -> Result<D3dResources> {
    let mut device: Option<ID3D11Device> = None;
    let mut context: Option<ID3D11DeviceContext> = None;
    let feature_levels = [D3D_FEATURE_LEVEL_11_0];

    unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            Some(&feature_levels),
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )
    }
    .map_err(|e| ClipForgeError::CaptureFailed(format!("D3D11CreateDevice failed: {e}")))?;

    let device = device.ok_or_else(|| {
        ClipForgeError::CaptureFailed("D3D11CreateDevice returned null device".to_string())
    })?;
    let context = context.ok_or_else(|| {
        ClipForgeError::CaptureFailed("D3D11CreateDevice returned null context".to_string())
    })?;

    let dxgi_device: IDXGIDevice = device
        .cast()
        .map_err(|e| ClipForgeError::CaptureFailed(format!("QI IDXGIDevice failed: {e}")))?;
    let inspectable =
        unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi_device) }.map_err(|e| {
            ClipForgeError::CaptureFailed(format!(
                "CreateDirect3D11DeviceFromDXGIDevice failed: {e}"
            ))
        })?;
    let winrt_device: IDirect3DDevice = inspectable
        .cast()
        .map_err(|e| ClipForgeError::CaptureFailed(format!("cast IDirect3DDevice failed: {e}")))?;

    Ok(D3dResources {
        device,
        context,
        winrt_device,
    })
}

fn create_capture_item(target: CaptureTarget) -> Result<GraphicsCaptureItem> {
    let interop = windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()
        .map_err(|e| {
            ClipForgeError::CaptureUnavailable(format!(
                "IGraphicsCaptureItemInterop unavailable: {e}"
            ))
        })?;

    match target {
        CaptureTarget::Monitor(raw_hmon) => unsafe {
            interop.CreateForMonitor::<GraphicsCaptureItem>(HMONITOR(raw_hmon as *mut c_void))
        }
        .map_err(|e| ClipForgeError::CaptureFailed(format!("CreateForMonitor failed: {e}"))),
        CaptureTarget::Window(raw_hwnd) => {
            unsafe { interop.CreateForWindow::<GraphicsCaptureItem>(HWND(raw_hwnd as *mut c_void)) }
                .map_err(|e| ClipForgeError::CaptureFailed(format!("CreateForWindow failed: {e}")))
        }
    }
}

fn create_staging_texture(
    device: &ID3D11Device,
    width: u32,
    height: u32,
) -> Result<ID3D11Texture2D> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_STAGING,
        BindFlags: 0,
        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
        MiscFlags: 0,
    };

    let mut tex: Option<ID3D11Texture2D> = None;
    unsafe { device.CreateTexture2D(&desc, None, Some(&mut tex)) }.map_err(|e| {
        ClipForgeError::CaptureFailed(format!("CreateTexture2D staging failed: {e}"))
    })?;
    tex.ok_or_else(|| {
        ClipForgeError::CaptureFailed("CreateTexture2D returned null staging texture".to_string())
    })
}

fn is_device_lost_hr(hr: windows::core::HRESULT) -> bool {
    hr == DXGI_ERROR_DEVICE_REMOVED || hr == DXGI_ERROR_DEVICE_RESET
}

/// Handle to an active Windows Graphics Capture session.
///
/// Stops and joins the capture worker thread on [`Self::stop`] and [`Drop`].
pub struct WgcCaptureHandle {
    stop_flag: Arc<AtomicBool>,
    signal: Arc<(Mutex<bool>, Condvar)>,
    dimensions: Arc<(AtomicU32, AtomicU32)>,
    thread: Option<JoinHandle<()>>,
}

impl WgcCaptureHandle {
    /// Starts capturing the specified monitor (or the primary monitor if `monitor_name` is `None`).
    pub fn start_monitor(
        monitor_name: Option<&str>,
        fps: u32,
        tx: mpsc::Sender<CapturedVideoFrame>,
    ) -> Result<Self> {
        let monitors = enumerate_monitors_internal()?;
        if monitors.is_empty() {
            return Err(ClipForgeError::NotFound(
                "no active display monitors found".to_string(),
            ));
        }

        let chosen = match monitor_name.map(str::trim).filter(|s| !s.is_empty()) {
            None | Some("primary" | "screen") => monitors
                .iter()
                .find(|m| m.is_primary)
                .unwrap_or(&monitors[0]),
            Some(name) => monitors
                .iter()
                .find(|m| m.name == name)
                .or_else(|| monitors.iter().find(|m| m.name.eq_ignore_ascii_case(name)))
                .ok_or_else(|| ClipForgeError::NotFound(format!("monitor '{name}' not found")))?,
        };

        Self::start_target(CaptureTarget::Monitor(chosen.hmonitor), fps, tx)
    }

    /// Starts capturing the currently focused foreground window.
    pub fn start_focused_window(fps: u32, tx: mpsc::Sender<CapturedVideoFrame>) -> Result<Self> {
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.is_invalid() {
            return Err(ClipForgeError::CaptureFailed(
                "no foreground window is currently focused".to_string(),
            ));
        }
        Self::start_target(CaptureTarget::Window(hwnd.0 as isize), fps, tx)
    }

    fn start_target(
        target: CaptureTarget,
        fps: u32,
        tx: mpsc::Sender<CapturedVideoFrame>,
    ) -> Result<Self> {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let signal = Arc::new((Mutex::new(false), Condvar::new()));
        let dimensions = Arc::new((AtomicU32::new(0), AtomicU32::new(0)));

        let (init_tx, init_rx) = std::sync::mpsc::sync_channel::<Result<(u32, u32)>>(1);

        let stop_clone = Arc::clone(&stop_flag);
        let signal_clone = Arc::clone(&signal);
        let dims_clone = Arc::clone(&dimensions);

        let thread = thread::Builder::new()
            .name("clipforge-wgc".to_string())
            .spawn(move || {
                run_wgc_loop(
                    target,
                    fps.max(1),
                    tx,
                    stop_clone,
                    signal_clone,
                    dims_clone,
                    init_tx,
                );
            })
            .map_err(|e| {
                ClipForgeError::CaptureFailed(format!("failed to spawn WGC thread: {e}"))
            })?;

        match init_rx.recv() {
            Ok(Ok((w, h))) => {
                dimensions.0.store(w, Ordering::Relaxed);
                dimensions.1.store(h, Ordering::Relaxed);
                Ok(Self {
                    stop_flag,
                    signal,
                    dimensions,
                    thread: Some(thread),
                })
            }
            Ok(Err(err)) => {
                stop_flag.store(true, Ordering::SeqCst);
                let _ = thread.join();
                Err(err)
            }
            Err(_) => {
                stop_flag.store(true, Ordering::SeqCst);
                let _ = thread.join();
                Err(ClipForgeError::CaptureFailed(
                    "WGC capture thread exited before initialization completed".to_string(),
                ))
            }
        }
    }

    /// Returns the current `(width, height)` of the captured surface.
    pub fn dimensions(&self) -> (u32, u32) {
        (
            self.dimensions.0.load(Ordering::Relaxed),
            self.dimensions.1.load(Ordering::Relaxed),
        )
    }

    /// Signals the capture thread to stop and joins it cleanly.
    pub fn stop(&mut self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        {
            let (lock, cvar) = &*self.signal;
            if let Ok(mut ready) = lock.lock() {
                *ready = true;
                cvar.notify_all();
            }
        }
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for WgcCaptureHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run_wgc_loop(
    target: CaptureTarget,
    fps: u32,
    tx: mpsc::Sender<CapturedVideoFrame>,
    stop_flag: Arc<AtomicBool>,
    signal: Arc<(Mutex<bool>, Condvar)>,
    dimensions: Arc<(AtomicU32, AtomicU32)>,
    init_tx: std::sync::mpsc::SyncSender<Result<(u32, u32)>>,
) {
    let _ro_guard = match RoInitGuard::new_multithreaded() {
        Ok(g) => g,
        Err(e) => {
            let _ = init_tx.send(Err(e));
            return;
        }
    };

    match GraphicsCaptureSession::IsSupported() {
        Ok(true) => {}
        Ok(false) => {
            let _ = init_tx.send(Err(ClipForgeError::CaptureUnavailable(
                "Windows Graphics Capture is not supported on this system".to_string(),
            )));
            return;
        }
        Err(e) => {
            let _ = init_tx.send(Err(ClipForgeError::CaptureUnavailable(format!(
                "GraphicsCaptureSession::IsSupported failed: {e}"
            ))));
            return;
        }
    }

    let mut d3d = match create_d3d_resources() {
        Ok(r) => r,
        Err(e) => {
            let _ = init_tx.send(Err(e));
            return;
        }
    };

    let item = match create_capture_item(target) {
        Ok(i) => i,
        Err(e) => {
            let _ = init_tx.send(Err(e));
            return;
        }
    };

    let initial_size = match item.Size() {
        Ok(s) if s.Width > 0 && s.Height > 0 => s,
        Ok(s) => {
            let _ = init_tx.send(Err(ClipForgeError::CaptureFailed(format!(
                "invalid capture item size: {}x{}",
                s.Width, s.Height
            ))));
            return;
        }
        Err(e) => {
            let _ = init_tx.send(Err(ClipForgeError::CaptureFailed(format!(
                "GraphicsCaptureItem::Size failed: {e}"
            ))));
            return;
        }
    };

    let mut current_size = initial_size;
    let frame_pool = match Direct3D11CaptureFramePool::CreateFreeThreaded(
        &d3d.winrt_device,
        DirectXPixelFormat::B8G8R8A8UIntNormalized,
        2,
        current_size,
    ) {
        Ok(p) => p,
        Err(e) => {
            let _ = init_tx.send(Err(ClipForgeError::CaptureFailed(format!(
                "Direct3D11CaptureFramePool::CreateFreeThreaded failed: {e}"
            ))));
            return;
        }
    };

    let item_closed = Arc::new(AtomicBool::new(false));
    let closed_flag = Arc::clone(&item_closed);
    let closed_signal = Arc::clone(&signal);
    let _closed_token = item.Closed(&TypedEventHandler::<
        GraphicsCaptureItem,
        windows::core::IInspectable,
    >::new(move |_, _| {
        closed_flag.store(true, Ordering::SeqCst);
        let (lock, cvar) = &*closed_signal;
        if let Ok(mut ready) = lock.lock() {
            *ready = true;
            cvar.notify_all();
        }
        Ok(())
    }));

    let frame_signal = Arc::clone(&signal);
    let _frame_token = frame_pool.FrameArrived(&TypedEventHandler::<
        Direct3D11CaptureFramePool,
        windows::core::IInspectable,
    >::new(move |_, _| {
        let (lock, cvar) = &*frame_signal;
        if let Ok(mut ready) = lock.lock() {
            *ready = true;
            cvar.notify_one();
        }
        Ok(())
    }));

    let session = match frame_pool.CreateCaptureSession(&item) {
        Ok(s) => s,
        Err(e) => {
            let _ = frame_pool.Close();
            let _ = init_tx.send(Err(ClipForgeError::CaptureFailed(format!(
                "CreateCaptureSession failed: {e}"
            ))));
            return;
        }
    };

    if let Err(e) = session.StartCapture() {
        let _ = session.Close();
        let _ = frame_pool.Close();
        let _ = init_tx.send(Err(ClipForgeError::CaptureFailed(format!(
            "GraphicsCaptureSession::StartCapture failed: {e}"
        ))));
        return;
    }

    let _ = init_tx.send(Ok((current_size.Width as u32, current_size.Height as u32)));

    let start_instant = Instant::now();
    let frame_interval = Duration::from_secs_f64(1.0 / f64::from(fps.max(1)));
    let mut next_frame_due = Instant::now();
    let mut staging: Option<(u32, u32, ID3D11Texture2D)> = None;
    let mut device_reset_retries: u32 = 0;

    while !stop_flag.load(Ordering::Acquire) && !item_closed.load(Ordering::Acquire) {
        {
            let (lock, cvar) = &*signal;
            if let Ok(mut ready) = lock.lock() {
                while !*ready
                    && !stop_flag.load(Ordering::Acquire)
                    && !item_closed.load(Ordering::Acquire)
                {
                    let (guard, timeout) = match cvar.wait_timeout(ready, Duration::from_millis(50))
                    {
                        Ok(res) => res,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    ready = guard;
                    if timeout.timed_out() {
                        break;
                    }
                }
                *ready = false;
            }
        }

        if stop_flag.load(Ordering::Acquire) || item_closed.load(Ordering::Acquire) {
            break;
        }

        // Drain queued frames, keeping the freshest frame within FPS pacing.
        loop {
            if stop_flag.load(Ordering::Acquire) || item_closed.load(Ordering::Acquire) {
                break;
            }
            let frame = match frame_pool.TryGetNextFrame() {
                Ok(f) => f,
                Err(_) => break,
            };

            let content_size = match frame.ContentSize() {
                Ok(s) => s,
                Err(_) => {
                    let _ = frame.Close();
                    continue;
                }
            };

            if content_size.Width <= 0 || content_size.Height <= 0 {
                let _ = frame.Close();
                continue;
            }

            if content_size.Width != current_size.Width
                || content_size.Height != current_size.Height
            {
                debug!(
                    old_w = current_size.Width,
                    old_h = current_size.Height,
                    new_w = content_size.Width,
                    new_h = content_size.Height,
                    "WGC content size changed; recreating frame pool"
                );
                if let Err(e) = frame_pool.Recreate(
                    &d3d.winrt_device,
                    DirectXPixelFormat::B8G8R8A8UIntNormalized,
                    2,
                    content_size,
                ) {
                    warn!("frame_pool.Recreate failed: {e}");
                    let _ = frame.Close();
                    break;
                }
                current_size = content_size;
                dimensions
                    .0
                    .store(current_size.Width as u32, Ordering::Relaxed);
                dimensions
                    .1
                    .store(current_size.Height as u32, Ordering::Relaxed);
                staging = None;
            }

            let now = Instant::now();
            if now < next_frame_due {
                let _ = frame.Close();
                continue;
            }
            while next_frame_due <= now {
                next_frame_due += frame_interval;
            }

            let surface = match frame.Surface() {
                Ok(s) => s,
                Err(_) => {
                    let _ = frame.Close();
                    continue;
                }
            };
            let access: IDirect3DDxgiInterfaceAccess = match surface.cast() {
                Ok(a) => a,
                Err(_) => {
                    let _ = frame.Close();
                    continue;
                }
            };
            let src_tex: ID3D11Texture2D = match unsafe { access.GetInterface() } {
                Ok(t) => t,
                Err(_) => {
                    let _ = frame.Close();
                    continue;
                }
            };

            let mut src_desc = D3D11_TEXTURE2D_DESC::default();
            unsafe { src_tex.GetDesc(&mut src_desc) };
            let copy_w = (current_size.Width as u32).min(src_desc.Width);
            let copy_h = (current_size.Height as u32).min(src_desc.Height);
            if copy_w == 0 || copy_h == 0 {
                let _ = frame.Close();
                continue;
            }

            let needs_new_staging = match &staging {
                Some((sw, sh, _)) => *sw != copy_w || *sh != copy_h,
                None => true,
            };
            if needs_new_staging {
                match create_staging_texture(&d3d.device, copy_w, copy_h) {
                    Ok(tex) => staging = Some((copy_w, copy_h, tex)),
                    Err(e) => {
                        warn!("{e}");
                        let _ = frame.Close();
                        continue;
                    }
                }
            }

            let staging_tex = &staging.as_ref().expect("staging texture initialized").2;
            let dst_res: ID3D11Resource = match staging_tex.cast() {
                Ok(r) => r,
                Err(_) => {
                    let _ = frame.Close();
                    continue;
                }
            };
            let src_res: ID3D11Resource = match src_tex.cast() {
                Ok(r) => r,
                Err(_) => {
                    let _ = frame.Close();
                    continue;
                }
            };

            let copy_box = D3D11_BOX {
                left: 0,
                top: 0,
                front: 0,
                right: copy_w,
                bottom: copy_h,
                back: 1,
            };
            unsafe {
                d3d.context.CopySubresourceRegion(
                    &dst_res,
                    0,
                    0,
                    0,
                    0,
                    &src_res,
                    0,
                    Some(&copy_box),
                );
            }

            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            if let Err(err) = unsafe {
                d3d.context
                    .Map(&dst_res, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
            } {
                let _ = frame.Close();
                if is_device_lost_hr(err.code()) && device_reset_retries < 3 {
                    device_reset_retries += 1;
                    warn!(
                        attempt = device_reset_retries,
                        "D3D11 device removed/reset; attempting recovery"
                    );
                    staging = None;
                    if let Ok(new_d3d) = create_d3d_resources()
                        && frame_pool
                            .Recreate(
                                &new_d3d.winrt_device,
                                DirectXPixelFormat::B8G8R8A8UIntNormalized,
                                2,
                                current_size,
                            )
                            .is_ok()
                    {
                        d3d = new_d3d;
                        continue;
                    }
                }
                warn!("D3D11 Map failed: {err}");
                break;
            }

            device_reset_retries = 0;
            let row_bytes = (copy_w as usize) * 4;
            let total_bytes = row_bytes * (copy_h as usize);
            let mut bgra = vec![0u8; total_bytes];
            let src_pitch = mapped.RowPitch as usize;
            let src_ptr = mapped.pData as *const u8;

            if !src_ptr.is_null() {
                if src_pitch == row_bytes {
                    unsafe {
                        std::ptr::copy_nonoverlapping(src_ptr, bgra.as_mut_ptr(), total_bytes);
                    }
                } else {
                    for y in 0..(copy_h as usize) {
                        unsafe {
                            let src_row = src_ptr.add(y * src_pitch);
                            let dst_row = bgra.as_mut_ptr().add(y * row_bytes);
                            std::ptr::copy_nonoverlapping(src_row, dst_row, row_bytes);
                        }
                    }
                }
            }

            unsafe { d3d.context.Unmap(&dst_res, 0) };
            let _ = frame.Close();

            let pts_ns = start_instant.elapsed().as_nanos() as i64;
            if tx
                .blocking_send(CapturedVideoFrame {
                    width: copy_w,
                    height: copy_h,
                    pts_ns,
                    bgra,
                })
                .is_err()
            {
                stop_flag.store(true, Ordering::SeqCst);
                break;
            }
        }
    }

    let _ = session.Close();
    let _ = frame_pool.Close();
}
