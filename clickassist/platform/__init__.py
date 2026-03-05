import sys
import os

from clickassist.platform.backend import Backend

def get_backend() -> Backend:
    if sys.platform == "win32":
        from .impl.windows import WindowsBackend
        return WindowsBackend
    else:
        from .impl.wayland import WaylandBackend, KDEWaylandBackend
        XDG_CURRENT_DESKTOP = os.getenv("XDG_CURRENT_DESKTOP")
        if XDG_CURRENT_DESKTOP == "KDE":
            return KDEWaylandBackend
        return WaylandBackend
