import subprocess
import re
from typing import Optional

import libevdev

from PySide6.QtCore import QPoint
from PySide6.QtGui import QKeySequence

from clickassist.platform.hotkey import AbstractHotkeyListener
from clickassist.platform.backend import AbstractBackend

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


# Touchscreen resolution bounds for the virtual device
_TOUCH_MAX_X = 32767
_TOUCH_MAX_Y = 32767


class WaylandBackend(AbstractBackend):
    """Linux-specific backend implementation using uinput."""

    def __init__(self):
        dev = libevdev.Device()
        dev.name = "ClickAssist Virtual Touchscreen"

        dev.enable(libevdev.EV_KEY.BTN_TOUCH)

        dev.enable(
            libevdev.EV_ABS.ABS_X,
            libevdev.InputAbsInfo(minimum=0, maximum=_TOUCH_MAX_X),
        )
        dev.enable(
            libevdev.EV_ABS.ABS_Y,
            libevdev.InputAbsInfo(minimum=0, maximum=_TOUCH_MAX_Y),
        )
        dev.enable(
            libevdev.EV_ABS.ABS_MT_POSITION_X,
            libevdev.InputAbsInfo(minimum=0, maximum=_TOUCH_MAX_X),
        )
        dev.enable(
            libevdev.EV_ABS.ABS_MT_POSITION_Y,
            libevdev.InputAbsInfo(minimum=0, maximum=_TOUCH_MAX_Y),
        )
        dev.enable(
            libevdev.EV_ABS.ABS_MT_SLOT,
            libevdev.InputAbsInfo(minimum=0, maximum=9),
        )
        dev.enable(
            libevdev.EV_ABS.ABS_MT_TRACKING_ID,
            libevdev.InputAbsInfo(minimum=0, maximum=65535),
        )

        dev.enable(libevdev.EV_PROP.INPUT_PROP_DIRECT)

        self._uinput = dev.create_uinput_device()
        self._tracking_id = 0

    def _send_touch(self, x: int, y: int, pressed: bool):
        """Send touch events to the virtual device."""
        events = [
            libevdev.InputEvent(libevdev.EV_ABS.ABS_MT_SLOT, 0),
        ]

        if pressed:
            self._tracking_id += 1
            events += [
                libevdev.InputEvent(libevdev.EV_ABS.ABS_MT_TRACKING_ID, self._tracking_id),
                libevdev.InputEvent(libevdev.EV_ABS.ABS_MT_POSITION_X, x),
                libevdev.InputEvent(libevdev.EV_ABS.ABS_MT_POSITION_Y, y),
                libevdev.InputEvent(libevdev.EV_ABS.ABS_X, x),
                libevdev.InputEvent(libevdev.EV_ABS.ABS_Y, y),
                libevdev.InputEvent(libevdev.EV_KEY.BTN_TOUCH, 1),
            ]
        else:
            events += [
                libevdev.InputEvent(libevdev.EV_ABS.ABS_MT_TRACKING_ID, -1),
                libevdev.InputEvent(libevdev.EV_KEY.BTN_TOUCH, 0),
            ]

        events.append(libevdev.InputEvent(libevdev.EV_SYN.SYN_REPORT, 0))
        self._uinput.send_events(events)

    def mouse_down(self, x: int, y: int):
        """Perform a touch down at the given coordinates."""
        self._send_touch(x, y, pressed=True)

    def mouse_up(self, x: int, y: int):
        """Perform a touch up at the given coordinates."""
        self._send_touch(x, y, pressed=False)

    def get_cursor_pos(self) -> QPoint:
        raise NotImplementedError("todo")

    def create_hotkey_listener(self):
        return WaylandHotkeyListener()
