"""Entry point for Click Assistant."""

import sys

from PySide6.QtWidgets import QApplication

from clickassist.utils.constants import PLATFORM_WINDOWS
from clickassist.platform.backend import WindowsBackend, WaylandBackend
from clickassist.ui.main_window import MainWindow


def main():
    app = QApplication(sys.argv)
    app.setQuitOnLastWindowClosed(False)

    # Create the appropriate backend for the platform
    if PLATFORM_WINDOWS:
        backend = WindowsBackend()
    else:
        backend = WaylandBackend()

    window = MainWindow(backend)
    window.show()
    sys.exit(app.exec())


if __name__ == "__main__":
    main()
