"""Источник «папка на диске»: выгрузка Takeout, архив учителя, локальная копия диска.

Изменений от API тут нет — движок сам сравнивает снимок папки с состоянием по хешу.
"""
import hashlib
import os
from pathlib import Path

SKIP_DIRS = {"Корзина", "Trash", ".git", "__pycache__", "node_modules", ".venv", "venv"}
SKIP_NAMES = {"desktop.ini", ".DS_Store", "Thumbs.db"}


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


class LocalFolderSource:
    name = "local"
    incremental = False          # изменений от источника нет, движок сравнивает сам

    def __init__(self, root):
        self.root = Path(root).expanduser().resolve()

    def full_scan(self, state=None):
        for dirpath, dirnames, filenames in os.walk(self.root):
            dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
            for name in filenames:
                if name in SKIP_NAMES:
                    continue
                full = Path(dirpath) / name
                if full.is_symlink() or not full.is_file():
                    continue
                st = full.stat()
                rel = full.relative_to(self.root).as_posix()
                yield {
                    "id": rel,
                    "name": name,
                    "path": rel,
                    "local_path": str(full),
                    "size": st.st_size,
                    "mtime": int(st.st_mtime),
                    "hash": sha256(full),
                }

    def changes(self, cursor, state):
        raise NotImplementedError("локальная папка отдаёт снимок, а не изменения")

    def fetch(self, item):
        """Байты файла: понадобится, когда события начнут импортироваться."""
        return Path(item["local_path"]).read_bytes()
