//! Touch injection engine: press, release, multi-touch, interpolated gesture
//! moves.
//!
//! Spawns a **child process** that calls `InitializeTouchInjection` +
//! `InjectTouchInput` (Windows 8+) so that the per-process internal touch
//! state is isolated from the main application.
//!
//! Communication is JSON over the child's **stdin**/**stdout**:
//!
//! ```text
//! Parent → child           Child → parent
//! ─────────────────────    ──────────────────────────
//! {"cmd":"down","x":…}     {"type":"ready"}
//! {"cmd":"up",…}           {"type":"allocated","pointer_id":…}
//! {"cmd":"move",…}
//! {"cmd":"shutdown"}
//! ```
//!
//! If `InjectTouchInput` fails the worker exits.  The parent detects a broken
//! pipe and transparently restarts the worker process.

mod worker;

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Mutex;

use windows_sys::Win32::Foundation::POINT;

// ---------------------------------------------------------------------------
// Pointer ID
// ---------------------------------------------------------------------------

/// Opaque handle for an active touch contact.
///
/// Not `Copy` or `Clone`: each contact gets a unique token.  Passing
/// `PointerId` by value to [`touch_up`] consumes it, preventing double-release.
#[derive(Debug)]
pub struct PointerId(u32);

// ---------------------------------------------------------------------------
// Singleton
// ---------------------------------------------------------------------------

struct EngineState {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
    max_contacts: u32,
}

static ENGINE: Mutex<Option<EngineState>> = Mutex::new(None);

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Spawn the touch-injection worker process and wait for its `ready` signal.
pub fn init_touch_injection(max_contacts: u32) {
    let exe =
        std::env::current_exe().expect("cannot determine path to own executable for touch worker");

    let mut child = Command::new(&exe)
        .args([
            "--touch-worker",
            "--max-contacts",
            &max_contacts.to_string(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("failed to spawn touch-worker process");

    let stdin = child.stdin.take().expect("child has no stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("child has no stdout"));

    // Wait for the worker to signal that `InitializeTouchInjection` completed.
    let mut line = String::new();
    stdout
        .read_line(&mut line)
        .expect("failed to read ready signal from touch-worker");
    let v: serde_json::Value =
        serde_json::from_str(&line).expect("invalid JSON from touch-worker");
    if v["type"] != "ready" {
        eprintln!("[touch] unexpected worker startup message: {line}");
    }

    let mut guard = ENGINE.lock().expect("ENGINE lock poisoned");
    // Drop any previous state (shouldn't happen, but be safe).
    *guard = Some(EngineState {
        child,
        stdin,
        stdout,
        max_contacts,
    });
}

/// Send `shutdown` to the worker and wait for the child process to exit.
pub fn deinit_touch_injection() {
    let mut guard = ENGINE.lock().expect("ENGINE lock poisoned");
    if let Some(mut state) = guard.take() {
        // Fire-and-forget: even if the child has already exited, ignore.
        let cmd = serde_json::json!({"cmd": "shutdown"});
        let _ = writeln!(state.stdin, "{cmd}");
        let _ = state.stdin.flush();
        let _ = state.child.wait();
    }
}

/// Begin a new touch at `pos`.  Returns a unique [`PointerId`], or `None` if
/// the worker has no free pointer IDs or the connection is broken.
///
/// Automatically restarts the worker process if the previous one died.
pub fn touch_down(pos: POINT) -> Option<PointerId> {
    let mut guard = ENGINE.lock().expect("ENGINE lock poisoned");
    let state = guard.as_mut()?;

    match try_touch_down(state, pos) {
        Some(id) => Some(id),
        None => {
            state.restart();
            try_touch_down(state, pos)
        }
    }
}

/// End a touch.  The [`PointerId`] is consumed; attempting to use it again is
/// a compile error.
///
/// If the worker has died this call restarts it (the touch is lost, but the
/// token is still consumed).
pub fn touch_up(pid: PointerId, pos: POINT) {
    let mut guard = ENGINE.lock().expect("ENGINE lock poisoned");
    if let Some(ref mut state) = *guard {
        let cmd = serde_json::json!({"cmd": "up", "pointer_id": pid.0, "x": pos.x, "y": pos.y});
        if writeln!(state.stdin, "{cmd}").is_err() || state.stdin.flush().is_err() {
            state.restart();
        }
    }
}

/// Begin an interpolated move gesture from `from` to `to`.
///
/// If the worker has died this call restarts it (the move is lost).
pub fn touch_move(pid: &PointerId, from: POINT, to: POINT) {
    let mut guard = ENGINE.lock().expect("ENGINE lock poisoned");
    if let Some(ref mut state) = *guard {
        let cmd = serde_json::json!({
            "cmd": "move",
            "pointer_id": pid.0,
            "from_x": from.x,
            "from_y": from.y,
            "to_x": to.x,
            "to_y": to.y,
        });
        if writeln!(state.stdin, "{cmd}").is_err() || state.stdin.flush().is_err() {
            state.restart();
        }
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

impl EngineState {
    /// Kill the current worker and spawn a fresh one.  Blocks until the
    /// new worker sends its `ready` signal.
    fn restart(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();

        let exe = std::env::current_exe()
            .expect("cannot get executable path for touch-worker restart");

        let mut child = Command::new(&exe)
            .args([
                "--touch-worker",
                "--max-contacts",
                &self.max_contacts.to_string(),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("failed to restart touch-worker");

        self.stdin = child.stdin.take().expect("restarted child has no stdin");
        self.stdout =
            BufReader::new(child.stdout.take().expect("restarted child has no stdout"));
        self.child = child;

        // Wait for ready.
        let mut line = String::new();
        self.stdout
            .read_line(&mut line)
            .expect("failed to read ready signal from restarted touch-worker");
        let v: serde_json::Value =
            serde_json::from_str(&line).expect("invalid JSON from restarted touch-worker");
        if v["type"] != "ready" {
            eprintln!("[touch] unexpected worker restart message: {line}");
        }
    }
}

fn try_touch_down(state: &mut EngineState, pos: POINT) -> Option<PointerId> {
    let cmd = serde_json::json!({"cmd": "down", "x": pos.x, "y": pos.y});
    if writeln!(state.stdin, "{cmd}").is_err() {
        return None;
    }
    if state.stdin.flush().is_err() {
        return None;
    }

    let mut line = String::new();
    state.stdout.read_line(&mut line).ok()?;
    let v: serde_json::Value = serde_json::from_str(&line).ok()?;
    if v["type"] == "allocated" {
        Some(PointerId(v["pointer_id"].as_u64()? as u32))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Worker entry point
// ---------------------------------------------------------------------------

/// Called from `main()` when `--touch-worker` is on the command line.
/// Never returns.
pub fn run_worker() -> ! {
    let max_contacts: u32 = std::env::args()
        .skip_while(|a| a != "--max-contacts")
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);
    worker::run(max_contacts);
}
