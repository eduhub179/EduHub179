"""Откуда берём файлы: папка на диске сейчас, Google Drive потом.

Смысл в том, чтобы весь остальной код не знал разницы. Сейчас источник — выгрузка
Google Takeout (или просто папка с материалами). Когда появится доступ к дискам классов,
добавляется DriveSource, а scan/classify/copy остаются те же.

    python3 -m eduhub_agent.ingest scan /path/to/takeout/Диск --out manifest.jsonl
    python3 -m eduhub_agent.ingest scan --drive "1AbC...folderId" --out manifest.jsonl
"""
import os
from pathlib import Path

# Что не считаем материалом: служебные папки Google и мусор
SKIP_DIRS = {"Корзина", "Trash", ".git", "__pycache__", "node_modules", ".venv", "venv"}

# Файлы, которые Drive кладёт рядом и которые нам не нужны
SKIP_NAMES = {"desktop.ini", ".DS_Store", "Thumbs.db"}


class LocalSource:
    """Обычная папка: выгрузка Takeout, распакованный архив, диск учителя."""

    kind = "local"

    def __init__(self, root):
        self.root = Path(root).expanduser().resolve()

    def available(self):
        if not self.root.is_dir():
            return False, f"нет папки {self.root}"
        return True, "ok"

    def files(self):
        """(абсолютный путь, путь относительно корня) для всех файлов, кроме служебных."""
        for dirpath, dirnames, filenames in os.walk(self.root):
            dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
            for name in filenames:
                if name in SKIP_NAMES:
                    continue
                full = Path(dirpath) / name
                try:
                    if full.is_symlink() or not full.is_file():
                        continue
                except OSError:
                    continue
                yield full, full.relative_to(self.root).as_posix()

    def fetch(self, rel_path):
        """Байты файла. Для локальной папки — просто чтение."""
        return (self.root / rel_path).read_bytes()


class DriveSource:
    """Google Drive через service account.

    Не подключён: нужны ключ и доступ к дискам классов. Всё, что от него требуется, —
    отдать список файлов и байты по идентификатору; остальной код не меняется.

    Что понадобится, когда дадут доступ:
      1. service account в Google Cloud, включённый Drive API;
      2. папку класса расшарить на адрес этого аккаунта (или взять Shared Drive);
      3. переменные окружения GOOGLE_APPLICATION_CREDENTIALS и DRIVE_FOLDER_ID;
      4. пакет google-api-python-client (или прямой REST через urllib).
    """

    kind = "drive"

    def __init__(self, folder_id=None, credentials=None):
        self.folder_id = folder_id or os.environ.get("DRIVE_FOLDER_ID")
        self.credentials = credentials or os.environ.get("GOOGLE_APPLICATION_CREDENTIALS")

    def available(self):
        if not self.folder_id:
            return False, "нет DRIVE_FOLDER_ID"
        if not self.credentials:
            return False, "нет GOOGLE_APPLICATION_CREDENTIALS"
        return False, "клиент Drive ещё не написан (нужен доступ к дискам классов)"

    def files(self):
        raise NotImplementedError("DriveSource: см. docstring — нужен доступ к дискам классов")

    def fetch(self, rel_path):
        raise NotImplementedError("DriveSource: нужен доступ к дискам классов")


def open_source(path=None, drive_id=None):
    """Выбрать источник по аргументам командной строки."""
    if drive_id:
        return DriveSource(drive_id)
    if not path:
        raise SystemExit("нужен путь к папке или --drive <folder_id>")
    return LocalSource(path)
