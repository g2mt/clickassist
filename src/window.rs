//! Main window created via `winwrapper`'s `Window` trait, with child
//! buttons managed by `winwrapper::controls` and automatically laid out
//! using `winwrapper::layout::Layout` on every `WM_SIZE`.

use std::sync::Arc;

use windows_sys::core::w;
use windows_sys::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use winwrapper::controls;
use winwrapper::error::WinError;
use winwrapper::layout::{Item, Layout, Orientation};
use winwrapper::mutex::Mutex;
use winwrapper::utils::HWNDWrapper;
use winwrapper::window::{register_classname, Base, BaseRef, Window};

use crate::app::{constants, STATE};
use crate::tray;

/// Custom message sent by the tray icon on mouse events.
pub const WM_TRAY: u32 = WM_APP + 1;

// Layout constants
const BTN_WIDTH: i32 = 120;
const WINDOW_WIDTH: i32 = (BTN_WIDTH + 12) * 5 + 12;
const WINDOW_HEIGHT: i32 = 100;

// ---------------------------------------------------------------------------
// Main window struct
// ---------------------------------------------------------------------------

pub struct MainWindow {
    base: BaseRef,
    layout: Mutex<Layout>,
    // Buttons are stored for reference; the Layout already holds copies.
    #[allow(dead_code)]
    btn_record: HWNDWrapper,
    #[allow(dead_code)]
    btn_show_positions: HWNDWrapper,
    #[allow(dead_code)]
    btn_reset: HWNDWrapper,
    #[allow(dead_code)]
    btn_start: HWNDWrapper,
    #[allow(dead_code)]
    btn_quit: HWNDWrapper,
}

impl MainWindow {
    /// Create the main application window (hidden until `ShowWindow`).
    pub fn create(hinstance: HINSTANCE) -> Arc<Self> {
        let class = register_classname("ClickAssistMain");

        Base::create_window::<Self, _, WinError>(
            0,
            class,
            w!("ClickAssist"),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            WINDOW_WIDTH,
            WINDOW_HEIGHT,
            HWND::default(),
            None,
            hinstance,
            |base| {
                let hwnd = base.hwnd();

                let btn_record = HWNDWrapper(controls::create_button(
                    "Record",
                    0,
                    0,
                    0,
                    0,
                    hwnd,
                    Some(constants::ID_RECORD as isize as _),
                    hinstance,
                ));
                let btn_show_positions = HWNDWrapper(controls::create_button(
                    "Show Positions",
                    0,
                    0,
                    0,
                    0,
                    hwnd,
                    Some(constants::ID_SHOW_POSITIONS as isize as _),
                    hinstance,
                ));
                let btn_reset = HWNDWrapper(controls::create_button(
                    "Reset",
                    0,
                    0,
                    0,
                    0,
                    hwnd,
                    Some(constants::ID_RESET as isize as _),
                    hinstance,
                ));
                let btn_start = HWNDWrapper(controls::create_button(
                    "Start",
                    0,
                    0,
                    0,
                    0,
                    hwnd,
                    Some(constants::ID_START as isize as _),
                    hinstance,
                ));
                let btn_quit = HWNDWrapper(controls::create_button(
                    "Quit",
                    0,
                    0,
                    0,
                    0,
                    hwnd,
                    Some(constants::ID_QUIT as isize as _),
                    hinstance,
                ));

                let layout = Layout {
                    orientation: Orientation::Horizontal,
                    items: vec![
                        Item::Fixed {
                            hwnd: btn_record.clone(),
                            size: BTN_WIDTH,
                        },
                        Item::Fixed {
                            hwnd: btn_show_positions.clone(),
                            size: BTN_WIDTH,
                        },
                        Item::Fixed {
                            hwnd: btn_reset.clone(),
                            size: BTN_WIDTH,
                        },
                        Item::Fixed {
                            hwnd: btn_start.clone(),
                            size: BTN_WIDTH,
                        },
                        Item::Fixed {
                            hwnd: btn_quit.clone(),
                            size: BTN_WIDTH,
                        },
                    ],
                    ..Default::default()
                };

                let window = Arc::new(Self {
                    base,
                    layout: Mutex::new(layout),
                    btn_record,
                    btn_show_positions,
                    btn_reset,
                    btn_start,
                    btn_quit,
                });

                // Perform initial layout.
                window.layout_widgets();

                Ok(window)
            },
        )
        .expect("failed to create main window")
    }

    /// Position all child buttons according to the current client rect.
    fn layout_widgets(&self) {
        let mut rect = RECT::default();
        unsafe {
            GetClientRect(self.base.hwnd(), &mut rect);
        }
        self.layout.lock().arrange(rect);
    }
}

// ---------------------------------------------------------------------------
// Window trait
// ---------------------------------------------------------------------------

impl Window for MainWindow {
    fn base(&self) -> &BaseRef {
        &self.base
    }

    fn wndproc(&self, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        match msg {
            WM_SIZE => {
                self.layout_widgets();
                0
            }

            WM_COMMAND => {
                let id = (wparam & 0xFFFF) as u16;
                STATE.with(|s| s.borrow_mut().on_toolbar_command(id));
                0
            }

            WM_TRAY => {
                tray::handle_tray_message(self.base.hwnd(), wparam, lparam);
                0
            }

            WM_CLOSE => {
                unsafe {
                    ShowWindow(self.base.hwnd(), SW_HIDE);
                }
                0
            }

            WM_DESTROY => {
                unsafe {
                    PostQuitMessage(0);
                }
                0
            }

            _ => unsafe { DefWindowProcW(self.base.hwnd(), msg, wparam, lparam) },
        }
    }
}
