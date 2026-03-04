import sys

from clickassist.platform.backend import AbstractBackend

def get_backend() -> AbstractBackend:
    if sys.platform == "win32":
        from .impl.windows import WindowsBackend
        return WindowsBackend
    else:
        from .impl.wayland import WaylandBackend
        return WaylandBackend
