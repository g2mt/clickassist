//! Touch injection engine: press, release, multi-touch, interpolated gesture moves.
//!
//! Uses `InitializeTouchInjection` + `InjectSyntheticPointerInput`, which
//! require Windows 8+.

use std::thread;
use std::time::Duration;

use windows_sys::Win32::Foundation::POINT;
use windows_sys::Win32::UI::Input::Pointer::{
    InitializeTouchInjection, InjectSyntheticPointerInput, POINTER_FLAG_DOWN,
    POINTER_FLAG_INCONTACT, POINTER_FLAG_INRANGE, POINTER_FLAG_UP, POINTER_FLAG_UPDATE,
    POINTER_FLAGS, POINTER_TOUCH_INFO, POINTER_TYPE_INFO, TOUCH_FEEDBACK_DEFAULT,
};

/// Maximum simultaneous touches we support.
const MAX_TOUCHES: u32 = 10;

/// Initialise the touch injection system. Call once at startup.
pub fn init_touch_injection() {
    let ok = unsafe { InitializeTouchInjection(MAX_TOUCHES, TOUCH_FEEDBACK_DEFAULT) };
    if ok == 0 {
        eprintln!(
            "Warning: InitializeTouchInjection failed (is this Windows 8+ with a touch digitizer?)"
        );
    }
}

/// Build a `POINTER_TOUCH_INFO` for a given pointer ID, position, and flags.
pub fn make_touch_info(pointer_id: u32, pos: POINT, flags: POINTER_FLAGS) -> POINTER_TOUCH_INFO {
    POINTER_TOUCH_INFO {
        pointerInfo: windows_sys::Win32::UI::Input::Pointer::POINTER_INFO {
            pointerType: 2, // PT_TOUCH = 2
            pointerId: pointer_id,
            frameId: 0,
            pointerFlags: flags,
            sourceDevice: std::ptr::null_mut(),
            hwndTarget: std::ptr::null_mut(),
            ptPixelLocation: pos,
            ptHimetricLocation: POINT { x: 0, y: 0 },
            ptPixelLocationRaw: pos,
            ptHimetricLocationRaw: POINT { x: 0, y: 0 },
            dwTime: 0,
            historyCount: 0,
            InputData: 0,
            dwKeyStates: 0,
            PerformanceCount: 0,
            ButtonChangeType: 0, // POINTER_CHANGE_NONE
        },
        touchFlags: 0,
        touchMask: 0,
        rcContact: windows_sys::Win32::Foundation::RECT {
            left: pos.x - 2,
            top: pos.y - 2,
            right: pos.x + 2,
            bottom: pos.y + 2,
        },
        rcContactRaw: windows_sys::Win32::Foundation::RECT {
            left: pos.x - 2,
            top: pos.y - 2,
            right: pos.x + 2,
            bottom: pos.y + 2,
        },
        orientation: 0,
        pressure: 32000, // ≈ half pressure
    }
}

/// Inject a touch-down at the given position.
pub fn touch_down(pointer_id: u32, pos: POINT) {
    let info = make_touch_info(
        pointer_id,
        pos,
        POINTER_FLAG_DOWN | POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT,
    );
    inject(&[info]);
}

/// Inject a touch-up at the given position.
pub fn touch_up(pointer_id: u32, pos: POINT) {
    let info = make_touch_info(pointer_id, pos, POINTER_FLAG_UP);
    inject(&[info]);
}

/// Inject a touch-move gesture from `from` to `to` with the given number of
/// interpolation steps. A small sleep between steps makes the gesture look
/// natural to the target application's gesture recogniser.
pub fn touch_move(pointer_id: u32, from: POINT, to: POINT, steps: u32) {
    if steps == 0 {
        return;
    }

    let dx = (to.x - from.x) as f64;
    let dy = (to.y - from.y) as f64;

    for i in 1..=steps {
        let t = i as f64 / steps as f64;
        let interp = POINT {
            x: from.x + (dx * t).round() as i32,
            y: from.y + (dy * t).round() as i32,
        };

        let info = make_touch_info(
            pointer_id,
            interp,
            POINTER_FLAG_UPDATE | POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT,
        );
        inject(&[info]);

        // Small delay per step so gesture recognisers see distinct events.
        thread::sleep(Duration::from_millis(8));
    }
}

/// Low-level wrapper around `InjectSyntheticPointerInput`.
/// In windows-sys 0.59+, `InjectSyntheticPointerInput` takes:
///   (device: *mut c_void, info: *const POINTER_TYPE_INFO, count: u32)
fn inject(touches: &[POINTER_TOUCH_INFO]) {
    // Build POINTER_TYPE_INFO array — each entry wraps a POINTER_TOUCH_INFO.
    // POINTER_TYPE_INFO has an anonymous union; we set type = PT_TOUCH (2)
    // and provide the touchInfo pointer.
    let mut type_infos: Vec<POINTER_TYPE_INFO> = Vec::with_capacity(touches.len());
    for t in touches {
        type_infos.push(POINTER_TYPE_INFO {
            r#type: 2, // PT_TOUCH
            Anonymous: windows_sys::Win32::UI::Input::Pointer::POINTER_TYPE_INFO_0 {
                touchInfo: t as *const POINTER_TOUCH_INFO as _,
            },
        });
    }

    let count = type_infos.len() as u32;
    let ok = unsafe {
        InjectSyntheticPointerInput(
            std::ptr::null_mut(), // device
            type_infos.as_ptr(),
            count,
        )
    };
    if ok == 0 {
        // Silently ignore — touch injection may not be available.
    }
}
