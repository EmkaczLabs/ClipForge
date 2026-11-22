#[cfg(target_os = "linux")]
use std::collections::HashMap;
#[cfg(target_os = "linux")]
use std::future::poll_fn;
#[cfg(target_os = "linux")]
use std::pin::Pin;
use std::sync::Arc;
#[cfg(target_os = "linux")]
use std::sync::atomic::AtomicU64;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "linux")]
use std::thread::{self, JoinHandle};
#[cfg(target_os = "linux")]
use std::time::Duration;

use clipforge_common::{ClipForgeError, Result};
use tokio::sync::mpsc;
#[cfg(target_os = "linux")]
use tokio::sync::oneshot;
#[cfg(target_os = "linux")]
use tracing::{debug, info, warn};
#[cfg(target_os = "linux")]
use x11rb::connection::Connection;
#[cfg(target_os = "linux")]
use x11rb::protocol::Event;
#[cfg(target_os = "linux")]
use x11rb::protocol::xproto::{ConnectionExt, EventMask, GrabMode, ModMask, Window};
#[cfg(target_os = "linux")]
use zbus::export::futures_core::Stream;
#[cfg(target_os = "linux")]
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

const MOD_SHIFT: u16 = 1;
const MOD_CONTROL: u16 = 1 << 2;
const MOD_ALT: u16 = 1 << 3;
const MOD_SUPER: u16 = 1 << 6;

#[cfg(target_os = "linux")]
const PORTAL_DEST: &str = "org.freedesktop.portal.Desktop";
#[cfg(target_os = "linux")]
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
#[cfg(target_os = "linux")]
const GLOBAL_SHORTCUTS_IFACE: &str = "org.freedesktop.portal.GlobalShortcuts";
#[cfg(target_os = "linux")]
const REQUEST_IFACE: &str = "org.freedesktop.portal.Request";
#[cfg(target_os = "linux")]
const SESSION_IFACE: &str = "org.freedesktop.portal.Session";

#[cfg(target_os = "linux")]
static TOKEN_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Logical action triggered by a global or focused-window hotkey.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HotkeyAction {
    SaveReplay,
    ToggleReplayBuffer,
    ToggleRecording,
}

impl HotkeyAction {
    /// Stable shortcut identifier used with XDG Desktop Portal `GlobalShortcuts`.
    pub fn portal_id(self) -> &'static str {
        match self {
            Self::SaveReplay => "save_replay",
            Self::ToggleReplayBuffer => "toggle_replay",
            Self::ToggleRecording => "toggle_record",
        }
    }

    /// Maps an XDG Desktop Portal `shortcut_id` back to a [`HotkeyAction`].
    pub fn from_portal_id(id: &str) -> Option<Self> {
        match id {
            "save_replay" => Some(Self::SaveReplay),
            "toggle_replay" => Some(Self::ToggleReplayBuffer),
            "toggle_record" => Some(Self::ToggleRecording),
            _ => None,
        }
    }

    /// Human-readable shortcut description shown in the Wayland portal permission dialog.
    pub fn description(self) -> &'static str {
        match self {
            Self::SaveReplay => "Save Replay Buffer",
            Self::ToggleReplayBuffer => "Toggle Replay Buffer",
            Self::ToggleRecording => "Start / Stop Recording",
        }
    }
}

/// Selected global hotkey backend for the current session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyBackendKind {
    WaylandPortal,
    X11,
    Windows,
    FocusedOnly,
}

/// Truthful registration status of global hotkeys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobalHotkeyStatus {
    /// XDG Desktop Portal `GlobalShortcuts` session is active and shortcuts are bound.
    WaylandPortalActive,
    /// Waiting for user approval in the Wayland portal permission dialog.
    PermissionPending,
    /// X11 global key grabs are active.
    X11GlobalActive,
    /// Windows `RegisterHotKey` global shortcuts are active.
    WindowsGlobalActive,
    /// Only focused-window shortcuts are active (e.g. headless, cancelled, or unsupported).
    FocusedWindowOnly { reason: Option<String> },
    /// Global shortcut registration failed; focused-window shortcuts still work.
    RegistrationError { message: String },
}

impl GlobalHotkeyStatus {
    /// Human-readable status label for UI display and diagnostics.
    pub fn label(&self) -> String {
        match self {
            Self::WaylandPortalActive => "GlobalShortcuts (Wayland Portal Active)".to_string(),
            Self::PermissionPending => "Permission Pending".to_string(),
            Self::X11GlobalActive => "X11 Global".to_string(),
            Self::WindowsGlobalActive => "Windows Global (RegisterHotKey)".to_string(),
            Self::FocusedWindowOnly { reason: None } => "Focused-Window Only".to_string(),
            Self::FocusedWindowOnly { reason: Some(r) } => {
                format!("Focused-Window Only ({r})")
            }
            Self::RegistrationError { message } => format!("Registration Error: {message}"),
        }
    }

    /// Returns `true` when system-wide global hotkeys are actively registered.
    pub fn is_global_active(&self) -> bool {
        matches!(
            self,
            Self::WaylandPortalActive | Self::X11GlobalActive | Self::WindowsGlobalActive
        )
    }
}

/// Selects the hotkey backend from environment variables.
pub fn select_hotkey_backend(
    xdg_session_type: Option<&str>,
    wayland_display: Option<&str>,
    display: Option<&str>,
) -> HotkeyBackendKind {
    let session = xdg_session_type.map(str::trim).unwrap_or("");
    let wayland = wayland_display.map(str::trim).unwrap_or("");
    let x11 = display.map(str::trim).unwrap_or("");

    if session.eq_ignore_ascii_case("wayland") || (session.is_empty() && !wayland.is_empty()) {
        HotkeyBackendKind::WaylandPortal
    } else if session.eq_ignore_ascii_case("x11") || !x11.is_empty() {
        HotkeyBackendKind::X11
    } else {
        HotkeyBackendKind::FocusedOnly
    }
}

/// Detects the hotkey backend for the current process environment.
pub fn detect_hotkey_backend() -> HotkeyBackendKind {
    #[cfg(target_os = "linux")]
    {
        let xdg = std::env::var("XDG_SESSION_TYPE").ok();
        let wayland = std::env::var("WAYLAND_DISPLAY").ok();
        let display = std::env::var("DISPLAY").ok();
        select_hotkey_backend(xdg.as_deref(), wayland.as_deref(), display.as_deref())
    }
    #[cfg(target_os = "windows")]
    {
        HotkeyBackendKind::Windows
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        HotkeyBackendKind::FocusedOnly
    }
}

/// Parsed hotkey specification (X11 modifier mask + X11 keysym + display string).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeySpec {
    pub modifiers: u16,
    pub keysym: u32,
    pub display: String,
}

impl HotkeySpec {
    /// Returns `true` if the given modifier flags and X11 keysym match this hotkey spec.
    pub fn matches_parts(
        &self,
        ctrl: bool,
        alt: bool,
        shift: bool,
        super_key: bool,
        keysym: u32,
    ) -> bool {
        let mut mods: u16 = 0;
        if ctrl {
            mods |= MOD_CONTROL;
        }
        if alt {
            mods |= MOD_ALT;
        }
        if shift {
            mods |= MOD_SHIFT;
        }
        if super_key {
            mods |= MOD_SUPER;
        }
        self.modifiers == mods && self.keysym == keysym
    }

    /// Formats this binding as an XDG/GTK accelerator string for `GlobalShortcuts` `preferred_trigger`.
    pub fn to_xdg_trigger(&self) -> String {
        let mut out = String::new();
        if self.modifiers & MOD_CONTROL != 0 {
            out.push_str("<Control>");
        }
        if self.modifiers & MOD_ALT != 0 {
            out.push_str("<Alt>");
        }
        if self.modifiers & MOD_SHIFT != 0 {
            out.push_str("<Shift>");
        }
        if self.modifiers & MOD_SUPER != 0 {
            out.push_str("<Super>");
        }
        out.push_str(&keysym_to_xdg_key_name(self.keysym));
        out
    }
}

fn keysym_to_xdg_key_name(keysym: u32) -> String {
    if (0xFFBE..=0xFFD5).contains(&keysym) {
        return format!("F{}", (keysym - 0xFFBE) + 1);
    }
    if let Some(ch) = char::from_u32(keysym)
        && ch.is_ascii_alphanumeric()
    {
        return ch.to_string();
    }
    match keysym {
        0x0020 => "space".to_string(),
        0xFF0D => "Return".to_string(),
        0xFF1B => "Escape".to_string(),
        0xFF09 => "Tab".to_string(),
        0xFF61 => "Print".to_string(),
        0xFF63 => "Insert".to_string(),
        0xFFFF => "Delete".to_string(),
        0xFF50 => "Home".to_string(),
        0xFF57 => "End".to_string(),
        0xFF13 => "Pause".to_string(),
        _ => format!("0x{keysym:x}"),
    }
}

/// Parses a human-readable hotkey string such as `"F9"`, `"Ctrl+Alt+F9"`, or `"Shift+Super+R"`.
pub fn parse_hotkey(input: &str) -> Result<HotkeySpec> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(ClipForgeError::InvalidInput(
            "hotkey specification must not be empty".to_string(),
        ));
    }

    let parts: Vec<&str> = trimmed.split('+').map(str::trim).collect();
    if parts.iter().any(|p| p.is_empty()) {
        return Err(ClipForgeError::InvalidInput(format!(
            "invalid hotkey syntax: '{trimmed}'"
        )));
    }

    let mut modifiers: u16 = 0;
    for &mod_str in &parts[..parts.len() - 1] {
        match mod_str.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers |= MOD_CONTROL,
            "alt" | "mod1" => modifiers |= MOD_ALT,
            "shift" => modifiers |= MOD_SHIFT,
            "super" | "meta" | "win" | "mod4" => modifiers |= MOD_SUPER,
            other => {
                return Err(ClipForgeError::InvalidInput(format!(
                    "unknown hotkey modifier '{other}' in '{trimmed}'"
                )));
            }
        }
    }

    let key_str = parts[parts.len() - 1];
    let keysym = parse_keysym(key_str).ok_or_else(|| {
        ClipForgeError::InvalidInput(format!("unsupported key '{key_str}' in '{trimmed}'"))
    })?;

    Ok(HotkeySpec {
        modifiers,
        keysym,
        display: trimmed.to_string(),
    })
}

fn parse_keysym(key: &str) -> Option<u32> {
    let lower = key.to_ascii_lowercase();
    // F1..F24 (XK_F1 = 0xFFBE .. XK_F24 = 0xFFD5)
    if let Some(num_str) = lower.strip_prefix('f')
        && let Ok(n) = num_str.parse::<u32>()
        && (1..=24).contains(&n)
    {
        return Some(0xFFBE + (n - 1));
    }

    // Single ASCII letter or digit (lowercase keysym for X11)
    if lower.len() == 1 {
        let ch = lower.chars().next()?;
        if ch.is_ascii_alphanumeric() {
            return Some(ch as u32);
        }
    }

    match lower.as_str() {
        "space" => Some(0x0020),
        "return" | "enter" => Some(0xFF0D),
        "escape" | "esc" => Some(0xFF1B),
        "tab" => Some(0xFF09),
        "print" | "printscreen" => Some(0xFF61),
        "insert" => Some(0xFF63),
        "delete" | "del" => Some(0xFFFF),
        "home" => Some(0xFF50),
        "end" => Some(0xFF57),
        "pause" => Some(0xFF13),
        _ => None,
    }
}

/// Windows virtual-key + `RegisterHotKey` modifier bitmask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowsHotkeyCode {
    /// Win32 `HOT_KEY_MODIFIERS` bitmask (`MOD_ALT`, `MOD_CONTROL`, `MOD_SHIFT`, `MOD_WIN`, `MOD_NOREPEAT`).
    pub modifiers: u32,
    /// Win32 Virtual-Key code (`VK_*`).
    pub vk: u32,
}

const WIN_MOD_ALT: u32 = 0x0001;
const WIN_MOD_CONTROL: u32 = 0x0002;
const WIN_MOD_SHIFT: u32 = 0x0004;
const WIN_MOD_WIN: u32 = 0x0008;
const WIN_MOD_NOREPEAT: u32 = 0x4000;

/// Converts a [`HotkeySpec`] into a Win32 [`WindowsHotkeyCode`] for `RegisterHotKey`.
pub fn hotkey_spec_to_windows_code(spec: &HotkeySpec) -> Result<WindowsHotkeyCode> {
    let mut modifiers = WIN_MOD_NOREPEAT;
    if spec.modifiers & MOD_ALT != 0 {
        modifiers |= WIN_MOD_ALT;
    }
    if spec.modifiers & MOD_CONTROL != 0 {
        modifiers |= WIN_MOD_CONTROL;
    }
    if spec.modifiers & MOD_SHIFT != 0 {
        modifiers |= WIN_MOD_SHIFT;
    }
    if spec.modifiers & MOD_SUPER != 0 {
        modifiers |= WIN_MOD_WIN;
    }

    let vk = if (0xFFBE..=0xFFD5).contains(&spec.keysym) {
        // VK_F1 (0x70) .. VK_F24 (0x87)
        0x70 + (spec.keysym - 0xFFBE)
    } else if (b'a' as u32..=b'z' as u32).contains(&spec.keysym) {
        // ASCII uppercase 'A'..'Z'
        (b'A' as u32) + (spec.keysym - b'a' as u32)
    } else if (b'0' as u32..=b'9' as u32).contains(&spec.keysym) {
        spec.keysym
    } else {
        match spec.keysym {
            0x0020 => 0x20, // VK_SPACE
            0xFF0D => 0x0D, // VK_RETURN
            0xFF1B => 0x1B, // VK_ESCAPE
            0xFF09 => 0x09, // VK_TAB
            0xFF61 => 0x2C, // VK_SNAPSHOT (PrintScreen)
            0xFF63 => 0x2D, // VK_INSERT
            0xFFFF => 0x2E, // VK_DELETE
            0xFF50 => 0x24, // VK_HOME
            0xFF57 => 0x23, // VK_END
            0xFF13 => 0x13, // VK_PAUSE
            other => {
                return Err(ClipForgeError::InvalidInput(format!(
                    "unsupported Windows hotkey keysym 0x{other:x} for '{}'",
                    spec.display
                )));
            }
        }
    };

    Ok(WindowsHotkeyCode { modifiers, vk })
}

/// Parses a human-readable hotkey string (e.g. `"F9"`, `"Ctrl+Alt+F10"`) directly into
/// a Win32 [`WindowsHotkeyCode`].
pub fn parse_windows_hotkey(input: &str) -> Result<WindowsHotkeyCode> {
    let spec = parse_hotkey(input)?;
    hotkey_spec_to_windows_code(&spec)
}

/// Manages global hotkeys via XDG Desktop Portal `GlobalShortcuts` on Wayland,
/// `x11rb` on X11, or Win32 `RegisterHotKey` on Windows.
pub struct HotkeyManager {
    stop_flag: Arc<AtomicBool>,
    #[cfg(target_os = "linux")]
    x11_handle: Option<JoinHandle<()>>,
    #[cfg(target_os = "linux")]
    portal_stop_tx: Option<oneshot::Sender<()>>,
    #[cfg(target_os = "windows")]
    win_thread: Option<std::thread::JoinHandle<()>>,
    #[cfg(target_os = "windows")]
    win_thread_id: Option<Arc<std::sync::atomic::AtomicU32>>,
}

impl HotkeyManager {
    /// Creates an idle manager when only focused-window shortcuts are active.
    pub fn idle() -> Self {
        Self {
            stop_flag: Arc::new(AtomicBool::new(false)),
            #[cfg(target_os = "linux")]
            x11_handle: None,
            #[cfg(target_os = "linux")]
            portal_stop_tx: None,
            #[cfg(target_os = "windows")]
            win_thread: None,
            #[cfg(target_os = "windows")]
            win_thread_id: None,
        }
    }

    /// Starts global hotkey registration for the detected display backend.
    ///
    /// Must be called from within a Tokio runtime when running on Wayland.
    pub fn start_auto(
        bindings: Vec<(HotkeyAction, HotkeySpec)>,
        tx: mpsc::UnboundedSender<HotkeyAction>,
        status_tx: mpsc::UnboundedSender<GlobalHotkeyStatus>,
    ) -> Self {
        Self::start_with_backend(detect_hotkey_backend(), bindings, tx, status_tx)
    }

    /// Starts global hotkey registration for an explicit [`HotkeyBackendKind`].
    #[cfg(target_os = "linux")]
    pub fn start_with_backend(
        backend: HotkeyBackendKind,
        bindings: Vec<(HotkeyAction, HotkeySpec)>,
        tx: mpsc::UnboundedSender<HotkeyAction>,
        status_tx: mpsc::UnboundedSender<GlobalHotkeyStatus>,
    ) -> Self {
        if bindings.is_empty() {
            let _ = status_tx.send(GlobalHotkeyStatus::RegistrationError {
                message: "at least one hotkey binding is required".to_string(),
            });
            return Self::idle();
        }

        match backend {
            HotkeyBackendKind::WaylandPortal => Self::start_wayland_portal(bindings, tx, status_tx),
            HotkeyBackendKind::X11 => match Self::start_x11(bindings, tx) {
                Ok(mgr) => {
                    let _ = status_tx.send(GlobalHotkeyStatus::X11GlobalActive);
                    mgr
                }
                Err(err) => {
                    warn!(error = %err, "X11 global hotkey registration failed");
                    let _ = status_tx.send(GlobalHotkeyStatus::RegistrationError {
                        message: err.to_string(),
                    });
                    Self::idle()
                }
            },
            HotkeyBackendKind::FocusedOnly | HotkeyBackendKind::Windows => {
                let _ = status_tx.send(GlobalHotkeyStatus::FocusedWindowOnly {
                    reason: Some("no Wayland portal or X11 display detected".to_string()),
                });
                Self::idle()
            }
        }
    }

    /// Starts global hotkey registration on Windows via `RegisterHotKey`.
    #[cfg(target_os = "windows")]
    pub fn start_with_backend(
        backend: HotkeyBackendKind,
        bindings: Vec<(HotkeyAction, HotkeySpec)>,
        tx: mpsc::UnboundedSender<HotkeyAction>,
        status_tx: mpsc::UnboundedSender<GlobalHotkeyStatus>,
    ) -> Self {
        if bindings.is_empty() {
            let _ = status_tx.send(GlobalHotkeyStatus::RegistrationError {
                message: "at least one hotkey binding is required".to_string(),
            });
            return Self::idle();
        }
        if backend == HotkeyBackendKind::FocusedOnly {
            let _ = status_tx.send(GlobalHotkeyStatus::FocusedWindowOnly {
                reason: Some("focused-window shortcuts requested".to_string()),
            });
            return Self::idle();
        }
        Self::start_windows_register_hotkey(bindings, tx, status_tx)
    }

    /// Starts global hotkey registration on non-Linux, non-Windows platforms.
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    pub fn start_with_backend(
        _backend: HotkeyBackendKind,
        bindings: Vec<(HotkeyAction, HotkeySpec)>,
        _tx: mpsc::UnboundedSender<HotkeyAction>,
        status_tx: mpsc::UnboundedSender<GlobalHotkeyStatus>,
    ) -> Self {
        if bindings.is_empty() {
            let _ = status_tx.send(GlobalHotkeyStatus::RegistrationError {
                message: "at least one hotkey binding is required".to_string(),
            });
            return Self::idle();
        }
        let _ = status_tx.send(GlobalHotkeyStatus::FocusedWindowOnly {
            reason: Some("global hotkeys are not supported on this platform".to_string()),
        });
        Self::idle()
    }

    /// Starts a Win32 `RegisterHotKey` message-loop thread on Windows.
    #[cfg(target_os = "windows")]
    pub fn start_windows_register_hotkey(
        bindings: Vec<(HotkeyAction, HotkeySpec)>,
        tx: mpsc::UnboundedSender<HotkeyAction>,
        status_tx: mpsc::UnboundedSender<GlobalHotkeyStatus>,
    ) -> Self {
        use std::sync::atomic::AtomicU32;
        use windows::Win32::System::Threading::GetCurrentThreadId;
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            HOT_KEY_MODIFIERS, RegisterHotKey, UnregisterHotKey,
        };
        use windows::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG, WM_HOTKEY};

        let mut parsed = Vec::with_capacity(bindings.len());
        for (idx, (action, spec)) in bindings.into_iter().enumerate() {
            match hotkey_spec_to_windows_code(&spec) {
                Ok(code) => parsed.push(((idx as i32) + 1, action, spec.display, code)),
                Err(err) => {
                    let _ = status_tx.send(GlobalHotkeyStatus::RegistrationError {
                        message: err.to_string(),
                    });
                    return Self::idle();
                }
            }
        }

        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_clone = Arc::clone(&stop_flag);
        let thread_id = Arc::new(AtomicU32::new(0));
        let thread_id_clone = Arc::clone(&thread_id);

        let handle = std::thread::Builder::new()
            .name("clipforge-win-hotkey".to_string())
            .spawn(move || unsafe {
                let tid = GetCurrentThreadId();
                thread_id_clone.store(tid, Ordering::SeqCst);

                let mut registered_ids = Vec::new();
                for (id, _action, display, code) in &parsed {
                    if let Err(err) =
                        RegisterHotKey(None, *id, HOT_KEY_MODIFIERS(code.modifiers), code.vk)
                    {
                        for reg_id in registered_ids {
                            let _ = UnregisterHotKey(None, reg_id);
                        }
                        let _ = status_tx.send(GlobalHotkeyStatus::RegistrationError {
                            message: format!("RegisterHotKey failed for '{display}': {err}"),
                        });
                        return;
                    }
                    registered_ids.push(*id);
                }

                let _ = status_tx.send(GlobalHotkeyStatus::WindowsGlobalActive);

                let mut msg = MSG::default();
                while !stop_clone.load(Ordering::Relaxed) {
                    let ret = GetMessageW(&mut msg, None, 0, 0);
                    if ret.0 <= 0 {
                        break;
                    }
                    if msg.message == WM_HOTKEY {
                        let hotkey_id = msg.wParam.0 as i32;
                        if let Some((_, action, _, _)) =
                            parsed.iter().find(|(id, _, _, _)| *id == hotkey_id)
                        {
                            let _ = tx.send(*action);
                        }
                    }
                }

                for reg_id in registered_ids {
                    let _ = UnregisterHotKey(None, reg_id);
                }
            });

        match handle {
            Ok(join) => Self {
                stop_flag,
                win_thread: Some(join),
                win_thread_id: Some(thread_id),
            },
            Err(err) => {
                let _ = status_tx.send(GlobalHotkeyStatus::RegistrationError {
                    message: format!("failed to spawn Windows hotkey thread: {err}"),
                });
                Self::idle()
            }
        }
    }

    /// Starts an async XDG Desktop Portal `GlobalShortcuts` listener on the current Tokio runtime.
    #[cfg(target_os = "linux")]
    pub fn start_wayland_portal(
        bindings: Vec<(HotkeyAction, HotkeySpec)>,
        tx: mpsc::UnboundedSender<HotkeyAction>,
        status_tx: mpsc::UnboundedSender<GlobalHotkeyStatus>,
    ) -> Self {
        let (stop_tx, stop_rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            run_wayland_portal_loop(bindings, tx, status_tx, stop_rx).await;
        });

        Self {
            stop_flag: Arc::new(AtomicBool::new(false)),
            x11_handle: None,
            portal_stop_tx: Some(stop_tx),
        }
    }

    /// Registers the given `(HotkeyAction, HotkeySpec)` bindings on the active X11 display.
    ///
    /// Returns `ClipForgeError::CaptureUnavailable` if no X11 display is reachable.
    #[cfg(target_os = "linux")]
    pub fn start_x11(
        bindings: Vec<(HotkeyAction, HotkeySpec)>,
        tx: mpsc::UnboundedSender<HotkeyAction>,
    ) -> Result<Self> {
        if bindings.is_empty() {
            return Err(ClipForgeError::InvalidInput(
                "at least one hotkey binding is required".to_string(),
            ));
        }

        let (conn, screen_num) = x11rb::connect(None).map_err(|e| {
            ClipForgeError::CaptureUnavailable(format!(
                "cannot connect to X11 display for global hotkeys: {e}"
            ))
        })?;

        let root: Window = conn.setup().roots[screen_num].root;
        let min_keycode = conn.setup().min_keycode;
        let max_keycode = conn.setup().max_keycode;
        let count = max_keycode.saturating_sub(min_keycode) + 1;

        let mapping = conn
            .get_keyboard_mapping(min_keycode, count)
            .map_err(|e| ClipForgeError::CaptureFailed(format!("X11 keymap request failed: {e}")))?
            .reply()
            .map_err(|e| ClipForgeError::CaptureFailed(format!("X11 keymap reply failed: {e}")))?;

        let syms_per_code = mapping.keysyms_per_keycode as usize;
        if syms_per_code == 0 {
            return Err(ClipForgeError::CaptureFailed(
                "X11 keyboard mapping returned 0 keysyms per keycode".to_string(),
            ));
        }

        let mut resolved: Vec<(HotkeyAction, u8, u16)> = Vec::new();
        // Extra lock masks so hotkeys work even when CapsLock (LOCK) or NumLock (M2) is active.
        let lock_variants: [u16; 4] = [
            0,
            u16::from(ModMask::LOCK),
            u16::from(ModMask::M2),
            u16::from(ModMask::LOCK) | u16::from(ModMask::M2),
        ];

        for (action, spec) in bindings {
            let keycode = find_keycode(&mapping.keysyms, syms_per_code, min_keycode, spec.keysym)
                .ok_or_else(|| {
                ClipForgeError::InvalidInput(format!(
                    "could not map hotkey '{}' (keysym 0x{:x}) to an X11 keycode",
                    spec.display, spec.keysym
                ))
            })?;

            for extra in lock_variants {
                let mask = ModMask::from(spec.modifiers | extra);
                conn.grab_key(true, root, mask, keycode, GrabMode::ASYNC, GrabMode::ASYNC)
                    .map_err(|e| {
                        ClipForgeError::CaptureFailed(format!("X11 grab_key failed: {e}"))
                    })?;
            }
            info!(hotkey = %spec.display, ?action, keycode, "registered X11 global hotkey");
            resolved.push((action, keycode, spec.modifiers));
        }

        conn.change_window_attributes(
            root,
            &x11rb::protocol::xproto::ChangeWindowAttributesAux::new()
                .event_mask(EventMask::KEY_PRESS),
        )
        .ok();
        conn.flush()
            .map_err(|e| ClipForgeError::CaptureFailed(format!("X11 flush failed: {e}")))?;

        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_clone = Arc::clone(&stop_flag);

        let handle = thread::Builder::new()
            .name("clipforge-x11-hotkeys".to_string())
            .spawn(move || {
                let ignore_locks = u16::from(ModMask::LOCK) | u16::from(ModMask::M2);
                while !stop_clone.load(Ordering::Relaxed) {
                    match conn.poll_for_event() {
                        Ok(Some(Event::KeyPress(ev))) => {
                            let state_bits = u16::from(ev.state) & !ignore_locks;
                            for &(action, keycode, mods) in &resolved {
                                if ev.detail == keycode && state_bits == mods {
                                    debug!(?action, "X11 global hotkey triggered");
                                    if tx.send(action).is_err() {
                                        return;
                                    }
                                }
                            }
                        }
                        Ok(Some(_)) => {}
                        Ok(None) => thread::sleep(Duration::from_millis(25)),
                        Err(err) => {
                            warn!(error = %err, "X11 hotkey event loop error; stopping");
                            break;
                        }
                    }
                }
            })
            .map_err(|e| {
                ClipForgeError::CaptureFailed(format!("failed to spawn hotkey thread: {e}"))
            })?;

        Ok(Self {
            stop_flag,
            x11_handle: Some(handle),
            portal_stop_tx: None,
        })
    }

    /// Stops the background hotkey listener (X11 thread, Wayland portal task, or Windows message loop).
    pub fn stop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        #[cfg(target_os = "linux")]
        {
            if let Some(tx) = self.portal_stop_tx.take() {
                let _ = tx.send(());
            }
            if let Some(handle) = self.x11_handle.take() {
                let _ = handle.join();
            }
        }
        #[cfg(target_os = "windows")]
        {
            if let Some(tid_arc) = self.win_thread_id.take() {
                let tid = tid_arc.load(Ordering::SeqCst);
                if tid != 0 {
                    use windows::Win32::Foundation::{LPARAM, WPARAM};
                    use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};
                    unsafe {
                        let _ = PostThreadMessageW(tid, WM_QUIT, WPARAM(0), LPARAM(0));
                    }
                }
            }
            if let Some(handle) = self.win_thread.take() {
                let _ = handle.join();
            }
        }
    }
}

impl Drop for HotkeyManager {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(target_os = "linux")]
mod linux_impl {
    use super::*;

    pub(super) async fn next_stream_item<S>(stream: &mut S) -> Option<S::Item>
    where
        S: Stream + Unpin,
    {
        poll_fn(|cx| Pin::new(&mut *stream).poll_next(cx)).await
    }

    fn next_token(prefix: &str) -> String {
        let seq = TOKEN_COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("clipforge_{prefix}_{}_{}", std::process::id(), seq)
    }

    fn expected_request_path(conn: &zbus::Connection, handle_token: &str) -> Option<String> {
        let unique = conn.unique_name()?;
        let sender = unique.as_str().trim_start_matches(':').replace('.', "_");
        Some(format!(
            "/org/freedesktop/portal/desktop/request/{sender}/{handle_token}"
        ))
    }

    pub(super) async fn run_wayland_portal_loop(
        bindings: Vec<(HotkeyAction, HotkeySpec)>,
        tx: mpsc::UnboundedSender<HotkeyAction>,
        status_tx: mpsc::UnboundedSender<GlobalHotkeyStatus>,
        mut stop_rx: oneshot::Receiver<()>,
    ) {
        let conn = match zbus::Connection::session().await {
            Ok(c) => c,
            Err(err) => {
                warn!(error = %err, "failed to connect to session D-Bus for GlobalShortcuts");
                let _ = status_tx.send(GlobalHotkeyStatus::RegistrationError {
                    message: format!("session D-Bus unavailable: {err}"),
                });
                return;
            }
        };

        let portal =
            match zbus::Proxy::new(&conn, PORTAL_DEST, PORTAL_PATH, GLOBAL_SHORTCUTS_IFACE).await {
                Ok(p) => p,
                Err(err) => {
                    warn!(error = %err, "failed to create GlobalShortcuts portal proxy");
                    let _ = status_tx.send(GlobalHotkeyStatus::RegistrationError {
                        message: format!("GlobalShortcuts portal proxy failed: {err}"),
                    });
                    return;
                }
            };

        // 1. CreateSession
        let create_token = next_token("create");
        let session_token = next_token("session");
        let Some(create_req_path) = expected_request_path(&conn, &create_token) else {
            let _ = status_tx.send(GlobalHotkeyStatus::RegistrationError {
                message: "D-Bus connection missing unique name".to_string(),
            });
            return;
        };

        let mut create_resp_stream =
            match zbus::Proxy::new(&conn, PORTAL_DEST, create_req_path.as_str(), REQUEST_IFACE)
                .await
            {
                Ok(req_proxy) => match req_proxy.receive_signal("Response").await {
                    Ok(s) => s,
                    Err(err) => {
                        let _ = status_tx.send(GlobalHotkeyStatus::RegistrationError {
                            message: format!(
                                "failed to subscribe to portal CreateSession response: {err}"
                            ),
                        });
                        return;
                    }
                },
                Err(err) => {
                    let _ = status_tx.send(GlobalHotkeyStatus::RegistrationError {
                        message: format!("failed to build portal Request proxy: {err}"),
                    });
                    return;
                }
            };

        let mut create_opts: HashMap<&str, Value<'_>> = HashMap::new();
        create_opts.insert("handle_token", Value::from(create_token.as_str()));
        create_opts.insert("session_handle_token", Value::from(session_token.as_str()));

        let returned_req: OwnedObjectPath =
            match portal.call("CreateSession", &(create_opts,)).await {
                Ok(p) => p,
                Err(err) => {
                    warn!(error = %err, "GlobalShortcuts CreateSession failed");
                    let _ = status_tx.send(GlobalHotkeyStatus::RegistrationError {
                        message: format!("GlobalShortcuts CreateSession failed: {err}"),
                    });
                    return;
                }
            };
        if returned_req.as_str() != create_req_path {
            debug!(
                expected = %create_req_path,
                actual = %returned_req,
                "portal CreateSession returned non-standard request path"
            );
        }

        let create_msg = tokio::select! {
            _ = &mut stop_rx => return,
            msg = next_stream_item(&mut create_resp_stream) => msg,
        };
        let Some(create_msg) = create_msg else {
            let _ = status_tx.send(GlobalHotkeyStatus::RegistrationError {
                message: "portal CreateSession response stream ended".to_string(),
            });
            return;
        };

        let (create_code, create_results): (u32, HashMap<String, OwnedValue>) =
            match create_msg.body().deserialize() {
                Ok(v) => v,
                Err(err) => {
                    let _ = status_tx.send(GlobalHotkeyStatus::RegistrationError {
                        message: format!("invalid CreateSession response: {err}"),
                    });
                    return;
                }
            };

        if create_code != 0 {
            let status = if create_code == 1 {
                GlobalHotkeyStatus::FocusedWindowOnly {
                    reason: Some("Wayland portal session cancelled".to_string()),
                }
            } else {
                GlobalHotkeyStatus::RegistrationError {
                    message: format!("portal CreateSession rejected (code {create_code})"),
                }
            };
            let _ = status_tx.send(status);
            return;
        }

        let session_handle_str: String = match create_results
            .get("session_handle")
            .and_then(|v| v.downcast_ref::<&str>().ok())
        {
            Some(s) => s.to_string(),
            None => {
                let _ = status_tx.send(GlobalHotkeyStatus::RegistrationError {
                    message: "portal CreateSession did not return session_handle".to_string(),
                });
                return;
            }
        };
        let Ok(session_path) = ObjectPath::try_from(session_handle_str.as_str()) else {
            let _ = status_tx.send(GlobalHotkeyStatus::RegistrationError {
                message: format!("invalid portal session_handle: {session_handle_str}"),
            });
            return;
        };
        let session_path_owned = OwnedObjectPath::from(session_path.to_owned());

        // 2. Subscribe to Activated BEFORE BindShortcuts so no activations are missed.
        let mut activated_stream = match portal.receive_signal("Activated").await {
            Ok(s) => s,
            Err(err) => {
                let _ = status_tx.send(GlobalHotkeyStatus::RegistrationError {
                    message: format!("failed to subscribe to GlobalShortcuts Activated: {err}"),
                });
                close_portal_session(&conn, &session_path_owned).await;
                return;
            }
        };

        // 3. BindShortcuts (may prompt user in KDE/GNOME portal dialog)
        let _ = status_tx.send(GlobalHotkeyStatus::PermissionPending);
        let bind_token = next_token("bind");
        let Some(bind_req_path) = expected_request_path(&conn, &bind_token) else {
            let _ = status_tx.send(GlobalHotkeyStatus::RegistrationError {
                message: "D-Bus connection missing unique name".to_string(),
            });
            close_portal_session(&conn, &session_path_owned).await;
            return;
        };

        let mut bind_resp_stream =
            match zbus::Proxy::new(&conn, PORTAL_DEST, bind_req_path.as_str(), REQUEST_IFACE).await
            {
                Ok(req_proxy) => match req_proxy.receive_signal("Response").await {
                    Ok(s) => s,
                    Err(err) => {
                        let _ = status_tx.send(GlobalHotkeyStatus::RegistrationError {
                            message: format!(
                                "failed to subscribe to portal BindShortcuts response: {err}"
                            ),
                        });
                        close_portal_session(&conn, &session_path_owned).await;
                        return;
                    }
                },
                Err(err) => {
                    let _ = status_tx.send(GlobalHotkeyStatus::RegistrationError {
                        message: format!(
                            "failed to build portal BindShortcuts Request proxy: {err}"
                        ),
                    });
                    close_portal_session(&conn, &session_path_owned).await;
                    return;
                }
            };

        let trigger_strings: Vec<(HotkeyAction, String)> = bindings
            .iter()
            .map(|(action, spec)| (*action, spec.to_xdg_trigger()))
            .collect();
        let shortcuts_payload: Vec<(&str, HashMap<&str, Value<'_>>)> = trigger_strings
            .iter()
            .map(|(action, trigger)| {
                let mut props: HashMap<&str, Value<'_>> = HashMap::new();
                props.insert("description", Value::from(action.description()));
                props.insert("preferred_trigger", Value::from(trigger.as_str()));
                (action.portal_id(), props)
            })
            .collect();

        let mut bind_opts: HashMap<&str, Value<'_>> = HashMap::new();
        bind_opts.insert("handle_token", Value::from(bind_token.as_str()));

        if let Err(err) = portal
            .call::<_, _, OwnedObjectPath>(
                "BindShortcuts",
                &(&session_path_owned, shortcuts_payload, "", bind_opts),
            )
            .await
        {
            warn!(error = %err, "GlobalShortcuts BindShortcuts failed");
            let _ = status_tx.send(GlobalHotkeyStatus::RegistrationError {
                message: format!("GlobalShortcuts BindShortcuts failed: {err}"),
            });
            close_portal_session(&conn, &session_path_owned).await;
            return;
        }

        let bind_msg = tokio::select! {
            _ = &mut stop_rx => {
                close_portal_session(&conn, &session_path_owned).await;
                return;
            }
            msg = next_stream_item(&mut bind_resp_stream) => msg,
        };
        let Some(bind_msg) = bind_msg else {
            let _ = status_tx.send(GlobalHotkeyStatus::RegistrationError {
                message: "portal BindShortcuts response stream ended".to_string(),
            });
            close_portal_session(&conn, &session_path_owned).await;
            return;
        };

        let (bind_code, _bind_results): (u32, HashMap<String, OwnedValue>) =
            match bind_msg.body().deserialize() {
                Ok(v) => v,
                Err(err) => {
                    let _ = status_tx.send(GlobalHotkeyStatus::RegistrationError {
                        message: format!("invalid BindShortcuts response: {err}"),
                    });
                    close_portal_session(&conn, &session_path_owned).await;
                    return;
                }
            };

        if bind_code != 0 {
            let status = if bind_code == 1 {
                GlobalHotkeyStatus::FocusedWindowOnly {
                    reason: Some("Wayland portal permission cancelled".to_string()),
                }
            } else {
                GlobalHotkeyStatus::RegistrationError {
                    message: format!("portal BindShortcuts rejected (code {bind_code})"),
                }
            };
            let _ = status_tx.send(status);
            close_portal_session(&conn, &session_path_owned).await;
            return;
        }

        info!(session = %session_path_owned, "Wayland GlobalShortcuts portal session active");
        let _ = status_tx.send(GlobalHotkeyStatus::WaylandPortalActive);

        // 4. Listen for Activated signals until stopped.
        loop {
            tokio::select! {
                _ = &mut stop_rx => {
                    close_portal_session(&conn, &session_path_owned).await;
                    break;
                }
                maybe_msg = next_stream_item(&mut activated_stream) => {
                    let Some(msg) = maybe_msg else {
                        warn!("GlobalShortcuts Activated signal stream ended");
                        let _ = status_tx.send(GlobalHotkeyStatus::FocusedWindowOnly {
                            reason: Some("Wayland portal signal stream closed".to_string()),
                        });
                        break;
                    };
                    match msg.body().deserialize::<(OwnedObjectPath, String, u64, HashMap<String, OwnedValue>)>() {
                        Ok((sig_session, shortcut_id, _timestamp, _opts)) => {
                            if sig_session.as_str() == session_path_owned.as_str()
                                && let Some(action) = HotkeyAction::from_portal_id(&shortcut_id)
                            {
                                debug!(%shortcut_id, ?action, "Wayland GlobalShortcuts Activated");
                                if tx.send(action).is_err() {
                                    close_portal_session(&conn, &session_path_owned).await;
                                    break;
                                }
                            }
                        }
                        Err(err) => {
                            warn!(error = %err, "failed to deserialize GlobalShortcuts Activated signal");
                        }
                    }
                }
            }
        }
    }

    async fn close_portal_session(conn: &zbus::Connection, session_path: &OwnedObjectPath) {
        if let Ok(proxy) =
            zbus::Proxy::new(conn, PORTAL_DEST, session_path.as_str(), SESSION_IFACE).await
        {
            let _ = proxy.call::<_, _, ()>("Close", &()).await;
        }
    }

    pub(super) fn find_keycode(
        keysyms: &[u32],
        syms_per_code: usize,
        min_keycode: u8,
        target_keysym: u32,
    ) -> Option<u8> {
        for (idx, chunk) in keysyms.chunks_exact(syms_per_code).enumerate() {
            if chunk.first().copied() == Some(target_keysym)
                || chunk.get(1).copied() == Some(target_keysym)
            {
                return u8::try_from(usize::from(min_keycode) + idx).ok();
            }
        }
        None
    }
} // mod linux_impl

#[cfg(target_os = "linux")]
use linux_impl::{find_keycode, run_wayland_portal_loop};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hotkey_valid_combinations() {
        let f9 = parse_hotkey("Ctrl+Alt+F9").expect("valid");
        assert_eq!(f9.modifiers, MOD_CONTROL | MOD_ALT);
        assert_eq!(f9.keysym, 0xFFBE + 8);
        assert_eq!(f9.to_xdg_trigger(), "<Control><Alt>F9");

        let letter = parse_hotkey("Shift+Super+R").expect("valid");
        assert_eq!(letter.modifiers, MOD_SHIFT | MOD_SUPER);
        assert_eq!(letter.keysym, 'r' as u32);
        assert_eq!(letter.to_xdg_trigger(), "<Shift><Super>r");

        #[cfg(target_os = "linux")]
        {
            assert_eq!(MOD_CONTROL, u16::from(ModMask::CONTROL));
            assert_eq!(MOD_ALT, u16::from(ModMask::M1));
            assert_eq!(MOD_SHIFT, u16::from(ModMask::SHIFT));
            assert_eq!(MOD_SUPER, u16::from(ModMask::M4));
        }

        let single = parse_hotkey("F12").expect("valid");
        assert_eq!(single.modifiers, 0);
        assert_eq!(single.keysym, 0xFFC9);
        assert_eq!(single.to_xdg_trigger(), "F12");
    }

    #[test]
    fn parse_hotkey_rejects_invalid() {
        assert!(parse_hotkey("").is_err());
        assert!(parse_hotkey("Ctrl+").is_err());
        assert!(parse_hotkey("BogusMod+F9").is_err());
        assert!(parse_hotkey("Ctrl+UnknownKey").is_err());
    }

    #[test]
    fn select_hotkey_backend_prefers_wayland_then_x11_then_focused() {
        assert_eq!(
            select_hotkey_backend(Some("wayland"), Some("wayland-1"), Some(":0")),
            HotkeyBackendKind::WaylandPortal
        );
        assert_eq!(
            select_hotkey_backend(None, Some("wayland-0"), Some(":0")),
            HotkeyBackendKind::WaylandPortal
        );
        assert_eq!(
            select_hotkey_backend(Some("x11"), None, Some(":0")),
            HotkeyBackendKind::X11
        );
        assert_eq!(
            select_hotkey_backend(None, None, Some(":0")),
            HotkeyBackendKind::X11
        );
        assert_eq!(
            select_hotkey_backend(None, None, None),
            HotkeyBackendKind::FocusedOnly
        );
    }

    #[test]
    fn portal_id_roundtrip_covers_all_capture_actions() {
        for action in [
            HotkeyAction::SaveReplay,
            HotkeyAction::ToggleReplayBuffer,
            HotkeyAction::ToggleRecording,
        ] {
            let id = action.portal_id();
            assert_eq!(HotkeyAction::from_portal_id(id), Some(action));
            assert!(!action.description().is_empty());
        }
        assert_eq!(HotkeyAction::from_portal_id("unknown"), None);
    }

    #[test]
    fn start_with_backend_empty_bindings_reports_registration_error() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let (status_tx, mut status_rx) = mpsc::unbounded_channel();
        let _mgr = HotkeyManager::start_with_backend(
            HotkeyBackendKind::FocusedOnly,
            vec![],
            tx,
            status_tx,
        );
        let status = status_rx.try_recv().expect("status emitted");
        assert!(matches!(
            status,
            GlobalHotkeyStatus::RegistrationError { .. }
        ));
        assert!(!status.is_global_active());
    }

    #[test]
    fn start_with_backend_focused_only_reports_truthful_status() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let (status_tx, mut status_rx) = mpsc::unbounded_channel();
        let bindings = vec![
            (HotkeyAction::SaveReplay, parse_hotkey("F9").unwrap()),
            (
                HotkeyAction::ToggleReplayBuffer,
                parse_hotkey("F10").unwrap(),
            ),
            (HotkeyAction::ToggleRecording, parse_hotkey("F11").unwrap()),
        ];
        let _mgr = HotkeyManager::start_with_backend(
            HotkeyBackendKind::FocusedOnly,
            bindings,
            tx,
            status_tx,
        );
        let status = status_rx.try_recv().expect("status emitted");
        assert!(matches!(
            status,
            GlobalHotkeyStatus::FocusedWindowOnly { .. }
        ));
        assert!(status.label().starts_with("Focused-Window Only"));
    }

    #[test]
    fn global_hotkey_status_labels_match_expected_strings() {
        st_assert_status_labels();
    }

    fn st_assert_status_labels() {
        assert_eq!(
            GlobalHotkeyStatus::WaylandPortalActive.label(),
            "GlobalShortcuts (Wayland Portal Active)"
        );
        assert!(GlobalHotkeyStatus::WaylandPortalActive.is_global_active());
        assert_eq!(GlobalHotkeyStatus::X11GlobalActive.label(), "X11 Global");
        assert!(GlobalHotkeyStatus::X11GlobalActive.is_global_active());
        assert_eq!(
            GlobalHotkeyStatus::WindowsGlobalActive.label(),
            "Windows Global (RegisterHotKey)"
        );
        assert!(GlobalHotkeyStatus::WindowsGlobalActive.is_global_active());
        assert_eq!(
            GlobalHotkeyStatus::PermissionPending.label(),
            "Permission Pending"
        );
        assert!(!GlobalHotkeyStatus::PermissionPending.is_global_active());
    }

    #[test]
    fn parse_windows_hotkey_maps_function_and_modified_keys() {
        let f9 = parse_windows_hotkey("F9").unwrap();
        assert_eq!(f9.vk, 0x78); // VK_F9
        assert_eq!(f9.modifiers, WIN_MOD_NOREPEAT);

        let f10 = parse_windows_hotkey("F10").unwrap();
        assert_eq!(f10.vk, 0x79); // VK_F10

        let f11 = parse_windows_hotkey("F11").unwrap();
        assert_eq!(f11.vk, 0x7A); // VK_F11

        let combo = parse_windows_hotkey("Ctrl+Alt+Shift+R").unwrap();
        assert_eq!(combo.vk, b'R' as u32);
        assert_eq!(
            combo.modifiers,
            WIN_MOD_CONTROL | WIN_MOD_ALT | WIN_MOD_SHIFT | WIN_MOD_NOREPEAT
        );

        let win_key = parse_windows_hotkey("Super+Space").unwrap();
        assert_eq!(win_key.vk, 0x20);
        assert_eq!(win_key.modifiers, WIN_MOD_WIN | WIN_MOD_NOREPEAT);

        assert!(parse_windows_hotkey("").is_err());
        assert!(parse_windows_hotkey("Ctrl+UnknownKey").is_err());
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[ignore = "requires live Wayland session with org.freedesktop.portal.GlobalShortcuts"]
    async fn live_wayland_portal_creates_session_and_attempts_bind() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let (status_tx, mut status_rx) = mpsc::unbounded_channel();
        let bindings = vec![
            (HotkeyAction::SaveReplay, parse_hotkey("F9").unwrap()),
            (
                HotkeyAction::ToggleReplayBuffer,
                parse_hotkey("F10").unwrap(),
            ),
            (HotkeyAction::ToggleRecording, parse_hotkey("F11").unwrap()),
        ];

        let mut mgr = HotkeyManager::start_wayland_portal(bindings, tx, status_tx);

        let mut statuses = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout(Duration::from_millis(250), status_rx.recv()).await {
                Ok(Some(st)) => {
                    let done = matches!(
                        st,
                        GlobalHotkeyStatus::WaylandPortalActive
                            | GlobalHotkeyStatus::FocusedWindowOnly { .. }
                            | GlobalHotkeyStatus::RegistrationError { .. }
                    );
                    statuses.push(st);
                    if done {
                        break;
                    }
                }
                Ok(None) => break,
                Err(_) => continue,
            }
        }
        mgr.stop();

        eprintln!("live Wayland portal statuses: {statuses:?}");
        assert!(
            !statuses.is_empty(),
            "expected at least one status from Wayland portal"
        );
        assert!(
            !matches!(
                statuses.last(),
                Some(GlobalHotkeyStatus::RegistrationError { .. })
            ),
            "unexpected portal registration error: {statuses:?}"
        );
    }
}
