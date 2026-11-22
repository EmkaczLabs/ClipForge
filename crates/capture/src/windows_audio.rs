#![cfg(target_os = "windows")]

use std::ffi::c_void;
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use clipforge_common::{ClipForgeError, Result};
use tokio::sync::mpsc;
use tracing::{debug, warn};
use windows::Win32::Media::Audio::{
    AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_E_DEVICE_INVALIDATED, AUDCLNT_SHAREMODE_SHARED,
    AUDCLNT_STREAMFLAGS_LOOPBACK, DEVICE_STATE_ACTIVE, EDataFlow, IAudioCaptureClient,
    IAudioClient, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, WAVEFORMATEX,
    WAVEFORMATEXTENSIBLE, eCapture, eConsole, eRender,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    CoUninitialize,
};
use windows::core::{GUID, HRESULT, Interface, PCWSTR, PWSTR};

const WAVE_FORMAT_PCM_TAG: u16 = 1;
const WAVE_FORMAT_IEEE_FLOAT_TAG: u16 = 3;
const WAVE_FORMAT_EXTENSIBLE_TAG: u16 = 0xFFFE;

const KSDATAFORMAT_SUBTYPE_PCM: GUID = GUID::from_u128(0x00000001_0000_0010_8000_00aa00389b71);
const KSDATAFORMAT_SUBTYPE_IEEE_FLOAT: GUID =
    GUID::from_u128(0x00000003_0000_0010_8000_00aa00389b71);

const VT_LPWSTR: u16 = 31;

/// `PKEY_Device_FriendlyName` (`{a45c254e-df1c-4efd-8020-67d146a850e0}`, pid 14).
#[repr(C)]
struct PropertyKey {
    fmtid: GUID,
    pid: u32,
}

const PKEY_DEVICE_FRIENDLY_NAME: PropertyKey = PropertyKey {
    fmtid: GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
    pid: 14,
};

/// Minimal 24-byte `PROPVARIANT` layout for `VT_LPWSTR` on 64-bit Windows.
#[repr(C)]
struct RawPropVariant {
    vt: u16,
    w_reserved1: u16,
    w_reserved2: u16,
    w_reserved3: u16,
    data: *mut u16,
    pad: u64,
}

impl Default for RawPropVariant {
    fn default() -> Self {
        Self {
            vt: 0,
            w_reserved1: 0,
            w_reserved2: 0,
            w_reserved3: 0,
            data: ptr::null_mut(),
            pad: 0,
        }
    }
}

/// Captured interleaved `f32le` PCM audio chunk from WASAPI.
#[derive(Debug, Clone)]
pub struct CapturedAudioChunk {
    pub sample_rate: u32,
    pub channels: u16,
    pub pts_ns: i64,
    /// Interleaved `f32le` PCM samples.
    pub samples_f32: Vec<f32>,
}

struct ComInitGuard {
    initialized: bool,
}

impl ComInitGuard {
    fn new_multithreaded() -> Result<Self> {
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if hr.is_ok() {
            Ok(Self { initialized: true })
        } else if hr.0 as u32 == 0x8001_0106 {
            // RPC_E_CHANGED_MODE: COM already initialized on this thread with a different mode.
            Ok(Self { initialized: false })
        } else {
            Err(ClipForgeError::CaptureFailed(format!(
                "CoInitializeEx(COINIT_MULTITHREADED) failed: {hr:?}"
            )))
        }
    }
}

impl Drop for ComInitGuard {
    fn drop(&mut self) {
        if self.initialized {
            unsafe { CoUninitialize() };
        }
    }
}

unsafe fn pwstr_to_string(pwstr: PWSTR) -> String {
    if pwstr.is_null() {
        return String::new();
    }
    let mut len = 0usize;
    unsafe {
        while *pwstr.0.add(len) != 0 {
            len += 1;
        }
        let slice = std::slice::from_raw_parts(pwstr.0, len);
        String::from_utf16_lossy(slice)
    }
}

fn get_device_id(device: &IMMDevice) -> Result<String> {
    let pwstr = unsafe { device.GetId() }
        .map_err(|e| ClipForgeError::CaptureFailed(format!("IMMDevice::GetId failed: {e}")))?;
    let id = unsafe { pwstr_to_string(pwstr) };
    unsafe { CoTaskMemFree(Some(pwstr.0 as *const c_void)) };
    Ok(id)
}

/// Invokes `IMMDevice::Activate` (vtable slot 3) without requiring `Win32_System_Variant`.
unsafe fn activate_audio_client(device: &IMMDevice) -> Result<IAudioClient> {
    type ActivateFn = unsafe extern "system" fn(
        this: *mut c_void,
        iid: *const GUID,
        dwclsctx: u32,
        pactivationparams: *const c_void,
        ppinterface: *mut *mut c_void,
    ) -> HRESULT;

    let raw_device = device.as_raw();
    let vtable = unsafe { *(raw_device as *const *const usize) };
    let activate: ActivateFn = unsafe { std::mem::transmute(*vtable.add(3)) };

    let mut out_ptr: *mut c_void = ptr::null_mut();
    let hr = unsafe {
        activate(
            raw_device,
            &IAudioClient::IID,
            CLSCTX_ALL.0,
            ptr::null(),
            &mut out_ptr,
        )
    };
    hr.ok()
        .map_err(|e| ClipForgeError::CaptureFailed(format!("IMMDevice::Activate failed: {e}")))?;

    if out_ptr.is_null() {
        return Err(ClipForgeError::CaptureFailed(
            "IMMDevice::Activate returned null IAudioClient".to_string(),
        ));
    }
    Ok(unsafe { IAudioClient::from_raw(out_ptr) })
}

/// Reads `PKEY_Device_FriendlyName` via `IMMDevice::OpenPropertyStore` (vtable slot 4).
fn get_device_friendly_name(device: &IMMDevice) -> Option<String> {
    type OpenPropertyStoreFn = unsafe extern "system" fn(
        this: *mut c_void,
        stgmaccess: u32,
        ppproperties: *mut *mut c_void,
    ) -> HRESULT;
    type GetValueFn = unsafe extern "system" fn(
        this: *mut c_void,
        key: *const PropertyKey,
        pv: *mut RawPropVariant,
    ) -> HRESULT;

    windows::core::link!("ole32.dll" "system" fn PropVariantClear(pvar: *mut RawPropVariant) -> HRESULT);

    let raw_device = device.as_raw();
    let vtable = unsafe { *(raw_device as *const *const usize) };
    let open_props: OpenPropertyStoreFn = unsafe { std::mem::transmute(*vtable.add(4)) };

    let mut store_ptr: *mut c_void = ptr::null_mut();
    // STGM_READ = 0
    let hr = unsafe { open_props(raw_device, 0, &mut store_ptr) };
    if hr.is_err() || store_ptr.is_null() {
        return None;
    }

    // Wrap in IUnknown so Release() is called automatically on drop.
    let _store_guard = unsafe { windows::core::IUnknown::from_raw(store_ptr) };
    let store_vtable = unsafe { *(store_ptr as *const *const usize) };
    // IPropertyStore::GetValue is vtable slot 5 (IUnknown 0..2, GetCount 3, GetAt 4, GetValue 5).
    let get_value: GetValueFn = unsafe { std::mem::transmute(*store_vtable.add(5)) };

    let mut propvar = RawPropVariant::default();
    let hr = unsafe { get_value(store_ptr, &PKEY_DEVICE_FRIENDLY_NAME, &mut propvar) };
    if hr.is_err() {
        return None;
    }

    let result = if propvar.vt == VT_LPWSTR && !propvar.data.is_null() {
        let name = unsafe { pwstr_to_string(PWSTR(propvar.data)) };
        (!name.is_empty()).then_some(name)
    } else {
        None
    };

    unsafe {
        let _ = PropVariantClear(&mut propvar);
    }
    result
}

fn enumerate_flow(
    enumerator: &IMMDeviceEnumerator,
    flow: EDataFlow,
    prefix: &str,
    out: &mut Vec<crate::discovery::AudioDeviceInfo>,
) -> Result<()> {
    let collection = unsafe { enumerator.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE) }
        .map_err(|e| ClipForgeError::CaptureFailed(format!("EnumAudioEndpoints failed: {e}")))?;
    let count = unsafe { collection.GetCount() }.map_err(|e| {
        ClipForgeError::CaptureFailed(format!("IMMDeviceCollection::GetCount failed: {e}"))
    })?;

    for idx in 0..count {
        let Ok(device) = (unsafe { collection.Item(idx) }) else {
            continue;
        };
        let Ok(id) = get_device_id(&device) else {
            continue;
        };
        let label = get_device_friendly_name(&device).unwrap_or_else(|| id.clone());
        out.push(crate::discovery::AudioDeviceInfo {
            id,
            label: format!("[{prefix}] {label}"),
        });
    }
    Ok(())
}

/// Enumerates active WASAPI render (loopback-capable) and capture (microphone) endpoints.
pub fn enumerate_wasapi_devices() -> Result<Vec<crate::discovery::AudioDeviceInfo>> {
    let _com = ComInitGuard::new_multithreaded()?;
    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }.map_err(|e| {
            ClipForgeError::CaptureUnavailable(format!(
                "CoCreateInstance(MMDeviceEnumerator) failed: {e}"
            ))
        })?;

    let mut devices = Vec::new();
    enumerate_flow(&enumerator, eRender, "output", &mut devices)?;
    enumerate_flow(&enumerator, eCapture, "input", &mut devices)?;
    Ok(devices)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SampleFormat {
    F32,
    I16,
    I24,
    I32,
    U8,
}

fn parse_wave_format(fmt: &WAVEFORMATEX) -> Result<(u32, u16, usize, SampleFormat)> {
    let sample_rate = fmt.nSamplesPerSec;
    let channels = fmt.nChannels;
    let bits = fmt.wBitsPerSample;
    let block_align = fmt.nBlockAlign as usize;

    let sample_format = match fmt.wFormatTag {
        WAVE_FORMAT_IEEE_FLOAT_TAG if bits == 32 => SampleFormat::F32,
        WAVE_FORMAT_PCM_TAG => match bits {
            8 => SampleFormat::U8,
            16 => SampleFormat::I16,
            24 => SampleFormat::I24,
            32 => SampleFormat::I32,
            other => {
                return Err(ClipForgeError::CaptureFailed(format!(
                    "unsupported WASAPI PCM bit depth: {other}"
                )));
            }
        },
        WAVE_FORMAT_EXTENSIBLE_TAG => {
            let ext = unsafe { &*(fmt as *const WAVEFORMATEX as *const WAVEFORMATEXTENSIBLE) };
            let subformat = unsafe { ptr::addr_of!(ext.SubFormat).read_unaligned() };
            if subformat == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT && bits == 32 {
                SampleFormat::F32
            } else if subformat == KSDATAFORMAT_SUBTYPE_PCM {
                match bits {
                    8 => SampleFormat::U8,
                    16 => SampleFormat::I16,
                    24 => SampleFormat::I24,
                    32 => SampleFormat::I32,
                    other => {
                        return Err(ClipForgeError::CaptureFailed(format!(
                            "unsupported WASAPI extensible PCM bit depth: {other}"
                        )));
                    }
                }
            } else {
                return Err(ClipForgeError::CaptureFailed(format!(
                    "unsupported WASAPI extensible subformat: {subformat:?} ({bits} bits)"
                )));
            }
        }
        tag => {
            return Err(ClipForgeError::CaptureFailed(format!(
                "unsupported WASAPI wave format tag: {tag}"
            )));
        }
    };

    if sample_rate == 0 || channels == 0 || block_align == 0 {
        return Err(ClipForgeError::CaptureFailed(
            "WASAPI returned invalid mix format (zero rate/channels/block_align)".to_string(),
        ));
    }

    Ok((sample_rate, channels, block_align, sample_format))
}

unsafe fn decode_samples_to_f32(
    data: *const u8,
    num_frames: u32,
    channels: u16,
    block_align: usize,
    fmt: SampleFormat,
) -> Vec<f32> {
    let total_samples = (num_frames as usize) * (channels as usize);
    let mut out = Vec::with_capacity(total_samples);
    let total_bytes = (num_frames as usize) * block_align;
    let bytes = unsafe { std::slice::from_raw_parts(data, total_bytes) };

    match fmt {
        SampleFormat::F32 => {
            for chunk in bytes.chunks_exact(4).take(total_samples) {
                out.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
            }
        }
        SampleFormat::I16 => {
            for chunk in bytes.chunks_exact(2).take(total_samples) {
                let v = i16::from_le_bytes([chunk[0], chunk[1]]);
                out.push(f32::from(v) / 32768.0);
            }
        }
        SampleFormat::I24 => {
            for chunk in bytes.chunks_exact(3).take(total_samples) {
                let raw =
                    ((chunk[2] as i8 as i32) << 16) | ((chunk[1] as i32) << 8) | (chunk[0] as i32);
                out.push(raw as f32 / 8_388_608.0);
            }
        }
        SampleFormat::I32 => {
            for chunk in bytes.chunks_exact(4).take(total_samples) {
                let v = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                out.push((v as f64 / 2_147_483_648.0) as f32);
            }
        }
        SampleFormat::U8 => {
            for &b in bytes.iter().take(total_samples) {
                out.push((f32::from(b) - 128.0) / 128.0);
            }
        }
    }
    out
}

/// Handle to an active WASAPI audio capture session (loopback or microphone).
///
/// Runs on a dedicated MTA COM thread and joins cleanly on [`Self::stop`] and [`Drop`].
pub struct WasapiCaptureHandle {
    stop_flag: Arc<AtomicBool>,
    sample_rate: Arc<AtomicU32>,
    channels: Arc<AtomicU16>,
    thread: Option<JoinHandle<()>>,
}

impl WasapiCaptureHandle {
    /// Starts WASAPI loopback capture on an `eRender` endpoint (default render device if `None`).
    ///
    /// Pads silence frames when the loopback endpoint is idle so A/V timestamps stay monotonic.
    pub fn start_loopback(
        device_id: Option<&str>,
        tx: mpsc::Sender<CapturedAudioChunk>,
    ) -> Result<Self> {
        Self::start_internal(true, device_id, tx)
    }

    /// Starts WASAPI microphone capture on an `eCapture` endpoint (default capture device if `None`).
    pub fn start_mic(
        device_id: Option<&str>,
        tx: mpsc::Sender<CapturedAudioChunk>,
    ) -> Result<Self> {
        Self::start_internal(false, device_id, tx)
    }

    fn start_internal(
        is_loopback: bool,
        device_id: Option<&str>,
        tx: mpsc::Sender<CapturedAudioChunk>,
    ) -> Result<Self> {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let sample_rate = Arc::new(AtomicU32::new(0));
        let channels = Arc::new(AtomicU16::new(0));

        let owned_id = device_id
            .map(str::trim)
            .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("default"))
            .map(str::to_owned);

        let (init_tx, init_rx) = std::sync::mpsc::sync_channel::<Result<(u32, u16)>>(1);
        let stop_clone = Arc::clone(&stop_flag);

        let thread_name = if is_loopback {
            "clipforge-wasapi-loopback"
        } else {
            "clipforge-wasapi-mic"
        };

        let thread = thread::Builder::new()
            .name(thread_name.to_string())
            .spawn(move || {
                run_wasapi_loop(is_loopback, owned_id, tx, stop_clone, init_tx);
            })
            .map_err(|e| {
                ClipForgeError::CaptureFailed(format!("failed to spawn WASAPI thread: {e}"))
            })?;

        match init_rx.recv() {
            Ok(Ok((rate, ch))) => {
                sample_rate.store(rate, Ordering::Relaxed);
                channels.store(ch, Ordering::Relaxed);
                Ok(Self {
                    stop_flag,
                    sample_rate,
                    channels,
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
                    "WASAPI capture thread exited before initialization completed".to_string(),
                ))
            }
        }
    }

    /// Returns the captured stream's sample rate in Hz.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate.load(Ordering::Relaxed)
    }

    /// Returns the captured stream's channel count.
    pub fn channels(&self) -> u16 {
        self.channels.load(Ordering::Relaxed)
    }

    /// Signals the WASAPI thread to stop and joins it cleanly.
    pub fn stop(&mut self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for WasapiCaptureHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run_wasapi_loop(
    is_loopback: bool,
    device_id: Option<String>,
    tx: mpsc::Sender<CapturedAudioChunk>,
    stop_flag: Arc<AtomicBool>,
    init_tx: std::sync::mpsc::SyncSender<Result<(u32, u16)>>,
) {
    let _com = match ComInitGuard::new_multithreaded() {
        Ok(g) => g,
        Err(e) => {
            let _ = init_tx.send(Err(e));
            return;
        }
    };

    let enumerator: IMMDeviceEnumerator =
        match unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) } {
            Ok(e) => e,
            Err(e) => {
                let _ = init_tx.send(Err(ClipForgeError::CaptureUnavailable(format!(
                    "CoCreateInstance(MMDeviceEnumerator) failed: {e}"
                ))));
                return;
            }
        };

    let flow = if is_loopback { eRender } else { eCapture };
    let device: IMMDevice = match &device_id {
        Some(id) => {
            let wide: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
            match unsafe { enumerator.GetDevice(PCWSTR(wide.as_ptr())) } {
                Ok(d) => d,
                Err(e) => {
                    let _ = init_tx.send(Err(ClipForgeError::NotFound(format!(
                        "WASAPI device '{id}' not found: {e}"
                    ))));
                    return;
                }
            }
        }
        None => match unsafe { enumerator.GetDefaultAudioEndpoint(flow, eConsole) } {
            Ok(d) => d,
            Err(e) => {
                let _ = init_tx.send(Err(ClipForgeError::NotFound(format!(
                    "no default WASAPI endpoint available: {e}"
                ))));
                return;
            }
        },
    };

    let audio_client = match unsafe { activate_audio_client(&device) } {
        Ok(c) => c,
        Err(e) => {
            let _ = init_tx.send(Err(e));
            return;
        }
    };

    let mix_fmt_ptr = match unsafe { audio_client.GetMixFormat() } {
        Ok(p) if !p.is_null() => p,
        Ok(_) => {
            let _ = init_tx.send(Err(ClipForgeError::CaptureFailed(
                "IAudioClient::GetMixFormat returned null".to_string(),
            )));
            return;
        }
        Err(e) => {
            let _ = init_tx.send(Err(ClipForgeError::CaptureFailed(format!(
                "IAudioClient::GetMixFormat failed: {e}"
            ))));
            return;
        }
    };

    let parsed_fmt = unsafe { parse_wave_format(&*mix_fmt_ptr) };
    let (sample_rate, channels, block_align, sample_format) = match parsed_fmt {
        Ok(v) => v,
        Err(e) => {
            unsafe { CoTaskMemFree(Some(mix_fmt_ptr as *const c_void)) };
            let _ = init_tx.send(Err(e));
            return;
        }
    };

    let stream_flags = if is_loopback {
        AUDCLNT_STREAMFLAGS_LOOPBACK
    } else {
        0
    };
    // 100ms buffer duration in 100-nanosecond units.
    let buffer_duration_hns: i64 = 1_000_000;
    let init_res = unsafe {
        audio_client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            stream_flags,
            buffer_duration_hns,
            0,
            mix_fmt_ptr,
            None,
        )
    };
    unsafe { CoTaskMemFree(Some(mix_fmt_ptr as *const c_void)) };

    if let Err(e) = init_res {
        let _ = init_tx.send(Err(ClipForgeError::CaptureFailed(format!(
            "IAudioClient::Initialize failed: {e}"
        ))));
        return;
    }

    let capture_client: IAudioCaptureClient = match unsafe { audio_client.GetService() } {
        Ok(c) => c,
        Err(e) => {
            let _ = init_tx.send(Err(ClipForgeError::CaptureFailed(format!(
                "IAudioClient::GetService(IAudioCaptureClient) failed: {e}"
            ))));
            return;
        }
    };

    if let Err(e) = unsafe { audio_client.Start() } {
        let _ = init_tx.send(Err(ClipForgeError::CaptureFailed(format!(
            "IAudioClient::Start failed: {e}"
        ))));
        return;
    }

    let _ = init_tx.send(Ok((sample_rate, channels)));
    debug!(
        is_loopback,
        sample_rate, channels, "WASAPI capture stream started"
    );

    let start_instant = Instant::now();
    let mut total_frames_emitted: u64 = 0;
    let min_pad_frames = u64::from(sample_rate / 100).max(1); // ~10ms
    let max_pad_frames = u64::from(sample_rate / 20).max(1); // ~50ms

    while !stop_flag.load(Ordering::Acquire) {
        let mut received_packet = false;

        loop {
            if stop_flag.load(Ordering::Acquire) {
                break;
            }
            let packet_len = match unsafe { capture_client.GetNextPacketSize() } {
                Ok(n) => n,
                Err(e) => {
                    if e.code() == AUDCLNT_E_DEVICE_INVALIDATED {
                        warn!("WASAPI audio device invalidated; stopping capture");
                    } else {
                        warn!("WASAPI GetNextPacketSize failed: {e}");
                    }
                    stop_flag.store(true, Ordering::SeqCst);
                    break;
                }
            };

            if packet_len == 0 {
                break;
            }

            let mut data_ptr: *mut u8 = ptr::null_mut();
            let mut num_frames: u32 = 0;
            let mut flags: u32 = 0;

            if let Err(e) = unsafe {
                capture_client.GetBuffer(&mut data_ptr, &mut num_frames, &mut flags, None, None)
            } {
                if e.code() == AUDCLNT_E_DEVICE_INVALIDATED {
                    warn!("WASAPI audio device invalidated during GetBuffer");
                } else {
                    warn!("WASAPI GetBuffer failed: {e}");
                }
                stop_flag.store(true, Ordering::SeqCst);
                break;
            }

            received_packet = true;
            let is_silent = (flags & (AUDCLNT_BUFFERFLAGS_SILENT.0 as u32)) != 0;
            let samples_f32 = if is_silent || data_ptr.is_null() {
                vec![0.0f32; (num_frames as usize) * (channels as usize)]
            } else {
                unsafe {
                    decode_samples_to_f32(
                        data_ptr,
                        num_frames,
                        channels,
                        block_align,
                        sample_format,
                    )
                }
            };

            if let Err(e) = unsafe { capture_client.ReleaseBuffer(num_frames) } {
                warn!("WASAPI ReleaseBuffer failed: {e}");
                stop_flag.store(true, Ordering::SeqCst);
                break;
            }

            if num_frames > 0 {
                let pts_ns = (i128::from(total_frames_emitted) * 1_000_000_000i128
                    / i128::from(sample_rate)) as i64;
                total_frames_emitted += u64::from(num_frames);

                if tx
                    .blocking_send(CapturedAudioChunk {
                        sample_rate,
                        channels,
                        pts_ns,
                        samples_f32,
                    })
                    .is_err()
                {
                    stop_flag.store(true, Ordering::SeqCst);
                    break;
                }
            }
        }

        if stop_flag.load(Ordering::Acquire) {
            break;
        }

        if !received_packet {
            thread::sleep(Duration::from_millis(10));

            // Loopback produces no packets while the render endpoint is idle; pad silence
            // so the audio timeline stays monotonic and synchronized with video PTS.
            if is_loopback {
                let expected_frames = ((start_instant.elapsed().as_nanos()
                    * u128::from(sample_rate))
                    / 1_000_000_000u128) as u64;
                if expected_frames > total_frames_emitted + min_pad_frames {
                    let missing = (expected_frames - total_frames_emitted).min(max_pad_frames);
                    let pts_ns = (i128::from(total_frames_emitted) * 1_000_000_000i128
                        / i128::from(sample_rate)) as i64;
                    total_frames_emitted += missing;

                    let samples_f32 = vec![0.0f32; (missing as usize) * (channels as usize)];
                    if tx
                        .blocking_send(CapturedAudioChunk {
                            sample_rate,
                            channels,
                            pts_ns,
                            samples_f32,
                        })
                        .is_err()
                    {
                        stop_flag.store(true, Ordering::SeqCst);
                        break;
                    }
                }
            }
        }
    }

    let _ = unsafe { audio_client.Stop() };
}
