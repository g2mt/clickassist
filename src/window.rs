//! Main window creation and `WndProc` message routing.

use windows_sys::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::COLOR_WINDOW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::{toolbar, tray, win};

pub const WM_TRAY: u32 = WM_APP + 1;
pub const WM_KEY_EVENT: u32 = WM_APP + 2;

const CLASS_NAME: &str = "ClickAssistMain";

/// Register the window class and create the main window.
pub fn create_main_window(hinstance: HINSTANCE) -> HWND {
    unsafe {
        let class_name = win::wide(CLASS_NAME);
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: std::ptr::null_mut(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: (COLOR_WINDOW + 1) as isize as _,
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        if RegisterClassExW(&wc) == 0 {
            panic!("RegisterClassExW failed: {}", win::last_error());
        }
    }

    let hwnd = unsafe {
        CreateWindowExW(
            0,
            win::wide(CLASS_NAME).as_ptr(),
            win::wide("ClickAssist").as_ptr(),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            420,
            80,
            std::ptr::null_mut(), // no parent
            std::ptr::null_mut(), // no menu
            hinstance,
            std::ptr::null_mut(),
        )
    };

    if hwnd == std::ptr::null_mut() {
        panic!("CreateWindowExW failed: {}", win::last_error());
    }
    hwnd
}

/// Window procedure for the main window.
pub unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let _tb = toolbar::create_toolbar(hwnd, std::ptr::null_mut());
            0
        }

        WM_COMMAND => {
            let id = (wparam & 0xFFFF) as u16;
            match id {
                toolbar::ID_RECORD => {
                    post_app_command(hwnd, toolbar::ID_RECORD);
                }
                toolbar::ID_SHOW_POSITIONS => {
                    post_app_command(hwnd, toolbar::ID_SHOW_POSITIONS);
                }
                toolbar::ID_START => {
                    post_app_command(hwnd, toolbar::ID_START);
                }
                _ => {}
            }
            0
        }

        WM_SIZE => {
            reposition_toolbar(hwnd);
            0
        }

        WM_CLOSE => {
            unsafe { ShowWindow(hwnd, SW_HIDE) };
            0
        }

        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            0
        }

        WM_TRAY => {
            tray::handle_tray_message(hwnd, wparam, lparam);
            0
        }

        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn post_app_command(hwnd: HWND, id: u16) {
    unsafe {
        PostMessageW(hwnd, WM_COMMAND, id as WPARAM, 0);
    }
}

fn reposition_toolbar(hwnd: HWND) {
    unsafe {
        let mut rect = std::mem::zeroed();
        GetClientRect(hwnd, &mut rect);
        let w = rect.right - rect.left;
        let btn_w = (w - 40) / 3;
        let h = 32;
        let y = 8;

        for (i, id) in [
            toolbar::ID_RECORD,
            toolbar::ID_SHOW_POSITIONS,
            toolbar::ID_START,
        ]
        .iter()
        .enumerate()
        {
            if let Some(child) = find_child_button(hwnd, *id) {
                let x = 8 + (i as i32) * (btn_w + 8);
                SetWindowPos(child, std::ptr::null_mut(), x, y, btn_w, h, SWP_NOZORDER);
            }
        }
    }
}

fn find_child_button(parent: HWND, id: u16) -> Option<HWND> {
    unsafe {
        let child = GetDlgItem(parent, id as i32);
        if child == std::ptr::null_mut() {
            None
        } else {
            Some(child)
        }
    }
}
