from typing import Optional

from PySide6.QtWidgets import (
    QDialog, QVBoxLayout, QLabel, QPushButton, QDialogButtonBox, QLineEdit
)
from PySide6.QtCore import Qt

class KeybindDialog(QDialog):
    """Dialog that waits for the user to press a key and records it."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setWindowTitle("Press a key")
        self.setModal(True)
        self.key: Optional[str] = None

        layout = QVBoxLayout(self)

        self.label = QLabel("Press any key to bind...", self)
        self.label.setAlignment(Qt.AlignCenter)
        layout.addWidget(self.label)

        self.key_display = QLineEdit(self)
        self.key_display.setReadOnly(True)
        self.key_display.setPlaceholderText("No key bound")
        layout.addWidget(self.key_display)

        button_box = QDialogButtonBox(QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel, self)
        button_box.accepted.connect(self.accept)
        button_box.rejected.connect(self.reject)
        layout.addWidget(button_box)

    def keyPressEvent(self, event):
        key = event.key()
        if key in (Qt.Key.Key_unknown, Qt.Key.Key_Control, Qt.Key.Key_Shift,
                   Qt.Key.Key_Alt, Qt.Key.Key_Meta):
            return
        self.key = key.toString()
        self.key_display.setText(key.toString())
        self.accept()

    def exec(self) -> Optional[str]:
        if QDialog.exec_(self) != QDialog.Accepted:
            return None
        return self.key
