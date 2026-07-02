//! Toolbar with three buttons: Record, Show Positions, Start.

use windows_sys::Win32::Foundation::{HINSTANCE, HWND, WPARAM};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::win;

pub const ID_RECORD: u16 = 101;
pub const ID_SHOW_POSITIONS: u16 = 102;
pub const ID_START: u16 = 103;

/// Create three child BUTTON windows acting as a toolbar.
pub fn create_toolbar(parent: HWND, _hinstance: HINSTANCE) -> Vec<HWND> {
    let buttons = [
        (ID_RECORD, "Record"),
        (ID_SHOW_POSITIONS, "Show Positions"),
        (ID_START, "Start"),
    ];

    let mut hwnds = Vec::with_capacity(3);

    for (id, label) in &buttons {
        let btn_class = win::wide("BUTTON");
        let btn_text = win::wide(label);

        let hwnd = unsafe {
            CreateWindowExW(
                0,
                btn_class.as_ptr(),
                btn_text.as_ptr(),
                WS_CHILD | WS_VISIBLE | BS_PUSHBUTTON as u32,
                0,
                0,
                120,
                32,
                parent,
                *id as isize as _,
                std::ptr::null_mut(), // hInstance
                std::ptr::null_mut(),
            )
        };

        if hwnd != std::ptr::null_mut() {
            set_button_font(hwnd);
            hwnds.push(hwnd);
        }
    }

    hwnds
}

/// Set a basic GUI font on a child button so it doesn't use the system fixed font.
fn set_button_font(hwnd: HWND) {
    unsafe {
        let hfont = CreateFontW(
            18,        // height
            0,         // width (auto from height)
            0,         // escapement
            0,         // orientation
            FW_NORMAL as i32, // weight
            0,         // italic
            0,         // underline
            0,         // strikeout
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
