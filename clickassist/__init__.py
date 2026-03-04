"""Entry point for Click Assistant."""

import sys

from PySide6.QtWidgets import QApplication

from clickassist.platform.backend import OSBackend
from clickassist.ui.main_window import MainWindow


def main():
    app = QApplication(sys.argv)
    app.setQuitOnLastWindowClosed(False)

    window = MainWindow(OSBackend())
    window.show()
    sys.exit(app.exec())


if __name__ == "__main__":
    main()
