//! Main window creation, toolbar buttons, and `WndProc` message routing.

use windows_sys::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::app::STATE;
use crate::{tray, win};

pub const WM_TRAY: u32 = WM_APP + 1;
pub const WM_KEY_EVENT: u32 = WM_APP + 2;

const CLASS_NAME: &str = "ClickAssistMain";

/// Client-area width/height of the main window.
const WINDOW_WIDTH: i32 = 420;
const WINDOW_HEIGHT: i32 = 120;

/// Toolbar button layout.
const BTN_WIDTH: i32 = 120;
const BTN_HEIGHT: i32 = 32;
const BTN_MARGIN: i32 = 12;

/// Register the window class and create the main window.
pub fn create_main_window(hinstance: HINSTANCE) -> HWND {
    let class_name = win::wide(CLASS_NAME);
    let window_title = win::wide("ClickAssist");

    // ---------- Register the window class ----------
    let wc = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(wnd_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinstance,
        hIcon: unsafe { LoadIconW(std::ptr::null_mut(), IDI_APPLICATION) },
        hCursor: unsafe { LoadCursorW(std::ptr::null_mut(), IDC_ARROW) },
        hbrBackground: (COLOR_WINDOW + 1) as _,
        lpszMenuName: std::ptr::null(),
        lpszClassName: class_name.as_ptr(),
    };

    unsafe {
        RegisterClassW(&wc);
    }

    // ---------- Create the window ----------
    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            window_title.as_ptr(),
            // Fixed-size window: no maximize/resize.
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            WINDOW_WIDTH,
            WINDOW_HEIGHT,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinstance,
            std::ptr::null_mut(),
        )
    };

    hwnd
}

/// Create the three toolbar buttons (Record / Show Positions / Start) as
/// child `BUTTON` controls laid out in a row.
fn create_buttons(parent: HWND) {
    let buttons = [
        (crate::app::constants::ID_RECORD, "Record"),
        (crate::app::constants::ID_SHOW_POSITIONS, "Show Positions"),
        (crate::app::constants::ID_START, "Start"),
    ];

    let btn_class = win::wide("BUTTON");

    for (i, (id, label)) in buttons.iter().enumerate() {
        let btn_text = win::wide(label);
        let x = BTN_MARGIN + i as i32 * (BTN_WIDTH + BTN_MARGIN);

        let hwnd = unsafe {
            CreateWindowExW(
                0,
                btn_class.as_ptr(),
                btn_text.as_ptr(),
                WS_CHILD | WS_VISIBLE | BS_PUSHBUTTON as u32,
                x,
                BTN_MARGIN,
                BTN_WIDTH,
                BTN_HEIGHT,
                parent,
                *id as isize as _,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };

        if hwnd != std::ptr::null_mut() {
            set_button_font(hwnd);
        }
    }
}

/// Set a basic GUI font on a child button so it doesn't use the system
/// fixed font.
fn set_button_font(hwnd: HWND) {
    unsafe {
        let hfont = CreateFontW(
            18,
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
            SendMessageW(hwnd, WM_SETFONT, hfont as WPARAM, 1); // 1 = redraw
        }
    }
}

/// Window procedure for the main window.
pub unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        // ---------- Window created: build the toolbar ----------
        WM_CREATE => {
            create_buttons(hwnd);
            0
        }

        // ---------- Toolbar button / tray-menu commands ----------
        WM_COMMAND => {
            // Low word of wparam is the control/command id.
            let id = (wparam & 0xFFFF) as u16;
            STATE.with(|s| {
                s.borrow_mut().on_toolbar_command(id);
            });
            0
        }

        // ---------- Tray icon callback ----------
        WM_TRAY => {
            tray::handle_tray_message(hwnd, wparam, lparam);
            0
        }

        // ---------- Close: hide to tray instead of exiting ----------
        WM_CLOSE => {
            unsafe {
                ShowWindow(hwnd, SW_HIDE);
            }
            0
        }

        // ---------- Destroy: quit the message loop ----------
        WM_DESTROY => {
            unsafe {
                PostQuitMessage(0);
            }
            0
        }

        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
