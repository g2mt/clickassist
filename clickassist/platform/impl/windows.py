import ctypes
import ctypes.wintypes
import threading
import select
from typing import Optional

from clickassist.platform.backend import Backend
from clickassist.platform.key import KeyListener

from PySide6.QtCore import QPoint
from PySide6.QtWidgets import QApplication

# Win32 API constants
WH_KEYBOARD_LL = 13
WM_KEYDOWN = 0x0100
WM_KEYUP = 0x0101
WM_SYSKEYDOWN = 0x0104
WM_SYSKEYUP = 0x0105

MOUSEEVENTF_MOVE = 0x0001
MOUSEEVENTF_LEFTDOWN = 0x0002
MOUSEEVENTF_LEFTUP = 0x0004
MOUSEEVENTF_ABSOLUTE = 0x8000

VK_A = ord('A')
VK_Z = ord('Z')
VK_0 = ord('0')
VK_9 = ord('9')


# Win32 structures
class KBDLLHOOKSTRUCT(ctypes.Structure):
    _fields_ = [
        ("vkCode",      ctypes.wintypes.DWORD),
        ("scanCode",    ctypes.wintypes.DWORD),
        ("flags",       ctypes.wintypes.DWORD),
        ("time",        ctypes.wintypes.DWORD),
        ("dwExtraInfo", ctypes.POINTER(ctypes.c_ulong)),
    ]


# Win32 function signatures
_user32 = ctypes.windll.user32

_HOOKPROC = ctypes.WINFUNCTYPE(ctypes.c_long, ctypes.c_int, ctypes.wintypes.WPARAM, ctypes.wintypes.LPARAM)

_user32.SetWindowsHookExW.restype = ctypes.wintypes.HHOOK
_user32.SetWindowsHookExW.argtypes = [
    ctypes.c_int,
    _HOOKPROC,
    ctypes.wintypes.HINSTANCE,
    ctypes.wintypes.DWORD,
]
_user32.UnhookWindowsHookEx.restype = ctypes.wintypes.BOOL
_user32.UnhookWindowsHookEx.argtypes = [ctypes.wintypes.HHOOK]
_user32.CallNextHookEx.restype = ctypes.c_long
_user32.CallNextHookEx.argtypes = [
    ctypes.wintypes.HHOOK,
    ctypes.c_int,
    ctypes.wintypes.WPARAM,
    ctypes.wintypes.LPARAM,
]
_user32.GetMessageW.restype = ctypes.wintypes.BOOL
_user32.GetMessageW.argtypes = [
    ctypes.POINTER(ctypes.wintypes.MSG),
    ctypes.wintypes.HWND,
    ctypes.c_uint,
    ctypes.c_uint,
]
_user32.TranslateMessage.restype = ctypes.wintypes.BOOL
_user32.TranslateMessage.argtypes = [ctypes.POINTER(ctypes.wintypes.MSG)]
_user32.DispatchMessageW.restype = ctypes.c_long
_user32.DispatchMessageW.argtypes = [ctypes.POINTER(ctypes.wintypes.MSG)]
_user32.PostThreadMessageW.restype = ctypes.wintypes.BOOL
_user32.PostThreadMessageW.argtypes = [
    ctypes.wintypes.DWORD,
    ctypes.c_uint,
    ctypes.wintypes.WPARAM,
    ctypes.wintypes.LPARAM,
]

_user32.GetCursorPos.restype = ctypes.wintypes.BOOL
_user32.GetCursorPos.argtypes = [ctypes.POINTER(ctypes.wintypes.POINT)]

_user32.mouse_event.restype = None
_user32.mouse_event.argtypes = [
    ctypes.wintypes.DWORD,
    ctypes.wintypes.DWORD,
    ctypes.wintypes.DWORD,
    ctypes.wintypes.DWORD,
    ctypes.POINTER(ctypes.c_ulong),
]

WM_QUIT = 0x0012


class WindowsKeyListener(KeyListener):
    """
    Listens to global keyboard events on Windows using a low-level keyboard hook.
    """

    def __init__(self):
        super().__init__()
        self._stop_event = threading.Event()
        self._hook = None
        self._thread_id = None

        self._thread = threading.Thread(target=self._hook_loop, daemon=True)
        self._thread.start()

    def __del__(self):
        self._stop()

    def _stop(self):
        self._stop_event.set()
        if self._thread_id is not None:
            _user32.PostThreadMessageW(self._thread_id, WM_QUIT, 0, 0)
        self._thread.join()

    def _hook_loop(self):
        """Background thread: install hook and run message loop."""
        import ctypes.wintypes

        self._thread_id = ctypes.windll.kernel32.GetCurrentThreadId()

        def _hook_proc(nCode, wParam, lParam):
            if nCode >= 0:
                kb = ctypes.cast(lParam, ctypes.POINTER(KBDLLHOOKSTRUCT)).contents
                vk = kb.vkCode
                pressed = wParam in (WM_KEYDOWN, WM_SYSKEYDOWN)
                if VK_A <= vk <= VK_Z or VK_0 <= vk <= VK_9:
                    self.key_event.emit((chr(vk), pressed))
            return _user32.CallNextHookEx(self._hook, nCode, wParam, lParam)

        self._proc = _HOOKPROC(_hook_proc)
        self._hook = _user32.SetWindowsHookExW(WH_KEYBOARD_LL, self._proc, None, 0)
        if not self._hook:
            raise RuntimeError("Failed to install keyboard hook")

        msg = ctypes.wintypes.MSG()
        while not self._stop_event.is_set():
            ret = _user32.GetMessageW(ctypes.byref(msg), None, 0, 0)
            if ret == 0 or ret == -1:
                break
            _user32.TranslateMessage(ctypes.byref(msg))
            _user32.DispatchMessageW(ctypes.byref(msg))

        _user32.UnhookWindowsHookEx(self._hook)
        self._hook = None


class WindowsBackend(Backend):
    """Windows-specific backend implementation using Win32 API."""

    def __init__(self):
        # Get screen resolution using Qt
        app = QApplication.instance()
        if app is None:
            app = QApplication([])
        screen = app.primaryScreen()
        size = screen.size()
        self._screen_width = size.width()
        self._screen_height = size.height()

    def _to_absolute(self, x: int, y: int):
        """Convert pixel coordinates to Win32 absolute coordinates (0-65535)."""
        abs_x = int(x * 65535 / self._screen_width)
        abs_y = int(y * 65535 / self._screen_height)
        return abs_x, abs_y

    def mouse_down(self, x: int, y: int):
        """Perform a mouse left-button down at the given coordinates."""
        abs_x, abs_y = self._to_absolute(x, y)
        _user32.mouse_event(
            MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE,
            abs_x, abs_y, 0, None
        )
        _user32.mouse_event(
            MOUSEEVENTF_LEFTDOWN | MOUSEEVENTF_ABSOLUTE,
            abs_x, abs_y, 0, None
        )

    def mouse_up(self, x: int, y: int):
        """Perform a mouse left-button up at the given coordinates."""
        abs_x, abs_y = self._to_absolute(x, y)
        _user32.mouse_event(
            MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE,
            abs_x, abs_y, 0, None
        )
        _user32.mouse_event(
            MOUSEEVENTF_LEFTUP | MOUSEEVENTF_ABSOLUTE,
            abs_x, abs_y, 0, None
        )

    def get_cursor_pos(self) -> QPoint:
        """Get the current cursor position."""
        point = ctypes.wintypes.POINT()
        if not _user32.GetCursorPos(ctypes.byref(point)):
            raise RuntimeError("Failed to get cursor position")
        return QPoint(point.x, point.y)

    def create_key_listener(self) -> KeyListener:
        """Create and return a key listener for Windows."""
        return WindowsKeyListener()
