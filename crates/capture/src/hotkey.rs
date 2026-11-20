use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use clipforge_common::{ClipForgeError, Result};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{ConnectionExt, EventMask, GrabMode, ModMask, Window};

/// Logical action triggered by a global hotkey.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HotkeyAction {
    SaveReplay,
    ToggleRecording,
}

/// Parsed X11 hotkey specification (modifier mask + X11 keysym).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeySpec {
    pub modifiers: u16,
    pub keysym: u32,
    pub display: String,
}

/// Parses a human-readable hotkey string such as `"Ctrl+Alt+F9"` or `"Shift+Super+R"`.
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
            "ctrl" | "control" => modifiers |= u16::from(ModMask::CONTROL),
            "alt" | "mod1" => modifiers |= u16::from(ModMask::M1),
            "shift" => modifiers |= u16::from(ModMask::SHIFT),
            "super" | "meta" | "win" | "mod4" => modifiers |= u16::from(ModMask::M4),
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
    // F1..F12 (XK_F1 = 0xFFBE .. XK_F12 = 0xFFC9)
    if let Some(num_str) = lower.strip_prefix('f')
        && let Ok(n) = num_str.parse::<u32>()
        && (1..=12).contains(&n)
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

/// Manages global X11 hotkeys in a background thread using pure-Rust `x11rb`.
pub struct HotkeyManager {
    stop_flag: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl HotkeyManager {
    /// Registers the given `(HotkeyAction, HotkeySpec)` bindings on the active X11 display.
    ///
    /// Returns `ClipForgeError::CaptureUnavailable` if no X11 display is reachable.
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
                                    debug!(?action, "global hotkey triggered");
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
            handle: Some(handle),
        })
    }

    /// Stops the background hotkey listener thread.
    pub fn stop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for HotkeyManager {
    fn drop(&mut self) {
        self.stop();
    }
}

fn find_keycode(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hotkey_valid_combinations() {
        let f9 = parse_hotkey("Ctrl+Alt+F9").expect("valid");
        assert_eq!(
            f9.modifiers,
            u16::from(ModMask::CONTROL) | u16::from(ModMask::M1)
        );
        assert_eq!(f9.keysym, 0xFFBE + 8);

        let letter = parse_hotkey("Shift+Super+R").expect("valid");
        assert_eq!(
            letter.modifiers,
            u16::from(ModMask::SHIFT) | u16::from(ModMask::M4)
        );
        assert_eq!(letter.keysym, 'r' as u32);

        let single = parse_hotkey("F12").expect("valid");
        assert_eq!(single.modifiers, 0);
        assert_eq!(single.keysym, 0xFFC9);
    }

    #[test]
    fn parse_hotkey_rejects_invalid() {
        assert!(parse_hotkey("").is_err());
        assert!(parse_hotkey("Ctrl+").is_err());
        assert!(parse_hotkey("BogusMod+F9").is_err());
        assert!(parse_hotkey("Ctrl+UnknownKey").is_err());
    }
}
