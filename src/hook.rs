//! Low-level keyboard hook (`WH_KEYBOARD_LL`).
//!
//! The hook runs on the same UI thread that installed it, so it can access
//! `AppState` directly via the thread-local.

use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::app::STATE;

/// The hook handle, set by `install_keyboard_hook`.
static mut HOOK_HANDLE: HHOOK = std::ptr::null_mut();

/// Install the low-level keyboard hook.
///
/// Must be called from a thread that pumps messages (the UI thread).
pub fn install_keyboard_hook() -> HHOOK {
    let hmod = unsafe { GetModuleHandleW(std::ptr::null()) };

    let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), hmod, 0) };

    if hook == std::ptr::null_mut() {
        panic!("SetWindowsHookExW(WH_KEYBOARD_LL) failed");
    }

    unsafe {
        HOOK_HANDLE = hook;
    }

    hook
}

/// Uninstall the hook.
pub fn uninstall_keyboard_hook(hook: HHOOK) {
    if hook != std::ptr::null_mut() {
        unsafe {
            UnhookWindowsHookEx(hook);
            HOOK_HANDLE = std::ptr::null_mut();
        }
    }
}

/// The keyboard hook callback.
pub unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code < 0 {
        return unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) };
    }

    let kbd: &windows_sys::Win32::UI::WindowsAndMessaging::KBDLLHOOKSTRUCT =
        unsafe { &*(lparam as *const _) };
    let vk_code: u32 = kbd.vkCode;

    let (down, swallow) = match wparam as u32 {
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            // Auto-repeat filter: bit 30 is set for repeats
            let is_repeat = (kbd.flags & (1u32 << 30)) != 0;
            if is_repeat {
                let active = STATE.with(|s| s.borrow().active.contains_key(&vk_code));
                let bound = STATE.with(|s| s.borrow().bindings.contains_key(&vk_code));
                if active || bound {
                    return 1;
                }
                return unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) };
            }

            let result = STATE.with(|s| s.borrow_mut().on_key_event(vk_code, true));
            (true, result)
        }
        WM_KEYUP | WM_SYSKEYUP => {
            let result = STATE.with(|s| s.borrow_mut().on_key_event(vk_code, false));
            (false, result)
        }
        _ => (false, false),
    };

    // Esc cancels recording
    if vk_code == 0x1B && down {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            if s.mode == crate::app::Mode::Recording {
                s.mode = crate::app::Mode::Idle;
                crate::tooltip::hide_tooltip();
            }
        });
        return 1;
    }

    if swallow {
        1
    } else {
        unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
    }
}
