"""Мост между разбором (crawler/ingest) и действием (act.py).

crawler и ingest уже прочитали файл, разметили его и выбрали маршрут — второй раз делать то
же самое в очереди незачем. Здесь их строки превращаются в ту же запись, которую пишет
`queue.process()`: `state/file_labels.jsonl`, откуда `act.publish()` берёт `accepted`.

Что нужно для действия: файл на диске (или ссылка, если Drive в режиме ссылок), предмет,
класс и время появления. Класс — самое слабое место: в событиях есть только параллель
(`grade`), буквы нет. Букву берём из пути («11Б/физика/…»), иначе класс передаётся снаружи
(`act publish --class 11Б`), иначе файл честно уходит в review, а не в чужой класс.

Повторный прогон не дублирует: помним (id, hash) уже записанных файлов.
"""
import os
import time
from pathlib import Path

from eduhub_agent.files import act, taxonomy


def class_from(*texts):
    """«11Б» из пути или имени. None, если буквы нигде нет: параллель — ещё не класс."""
    for text in texts:
        found = taxonomy.class_from(str(text or ""))
        if found:
            return found
    return None


def _known():
    return {row["id"]: row.get("hash") for row in act._rows(act.LABELS)}


def record(row, base=None, class_name=None):
    """Строка краулера или манифеста -> запись метки для act. True, если записали."""
    if row.get("kind") == "deleted":
        return False                      # удалённый файл некуда девать: его больше нет
    digest = row.get("sha256") or row.get("hash") or ""
    fid = row.get("id") or digest or row.get("path") or row.get("name")
    if _known().get(fid) == digest and digest:
        return False                      # тот же файл, тот же отпечаток: уже разобран
    path = row.get("local_path")
    if not path:
        rel = row.get("path") or row.get("name") or fid
        candidate = Path(base) / rel if base and not str(rel).startswith("/") else Path(rel)
        path = str(candidate) if candidate.exists() else (row.get("link") or str(candidate))
    act.record({
        "id": fid, "name": row.get("name") or Path(str(path)).name,
        "path": path, "hash": digest, "link": row.get("link"),
        "subject": row.get("subject"), "type": row.get("type"),
        "confidence": row.get("confidence"), "subject_conf": row.get("subject_conf"),
        "type_conf": row.get("type_conf"), "grade": row.get("grade"),
        "class": row.get("class") or class_name or class_from(path, row.get("name"), row.get("chat")),
        "chat": row.get("chat"), "caption": row.get("caption"),
        "text_status": row.get("text_status"), "route": row.get("route"),
        "route_reason": row.get("route_reason"),
        "date": row.get("date"),
        "seen_at": row.get("seen_at") or _iso(row.get("mtime")),
        "ts": int(time.time()),
    })
    return True


def _iso(value):
    """mtime приходит то секундами, то строкой ISO (Drive: modifiedTime)."""
    if value in (None, ""):
        return None
    try:
        return __import__("datetime").datetime.fromtimestamp(float(value)).isoformat(timespec="seconds")
    except (TypeError, ValueError):
        return str(value)[:19]


def from_rows(rows, base=None, class_name=None):
    """Записать пачку строк (crawler.run / ingest.scan). -> сколько записей добавилось."""
    return sum(1 for row in rows if record(row, base=base, class_name=class_name))
