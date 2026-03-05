import sys

from PySide6.QtWidgets import QApplication

from clickassist.platform import get_backend
from clickassist.ui.main_window import MainWindow


def main():
    app = QApplication(sys.argv)
    app.setQuitOnLastWindowClosed(False)

    OSBackend = get_backend()
    window = MainWindow(OSBackend())
    window.show()
    sys.exit(app.exec())


if __name__ == "__main__":
    main()
