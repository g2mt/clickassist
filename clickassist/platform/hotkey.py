"""Platform-specific hotkey listener implementations."""

import sys
from abc import ABC, abstractmethod
from typing import Optional

from PySide6.QtCore import Qt
from PySide6.QtGui import QKeySequence

from clickassist.utils.constants import PLATFORM_WINDOWS

if PLATFORM_WINDOWS:
    import win32con
    import ctypes
    from ctypes import wintypes
    import threading


class AbstractHotkeyListener(ABC):
    """Abstract base class for global hotkey listeners."""

    @abstractmethod
    def register(self, key_sequence: QKeySequence, callback: callable) -> int:
        """Register a hotkey and return its ID."""
        pass

    @abstractmethod
    def unregister_all(self):
        """Unregister all hotkeys."""
        pass

    @abstractmethod
    def start(self):
        """Start listening for hotkeys."""
        pass


if PLATFORM_WINDOWS:
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

else:
    try:
        import keyboard as kb_lib
        _KB_AVAILABLE = True
    except ImportError:
        _KB_AVAILABLE = False

    class WaylandHotkeyListener(AbstractHotkeyListener):
        """Linux/Wayland hotkey listener using the keyboard library."""

        def __init__(self):
            self._hooks: list = []

        def register(self, key_sequence: QKeySequence, callback: callable) -> int:
            if not _KB_AVAILABLE:
                return -1
            hotkey_str = key_sequence.toString().lower().replace("+", "+")
            hook = kb_lib.add_hotkey(hotkey_str, callback)
            self._hooks.append(hook)
            return len(self._hooks) - 1

        def unregister_all(self):
            if not _KB_AVAILABLE:
                return
            for hook in self._hooks:
                try:
                    kb_lib.remove_hotkey(hook)
                except Exception:
                    pass
            self._hooks.clear()

        def start(self):
            """Keyboard library works without a thread."""
            pass
