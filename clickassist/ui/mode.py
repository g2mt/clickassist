from enum import Enum, auto

class Mode(Enum):
    """Represents the current active mode of the application."""
    ACTIVE = auto()      # Keybinds running / tray mode
    NORMAL = auto()      # No special mode active
    RECORDING = auto()   # Recording a new keybind
    MOVE = auto()        # Moving a bound position
    DELETE = auto()      # Deleting a bound position
