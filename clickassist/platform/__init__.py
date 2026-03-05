import sys

from clickassist.platform.backend import Backend

def get_backend() -> Backend:
    if sys.platform == "win32":
        from .impl.windows import WindowsBackend
        return WindowsBackend
    else:
        from .impl.wayland import WaylandBackend
        return WaylandBackend
