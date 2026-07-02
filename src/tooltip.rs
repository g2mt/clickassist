//! Record-mode instruction tooltip window.

use std::sync::atomic::{AtomicBool, Ordering};

use windows_sys::Win32::Foundation::{HWND, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::SystemServices::{SS_CENTER, SS_CENTERIMAGE};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::win;

static VISIBLE: AtomicBool = AtomicBool::new(false);

/// Track the tooltip HWND so we can destroy it later.
static mut TOOLTIP_HWND: HWND = std::ptr::null_mut();

/// Show a floating instruction tooltip near the main window.
pub fn show_instruction_tooltip(_anchor: HWND, text: &str) {
    if VISIBLE.swap(true, Ordering::SeqCst) {
        return; // already shown
    }

    let class = win::wide("STATIC");
    let wtext = win::wide(text);

    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class.as_ptr(),
            wtext.as_ptr(),
            WS_POPUP | WS_BORDER | SS_CENTER | SS_CENTERIMAGE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            420,
            60,
            std::ptr::null_mut(), // no parent
            std::ptr::null_mut(), // no menu
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };

    if hwnd != std::ptr::null_mut() {
        unsafe {
            // Set a reasonable font
            let hfont = CreateFontW(
                16,
                0,
                0,
                0,
                FW_NORMAL as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                OUT_DEFAULT_PRECIS as u32,
                CLIP_DEFAULT_PRECIS as u32,
                DEFAULT_QUALITY as u32,
                FF_DONTCARE as u32,
                win::wide("Segoe UI").as_ptr(),
            );
            if hfont != std::ptr::null_mut() {
                SendMessageW(hwnd, WM_SETFONT, hfont as WPARAM, 1);
            }

            // Position at the screen centre
            let mut rect: RECT = std::mem::zeroed();
            SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut rect as *mut _ as _, 0);
            let w = rect.right - rect.left;
            let h = rect.bottom - rect.top;
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                (w - 420) / 2,
                (h - 60) / 2,
                420,
                60,
                SWP_NOZORDER | SWP_SHOWWINDOW,
            );

            TOOLTIP_HWND = hwnd;
        }
    }
}

/// Hide and destroy the tooltip window.
pub fn hide_tooltip() {
    if !VISIBLE.swap(false, Ordering::SeqCst) {
        return;
    }
    unsafe {
        if TOOLTIP_HWND != std::ptr::null_mut() {
            DestroyWindow(TOOLTIP_HWND);
            TOOLTIP_HWND = std::ptr::null_mut();
        }
    }
}
