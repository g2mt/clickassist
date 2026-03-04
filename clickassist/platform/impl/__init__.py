import sys

if sys.platform == "win32":
    from .windows import WindowsBackend
    OSBackend = WindowsBackend
else:
    from .wayland import WaylandBackend
    OSBackend = WaylandBackend
