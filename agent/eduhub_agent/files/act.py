"""Что делать с вердиктом decide(): отдать файл в бэкенд или человеку.

Очередь разбирала файл и складывала вердикт в `state/file_labels.jsonl`, но читать его было
некому: `accepted` никуда не шёл. Здесь последний шаг — тот же, что у находок из чатов:
урок по предмету и дате, `POST /api/lessons/{id}/files`.

Правила (по вердикту, а не по тексту сообщения):

  accepted + confidence=high  → загружаем в бэкенд (это и есть «действуем по классификации»).
                                decide() ставит accepted только при high, так что гейт один.
  confirmed                   → человек уже подтвердил метки (черновик в ЛС): грузим даже
                                medium — это и есть «уже модерировано».
  review                      → `state/review.jsonl`: нужен человек (нет даты, нет класса,
                                метки неполные, Drive в режиме ссылок)
  queued / skip               → не наше дело: OCR-очередь и бинарники живут без бэкенда

Чего вердикту не хватает. У метки есть предмет и тип, но нет урока: дата берётся из подписи
или черновика, класс — из профиля (ЛС) или из названия чата/пути папки. Поэтому `publish()`
принимает контекст (`class_name`, `date`), а строка без него уходит в review, а не угадывается.

Идемпотентность. `state/file_actions.jsonl` помнит (id файла, урок): повторный прогон не
грузит одно и то же дважды. Сеть/500 — файл ждёт следующего прогона; 413/415/422 — бэкенд не
примет его никогда, помечаем `refused` и больше не пробуем.
"""
import argparse
import json
import time
from pathlib import Path

from eduhub_agent.files import due, taxonomy
from eduhub_agent.paths import STATE
from eduhub_agent.server import backend_api

LABELS = STATE / "file_labels.jsonl"
ACTIONS = STATE / "file_actions.jsonl"
REVIEW = STATE / "review.jsonl"

REFUSED = (413, 415, 422)   # слишком большой, не тот тип, плохое имя: повтор не поможет
ROUTES = ("accepted", "review", "queued", "skip")


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


def _write(path, row):
    STATE.mkdir(exist_ok=True)
    with open(path, "a", encoding="utf-8") as f:
        f.write(json.dumps(row, ensure_ascii=False) + "\n")


def record(row):
    """Записать готовую метку (её пишут feed.py из crawler/ingest и queue.process)."""
    _write(LABELS, {**row, "status": "done"})


def labels():
    """{id файла: последняя удачная метка}. Неудачи в очереди остаются в очереди, здесь их нет."""
    out = {}
    for row in _rows(LABELS):
        if row.get("status") == "done":
            out[row["id"]] = row
    return out


def acted():
    """{(id файла, урок): запись} — что уже загружено или окончательно отказано."""
    out = {}
    for row in _rows(ACTIONS):
        if row.get("status") in ("ok", "refused"):
            out[(row["id"], row.get("lesson"))] = row
    return out


def target(row, class_name=None, date=None):
    """(находка для backend_api, почему не годится). Находка — то, что понимает find_lesson().

    Дата: из метки (модель прочитала «к пятнице»), из явного --date, а если их нет — по
    расписанию от времени появления файла (files/due.py: та же логика, что у чатов).
    Не угадываем предмет и класс: без них — review.
    """
    if not row.get("path"):
        return None, "нет файла на диске"
    subject = taxonomy.ru_name(row.get("subject"))
    if not subject:
        return None, "предмет не опознан"
    cls = row.get("class") or class_name
    if not cls:
        return None, "нет класса: ни в профиле, ни в подписи, ни в пути"
    day = row.get("date") or date
    source = "метка" if day else None
    if not day:
        day, _lesson, why = due.due_for({**row, "class": cls}, subject=subject)
        if not day:
            return None, why
        source = "расписание"
    return {"subject": subject, "date": str(day)[:10], "class": cls, "date_source": source,
            "chat": row.get("chat") or "", "file_paths": [row["path"]]}, None


def _not_delivered(row, why, status="review"):
    """Записать файл в «посмотреть глазами». Один и тот же файл с той же причиной — один раз:
    иначе каждый прогон добавлял бы строку, и список review рос бы сам по себе."""
    if (row["id"], why) in {(r["id"], r.get("why")) for r in _rows(REVIEW)}:
        return False
    _write(REVIEW, {"id": row["id"], "name": row.get("name"), "subject": row.get("subject"),
                    "type": row.get("type"), "confidence": row.get("confidence"),
                    "route": row.get("route"), "why": why, "status": status,
                    "path": row.get("path"), "ts": int(time.time())})
    return True


def publish(class_name=None, date=None, by=None, limit=50, dry_run=True, confirmed=()):
    """Отдать бэкенду всё, что классификатор принял уверенно. -> счётчики для лога.

    dry_run=True (по умолчанию) ничего не грузит: печатает, что бы ушло. Боевой прогон —
    `python3 -m eduhub_agent.files.act --apply`.
    confirmed — id файлов, метки которых человек уже подтвердил в черновике ЛС.
    """
    stats = {"uploaded": 0, "waiting": 0, "review": 0, "refused": 0, "skipped": 0, "planned": 0}
    done, sent = labels(), acted()
    for fid, row in list(done.items())[:limit]:
        route = row.get("route")
        if route not in ROUTES:
            stats["skipped"] += 1
            continue
        if route == "skip":
            stats["skipped"] += 1
            continue
        if route == "queued":            # OCR-очередь: у неё свой владелец
            stats["skipped"] += 1
            continue
        if route == "review" and fid not in confirmed:
            if _not_delivered(row, row.get("route_reason") or "метки не подтверждены"):
                stats["review"] += 1
            else:
                stats["skipped"] += 1
            continue
        it, why = target(row, class_name=class_name, date=date)
        if not it:
            stats["review" if _not_delivered(row, why) else "skipped"] += 1
            continue
        lesson, why = backend_api.find_lesson(it)
        if not lesson:
            stats["review" if _not_delivered(row, why) else "skipped"] += 1
            continue
        if (fid, lesson["id"]) in sent:
            stats["skipped"] += 1
            continue
        if dry_run:
            print(f"   бы ушло: {row.get('name')} -> {lesson['subject_name']} {lesson['start_time']} "
                  f"{lesson['_date'] if '_date' in lesson else it['date']}")
            stats["planned"] += 1
            continue
        try:
            up = backend_api.upload(lesson["id"], it["file_paths"][0], by=by)
            _write(ACTIONS, {"id": fid, "lesson": lesson["id"], "homework_id": up.get("homework_id"),
                             "name": row.get("name"), "status": "ok", "ts": int(time.time())})
            stats["uploaded"] += 1
            print(f"   файл {row.get('name')} -> урок {it['date']} {lesson['start_time']} {lesson['subject_name']}")
        except backend_api.ApiError as e:
            if e.status in REFUSED:
                _write(ACTIONS, {"id": fid, "lesson": lesson["id"], "status": "refused",
                                 "why": str(e), "ts": int(time.time())})
                stats["refused"] += 1
            else:
                print(f"   бэкенд не принял {row.get('name')}: {e} — попробуем в следующий раз")
                stats["waiting"] += 1
        except OSError as e:
            print(f"   файл {row.get('name')} не прочитался: {e} — попробуем в следующий раз")
            stats["waiting"] += 1
    return stats


def review_rows():
    """Что человек должен посмотреть глазами: state/review.jsonl без повторов."""
    seen, out = set(), []
    for row in _rows(REVIEW):
        if row["id"] in seen:
            continue
        seen.add(row["id"])
        out.append(row)
    return out


def main(argv=None):
    p = argparse.ArgumentParser(description="Отдать бэкенду файлы, которые классификатор принял уверенно.")
    p.add_argument("--class", dest="class_name", help="класс, если его нет в метке: 11Б")
    p.add_argument("--date", help="дата урока, если её нет в метке: 2026-10-27")
    p.add_argument("--on-behalf", dest="by", help="uuid пользователя, от чьего имени пишем (X-Agent-On-Behalf-Of)")
    p.add_argument("--limit", type=int, default=50)
    p.add_argument("--apply", action="store_true", help="грузить по-настоящему (по умолчанию только показать)")
    a = p.parse_args(argv)
    stats = publish(class_name=a.class_name, date=a.date, by=a.by, limit=a.limit, dry_run=not a.apply)
    print("итого: " + ", ".join(f"{k} {v}" for k, v in stats.items() if v))
    for row in review_rows():
        print(f"   глазами: {row.get('name')} — {row.get('why')}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
