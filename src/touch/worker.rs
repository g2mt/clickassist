//! Touch-injection worker process.
//!
//! This module runs in a **child process** spawned by [`super::init_touch_injection`]
//! so that the per-process internal touch state is isolated from the main
//! application.
//!
//! A reader thread parses JSON commands from **stdin** and pushes them onto a
//! channel.  The main thread runs a fixed-timestep game loop at ~60 Hz that
//! drains the channel, drives contact state machines, and calls
//! `InjectTouchInput`.  Responses (pointer-ID allocations) are written to
//! **stdout** as JSON.

use std::io::{BufRead, Write};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{GetLastError, POINT, RECT};
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::Input::Pointer::{
    InitializeTouchInjection, InjectTouchInput, POINTER_FLAG_DOWN, POINTER_FLAG_INCONTACT,
    POINTER_FLAG_INRANGE, POINTER_FLAG_UP, POINTER_FLAG_UPDATE, POINTER_FLAGS, POINTER_TOUCH_INFO,
    TOUCH_FEEDBACK_DEFAULT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    PT_TOUCH, TOUCH_FLAG_NONE, TOUCH_MASK_CONTACTAREA, TOUCH_MASK_ORIENTATION, TOUCH_MASK_PRESSURE,
};

use crate::touch::cmd::{Cmd, WorkerResponse};

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

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const FRAME_DURATION: Duration = Duration::from_nanos(16_666_667);
const MOVE_DURATION: Duration = Duration::from_secs(1);

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub fn run(max_contacts: u32) -> ! {
    let ok = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    if ok == 0 {
        eprintln!("Failed to set DPI awareness. Pointer location may be inaccurate.");
    }

    let ok = unsafe { InitializeTouchInjection(max_contacts, TOUCH_FEEDBACK_DEFAULT) };
    if ok == 0 {
        // Best-effort; we still run the loop so the parent doesn't hang.
        eprintln!("[touch-worker] InitializeTouchInjection failed");
    }

    // Free stack: push IDs in reverse so we pop small numbers first.
    let mut free_stack: Vec<u32> = (0..max_contacts).rev().collect();

    // Channel from reader thread → game loop.
    let (tx, rx) = mpsc::channel::<Cmd>();

    // Spawn stdin reader thread.
    std::thread::Builder::new()
        .name("touch-worker-stdin".into())
        .spawn(move || read_stdin(tx))
        .expect("failed to spawn stdin reader");

    // Signal parent that we are ready.
    println!("{}", serde_json::to_string(&WorkerResponse::Ready).unwrap());
    let _ = std::io::stdout().flush();

    // Run game loop on main thread (never returns).
    run_game_loop(rx, &mut free_stack);
}

// ---------------------------------------------------------------------------
// Stdin reader
// ---------------------------------------------------------------------------

fn read_stdin(tx: Sender<Cmd>) {
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[touch-worker] stdin error: {e}");
                break;
            }
        };

        let cmd: Cmd = match serde_json::from_str(&line) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[touch-worker] JSON parse error: {e}");
                continue;
            }
        };

        let is_shutdown = matches!(cmd, Cmd::Shutdown);
        if tx.send(cmd).is_err() {
            break;
        }
        if is_shutdown {
            break;
        }
    }
}

// ---------------------------------------------------------------------------
// Game loop
// ---------------------------------------------------------------------------

fn run_game_loop(rx: Receiver<Cmd>, free_stack: &mut Vec<u32>) -> ! {
    let mut contacts: Vec<Contact> = Vec::new();
    let mut infos: Vec<POINTER_TOUCH_INFO> = Vec::new();
    let mut shutting_down = false;

    loop {
        let frame_start = Instant::now();

        // -- 1. Drain commands ------------------------------------------

        shutting_down |= drain_commands(&rx, &mut contacts, free_stack, shutting_down);

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
                    eprintln!(
                        "[touch-worker] InjectTouchInput failed: error={}",
                        GetLastError()
                    );
                    std::process::exit(1);
                }
            }
        }

        // -- 5. Advance counters; remove finished contacts ---------------

        for contact in &mut contacts {
            contact.frames_emitted += 1;
        }

        // Remove in reverse to preserve indices; return IDs to free stack.
        for &idx in removal_indices.iter().rev() {
            free_stack.push(contacts[idx].pointer_id);
            contacts.remove(idx);
        }

        // -- 6. Exit check -----------------------------------------------

        if shutting_down && contacts.is_empty() {
            std::process::exit(0);
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
    rx: &Receiver<Cmd>,
    contacts: &mut Vec<Contact>,
    free_stack: &mut Vec<u32>,
    already_shutting_down: bool,
) -> bool {
    let mut shutting_down = already_shutting_down;

    loop {
        match rx.try_recv() {
            Ok(Cmd::Down { x, y }) => {
                let pos = POINT { x, y };
                let id = match free_stack.pop() {
                    Some(id) => id,
                    None => {
                        eprintln!("[touch-worker] no free pointer IDs");
                        continue;
                    }
                };
                // Notify parent of allocated ID.
                println!(
                    "{}",
                    serde_json::to_string(&WorkerResponse::Allocated { pointer_id: id }).unwrap()
                );
                let _ = std::io::stdout().flush();

                contacts.push(Contact {
                    pointer_id: id,
                    pos,
                    frames_emitted: 0,
                    pending_up: false,
                    transition: None,
                });
            }

            Ok(Cmd::Up { pointer_id, x, y }) => {
                let pos = POINT { x, y };
                if let Some(c) = contacts.iter_mut().find(|c| c.pointer_id == pointer_id) {
                    c.pending_up = true;
                    c.pos = pos;
                    c.transition = None;
                }
            }

            Ok(Cmd::Move {
                pointer_id,
                from_x,
                from_y,
                to_x,
                to_y,
            }) => {
                let from = POINT {
                    x: from_x,
                    y: from_y,
                };
                let to = POINT { x: to_x, y: to_y };
                if let Some(c) = contacts.iter_mut().find(|c| c.pointer_id == pointer_id) {
                    if let Some(t) = c.transition.as_ref()
                        && t.to.x == to.x
                        && t.to.y == to.y
                    {
                        continue;
                    }
                    c.transition = Some(Transition {
                        start: Instant::now(),
                        duration: MOVE_DURATION,
                        from,
                        to,
                    });
                }
            }

            Ok(Cmd::Shutdown) => {
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
/// | 0                | —           | `INRANGE \| INCONTACT \| DOWN`            |
/// | ≥ 1              | false       | `INRANGE \| INCONTACT \| UPDATE`          |
/// | 1                | true        | `INRANGE \| INCONTACT \| UPDATE` (pre-up) |
/// | ≥ 2              | true        | `INRANGE \| UP`                          |
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
