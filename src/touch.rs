//! Touch injection engine: press, release, multi-touch, interpolated gesture
//! moves.
//!
//! Uses `InitializeTouchInjection` + `InjectTouchInput` (Windows 8+).
//!
//! A dedicated background thread runs a fixed-timestep game loop at ~60 Hz.
//! Every frame:
//!
//! 1. Drain the command channel (non-blocking).
//! 2. Advance move interpolations.
//! 3. Compute `POINTER_FLAG_*` for each contact via a state machine that
//!    guarantees a `UPDATE` frame between `DOWN` and `UP` so
//!    `ERROR_INVALID_PARAMETER` never occurs.
//! 4. Call `InjectTouchInput` **once**.
//! 5. Increment frame counters; contacts that emitted `UP` are removed.
//! 6. Sleep until the next frame boundary.

use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{GetLastError, POINT, RECT};
use windows_sys::Win32::UI::Input::Pointer::{
    InitializeTouchInjection, InjectTouchInput, POINTER_FLAGS, POINTER_FLAG_DOWN,
    POINTER_FLAG_INCONTACT, POINTER_FLAG_INRANGE, POINTER_FLAG_UP, POINTER_FLAG_UPDATE,
    POINTER_TOUCH_INFO, TOUCH_FEEDBACK_DEFAULT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    PT_TOUCH, TOUCH_FLAG_NONE, TOUCH_MASK_CONTACTAREA, TOUCH_MASK_ORIENTATION, TOUCH_MASK_PRESSURE,
};

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

enum TouchCommand {
    Down { pointer_id: u32, pos: POINT },
    Up { pointer_id: u32, pos: POINT },
    Move { pointer_id: u32, from: POINT, to: POINT },
    Shutdown,
}

impl std::fmt::Debug for TouchCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Down { pointer_id, pos } => f
                .debug_struct("Down")
                .field("pointer_id", pointer_id)
                .field("pos.x", &pos.x)
                .field("pos.y", &pos.y)
                .finish(),
            Self::Up { pointer_id, pos } => f
                .debug_struct("Up")
                .field("pointer_id", pointer_id)
                .field("pos.x", &pos.x)
                .field("pos.y", &pos.y)
                .finish(),
            Self::Move {
                pointer_id, from, to,
            } => f
                .debug_struct("Move")
                .field("pointer_id", pointer_id)
                .field("from.x", &from.x)
                .field("from.y", &from.y)
                .field("to.x", &to.x)
                .field("to.y", &to.y)
                .finish(),
            Self::Shutdown => f.debug_struct("Shutdown").finish(),
        }
    }
}

// ---------------------------------------------------------------------------
// Per-contact state
// ---------------------------------------------------------------------------

struct Contact {
    pointer_id: u32,
    pos: POINT,
    /// Frames emitted for this contact (0 = not yet injected).
    frames_emitted: u32,
    pending_up: bool,
    transition: Option<Transition>,
}

struct Transition {
    start: Instant,
    duration: Duration,
    from: POINT,
    to: POINT,
}

impl std::fmt::Debug for Transition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Transition")
            .field("elapsed", &self.start.elapsed().as_secs_f64())
            .field("duration", &self.duration.as_secs_f64())
            .field("from.x", &self.from.x)
            .field("from.y", &self.from.y)
            .field("to.x", &self.to.x)
            .field("to.y", &self.to.y)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Singleton
// ---------------------------------------------------------------------------

struct EngineState {
    sender: Sender<TouchCommand>,
    thread: Option<std::thread::JoinHandle<()>>,
}

static ENGINE: Mutex<Option<EngineState>> = Mutex::new(None);

const FRAME_DURATION: Duration = Duration::from_nanos(16_666_667);
const MOVE_DURATION: Duration = Duration::from_secs(1);

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn init_touch_injection(max_contacts: u32) {
    let (tx, rx) = mpsc::channel::<TouchCommand>();

    let handle = std::thread::Builder::new()
        .name("touch-inject".into())
        .spawn(move || {
            let ok = unsafe { InitializeTouchInjection(max_contacts, TOUCH_FEEDBACK_DEFAULT) };
            if ok == 0 {
                eprintln!("[touch] InitializeTouchInjection failed");
            }
            run_game_loop(rx);
        })
        .expect("failed to spawn touch injection thread");

    let mut guard = ENGINE.lock().expect("ENGINE lock poisoned");
    *guard = Some(EngineState {
        sender: tx,
        thread: Some(handle),
    });
}

pub fn deinit_touch_injection() {
    let mut guard = ENGINE.lock().expect("ENGINE lock poisoned");
    if let Some(mut state) = guard.take() {
        let _ = state.sender.send(TouchCommand::Shutdown);
        if let Some(handle) = state.thread.take() {
            let _ = handle.join();
        }
    }
}

pub fn touch_down(pointer_id: u32, pos: POINT) {
    let guard = ENGINE.lock().expect("ENGINE lock poisoned");
    if let Some(ref state) = *guard {
        let _ = state.sender.send(TouchCommand::Down { pointer_id, pos });
    }
}

pub fn touch_up(pointer_id: u32, pos: POINT) {
    let guard = ENGINE.lock().expect("ENGINE lock poisoned");
    if let Some(ref state) = *guard {
        let _ = state.sender.send(TouchCommand::Up { pointer_id, pos });
    }
}

pub fn touch_move(pointer_id: u32, from: POINT, to: POINT) {
    let guard = ENGINE.lock().expect("ENGINE lock poisoned");
    if let Some(ref state) = *guard {
        let _ = state.sender.send(TouchCommand::Move {
            pointer_id,
            from,
            to,
        });
    }
}

// ---------------------------------------------------------------------------
// Game loop
// ---------------------------------------------------------------------------

fn run_game_loop(rx: Receiver<TouchCommand>) {
    let mut contacts: Vec<Contact> = Vec::new();
    let mut infos: Vec<POINTER_TOUCH_INFO> = Vec::new();
    let mut shutting_down = false;

    loop {
        let frame_start = Instant::now();

        // -- 1. Drain commands ------------------------------------------

        shutting_down |= drain_commands(&rx, &mut contacts, shutting_down);

        // -- 2. Advance interpolations ----------------------------------

        let now = Instant::now();
        for contact in &mut contacts {
            if let Some(ref trans) = contact.transition {
                let elapsed = now.duration_since(trans.start);
                if elapsed >= trans.duration {
                    contact.pos = trans.to;
                    contact.transition = None;
                } else {
                    let t = elapsed.as_secs_f64() / trans.duration.as_secs_f64();
                    contact.pos.x = lerp_i32(trans.from.x, trans.to.x, t);
                    contact.pos.y = lerp_i32(trans.from.y, trans.to.y, t);
                }
            }
        }

        // -- 3. Build POINTER_TOUCH_INFO array --------------------------

        infos.clear();
        let mut removal_indices: Vec<usize> = Vec::new();

        for (idx, contact) in contacts.iter().enumerate() {
            let flags = contact_flags(contact);
            infos.push(make_touch_info(contact.pointer_id, contact.pos, flags));
            if flags & POINTER_FLAG_UP != 0 {
                removal_indices.push(idx);
            }
        }

        // -- 4. Single InjectTouchInput ---------------------------------

        if !infos.is_empty() {
            unsafe {
                if InjectTouchInput(infos.len() as u32, infos.as_ptr()) == 0 {
                    eprintln!("[touch] InjectTouchInput failed: error={}", GetLastError());
                }
            }
        }

        // -- 5. Advance counters; remove finished contacts ---------------

        for contact in &mut contacts {
            contact.frames_emitted += 1;
        }

        // Remove in reverse to preserve indices.
        for &idx in removal_indices.iter().rev() {
            contacts.remove(idx);
        }

        // -- 6. Exit check -----------------------------------------------

        if shutting_down && contacts.is_empty() {
            break;
        }

        // -- 7. Sleep to next frame --------------------------------------

        let elapsed = frame_start.elapsed();
        if elapsed < FRAME_DURATION {
            std::thread::sleep(FRAME_DURATION - elapsed);
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn drain_commands(
    rx: &Receiver<TouchCommand>,
    contacts: &mut Vec<Contact>,
    already_shutting_down: bool,
) -> bool {
    let mut shutting_down = already_shutting_down;

    loop {
        match rx.try_recv() {
            Ok(TouchCommand::Down { pointer_id, pos }) => {
                if contacts.iter().any(|c| c.pointer_id == pointer_id) {
                    continue;
                }
                contacts.push(Contact {
                    pointer_id,
                    pos,
                    frames_emitted: 0,
                    pending_up: false,
                    transition: None,
                });
            }

            Ok(TouchCommand::Up { pointer_id, pos }) => {
                if let Some(c) = contacts.iter_mut().find(|c| c.pointer_id == pointer_id) {
                    c.pending_up = true;
                    c.pos = pos;
                    c.transition = None;
                }
            }

            Ok(TouchCommand::Move {
                pointer_id,
                from,
                to,
            }) => {
                if let Some(c) = contacts.iter_mut().find(|c| c.pointer_id == pointer_id) {
                    c.transition = Some(Transition {
                        start: Instant::now(),
                        duration: MOVE_DURATION,
                        from,
                        to,
                    });
                }
            }

            Ok(TouchCommand::Shutdown) => {
                shutting_down = true;
                for c in contacts.iter_mut() {
                    c.pending_up = true;
                    c.transition = None;
                }
            }

            Err(TryRecvError::Empty) => break,

            Err(TryRecvError::Disconnected) => {
                shutting_down = true;
                for c in contacts.iter_mut() {
                    c.pending_up = true;
                    c.transition = None;
                }
                break;
            }
        }
    }

    shutting_down
}

/// | `frames_emitted` | `pending_up` | Flags                                    |
/// |------------------|-------------|------------------------------------------|
/// | 0                | —           | `INRANGE | INCONTACT | DOWN`            |
/// | ≥ 1              | false       | `INRANGE | INCONTACT | UPDATE`          |
/// | 1                | true        | `INRANGE | INCONTACT | UPDATE` (pre-up) |
/// | ≥ 2              | true        | `INRANGE | UP`                          |
fn contact_flags(contact: &Contact) -> POINTER_FLAGS {
    if contact.frames_emitted == 0 {
        POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT | POINTER_FLAG_DOWN
    } else if contact.pending_up && contact.frames_emitted >= 2 {
        POINTER_FLAG_INRANGE | POINTER_FLAG_UP
    } else {
        POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT | POINTER_FLAG_UPDATE
    }
}

fn make_touch_info(pointer_id: u32, pos: POINT, flags: POINTER_FLAGS) -> POINTER_TOUCH_INFO {
    let mut info: POINTER_TOUCH_INFO = unsafe { std::mem::zeroed() };
    info.pointerInfo.pointerType = PT_TOUCH;
    info.pointerInfo.pointerId = pointer_id;
    info.pointerInfo.ptPixelLocation = pos;
    info.pointerInfo.pointerFlags = flags;
    info.rcContact = RECT {
        left: pos.x - 5,
        top: pos.y - 5,
        right: pos.x + 5,
        bottom: pos.y + 5,
    };
    info.touchFlags = TOUCH_FLAG_NONE;
    info.touchMask = TOUCH_MASK_CONTACTAREA | TOUCH_MASK_PRESSURE | TOUCH_MASK_ORIENTATION;
    info.pressure = 32000;
    info.orientation = 0;
    info
}

#[inline]
fn lerp_i32(a: i32, b: i32, t: f64) -> i32 {
    (a as f64 + (b - a) as f64 * t).round() as i32
}
