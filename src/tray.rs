//! Notification-area (tray) icon and context menu.

use windows_sys::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::win;
use crate::window::WM_TRAY;

/// Add the ClickAssist icon to the notification area.
pub fn add_tray_icon(hwnd: HWND) -> Box<NOTIFYICONDATAW> {
    let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };

    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    nid.uCallbackMessage = WM_TRAY;

    // Use the default application icon
    nid.hIcon = unsafe { LoadIconW(std::ptr::null_mut(), IDI_APPLICATION) };

    let tip = win::wide("ClickAssist");
    let tip_len = tip.len().min(127);
    nid.szTip[..tip_len].copy_from_slice(&tip[..tip_len]);

    unsafe {
        Shell_NotifyIconW(NIM_ADD, &nid);
    }

    Box::new(nid)
}

/// Remove the tray icon.
pub fn remove_tray_icon(nid: &NOTIFYICONDATAW) {
    unsafe {
        Shell_NotifyIconW(NIM_DELETE, nid);
    }
}

/// Handle a `WM_TRAY` message from the tray icon.
pub fn handle_tray_message(hwnd: HWND, _wparam: WPARAM, lparam: LPARAM) {
    let lparam = lparam as u32;

    match lparam {
        WM_LBUTTONUP => unsafe {
            ShowWindow(hwnd, SW_SHOW);
            SetForegroundWindow(hwnd);
        },
        WM_RBUTTONUP => {
            show_tray_menu(hwnd);
        }
        _ => {}
    }
}

/// Show a right-click context menu at the cursor position.
pub fn show_tray_menu(hwnd: HWND) {
    unsafe {
        let menu = CreatePopupMenu();
        if menu == std::ptr::null_mut() {
            return;
        }

        AppendMenuW(menu, MF_STRING, 201, win::wide("Show").as_ptr());
        AppendMenuW(menu, MF_STRING, 202, win::wide("Stop").as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
        AppendMenuW(menu, MF_STRING, 203, win::wide("Exit").as_ptr());

        SetForegroundWindow(hwnd);

        let mut pt = std::mem::zeroed();
        GetCursorPos(&mut pt);

        let cmd = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            pt.x,
            pt.y,
            0,
            hwnd,
            std::ptr::null(),
        );

        DestroyMenu(menu);

        match cmd {
            201 => {
                ShowWindow(hwnd, SW_SHOW);
                SetForegroundWindow(hwnd);
            }
            202 => {
                PostMessageW(hwnd, WM_COMMAND, 104 as WPARAM, 0);
            }
            203 => {
                DestroyWindow(hwnd);
            }
            _ => {}
        }
    }
}
