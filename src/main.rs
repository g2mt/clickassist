//! ClickAssist — bind mouse positions to keyboard keys and inject them as
//! synthetic touchscreen events on Windows.
//!
//! Entry point: set DPI awareness, load config, create windows (main +
//! overlay), install the keyboard hook, and run the message loop.
//!
//! The main window is built on [`winwrapper`]'s `Window` trait, with child
//! buttons created via [`winwrapper::controls`] and automatically laid out
//! using [`winwrapper::layout::Layout`].

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod bindings;
mod config;
mod hook;
mod overlay;
mod tooltip;
mod touch;
mod tray;
mod utils;
mod window;

use std::collections::HashMap;
use std::mem;

use winwrapper::window::Window;
use windows_sys::Win32::Foundation::{HINSTANCE, POINT};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

fn main() {
    // If invoked with --touch-worker, run the touch-injection child process.
    if std::env::args().any(|a| a == "--touch-worker") {
        touch::run_worker();
    }

    // ---------- DPI awareness ----------
    utils::set_dpi_awareness();

    // ---------- Load config ----------
    let cfg = config::load();
    let mut bindings: HashMap<u32, POINT> = HashMap::new();
    for b in &cfg.bindings {
        bindings.insert(b.vk, POINT { x: b.x, y: b.y });
    }

    // ---------- Get module instance ----------
    let hinstance: HINSTANCE = unsafe { GetModuleHandleW(std::ptr::null()) };

    // ---------- Initialise touch injection ----------
    touch::init_touch_injection(10);

    // ---------- Create windows ----------
    let main_window = window::MainWindow::create(hinstance);
    let main_hwnd = main_window.base().hwnd();
    let overlay_hwnd = overlay::create_overlay_window(hinstance);

    // ---------- Initialise app state ----------
    app::STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.bindings = bindings;
        s.main_hwnd = main_hwnd;
        s.overlay_hwnd = overlay_hwnd;
    });

    // ---------- Add tray icon ----------
    let tray_data = tray::add_tray_icon(main_hwnd);

    // ---------- Show the main window ----------
    unsafe {
        ShowWindow(main_hwnd, SW_SHOW);
    }
    // Force child buttons to paint immediately.
    main_window.base().redraw(true);

    // ---------- Install keyboard hook ----------
    let _hook = hook::install_keyboard_hook();

    // ---------- Message loop ----------
    let exit_code = run_message_loop();

    // ---------- Cleanup ----------
    touch::deinit_touch_injection();
    hook::uninstall_keyboard_hook(_hook);
    tray::remove_tray_icon(&tray_data);
    unsafe {
        DestroyWindow(overlay_hwnd);
    }
    // `main_window` is dropped here (Base::drop → DestroyWindow).

    std::process::exit(exit_code);
}

/// Classic Win32 message pump. Dispatches toolbar commands to the app
/// controller.
fn run_message_loop() -> i32 {
    let mut msg: MSG = unsafe { mem::zeroed() };

    loop {
        let ret = unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) };
        match ret {
            -1 => {
                return 1;
            }
            0 => {
                return msg.wParam as i32;
            }
            _ => {
                unsafe {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }

                // Handle toolbar and tray menu commands after dispatch
                if msg.message == WM_COMMAND {
                    let id = (msg.wParam & 0xFFFF) as u16;
                    app::STATE.with(|s| {
                        s.borrow_mut().on_toolbar_command(id);
                    });
                }
            }
        }
    }
}
