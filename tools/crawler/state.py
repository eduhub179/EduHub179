"""Состояние синхронизации: курсор + отпечатки + кеш имён папок.

Пишется атомарно (tmp + replace): недописанное состояние после падения не должно
читаться как «мы всё видели».
"""
import json
from pathlib import Path


class SyncState:
    def __init__(self, path=None):
        self.path = Path(path) if path else None
        self.data = {"cursor": None, "items": {}, "folders": {}}
        if self.path and self.path.exists():
            try:
                self.data = json.loads(self.path.read_text(encoding="utf-8"))
            except (ValueError, OSError):
                pass
            self.data.setdefault("items", {})
            self.data.setdefault("folders", {})

    # --- отпечатки файлов ---------------------------------------------------------
    def get(self, key):
        return self.data["items"].get(key)

    def put(self, key, value):
        self.data["items"][key] = value

    def drop(self, key):
        self.data["items"].pop(key, None)

    def items(self):
        return self.data["items"]

    # --- имена папок (Drive отдаёт только id) -------------------------------------
    def folder(self, folder_id):
        """{"name": ..., "parents": [...]} или None. Строка из старых состояний читается как имя."""
        value = self.data["folders"].get(folder_id)
        if isinstance(value, str):
            return {"name": value, "parents": []}
        return value

    def put_folder(self, folder_id, name, parents=None):
        self.data["folders"][folder_id] = {"name": name, "parents": list(parents or [])}

    # --- курсор -------------------------------------------------------------------
    @property
    def cursor(self):
        return self.data.get("cursor")

    @cursor.setter
    def cursor(self, value):
        self.data["cursor"] = value

    def save(self):
        if not self.path:
            return
        self.path.parent.mkdir(parents=True, exist_ok=True)
        tmp = self.path.with_suffix(".tmp")
        tmp.write_text(json.dumps(self.data, ensure_ascii=False), encoding="utf-8")
        tmp.replace(self.path)
