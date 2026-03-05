import subprocess
import re
import ctypes
import ctypes.util
import threading
import select
from typing import Optional

import libevdev

from PySide6.QtCore import QPoint
from PySide6.QtGui import QKeySequence
from PySide6.QtWidgets import QApplication

from clickassist.platform.backend import Backend
from clickassist.platform.key import KeyListener
from .linux_keycodes import KEYCODE_TO_STR


# Load libudev and libinput from C library
_libudev = ctypes.CDLL(ctypes.util.find_library("udev"), use_errno=True)
_libinput = ctypes.CDLL(ctypes.util.find_library("input"), use_errno=True)

# libudev function signatures
_libudev.udev_new.restype = ctypes.c_void_p
_libudev.udev_new.argtypes = []
_libudev.udev_unref.restype = ctypes.c_void_p
_libudev.udev_unref.argtypes = [ctypes.c_void_p]

# libinput function signatures
_libinput.libinput_udev_create_context.restype = ctypes.c_void_p
_libinput.libinput_udev_create_context.argtypes = [
    ctypes.c_void_p,  # interface
    ctypes.c_void_p,  # user_data
    ctypes.c_void_p,  # udev
]
_libinput.libinput_udev_assign_seat.restype = ctypes.c_int
_libinput.libinput_udev_assign_seat.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
_libinput.libinput_unref.restype = ctypes.c_void_p
_libinput.libinput_unref.argtypes = [ctypes.c_void_p]
_libinput.libinput_get_fd.restype = ctypes.c_int
_libinput.libinput_get_fd.argtypes = [ctypes.c_void_p]
_libinput.libinput_dispatch.restype = ctypes.c_int
_libinput.libinput_dispatch.argtypes = [ctypes.c_void_p]
_libinput.libinput_get_event.restype = ctypes.c_void_p
_libinput.libinput_get_event.argtypes = [ctypes.c_void_p]
_libinput.libinput_event_get_type.restype = ctypes.c_int
_libinput.libinput_event_get_type.argtypes = [ctypes.c_void_p]
_libinput.libinput_event_destroy.restype = None
_libinput.libinput_event_destroy.argtypes = [ctypes.c_void_p]
_libinput.libinput_event_get_keyboard_event.restype = ctypes.c_void_p
_libinput.libinput_event_get_keyboard_event.argtypes = [ctypes.c_void_p]
_libinput.libinput_event_keyboard_get_key.restype = ctypes.c_uint32
_libinput.libinput_event_keyboard_get_key.argtypes = [ctypes.c_void_p]
_libinput.libinput_event_keyboard_get_key_state.restype = ctypes.c_int
_libinput.libinput_event_keyboard_get_key_state.argtypes = [ctypes.c_void_p]

# libinput event type for keyboard
LIBINPUT_EVENT_KEYBOARD_KEY = 300

# libinput key state
LIBINPUT_KEY_STATE_RELEASED = 0
LIBINPUT_KEY_STATE_PRESSED = 1

# Minimal libinput interface (open_restricted / close_restricted callbacks)
_OPEN_RESTRICTED_FUNC = ctypes.CFUNCTYPE(ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_void_p)
_CLOSE_RESTRICTED_FUNC = ctypes.CFUNCTYPE(None, ctypes.c_int, ctypes.c_void_p)

def _open_restricted(path, flags, user_data):
    import os
    fd = os.open(path.decode(), flags)
    return fd

def _close_restricted(fd, user_data):
    import os
    os.close(fd)

class _LibinputInterface(ctypes.Structure):
    _fields_ = [
        ("open_restricted", _OPEN_RESTRICTED_FUNC),
        ("close_restricted", _CLOSE_RESTRICTED_FUNC),
    ]

_interface_instance = _LibinputInterface(
    open_restricted=_OPEN_RESTRICTED_FUNC(_open_restricted),
    close_restricted=_CLOSE_RESTRICTED_FUNC(_close_restricted),
)


class WaylandKeyListener(KeyListener):
    """
    Listens to keyboard events from libinput on seat0.
    """

    def __init__(self):
        super().__init__()
        self._stop_event = threading.Event()

        # Set up udev
        self._udev = _libudev.udev_new()
        if not self._udev:
            raise RuntimeError("Failed to create udev context")

        # Set up libinput context
        self._li = _libinput.libinput_udev_create_context(
            ctypes.byref(_interface_instance),
            None,
            self._udev,
        )
        if not self._li:
            _libudev.udev_unref(self._udev)
            raise RuntimeError("Failed to create libinput context")

        # Assign seat
        ret = _libinput.libinput_udev_assign_seat(self._li, b"seat0")
        if ret != 0:
            _libinput.libinput_unref(self._li)
            _libudev.udev_unref(self._udev)
            raise RuntimeError("Failed to assign seat 'seat0' to libinput context")

        self._fd = _libinput.libinput_get_fd(self._li)

        # Spawn background thread to poll and enqueue events
        self._thread = threading.Thread(target=self._poll_loop, daemon=True)
        self._thread.start()

    def __del__(self):
        self._stop_event.set()
        self._thread.join()
        _libinput.libinput_unref(self._li)
        _libudev.udev_unref(self._udev)


    def _poll_loop(self):
        """Background thread: poll libinput fd and enqueue key events."""
        while not self._stop_event.is_set():
            r, _, _ = select.select([self._fd], [], [], 0.1)
            if not r:
                continue

            _libinput.libinput_dispatch(self._li)

            while True:
                event = _libinput.libinput_get_event(self._li)
                if not event:
                    break

                event_type = _libinput.libinput_event_get_type(event)

                if event_type == LIBINPUT_EVENT_KEYBOARD_KEY:
                    kb_event = _libinput.libinput_event_get_keyboard_event(event)
                    key_code = _libinput.libinput_event_keyboard_get_key(kb_event)
                    key_state = _libinput.libinput_event_keyboard_get_key_state(kb_event)
                    pressed = (key_state == LIBINPUT_KEY_STATE_PRESSED)
                    if key_code in KEYCODE_TO_STR:
                        self.key_event.emit((KEYCODE_TO_STR[key_code], pressed))

                _libinput.libinput_event_destroy(event)

class WaylandBackend(Backend):
    """Linux-specific backend implementation using uinput."""

    def __init__(self):
        # Get screen resolution using Qt
        # Ensure a QApplication instance exists
        app = QApplication.instance()
        if app is None:
            app = QApplication([])
        screen = app.primaryScreen()
        size = screen.size()
        touch_max_x = size.width()
        touch_max_y = size.height()

        dev = libevdev.Device()
        dev.name = "ClickAssist Virtual Touchscreen"

        dev.enable(libevdev.EV_KEY.BTN_TOUCH)

        dev.enable(
            libevdev.EV_ABS.ABS_X,
            libevdev.InputAbsInfo(minimum=0, maximum=touch_max_x),
        )
        dev.enable(
            libevdev.EV_ABS.ABS_Y,
            libevdev.InputAbsInfo(minimum=0, maximum=touch_max_y),
        )
        dev.enable(
            libevdev.EV_ABS.ABS_MT_POSITION_X,
            libevdev.InputAbsInfo(minimum=0, maximum=touch_max_x),
        )
        dev.enable(
            libevdev.EV_ABS.ABS_MT_POSITION_Y,
            libevdev.InputAbsInfo(minimum=0, maximum=touch_max_y),
        )
        dev.enable(
            libevdev.EV_ABS.ABS_MT_SLOT,
            libevdev.InputAbsInfo(minimum=0, maximum=9),
        )
        dev.enable(
            libevdev.EV_ABS.ABS_MT_TRACKING_ID,
            libevdev.InputAbsInfo(minimum=0, maximum=65535),
        )

        dev.enable(libevdev.INPUT_PROP_DIRECT)

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
        raise NotImplementedError()

    def create_key_listener(self) -> AbstractKeyListener:
        return WaylandKeyListener()

class KDEWaylandBackend(WaylandBackend):
    def __init__(self):
        import shutil
        if shutil.which("kdotool") is None:
            raise RuntimeError("kdotool not found in PATH. Please install kdotool for KDE Wayland support.")

    def get_cursor_pos(self) -> QPoint:
        # Run kdotool getmouselocation and parse output
        import subprocess
        import re
        try:
            output = subprocess.check_output(
                ["kdotool", "getmouselocation"],
                stderr=subprocess.DEVNULL,
                text=True
            )
        except subprocess.CalledProcessError:
            raise RuntimeError("Failed to get mouse location via kdotool")

        # Expected output format: "x:605 y:437 screen:0 window:..."
        match = re.search(r'x:(\d+)\s+y:(\d+)', output)
        if not match:
            raise RuntimeError(f"Could not parse kdotool output: {output}")
        x = int(match.group(1))
        y = int(match.group(2))
        return QPoint(x, y)
