//! Transparent, click-through, top-most overlay window that draws each bound
//! key as a dot with a centred label above it.

use std::collections::HashMap;

use windows_sys::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::{bindings, win};

const OVERLAY_CLASS: &str = "ClickAssistOverlay";
const DOT_RADIUS: i32 = 6;
const LABEL_GAP: i32 = 4;

/// Create the overlay window (initially hidden).
pub fn create_overlay_window(hinstance: HINSTANCE) -> HWND {
    unsafe {
        let class_name = win::wide(OVERLAY_CLASS);
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(overlay_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: std::ptr::null_mut(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        RegisterClassExW(&wc);
    }

    let x = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let y = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let w = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
    let h = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };

    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            win::wide(OVERLAY_CLASS).as_ptr(),
            std::ptr::null(),
            WS_POPUP,
            x,
            y,
            w,
            h,
            std::ptr::null_mut(), // no parent
            std::ptr::null_mut(), // no menu
            hinstance,
            std::ptr::null_mut(),
        )
    };

    if hwnd != std::ptr::null_mut() {
        unsafe {
            SetLayeredWindowAttributes(hwnd, (COLOR_WINDOW + 1) as u32, 0, LWA_COLORKEY);
        }
    }

    hwnd
}

/// Show the overlay and render all bindings.
pub fn show_overlay(overlay: HWND, bindings: &HashMap<u32, POINT>) {
    unsafe {
        ShowWindow(overlay, SW_SHOWNOACTIVATE);
        SetWindowPos(
            overlay,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
        InvalidateRect(overlay, std::ptr::null(), 1);
    }

    paint_now(overlay, bindings);
}

/// Hide the overlay.
pub fn hide_overlay(overlay: HWND) {
    unsafe {
        ShowWindow(overlay, SW_HIDE);
    }
}

/// Window procedure for the overlay.
pub unsafe extern "system" fn overlay_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            unsafe { ValidateRect(hwnd, std::ptr::null()) };
            0
        }
        WM_ERASEBKGND => {
            unsafe {
                let hdc = wparam as HDC;
                let brush = CreateSolidBrush((COLOR_WINDOW + 1) as u32);
                let mut rect = RECT {
                    left: 0,
                    top: 0,
                    right: 0,
                    bottom: 0,
                };
                GetClientRect(hwnd, &mut rect);
                FillRect(hdc, &rect, brush);
                DeleteObject(brush as _);
            }
            1
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// Render all bindings onto the overlay immediately.
fn paint_now(hwnd: HWND, bindings: &HashMap<u32, POINT>) {
    let hdc = unsafe { GetDC(hwnd) };
    if hdc == std::ptr::null_mut() {
        return;
    }

    paint_bindings(hdc, bindings);

    unsafe {
        ReleaseDC(hwnd, hdc);
    }
}

/// Draw dots and labels for each binding.
fn paint_bindings(hdc: HDC, bindings: &HashMap<u32, POINT>) {
    for (&vk, &pt) in bindings {
        draw_dot(hdc, pt.x, pt.y);
        let label = bindings::vk_to_label(vk);
        draw_centered_label(hdc, pt.x, pt.y - DOT_RADIUS - LABEL_GAP, &label);
    }
}

/// Draw a small filled circle at (cx, cy).
fn draw_dot(hdc: HDC, cx: i32, cy: i32) {
    unsafe {
        let brush = CreateSolidBrush(0x0000FF);
        let old_brush = SelectObject(hdc, brush as _);
        let pen = CreatePen(PS_SOLID, 1, 0x0000FF);
        let old_pen = SelectObject(hdc, pen as _);

        Ellipse(
            hdc,
            cx - DOT_RADIUS,
            cy - DOT_RADIUS,
            cx + DOT_RADIUS,
            cy + DOT_RADIUS,
        );

        SelectObject(hdc, old_pen);
        SelectObject(hdc, old_brush);
        DeleteObject(brush as _);
        DeleteObject(pen as _);
    }
}

/// Draw a text label centred horizontally with its top at (cx, top_y).
fn draw_centered_label(hdc: HDC, cx: i32, top_y: i32, label: &str) {
    let wide_label = win::wide(label);
    let len = wide_label.len() - 1; // exclude null terminator

    unsafe {
        let mut size = std::mem::zeroed();
        GetTextExtentPoint32W(hdc, wide_label.as_ptr(), len as i32, &mut size);

        let text_x = cx - size.cx / 2;

        SetBkMode(hdc, TRANSPARENT as i32);
        SetTextColor(hdc, 0x0000FF);

        TextOutW(hdc, text_x, top_y, wide_label.as_ptr(), len as i32);
    }
}
