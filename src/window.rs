//! Main window created via `winwrapper`'s `Window` trait, with child
//! buttons managed by `winwrapper::controls` and automatically laid out
//! using `winwrapper::layout::Layout` on every `WM_SIZE`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use windows_sys::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::UI::Controls::{
    InitCommonControls, TTF_IDISHWND, TTF_SUBCLASS, TTM_ADDTOOLW, TTM_SETMAXTIPWIDTH,
    TTTOOLINFOW, TTS_ALWAYSTIP, TTS_NOPREFIX,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use windows_sys::core::w;
use winwrapper::controls;
use winwrapper::error::WinError;
use winwrapper::layout::{Item, Layout, Orientation};
use winwrapper::mutex::Mutex;
use winwrapper::utils::HWNDWrapper;
use winwrapper::window::{Base, BaseRef, Window, register_classname};

use crate::app::{STATE, constants};
use crate::tray;

/// Custom message sent by the tray icon on mouse events.
pub const WM_TRAY: u32 = WM_APP + 1;

// Layout constants
const BTN_WIDTH: i32 = 120;
const WINDOW_WIDTH: i32 = (BTN_WIDTH + 12) * 6 + 12;
const WINDOW_HEIGHT: i32 = 100;

// ---------------------------------------------------------------------------
// Main window struct
// ---------------------------------------------------------------------------

pub struct MainWindow {
    base: BaseRef,
    layout: Mutex<Layout>,
    tooltip_hwnd: HWNDWrapper,
    explorer_running: AtomicBool,
    // Buttons are stored for reference; the Layout already holds copies.
    #[allow(dead_code)]
    btn_kill_explorer: HWNDWrapper,
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
                unsafe {
                    InitCommonControls();
                }

                let explorer_running = crate::explorer::is_running();
                let btn_kill_explorer = HWNDWrapper(controls::create_button(
                    if explorer_running {
                        "Kill Explorer"
                    } else {
                        "Launch Explorer"
                    },
                    0,
                    0,
                    0,
                    0,
                    hwnd,
                    Some(constants::ID_KILL_EXPLORER as isize as _),
                    hinstance,
                ));
                let tooltip_hwnd = unsafe {
                    CreateWindowExW(
                        WS_EX_TOPMOST,
                        w!("tooltips_class32"),
                        std::ptr::null(),
                        WS_POPUP | TTS_ALWAYSTIP | TTS_NOPREFIX,
                        CW_USEDEFAULT,
                        CW_USEDEFAULT,
                        CW_USEDEFAULT,
                        CW_USEDEFAULT,
                        hwnd,
                        std::ptr::null_mut(),
                        hinstance,
                        std::ptr::null(),
                    )
                };
                if !tooltip_hwnd.is_null() {
                    let mut tool_info: TTTOOLINFOW = unsafe { std::mem::zeroed() };
                    tool_info.cbSize = std::mem::size_of::<TTTOOLINFOW>() as u32;
                    tool_info.uFlags = TTF_IDISHWND | TTF_SUBCLASS;
                    tool_info.hwnd = hwnd;
                    tool_info.uId = btn_kill_explorer.0 as usize;
                    tool_info.hinst = hinstance;
                    tool_info.lpszText = w!(
                        "Left click to toggle Explorer. Explorer may handle some touch functions, which may interfere with ClickAssist touch input."
                    ) as *mut u16;
                    unsafe {
                        SendMessageW(
                            tooltip_hwnd,
                            TTM_ADDTOOLW,
                            0,
                            &mut tool_info as *mut TTTOOLINFOW as LPARAM,
                        );
                        SendMessageW(tooltip_hwnd, TTM_SETMAXTIPWIDTH, 0, 350);
                    }
                }

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
                            hwnd: btn_kill_explorer.clone(),
                            size: BTN_WIDTH,
                        },
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
                    tooltip_hwnd: HWNDWrapper(tooltip_hwnd),
                    explorer_running: AtomicBool::new(explorer_running),
                    btn_kill_explorer,
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
                if wparam as u32 == SIZE_MINIMIZED {
                    if self.explorer_running.load(Ordering::Relaxed) {
                        unsafe {
                            ShowWindow(self.base.hwnd(), SW_HIDE);
                        }
                    }
                } else {
                    self.layout_widgets();
                }
                0
            }

            WM_COMMAND => {
                let id = (wparam & 0xFFFF) as u16;
                if id == constants::ID_KILL_EXPLORER {
                    if self.explorer_running.load(Ordering::Relaxed) {
                        if crate::explorer::kill() {
                            self.explorer_running.store(false, Ordering::Relaxed);
                            unsafe {
                                SetWindowTextW(self.btn_kill_explorer.0, w!("Launch Explorer"));
                            }
                        }
                    } else {
                        unsafe {
                            windows_sys::Win32::UI::Shell::ShellExecuteW(
                                HWND::default(),
                                w!("open"),
                                w!("explorer.exe"),
                                std::ptr::null(),
                                std::ptr::null(),
                                SW_SHOWNORMAL,
                            );
                        }
                        self.explorer_running.store(true, Ordering::Relaxed);
                        unsafe {
                            SetWindowTextW(self.btn_kill_explorer.0, w!("Kill Explorer"));
                        }
                    }
                } else {
                    STATE.with(|s| {
                        let mut state = s.borrow_mut();
                        state.on_toolbar_command(id);
                        let label = if state.mode == crate::app::Mode::Started {
                            w!("Stop")
                        } else {
                            w!("Start")
                        };
                        unsafe {
                            SetWindowTextW(self.btn_start.0, label);
                        }
                    });
                }
                0
            }

            WM_TRAY => {
                tray::handle_tray_message(self.base.hwnd(), wparam, lparam);
                0
            }

            WM_CLOSE => {
                unsafe {
                    ShowWindow(self.base.hwnd(), SW_MINIMIZE);
                }
                0
            }

            WM_DESTROY => {
                unsafe {
                    if !self.tooltip_hwnd.0.is_null() {
                        DestroyWindow(self.tooltip_hwnd.0);
                    }
                    PostQuitMessage(0);
                }
                0
            }

            _ => unsafe { DefWindowProcW(self.base.hwnd(), msg, wparam, lparam) },
        }
    }
}
