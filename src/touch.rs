//! Touch injection engine: press, release, multi-touch, interpolated gesture
//! moves.
//!
//! Uses `InitializeTouchInjection` + `InjectTouchInput`, which require
//! Windows 8+.
//!
//! Touch events are emitted from a dedicated background thread so that gesture
//! interpolation never blocks the calling thread.  The public API functions
//! send commands to the worker via an mpsc channel and return immediately.
//!
//! # Movement timing
//!
//! [`touch_move`] interpolates linearly from `from` to `to` over a **1 second**
//! interval.  Update frames are injected at ~60 Hz (every ~16 ms) on the
//! worker thread.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{POINT, RECT};
use windows_sys::Win32::UI::Input::Pointer::{
    InitializeTouchInjection, InjectTouchInput, POINTER_FLAGS, POINTER_FLAG_DOWN,
    POINTER_FLAG_INCONTACT, POINTER_FLAG_INRANGE, POINTER_FLAG_UP, POINTER_FLAG_UPDATE,
    POINTER_TOUCH_INFO, TOUCH_FEEDBACK_DEFAULT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    PT_TOUCH, TOUCH_FLAG_NONE, TOUCH_MASK_CONTACTAREA, TOUCH_MASK_ORIENTATION, TOUCH_MASK_PRESSURE,
};

// ---------------------------------------------------------------------------
// Commands sent to the background thread
// ---------------------------------------------------------------------------

enum TouchCommand {
    Down {
        pointer_id: u32,
        pos: POINT,
    },
    Up {
        pointer_id: u32,
        pos: POINT,
    },
    Move {
        pointer_id: u32,
        from: POINT,
        to: POINT,
    },
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
                pointer_id,
                from,
                to,
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
// Additional metadata tracked alongside each POINTER_TOUCH_INFO
// ---------------------------------------------------------------------------

/// Per-pointer metadata stored in the linked `Vec`.
///
/// The two vectors `metas` and `infos` are kept in sync: element at index *i*
/// in `metas` describes the element at index *i* in `infos`.
struct TouchMeta {
    pointer_id: u32,

    /// `Some(…)` while a gesture transition is in progress for this pointer.
    transition: Option<Transition>,
}

/// Describes an in-flight linear interpolation between two touch positions.
struct Transition {
    /// When the transition started (used to compute elapsed time).
    start: Instant,

    /// Total duration of the transition (1 s per the spec).
    duration: Duration,

    /// Starting position.
    from: POINT,

    /// Target position.
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
// Singleton engine state
// ---------------------------------------------------------------------------

/// Holds the sender to the background thread so that it can be shared across
/// the public API without the caller threading a handle through every
/// call-site.
struct EngineState {
    sender: Sender<TouchCommand>,
    thread: Option<std::thread::JoinHandle<()>>,
}

static ENGINE: Mutex<Option<EngineState>> = Mutex::new(None);

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Initialise the touch injection subsystem and spawn the background worker
/// thread.
///
/// `max_contacts` is the maximum number of simultaneous touches expected.
///
/// Call **once** at startup.
pub fn init_touch_injection(max_contacts: u32) {
    // Create the command channel and spawn the worker.
    let (tx, rx) = mpsc::channel::<TouchCommand>();

    let handle = std::thread::Builder::new()
        .name("touch-inject".into())
        .spawn(move || {
            // InitializeTouchInjection must be called from the same thread
            // that will call InjectTouchInput.
            let ok = unsafe { InitializeTouchInjection(max_contacts, TOUCH_FEEDBACK_DEFAULT) };
            if ok == 0 {
                eprintln!("[touch] InitializeTouchInjection failed");
            }
            touch_thread(rx);
        })
        .expect("failed to spawn touch injection thread");

    let mut guard = ENGINE.lock().expect("ENGINE lock poisoned");
    *guard = Some(EngineState {
        sender: tx,
        thread: Some(handle),
    });
}

/// Tear down the touch injection thread.
///
/// Sends a shutdown command, joins the background thread, and clears the
/// singleton state.
pub fn deinit_touch_injection() {
    let mut guard = ENGINE.lock().expect("ENGINE lock poisoned");
    if let Some(mut state) = guard.take() {
        let _ = state.sender.send(TouchCommand::Shutdown);
        if let Some(handle) = state.thread.take() {
            let _ = handle.join();
        }
    }
}

/// Inject a touch-down at the given position.
///
/// Sends the command to the background thread; returns immediately.
pub fn touch_down(pointer_id: u32, pos: POINT) {
    let guard = ENGINE.lock().expect("ENGINE lock poisoned");
    if let Some(ref state) = *guard {
        let _ = state.sender.send(TouchCommand::Down { pointer_id, pos });
    }
}

/// Inject a touch-up at the given position.
///
/// Sends the command to the background thread; returns immediately.
pub fn touch_up(pointer_id: u32, pos: POINT) {
    let guard = ENGINE.lock().expect("ENGINE lock poisoned");
    if let Some(ref state) = *guard {
        let _ = state.sender.send(TouchCommand::Up { pointer_id, pos });
    }
}

/// Begin a touch-move gesture from `from` to `to`.
///
/// The movement is interpolated by the background thread over a **1 second**
/// interval.  This function returns immediately; interpolation continues on
/// the worker thread.
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
// Background worker
// ---------------------------------------------------------------------------

/// Entry-point for the background touch-injection thread.
fn touch_thread(rx: Receiver<TouchCommand>) {
    // Two linked vectors: element i in `metas` describes element i in `infos`.
    let mut metas: Vec<TouchMeta> = Vec::new();
    let mut infos: Vec<POINTER_TOUCH_INFO> = Vec::new();

    loop {
        // wait at least 1 ms to prevent timeout
        std::thread::sleep(Duration::from_millis(1));

        let now = Instant::now();

        // ---- Apply any active transitions for this frame ----
        let mut dirty = false;

        for (meta, info) in metas.iter_mut().zip(infos.iter_mut()) {
            if let Some(ref trans) = meta.transition {
                dirty = true;
                let elapsed = now.duration_since(trans.start);

                if elapsed >= trans.duration {
                    // Transition finished — snap to final position.
                    info.pointerInfo.ptPixelLocation = trans.to;
                    update_contact_rect(info);
                    meta.transition = None;
                } else {
                    // Linear interpolation.
                    let t = elapsed.as_secs_f64() / trans.duration.as_secs_f64();
                    info.pointerInfo.ptPixelLocation.x = lerp_i32(trans.from.x, trans.to.x, t);
                    info.pointerInfo.ptPixelLocation.y = lerp_i32(trans.from.y, trans.to.y, t);
                    update_contact_rect(info);
                }
            }
        }

        if dirty {
            // Inject *all* active pointers so the OS sees the full multi-touch
            // picture.  Non-transitioning pointers keep their current flag set
            // (already INRANGE | INCONTACT from their last frame).
            unsafe {
                InjectTouchInput(infos.len() as u32, infos.as_ptr());
            }
        }

        // wait at least 1 ms to prevent timeout
        std::thread::sleep(Duration::from_millis(2));

        // ---- Wait for the next command (16 ms timeout so animation
        //      continues at ~60 Hz) ----
        let command = rx.recv_timeout(Duration::from_millis(16));
        if command.is_ok() {
            eprintln!("{:?}", command);
        }
        match command {
            Ok(TouchCommand::Down { pointer_id, pos }) => {
                // Ignore duplicate downs for an already-active pointer.
                if metas.iter().any(|m| m.pointer_id == pointer_id) {
                    continue;
                }

                let info = make_touch_info(
                    pointer_id,
                    pos,
                    POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT | POINTER_FLAG_DOWN,
                );

                metas.push(TouchMeta {
                    pointer_id,
                    transition: None,
                });
                infos.push(info);

                // Inject the new DOWN frame immediately.
                unsafe {
                    InjectTouchInput(infos.len() as u32, infos.as_ptr());
                }
            }

            Ok(TouchCommand::Up { pointer_id, pos }) => {
                let idx = match metas.iter().position(|m| m.pointer_id == pointer_id) {
                    Some(i) => i,
                    None => continue,
                };

                // Prepare the UP frame on a *copy* and inject *only* this one
                // pointer so that the OS processes the release independently
                // of other active contacts.
                let mut up_info = infos[idx];
                up_info.pointerInfo.ptPixelLocation = pos;
                up_info.pointerInfo.pointerFlags = POINTER_FLAG_INRANGE | POINTER_FLAG_UP;
                update_contact_rect(&mut up_info);

                unsafe {
                    InjectTouchInput(1, &up_info);
                }

                // Remove from both linked vectors using swap_remove so the
                // remaining elements stay contiguous and indices stay in sync.
                let _ = metas.swap_remove(idx);
                let _ = infos.swap_remove(idx);
            }

            Ok(TouchCommand::Move {
                pointer_id,
                from,
                to,
            }) => {
                let idx = match metas.iter().position(|m| m.pointer_id == pointer_id) {
                    Some(i) => i,
                    None => continue,
                };

                // Set pointer flags to UPDATE so the OS knows this is a move,
                // not a new contact.
                infos[idx].pointerInfo.pointerFlags =
                    POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT | POINTER_FLAG_UPDATE;

                eprintln!("existing: {:?}", metas[idx].transition);
                // Record the transition.  The worker loop will interpolate
                // each frame until `duration` is reached.
                metas[idx].transition = Some(Transition {
                    start: Instant::now(),
                    duration: Duration::from_secs(1),
                    from,
                    to,
                });
            }

            Ok(TouchCommand::Shutdown) => break,

            Err(RecvTimeoutError::Timeout) => {
                // No command — the loop will re-check transitions next
                // iteration.
            }

            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a `POINTER_TOUCH_INFO` for a single touch contact.
fn make_touch_info(pointer_id: u32, pos: POINT, flags: POINTER_FLAGS) -> POINTER_TOUCH_INFO {
    let mut info: POINTER_TOUCH_INFO = unsafe { std::mem::zeroed() };

    info.pointerInfo.pointerType = PT_TOUCH;
    info.pointerInfo.pointerId = pointer_id;
    info.pointerInfo.ptPixelLocation = pos;
    info.pointerInfo.pointerFlags = flags;

    // 10×10 px contact area centred on the touch point.
    info.rcContact = RECT {
        left: pos.x - 5,
        top: pos.y - 5,
        right: pos.x + 5,
        bottom: pos.y + 5,
    };

    info.touchFlags = TOUCH_FLAG_NONE;
    info.touchMask = TOUCH_MASK_CONTACTAREA | TOUCH_MASK_PRESSURE | TOUCH_MASK_ORIENTATION;
    info.pressure = 32000; // mid-range (0–65535)
    info.orientation = 0;

    info
}

/// Recompute `rcContact` from the current pixel position in `info`.
fn update_contact_rect(info: &mut POINTER_TOUCH_INFO) {
    let x = info.pointerInfo.ptPixelLocation.x;
    let y = info.pointerInfo.ptPixelLocation.y;
    info.rcContact = RECT {
        left: x - 5,
        top: y - 5,
        right: x + 5,
        bottom: y + 5,
    };
}

/// Linear interpolation between two `i32` values.
#[inline]
fn lerp_i32(a: i32, b: i32, t: f64) -> i32 {
    (a as f64 + (b - a) as f64 * t).round() as i32
}
