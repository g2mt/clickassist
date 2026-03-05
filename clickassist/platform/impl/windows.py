import ctypes
import threading
from typing import Optional

import win32api
import win32con
from PySide6.QtCore import QPoint, Qt
from PySide6.QtGui import QKeySequence

from clickassist.platform.hotkey import AbstractHotkeyListener
from clickassist.platform.backend import Backend


class WindowsHotkeyListener(threading.Thread, AbstractHotkeyListener):
    """Registers Win32 global hotkeys and fires callbacks."""

    def __init__(self):
        super().__init__(daemon=True)
        self._bindings: dict[int, tuple[QKeySequence, callable]] = {}
        self._id_counter: int = 1
        self._hwnd: Optional[int] = None

    def register(self, key_sequence: QKeySequence, callback: callable) -> int:
        hk_id = self._id_counter
        self._id_counter += 1
        self._bindings[hk_id] = (key_sequence, callback)
        return hk_id

    def unregister_all(self):
        if self._hwnd:
            for hk_id in list(self._bindings.keys()):
                ctypes.windll.user32.UnregisterHotKey(self._hwnd, hk_id)
        self._bindings.clear()

    def _qs_to_win32(self, qs: QKeySequence):
        key = qs[0].key()
        mods = qs[0].keyboardModifiers()
        win_mod = 0
        if mods & Qt.KeyboardModifier.AltModifier:
            win_mod |= win32con.MOD_ALT
        if mods & Qt.KeyboardModifier.ControlModifier:
            win_mod |= win32con.MOD_CONTROL
        if mods & Qt.KeyboardModifier.ShiftModifier:
            win_mod |= win32con.MOD_SHIFT
        if mods & Qt.KeyboardModifier.MetaModifier:
            win_mod |= win32con.MOD_WIN
        vk = int(key)
        # Map Qt key to VK
        vk_code = ctypes.windll.user32.VkKeyScanW(vk) & 0xFF
        return win_mod, vk_code

    def run(self):
        import ctypes.wintypes as wt
        # Create a message-only window handle via a dummy approach
        for hk_id, (qs, _cb) in self._bindings.items():
            mod, vk = self._qs_to_win32(qs)
            ctypes.windll.user32.RegisterHotKey(None, hk_id, mod, vk)

        msg = wt.MSG()
        while ctypes.windll.user32.GetMessageW(ctypes.byref(msg), None, 0, 0) != 0:
            if msg.message == win32con.WM_HOTKEY:
                hk_id = msg.wParam
                if hk_id in self._bindings:
                    _qs, cb = self._bindings[hk_id]
                    cb()
            ctypes.windll.user32.TranslateMessage(ctypes.byref(msg))
            ctypes.windll.user32.DispatchMessageW(ctypes.byref(msg))

    def start(self):
        """Start the listener thread."""
        super().start()


class WindowsBackend(Backend):
    """Windows-specific backend implementation using Win32 API."""

    def click(self, x: int, y: int):
        win32api.SetCursorPos((x, y))
        win32api.mouse_event(win32con.MOUSEEVENTF_LEFTDOWN, x, y, 0, 0)
        win32api.mouse_event(win32con.MOUSEEVENTF_LEFTUP, x, y, 0, 0)

    def get_cursor_pos(self) -> QPoint:
        pos = win32api.GetCursorPos()
        return QPoint(pos[0], pos[1])

    def create_hotkey_listener(self):
        return WindowsHotkeyListener()
