"""Один файл через весь конвейер: что прочиталось, как размечено, что с ним будет.

    python3 -m eduhub_agent.files.check ~/Downloads/ДЗ-15.pdf
    python3 -m eduhub_agent.files.check photo.jpg --caption "физика к пятнице" --class 11Б
    python3 -m eduhub_agent.files.check file.pdf --no-model      # только бесплатные слои
    python3 -m eduhub_agent.files.check file.pdf --lesson        # + урок по расписанию (нужен API_URL)

Печатает то же, что решает живой конвейер, и ничего не отправляет:
  read()   — статус текста (native / ocr / scanned / binary / empty)
  label()  — предмет, тип, уверенность, откуда что взялось (подпись, имя, путь, модель)
  decide() — маршрут: accepted / queued / review / skip
  due()    — к какому уроку это попадёт (только с --lesson и настроенным бэкендом)

Это самый быстрый способ проверить классификатор: ни Telegram, ни очереди, ни бэкенда.
"""
import argparse
import json
import os
import sys

from eduhub_agent.config import env
from eduhub_agent.files import act
from eduhub_agent.files.decide import decide as route_of
from eduhub_agent.files.labels import label
from eduhub_agent.files.read import read


def check(path, caption=None, chat=None, chat_class=None, chat_subjects=None, model=True,
          lesson=False):
    """Прогнать файл и вернуть словарь с решением (его же печатает CLI)."""
    r = read(path)
    meta = label(path, os.path.basename(path), r.text, caption=caption, chat=chat,
                 chat_subjects=chat_subjects, chat_class=chat_class, model=model)
    route, why = route_of(r.status, meta)
    out = {
        "file": path,
        "read": {"status": r.status, "engine": r.engine, "chars": len(r.text or ""),
                 "pages": r.pages, "ocr_confidence": r.ocr_confidence},
        "label": {k: meta.get(k) for k in ("subject", "type", "grade", "confidence",
                                           "subject_conf", "type_conf", "subject_source",
                                           "type_source", "input_mode", "date", "lesson",
                                           "subject_conflict")},
        "route": route,
        "route_reason": why,
    }
    if lesson:
        if not env("API_URL"):
            out["lesson"] = "бэкенд не настроен: нужен API_URL (и AGENT_KEY) в .env"
        else:
            row = {"path": path, **meta, "name": os.path.basename(path)}
            if chat_class:
                row["class"] = chat_class
            it, why = act.target(row)
            out["lesson"] = it or f"не нашёл: {why}"
    return out


def show(res):
    """Человеческий вывод: одна строка на решение, без JSON-шума."""
    r, m = res["read"], res["label"]
    print(f"файл:    {res['file']}")
    print(f"чтение:  {r['status']} ({r['engine']}), символов {r['chars']}, страниц {r['pages']}"
          + (f", OCR {r['ocr_confidence']:.0f}" if r.get("ocr_confidence") else ""))
    print(f"метки:   предмет {m['subject']} ({m['subject_conf']}, {m['subject_source'] or '—'})"
          f" · тип {m['type']} ({m['type_conf']}, {m['type_source'] or '—'})"
          f" · класс {m['grade'] or '—'} · уверенность {m['confidence']}")
    if m.get("date") or m.get("lesson"):
        print(f"         из текста: дата {m['date'] or '—'}, урок {m['lesson'] or '—'}")
    if m.get("subject_conflict"):
        print(f"         внимание: модель сказала «{m['subject_conflict']}», оставили подсказку")
    print(f"решение: {res['route']} — {res['route_reason']}")
    if "lesson" in res:
        les = res["lesson"]
        print(f"урок:    " + (les if isinstance(les, str) else
                              f"{les['date']} · {les['class']} · {les['subject']} · {les.get('date_source') or '—'}"))


def main(argv=None):
    p = argparse.ArgumentParser(description="Проверить классификатор файла: чтение, метки, маршрут.")
    p.add_argument("file")
    p.add_argument("--caption", help="подпись под файлом (как в Telegram)")
    p.add_argument("--chat", help="название чата, если файл пришёл из чата")
    p.add_argument("--class", dest="chat_class", help="класс из профиля/пути: 11Б")
    p.add_argument("--subjects", help="предметы чата через запятую: «Физика,Алгебра»")
    p.add_argument("--no-model", action="store_true", help="только бесплатные слои: подсказки и имена")
    p.add_argument("--lesson", action="store_true", help="найти урок по расписанию (нужен API_URL)")
    p.add_argument("--json", action="store_true", help="машинный вывод целиком")
    a = p.parse_args(argv)
    subjects = [s.strip() for s in (a.subjects or "").split(",") if s.strip()]
    res = check(a.file, caption=a.caption, chat=a.chat, chat_class=a.chat_class,
                chat_subjects=subjects, model=not a.no_model, lesson=a.lesson)
    if a.json:
        print(json.dumps(res, ensure_ascii=False, indent=2))
    else:
        show(res)
        if not a.lesson:
            print("\nподсказка: с --lesson покажу, к какому уроку файл уйдёт (нужны API_URL и AGENT_KEY)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
