//! Touch injection engine: press, release, multi-touch, interpolated gesture moves.
//!
//! Uses `InitializeTouchInjection` + `InjectSyntheticPointerInput`, which
//! require Windows 8+.
//!
//! TODO: Re-implement when `windows-sys` touch injection APIs stabilise
//! (POINTER_TYPE_INFO et al. moved across modules in 0.61).

use std::time::Duration;

use windows_sys::Win32::Foundation::POINT;
use windows_sys::Win32::UI::Input::Pointer::POINTER_FLAGS;

/// Maximum simultaneous touches we support.
const MAX_TOUCHES: u32 = 10;

/// Initialise the touch injection system. Call once at startup.
pub fn init_touch_injection() {
    eprintln!("touch injection initialisation (InitializeTouchInjection)")
}

/// Build a `POINTER_TOUCH_INFO` for a given pointer ID, position, and flags.
pub fn make_touch_info(_pointer_id: u32, _pos: POINT, _flags: POINTER_FLAGS) {
    eprintln!("touch injection: make_touch_info")
}

/// Inject a touch-down at the given position.
pub fn touch_down(_pointer_id: u32, _pos: POINT) {
    eprintln!("touch injection: touch_down")
}

/// Inject a touch-up at the given position.
pub fn touch_up(_pointer_id: u32, _pos: POINT) {
    eprintln!("touch injection: touch_up")
}

/// Inject a touch-move gesture from `from` to `to` with the given number of
/// interpolation steps. A small sleep between steps makes the gesture look
/// natural to the target application's gesture recogniser.
pub fn touch_move(_pointer_id: u32, _from: POINT, _to: POINT, _steps: u32) {
    eprintln!("touch injection: touch_move")
}
