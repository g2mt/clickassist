//! Thin Win32 helpers: wide strings, DPI awareness, error checking.
//!
//! Since `windows-sys` exposes raw FFI without `windows::core::Result` wrappers,
//! this module centralises `GetLastError`-based error checking.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};

/// Convert a Rust `&str` to a null-terminated wide string (`Vec<u16>`).
pub fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

/// Set per-monitor DPI awareness (v2). Falls back silently on failure.
pub fn set_dpi_awareness() {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

