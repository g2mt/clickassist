import sys

from clickassist.platform.backend import AbstractBackend

def get_backend() -> AbstractBackend:
    if sys.platform == "win32":
        from .windows import WindowsBackend
        return WindowsBackend
    else:
        from .wayland import WaylandBackend
        return WaylandBackend
