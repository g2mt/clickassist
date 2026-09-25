//! Transparent, click-through, top-most overlay window that draws each bound
//! key as a dot with a centred label above it.

use std::collections::HashMap;

use windows_sys::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::app::STATE;
use crate::{bindings, utils};

const OVERLAY_CLASS: &str = "ClickAssistOverlay";
const DOT_RADIUS: i32 = 6;
const LABEL_GAP: i32 = 4;
const HIT_RADIUS: i32 = DOT_RADIUS + 6;

/// Create the overlay window (initially hidden).
pub fn create_overlay_window(hinstance: HINSTANCE) -> HWND {
    unsafe {
        let class_name = utils::wide(OVERLAY_CLASS);
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
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            utils::wide(OVERLAY_CLASS).as_ptr(),
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
}

/// Redraw a visible overlay after its bindings change.
pub fn refresh_overlay(overlay: HWND) {
    unsafe {
        InvalidateRect(overlay, std::ptr::null(), 1);
    }
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
            unsafe {
                let mut ps: PAINTSTRUCT = std::mem::zeroed();
                let hdc = BeginPaint(hwnd, &mut ps);

                if hdc != std::ptr::null_mut() {
                    // The layered window uses this colour as its transparent
                    // colorkey, so erase with it before redrawing the labels.
                    let brush = CreateSolidBrush((COLOR_WINDOW + 1) as u32);
                    FillRect(hdc, &ps.rcPaint, brush);
                    DeleteObject(brush as _);

                    STATE.with(|state| paint_bindings(hdc, &state.borrow().bindings));
                }

                EndPaint(hwnd, &ps);
            }
            0
        }
        WM_NCHITTEST => {
            // WM_NCHITTEST supplies screen coordinates directly.
            let x = (lparam as i32 & 0xFFFF) as i16 as i32;
            let y = ((lparam as i32 >> 16) & 0xFFFF) as i16 as i32;

            // Remain click-through everywhere except a dot, so applications
            // beneath the overlay continue to receive normal mouse input.
            if binding_at(x, y).is_some() {
                HTCLIENT as LRESULT
            } else {
                HTTRANSPARENT as LRESULT
            }
        }
        WM_RBUTTONUP => {
            let mut point = POINT {
                x: (lparam as i32 & 0xFFFF) as i16 as i32,
                y: ((lparam as i32 >> 16) & 0xFFFF) as i16 as i32,
            };
            unsafe {
                ClientToScreen(hwnd, &mut point);
            }

            if let Some(vk) = binding_at(point.x, point.y) {
                STATE.with(|state| state.borrow_mut().remove_binding(vk));
            }
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

/// Return the binding whose dot contains the given screen-coordinate point.
fn binding_at(x: i32, y: i32) -> Option<u32> {
    STATE.with(|state| {
        state
            .borrow()
            .bindings
            .iter()
            .find(|(_, position)| {
                let dx = x - position.x;
                let dy = y - position.y;
                dx * dx + dy * dy <= HIT_RADIUS * HIT_RADIUS
            })
            .map(|(&vk, _)| vk)
    })
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
    let wide_label = utils::wide(label);
    let len = wide_label.len() - 1; // exclude null terminator

    unsafe {
        let mut size = std::mem::zeroed();
        GetTextExtentPoint32W(hdc, wide_label.as_ptr(), len as i32, &mut size);

        let text_x = cx - size.cx / 2;

        SetBkMode(hdc, TRANSPARENT as i32);
        SetTextColor(hdc, 0x0000FF);

        TextOutW(
            hdc,
            text_x,
            top_y - size.cy,
            wide_label.as_ptr(),
            len as i32,
        );
    }
}
