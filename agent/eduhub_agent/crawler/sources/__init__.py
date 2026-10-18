"""Источники краулера: папка на диске и Google Drive."""
from .local import LocalFolderSource
from .drive import DriveSource

__all__ = ["LocalFolderSource", "DriveSource"]
