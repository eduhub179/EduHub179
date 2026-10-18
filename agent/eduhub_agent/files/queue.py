"""Очередь классификации: файл сначала принимаем, метки считаем потом.

Живой бот не может ждать 10-14 секунд на файл, поэтому:

  enqueue(...)  — файл поставлен в очередь, сообщение уже ушло пользователю
  process(...)  — воркер: читает файл, метит, решает маршрут, пишет результат

Данные лежат двумя JSONL в state/:
  file_queue.jsonl   что просили разобрать
  file_labels.jsonl  что получилось (включая неудачи с числом попыток)

Ожидающие = очередь минус уже обработанные id. Повторный запуск ничего не дублирует,
а упавшая модель оставляет файл в очереди до трёх попыток — это и есть смысл очереди:
«пока не смогли» ≠ «потеряли».
"""
import json
import time
from pathlib import Path

from eduhub_agent.files.decide import decide
from eduhub_agent.files.labels import label
from eduhub_agent.files.read import read
from eduhub_agent.paths import STATE

QUEUE = STATE / "file_queue.jsonl"
LABELS = STATE / "file_labels.jsonl"
MAX_ATTEMPTS = 3


def _rows(path):
    if not path.exists():
        return []
    out = []
    for line in open(path, encoding="utf-8"):
        try:
            out.append(json.loads(line))
        except ValueError:
            continue
    return out


def enqueue(file_id, name, source="unknown", path=None, link=None, caption=None,
            chat=None, chat_subjects=None, chat_class=None):
    """Поставить файл в очередь. Тот же id второй раз не добавляется."""
    STATE.mkdir(exist_ok=True)
    for row in _rows(QUEUE):
        if row["id"] == file_id:
            return False
    row = {"id": file_id, "name": name, "source": source, "path": path, "link": link,
           "caption": caption, "chat": chat, "chat_subjects": chat_subjects,
           "chat_class": chat_class, "ts": int(time.time())}
    with open(QUEUE, "a", encoding="utf-8") as f:
        f.write(json.dumps(row, ensure_ascii=False) + "\n")
    return True


def done_ids():
    return {r["id"]: r for r in _rows(LABELS) if r.get("status") == "done"}


def pending(limit=None):
    """Записи, которые ещё не разобраны и не исчерпали попытки."""
    done = done_ids()
    failed = {r["id"]: r.get("attempts", 1) for r in _rows(LABELS) if r.get("status") == "failed"}
    out = []
    seen = set()
    for row in _rows(QUEUE):
        fid = row["id"]
        if fid in done or fid in seen:
            continue
        if failed.get(fid, 0) >= MAX_ATTEMPTS:
            continue
        seen.add(fid)
        out.append(row)
    return out[:limit] if limit else out


def _write(row):
    STATE.mkdir(exist_ok=True)
    with open(LABELS, "a", encoding="utf-8") as f:
        f.write(json.dumps(row, ensure_ascii=False) + "\n")


def process(limit=50, classifier=None, with_text=True):
    """Разобрать до limit файлов. Возвращает счётчики для лога."""
    stats = {"accepted": 0, "queued": 0, "review": 0, "skip": 0, "failed": 0}
    for row in pending(limit):
        fid = row["id"]
        try:
            r = read(row["path"]) if (with_text and row.get("path")) else None
            status = r.status if r else "not_downloaded"
            text = r.text if r else ""
            meta = label(row.get("path") or fid, row["name"], text,
                         caption=row.get("caption"), chat=row.get("chat"),
                         chat_subjects=row.get("chat_subjects"), chat_class=row.get("chat_class"),
                         classifier=classifier)
            route, reason = decide(status, meta)
            _write({"id": fid, "status": "done", "text_status": status,
                    "text_chars": len(text), "route": route, "route_reason": reason,
                    "input_mode": "text" if status in ("native", "ocr") else "name_only",
                    **meta, "ts": int(time.time())})
            stats[route] += 1
        except Exception as e:                     # модель упала — попробуем позже
            prev = [x for x in _rows(LABELS) if x["id"] == fid and x.get("status") == "failed"]
            attempts = (prev[-1].get("attempts", 0) if prev else 0) + 1
            _write({"id": fid, "status": "failed", "attempts": attempts,
                    "error": str(e)[:200], "ts": int(time.time())})
            stats["failed"] += 1
    return stats


def stats():
    return {"queued": len(pending()), "done": len(done_ids()),
            "failed": len({r["id"] for r in _rows(LABELS) if r.get("status") == "failed"})}
