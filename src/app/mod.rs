//! Central controller: owns `AppState`, dispatches events, drives mode transitions.
//!
//! State is stored in a thread-local because the hook callback runs on the
//! same UI thread that owns the message loop.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use rand::Rng;
use windows_sys::Win32::Foundation::{HWND, POINT};
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use windows_sys::core::w;

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
    /// Bound keys are injected as touch events; the main window may remain visible.
    Started,
}

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

pub struct ActiveTouch {
    pointer_id: touch::PointerId,
    /// The randomized position used for the initial touch-down event.
    pos: POINT,
}

pub struct AppState {
    pub mode: Mode,
    pub bindings: HashMap<u32, POINT>,
    pub active: HashMap<u32, ActiveTouch>,
    /// Maximum distance, in pixels, by which a new touch may be displaced.
    pub random_touch_radius: u32,
    /// Interval in milliseconds for repeating a held key's touch; 0 disables it.
    pub repeat_ms: u32,
    /// Toggle bindings on/off with each press instead of holding them.
    pub toggle_mode: bool,
    /// Keys currently held by the physical keyboard and how each press behaves.
    binding_key_downs: HashMap<u32, bool>,
    /// Active bindings, also used by the overlay highlight.
    pub pressed: HashSet<u32>,
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
            random_touch_radius: 0,
            repeat_ms: 0,
            toggle_mode: false,
            binding_key_downs: HashMap::new(),
            pressed: HashSet::new(),
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
    /// Handle a toolbar or tray command (Record / Show Positions / Start / Stop).
    pub fn on_toolbar_command(&mut self, id: u16) {
        match id {
            constants::ID_RECORD => self.enter_recording(),
            constants::ID_SHOW_POSITIONS => self.toggle_overlay(),
            constants::ID_START => {
                if self.mode == Mode::Started {
                    self.stop();
                } else {
                    self.enter_started();
                }
            }
            constants::ID_STOP => self.stop(), // tray "Stop"
            constants::ID_QUIT => unsafe {
                PostQuitMessage(0);
            },
            constants::ID_RESET => {
                let confirmed = unsafe {
                    MessageBoxW(
                        self.main_hwnd,
                        w!("This will remove all key bindings. Are you sure you want to reset?"),
                        w!("Confirm Reset"),
                        MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2,
                    )
                } == IDYES;
                if confirmed {
                    self.reset_bindings();
                }
            }
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

        if !down {
            if let Some(was_toggle_mode) = self.binding_key_downs.remove(&vk) {
                if !was_toggle_mode {
                    let was_pressed = self.pressed.remove(&vk);
                    self.release_touch(vk);
                    if was_pressed && self.pressed.is_empty() {
                        self.update_repeat_timer();
                    }
                    self.refresh_overlay();
                }
                return true;
            }
        } else if self.binding_key_downs.contains_key(&vk) {
            // Swallow OS key-repeat until the matching physical key-up.
            return true;
        }

        match self.mode {
            Mode::Idle => false, // pass through

            Mode::Recording => {
                if down && !self.is_modifier(vk) {
                    self.bind_key(vk);
                    true // swallow
                } else if !down && self.bindings.contains_key(&vk) {
                    // The matching key-down was swallowed while recording;
                    // swallow its key-up too so the foreground app never
                    // receives an unmatched release.
                    true
                } else {
                    false
                }
            }

            Mode::Started => {
                if !down {
                    // Fallback for a key-down that was not tracked above.
                    let was_pressed = self.pressed.remove(&vk);
                    self.release_touch(vk);
                    if was_pressed && self.pressed.is_empty() {
                        self.update_repeat_timer();
                    }
                    self.refresh_overlay();
                    was_pressed
                } else if self.is_modifier(vk) {
                    false // pass through modifiers
                } else if self.bindings.contains_key(&vk) {
                    self.binding_key_downs.insert(vk, self.toggle_mode);

                    if self.toggle_mode && self.pressed.remove(&vk) {
                        self.release_touch(vk);
                        if self.pressed.is_empty() {
                            self.update_repeat_timer();
                        }
                    } else {
                        let was_empty = self.pressed.is_empty();
                        self.pressed.insert(vk);
                        if was_empty {
                            self.update_repeat_timer();
                        }
                        self.process_bound_key_down(vk);
                    }
                    self.refresh_overlay();
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

    /// Persist bindings and touch settings to `Documents/clickassist.json`.
    pub fn save_config(&self) {
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
            randomize_touch: self.random_touch_radius,
            repeat_ms: self.repeat_ms,
            toggle_mode: self.toggle_mode,
        };
        let _ = config::save(&cfg);
    }

    /// Set and persist the held-key touch-repeat interval.
    pub fn set_repeat_ms(&mut self, repeat_ms: u32) {
        self.repeat_ms = repeat_ms;
        self.save_config();
        self.update_repeat_timer();
    }

    pub fn set_toggle_mode(&mut self, toggle_mode: bool) {
        self.toggle_mode = toggle_mode;
        self.save_config();
    }

    fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        self.update_repeat_timer();
    }

    fn update_repeat_timer(&self) {
        unsafe {
            if self.mode == Mode::Started && !self.pressed.is_empty() && self.repeat_ms > 0 {
                SetTimer(
                    self.main_hwnd,
                    constants::TIMER_REPEAT_TOUCH,
                    self.repeat_ms,
                    None,
                );
            } else {
                KillTimer(self.main_hwnd, constants::TIMER_REPEAT_TOUCH);
            }
        }
    }

    fn bind_key(&mut self, vk: u32) {
        let mut pt = POINT { x: 0, y: 0 };
        unsafe {
            GetCursorPos(&mut pt);
        }

        bindings::upsert(&mut self.bindings, vk, pt);
        self.set_mode(Mode::Idle);

        // Persist
        self.save_config();

        // Preserve the positions overlay while recording. Invalidating it
        // causes a normal WM_PAINT redraw; it must not be toggled or hidden
        // when the recorded key is pressed.
        if self.overlay_visible && self.overlay_hwnd != std::ptr::null_mut() {
            overlay::refresh_overlay(self.overlay_hwnd);
        }

        // Hide instruction tooltip
        tooltip::hide_tooltip();
    }

    // ---------- Overlay ----------

    /// Remove a binding selected from the positions overlay.
    pub fn remove_binding(&mut self, vk: u32) {
        if self.bindings.remove(&vk).is_none() {
            return;
        }

        // Persist (includes the randomize_touch radius so it is not lost).
        self.save_config();

        // Keep Show Positions in sync immediately after a dot is removed.
        self.refresh_overlay();
    }

    /// Queue a redraw when the overlay is visible, including active-key state.
    fn refresh_overlay(&self) {
        if self.overlay_visible && self.overlay_hwnd != std::ptr::null_mut() {
            overlay::refresh_overlay(self.overlay_hwnd);
        }
    }

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
        self.pressed.clear();
        self.gesture_anchor = None;
        self.bindings.clear();
        self.set_mode(Mode::Idle);

        // Persist empty binding list (keeps the randomize_touch radius).
        self.save_config();

        // Hide overlay if visible
        if self.overlay_visible {
            self.overlay_visible = false;
            overlay::hide_overlay(self.overlay_hwnd);
        }
    }

    // ---------- Recording entry ----------

    pub fn enter_recording(&mut self) {
        self.set_mode(Mode::Recording);
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
        self.set_mode(Mode::Started);
    }

    // ---------- Stop ----------

    pub fn stop(&mut self) {
        // Release all active touches
        for vk in self.active.keys().copied().collect::<Vec<_>>() {
            self.release_touch(vk);
        }
        self.active.clear();
        self.pressed.clear();
        self.gesture_anchor = None;
        self.set_mode(Mode::Idle);
        unsafe {
            ShowWindow(self.main_hwnd, SW_RESTORE);
        }
    }

    // ---------- Touch injection ----------

    /// Return a uniformly random point within the configured circular radius.
    fn randomized_touch_position(&self, pos: POINT) -> POINT {
        if self.random_touch_radius == 0 {
            return pos;
        }

        let mut rng = rand::rng();
        let angle = rng.random_range(0.0..std::f64::consts::TAU);
        // sqrt produces a uniform distribution over the circle's area rather
        // than concentrating touches near its centre.
        let distance = (rng.random::<f64>().sqrt() * self.random_touch_radius as f64).round();
        POINT {
            x: pos
                .x
                .saturating_add((distance * angle.cos()).round() as i32),
            y: pos
                .y
                .saturating_add((distance * angle.sin()).round() as i32),
        }
    }

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
                if let Some(active) = self.active.get(&anchor_vk) {
                    // Inject interpolated move from the actual touch-down
                    // position, which may have been randomized.
                    touch::touch_move(&active.pointer_id, active.pos, pos);
                }
            } else {
                // First key while Ctrl held = gesture anchor
                self.gesture_anchor = Some(vk);
                let touch_pos = self.randomized_touch_position(pos);
                if let Some(pointer_id) = touch::touch_down(touch_pos) {
                    self.active.insert(
                        vk,
                        ActiveTouch {
                            pointer_id,
                            pos: touch_pos,
                        },
                    );
                }
            }
        } else if !self.active.contains_key(&vk) {
            // Simple touch press
            let touch_pos = self.randomized_touch_position(pos);
            if let Some(pointer_id) = touch::touch_down(touch_pos) {
                self.active.insert(
                    vk,
                    ActiveTouch {
                        pointer_id,
                        pos: touch_pos,
                    },
                );
            }
        }
    }

    /// Re-tap all held bindings when repeating is enabled.
    pub fn repeat_held_touches(&mut self) {
        if self.repeat_ms == 0
            || self.ctrl_down
            || self.mode != Mode::Started
            || self.pressed.is_empty()
        {
            return;
        }

        let held_bindings: Vec<u32> = self
            .pressed
            .iter()
            .copied()
            .filter(|vk| self.bindings.contains_key(vk))
            .collect();

        for vk in held_bindings {
            if let Some(active) = self.active.remove(&vk) {
                touch::touch_up(active.pointer_id, active.pos);
            }
            let touch_pos = self.randomized_touch_position(self.bindings[&vk]);
            if let Some(pointer_id) = touch::touch_down(touch_pos) {
                self.active.insert(
                    vk,
                    ActiveTouch {
                        pointer_id,
                        pos: touch_pos,
                    },
                );
            }
        }
    }

    fn release_touch(&mut self, vk: u32) {
        eprintln!(
            "key up: {vk}, gesture_anchor: {:?}, ctrl: {}",
            self.gesture_anchor, self.ctrl_down
        );
        if let Some(active) = self.active.remove(&vk) {
            touch::touch_up(active.pointer_id, active.pos);
        }
    }

    fn finalise_gesture(&mut self) {
        if let Some(anchor_vk) = self.gesture_anchor.take() {
            self.release_touch(anchor_vk);
            self.refresh_overlay();
        }
    }
}
