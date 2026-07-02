//! Central controller: owns `AppState`, dispatches events, drives mode transitions.
//!
//! State is stored in a thread-local because the hook callback runs on the
//! same UI thread that owns the message loop.

use std::cell::RefCell;
use std::collections::HashMap;

use windows_sys::Win32::Foundation::{HWND, POINT};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::{bindings, config, overlay, tooltip, touch};

pub mod constants;

// ---------------------------------------------------------------------------
// Mode
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Main window visible, keys pass through normally.
    Idle,
    /// Waiting for a key press to bind current cursor position.
    Recording,
    /// Window hidden, bound keys injected as touch events.
    Started,
}

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

pub struct AppState {
    pub mode: Mode,
    pub bindings: HashMap<u32, POINT>,
    pub active: HashMap<u32, u32>,
    pub next_pointer_id: u32,
    pub ctrl_down: bool,
    pub gesture_anchor: Option<u32>,
    pub overlay_visible: bool,
    pub main_hwnd: HWND,
    pub overlay_hwnd: HWND,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            mode: Mode::Idle,
            bindings: HashMap::new(),
            active: HashMap::new(),
            next_pointer_id: 1,
            ctrl_down: false,
            gesture_anchor: None,
            overlay_visible: false,
            main_hwnd: std::ptr::null_mut(),
            overlay_hwnd: std::ptr::null_mut(),
        }
    }
}

// ---------------------------------------------------------------------------
// Thread-local state
// ---------------------------------------------------------------------------

thread_local! {
    pub static STATE: RefCell<AppState> = RefCell::new(AppState::default());
}

// ---------------------------------------------------------------------------
// Public API — called from wnd_proc / hook
// ---------------------------------------------------------------------------

impl AppState {
    /// Handle a toolbar button command (Record / Show Positions / Start / Stop).
    pub fn on_toolbar_command(&mut self, id: u16) {
        match id {
            constants::ID_RECORD => self.enter_recording(),
            constants::ID_SHOW_POSITIONS => self.toggle_overlay(),
            constants::ID_START => self.enter_started(),
            constants::ID_STOP => self.stop(), // tray "Stop"
            constants::ID_QUIT => unsafe {
                PostQuitMessage(0);
            },
            constants::ID_RESET => self.reset_bindings(),
            _ => {}
        }
    }

    /// Handle a key event from the low-level keyboard hook.
    /// Returns `true` if the event should be swallowed (not passed to other apps).
    pub fn on_key_event(&mut self, vk: u32, down: bool) -> bool {
        // Track Ctrl state
        match vk {
            0x11 | 0xA2 | 0xA3 => {
                // VK_CONTROL, VK_LCONTROL, VK_RCONTROL
                self.ctrl_down = down;
                if !down {
                    // Ctrl released — finalise any gesture
                    self.finalise_gesture();
                }
                return false; // don't swallow Ctrl itself
            }
            _ => {}
        }

        match self.mode {
            Mode::Idle => false, // pass through

            Mode::Recording => {
                if down && !self.is_modifier(vk) {
                    self.bind_key(vk);
                    true // swallow
                } else {
                    false
                }
            }

            Mode::Started => {
                if !down {
                    // Key up: release touch if active
                    self.release_touch(vk);
                    // Don't swallow key-up for non-bound keys
                    self.active.contains_key(&vk)
                } else if self.is_modifier(vk) {
                    false // pass through modifiers
                } else if self.bindings.contains_key(&vk) {
                    self.process_bound_key_down(vk);
                    true // swallow
                } else {
                    false // non-bound key, pass through
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Internal helpers
    // ------------------------------------------------------------------

    fn is_modifier(&self, vk: u32) -> bool {
        matches!(vk, 0x10 | 0x11 | 0x12 | 0xA0..=0xA5)
    }

    // ---------- Recording ----------

    fn bind_key(&mut self, vk: u32) {
        let mut pt = POINT { x: 0, y: 0 };
        unsafe {
            GetCursorPos(&mut pt);
        }

        bindings::upsert(&mut self.bindings, vk, pt);
        self.mode = Mode::Idle;

        // Persist
        let cfg = config::Config {
            bindings: self
                .bindings
                .iter()
                .map(|(&vk, &pt)| bindings::Binding {
                    vk,
                    x: pt.x,
                    y: pt.y,
                })
                .collect(),
        };
        let _ = config::save(&cfg);

        // Redraw overlay if visible
        if self.overlay_visible && self.overlay_hwnd != std::ptr::null_mut() {
            overlay::show_overlay(self.overlay_hwnd, &self.bindings);
        }

        // Hide instruction tooltip
        tooltip::hide_tooltip();
    }

    // ---------- Overlay ----------

    fn toggle_overlay(&mut self) {
        self.overlay_visible = !self.overlay_visible;
        if self.overlay_visible {
            overlay::show_overlay(self.overlay_hwnd, &self.bindings);
        } else {
            overlay::hide_overlay(self.overlay_hwnd);
        }
    }

    // ---------- Reset bindings ----------

    fn reset_bindings(&mut self) {
        // Release all active touches
        for vk in self.active.keys().copied().collect::<Vec<_>>() {
            self.release_touch(vk);
        }
        self.active.clear();
        self.gesture_anchor = None;
        self.bindings.clear();

        // Persist empty config
        let _ = config::save(&config::Config { bindings: vec![] });

        // Hide overlay if visible
        if self.overlay_visible {
            self.overlay_visible = false;
            overlay::hide_overlay(self.overlay_hwnd);
        }
    }

    // ---------- Recording entry ----------

    pub fn enter_recording(&mut self) {
        self.mode = Mode::Recording;
        tooltip::show_instruction_tooltip(
            self.main_hwnd,
            "Move cursor and press any key to bind it to that position. Press Esc to cancel.",
        );
    }

    // ---------- Started entry ----------

    pub fn enter_started(&mut self) {
        if self.bindings.is_empty() {
            // Nothing to do
            return;
        }
        self.mode = Mode::Started;
        unsafe {
            ShowWindow(self.main_hwnd, SW_HIDE);
        }
        self.next_pointer_id = 0;
    }

    // ---------- Stop ----------

    pub fn stop(&mut self) {
        // Release all active touches
        for vk in self.active.keys().copied().collect::<Vec<_>>() {
            self.release_touch(vk);
        }
        self.active.clear();
        self.gesture_anchor = None;
        self.mode = Mode::Idle;
        unsafe {
            ShowWindow(self.main_hwnd, SW_SHOW);
        }
    }

    // ---------- Touch injection ----------

    fn process_bound_key_down(&mut self, vk: u32) {
        eprintln!(
            "key down: {vk}, gesture_anchor: {:?}, ctrl: {}",
            self.gesture_anchor, self.ctrl_down
        );
        let pos = self.bindings[&vk];

        if self.ctrl_down {
            if let Some(anchor_vk) = self.gesture_anchor {
                if anchor_vk == vk {
                    return;
                }
                // Gesture move: drag anchor to this key's position
                if let Some(&pointer_id) = self.active.get(&anchor_vk) {
                    let from = self.bindings[&anchor_vk];
                    // Inject interpolated move
                    touch::touch_move(pointer_id, from, pos);
                }
            } else {
                // First key while Ctrl held = gesture anchor
                self.gesture_anchor = Some(vk);
                let pid = self.allocate_pointer_id();
                if self.active.insert(vk, pid).is_none() {
                    touch::touch_down(pid, pos);
                }
            }
        } else {
            // Simple touch press
            let pid = self.allocate_pointer_id();
            if self.active.insert(vk, pid).is_none() {
                touch::touch_down(pid, pos);
            }
        }
    }

    fn release_touch(&mut self, vk: u32) {
        if let Some(pid) = self.active.remove(&vk) {
            let pos = self.bindings[&vk];
            touch::touch_up(pid, pos);
        }
    }

    fn finalise_gesture(&mut self) {
        if let Some(anchor_vk) = self.gesture_anchor.take() {
            self.release_touch(anchor_vk);
        }
    }

    fn allocate_pointer_id(&mut self) -> u32 {
        let id = self.next_pointer_id;
        self.next_pointer_id += 1;
        id
    }
}
