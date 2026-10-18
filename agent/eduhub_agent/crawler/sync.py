"""Движок синхронизации: один и тот же для папки и для Google Drive.

Источник отвечает на один из двух вопросов:
  * incremental=False → «вот снимок всего, что есть» (папка); движок сравнивает с состоянием;
  * incremental=True  → «вот что изменилось с токена» (Drive); движок применяет.

На выходе всегда один и тот же events.jsonl. Формат события не зависит от источника —
иначе импорт пришлось бы писать дважды.
"""
import json
import time
from pathlib import Path

from eduhub_agent.classify import classify
from eduhub_agent.files.decide import decide
from eduhub_agent.files.read import read
from eduhub_agent.ingest.grade import grade_of

from .state import SyncState


def fingerprint(item):
    return {"hash": item["hash"], "size": item.get("size", 0), "mtime": item.get("mtime"),
            "name": item.get("name", ""), "path": item.get("path", "")}


def diff_snapshot(items, state):
    """Сравнить снимок папки с тем, что уже видели."""
    events, seen = [], set()
    for item in items:
        key = item["id"]
        seen.add(key)
        old = state.get(key)
        if old is None:
            events.append({"kind": "created", "item": item})
            state.put(key, fingerprint(item))
        elif old.get("hash") != item["hash"]:
            events.append({"kind": "updated", "item": item})
            state.put(key, fingerprint(item))
        elif old.get("mtime") != item.get("mtime"):
            state.put(key, fingerprint(item))      # коснулись, содержимое то же — не событие
    for key in list(state.items()):
        if key not in seen:
            events.append({"kind": "deleted", "item": {"id": key, "name": Path(key).name}})
            state.drop(key)
    return events


def apply_changes(changes, state):
    """Применить ответ Drive. Google говорит «файл изменился», created/updated решаем сами."""
    events = []
    for ch in changes:
        if ch["kind"] == "deleted":
            old = state.get(ch["id"]) or {}
            events.append({"kind": "deleted", "item": {"id": ch["id"],
                                                       "name": ch.get("name") or old.get("name", ""),
                                                       "path": ch.get("id")}})
            state.drop(ch["id"])
            continue
        item = ch["item"]
        old = state.get(item["id"])
        if (old and old.get("hash") == item["hash"] and old.get("name") == item.get("name")
                and old.get("path") in (None, item.get("path"))):   # None — состояние старого формата
            continue                                # изменилось что-то третье: не событие
        events.append({"kind": "updated" if old else "created", "item": item})
        state.put(item["id"], fingerprint(item))
    return events


def event_row(kind, source, item, text_status="not_downloaded", text="", classifier=None,
              ocr_confidence=None, read_engine="none"):
    """Одна схема на все источники: импорт не должен ветвиться по source."""
    meta = (classifier or classify)(item.get("path") or item.get("name", ""), item.get("name", ""), text)
    meta = {**meta, "grade": grade_of(item.get("path") or item.get("id", ""), item.get("name", ""))}
    route, route_reason = decide(text_status, meta)
    return {
        "kind": kind,
        "source": source,
        "id": item["id"],
        "name": item.get("name", ""),
        "path": item.get("path") or item.get("id"),
        "size": item.get("size", 0),
        "mtime": item.get("mtime"),
        "hash": item.get("hash", ""),
        "text_status": text_status,
        "text_chars": len(text),
        "read_engine": read_engine,
        "ocr_confidence": ocr_confidence,
        # вариант 1: Drive отдаёт ссылку, а не файл — метки получены только по имени и пути
        "input_mode": "text" if text_status in ("native", "ocr") else "name_only",
        "route": route,
        "route_reason": route_reason,
        "link": item.get("link"),
        "local_path": item.get("local_path"),
        **meta,
    }


def enrich_local(item):
    """Для папки файл на диске — читаем общим read.py; для Drive в этом шаге не качаем."""
    if not item.get("local_path"):
        return "not_downloaded", "", None, "none"
    r = read(Path(item["local_path"]))
    return r.status, r.text, r.ocr_confidence, r.engine


def run(source, state_path, out_dir, classifier=None):
    state = SyncState(state_path)
    t0 = time.time()
    out = Path(out_dir)
    out.mkdir(parents=True, exist_ok=True)

    if source.incremental and state.cursor:
        changes, new_cursor = source.changes(state.cursor, state)
        events = apply_changes(changes, state)
        cursor_after = new_cursor
    else:
        # первый запуск Drive: сначала берём токен, потом обходим — иначе файл, добавленный
        # во время обхода, потеряется между «снимком» и «началом наблюдения».
        token = source.start_token() if getattr(source, "incremental", False) else None
        events = diff_snapshot(source.full_scan(state), state)
        cursor_after = token

    rows = []
    with open(out / "events.jsonl", "w", encoding="utf-8") as f:
        for ev in events:
            item = ev["item"]
            status, text, conf, engine = enrich_local(item)
            row = event_row(ev["kind"], source.name, item, status, text, classifier, conf, engine)
            rows.append(row)
            f.write(_json(row) + "\n")

    state.cursor = cursor_after
    state.save()                 # курсор двигается только здесь, после всех событий
    write_summary(out / "summary.md", rows, time.time() - t0, source.name)
    return rows, time.time() - t0


def _json(obj):
    return json.dumps(obj, ensure_ascii=False)


def sync_folder(root, state_path, out_dir, classifier=None):
    """Удобная обёртка для папки — ею пользуется CLI и тесты."""
    from .sources.local import LocalFolderSource
    return run(LocalFolderSource(root), state_path, out_dir, classifier)


def write_summary(path, rows, dur, source_name):
    kinds = {}
    for r in rows:
        kinds[r["kind"]] = kinds.get(r["kind"], 0) + 1
    lines = [f"# Синхронизация ({source_name})\n",
             f"- событий: **{len(rows)}** за {dur:.1f} с",
             f"- новых: **{kinds.get('created', 0)}**, изменённых: **{kinds.get('updated', 0)}**, "
             f"удалённых: **{kinds.get('deleted', 0)}**", ""]
    unsure = [r for r in rows if r["kind"] != "deleted" and r.get("subject_conf") != "high"]
    if unsure:
        lines.append(f"- требуют человека (неясен предмет): **{len(unsure)}**")
        for r in unsure[:20]:
            lines.append(f"  - `{r['path']}`")
    Path(path).write_text("\n".join(lines) + "\n", encoding="utf-8")
