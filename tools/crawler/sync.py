"""Синхронизация источника с платформой: не «скачать всё», а «что изменилось».

Главная функция краулера — замечать новое, изменённое и удалённое, а не переливать папку
целиком. Состояние хранится между запусками, поэтому второй прогон по той же папке даёт
ноль событий, а не 700 «новых» файлов.

    python3 -m tools.crawler sync "<папка>" --state state/drive.json --out out/sync

Выход — events.jsonl (created / updated / deleted) и сводка. Дальше эти события уходят
в импорт (в бэкенд) и в уведомления ученикам; сам краулер ничего не публикует.
"""
import hashlib
import json
import os
import time
from pathlib import Path

from tools.drive_ingest.classify import classify
from tools.drive_ingest.extract import extract
from tools.drive_ingest.source import LocalSource

SKIP_DIRS = {"Корзина", "Trash", ".git", "__pycache__", "node_modules", ".venv", "venv"}
SKIP_NAMES = {"desktop.ini", ".DS_Store", "Thumbs.db"}


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


class SyncState:
    """Что мы уже видели: по каждому файлу — отпечаток и время изменения.

    Ключ — внешний идентификатор (для папки это относительный путь; у Drive будет fileId).
    Хранится в JSON, чтобы падение не теряло прогресс и следующий запуск продолжил.
    """

    def __init__(self, path):
        self.path = Path(path)
        self.data = {"cursor": None, "items": {}}
        if self.path.exists():
            try:
                self.data = json.loads(self.path.read_text(encoding="utf-8"))
            except (ValueError, OSError):
                pass

    def get(self, key):
        return self.data["items"].get(key)

    def put(self, key, value):
        self.data["items"][key] = value

    def drop(self, key):
        self.data["items"].pop(key, None)

    def save(self, cursor=None):
        if cursor is not None:
            self.data["cursor"] = cursor
        self.path.parent.mkdir(parents=True, exist_ok=True)
        tmp = self.path.with_suffix(".tmp")
        tmp.write_text(json.dumps(self.data, ensure_ascii=False), encoding="utf-8")
        tmp.replace(self.path)          # атомарно: недописанное состояние не читается


def scan_local(root):
    """Все файлы папки: id = относительный путь, отпечаток = размер + mtime + sha256."""
    root = Path(root).expanduser().resolve()
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
        for name in filenames:
            if name in SKIP_NAMES:
                continue
            full = Path(dirpath) / name
            if full.is_symlink() or not full.is_file():
                continue
            st = full.stat()
            yield {
                "id": full.relative_to(root).as_posix(),
                "path": full,
                "name": name,
                "size": st.st_size,
                "mtime": int(st.st_mtime),
                "sha256": sha256(full),
            }


def diff(current, state):
    """Сравнить то, что нашли, с тем, что уже видели. Возвращает события."""
    events, seen = [], set()
    for item in current:
        key = item["id"]
        seen.add(key)
        old = state.get(key)
        if old is None:
            events.append({"kind": "created", "item": item})
        elif old["sha256"] != item["sha256"]:
            events.append({"kind": "updated", "item": item, "was": old["sha256"][:12]})
        elif old.get("mtime") != item["mtime"]:
            # коснулись файла, но содержимое то же: обновляем время, событие не шлём
            state.put(key, _fingerprint(item))
    for key in list(state.data["items"]):
        if key not in seen:
            events.append({"kind": "deleted", "item": {"id": key, "name": Path(key).name}})
            state.drop(key)
    return events


def _fingerprint(item):
    return {"sha256": item["sha256"], "size": item["size"], "mtime": item["mtime"]}


def enrich(item, root):
    """Что о файле нужно платформе: предмет, класс, тип, текстовый слой."""
    text, text_status = extract(item["path"])
    meta = classify(item["id"], item["name"], text)
    return {
        "id": item["id"],
        "name": item["name"],
        "size": item["size"],
        "mtime": item["mtime"],
        "sha256": item["sha256"],
        "text_status": text_status,
        "text_chars": len(text),
        **meta,
        "local_path": str(item["path"]) if root else None,
    }


def sync(root, state_path, out_dir, with_text=True):
    state = SyncState(state_path)
    t0 = time.time()
    events = diff(scan_local(root), state)
    out = Path(out_dir)
    out.mkdir(parents=True, exist_ok=True)

    rows = []
    with open(out / "events.jsonl", "w", encoding="utf-8") as f:
        for ev in events:
            item = ev["item"]
            row = {"kind": ev["kind"], "source": "local"}
            if ev["kind"] != "deleted":
                row.update(enrich(item, root) if with_text else
                           {"id": item["id"], "name": item["name"], "size": item["size"],
                            "sha256": item["sha256"]})
                state.put(item["id"], _fingerprint(item))
            else:
                row.update({"id": item["id"], "name": item["name"]})
            rows.append(row)
            f.write(json.dumps(row, ensure_ascii=False) + "\n")

    state.save(cursor=int(time.time()))
    write_summary(out / "summary.md", rows, time.time() - t0)
    return rows, time.time() - t0


def write_summary(path, rows, dur):
    kinds = {}
    for r in rows:
        kinds[r["kind"]] = kinds.get(r["kind"], 0) + 1
    lines = [f"# Синхронизация\n",
             f"- событий: **{len(rows)}** за {dur:.1f} с",
             f"- новых: **{kinds.get('created', 0)}**, изменённых: **{kinds.get('updated', 0)}**, "
             f"удалённых: **{kinds.get('deleted', 0)}**", ""]
    created = [r for r in rows if r["kind"] == "created"]
    if created:
        unsure = [r for r in created if r.get("subject_conf") != "high"]
        lines.append(f"- из новых требуют человека (неясен предмет): **{len(unsure)}**")
        for r in unsure[:20]:
            lines.append(f"  - `{r['id']}`")
    Path(path).write_text("\n".join(lines) + "\n", encoding="utf-8")
