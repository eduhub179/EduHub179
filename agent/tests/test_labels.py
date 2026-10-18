"""Метки: подсказки → правила по имени → модель на остаток. Без модели и без сети.

python3 tests/test_labels.py
"""
import json
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))

from eduhub_agent.files import hints, labels, name_rules, queue, taxonomy  # noqa: E402

fails = 0


def check(name, cond, detail=""):
    global fails
    print(f"{'OK  ' if cond else 'FAIL'} {name}{(' :: ' + str(detail)) if detail else ''}")
    fails += not cond


# --- предметы --------------------------------------------------------------------------
check("физика по подписи", taxonomy.code_for("физика к пятнице §12") == "physics")
check("матан → calculus", taxonomy.code_for("Матан 9 класс") == "calculus")
check("ОПЦТ → digital_tech", taxonomy.code_for("ОПЦТ 7Б") == "digital_tech")
check("неизвестный предмет — None, а не догадка", taxonomy.code_for("Математика 5 класс") is None)
check("класс из «11Б»", taxonomy.grade_from("Информатика 2027Б 11Б") == 11)
check("12 — не класс", taxonomy.grade_from("лист12д") is None)

# --- подсказки: подпись важнее чата, чат важнее пути -----------------------------------
h = hints.collect(caption="физика к пятнице", chat="Алгебра 9Б", path="Геометрия/8 класс/x.pdf")
check("подпись главнее", (h["subject"], h["source"]) == ("physics", "caption"), h)
h = hints.collect(chat="Физика 11б", path="Алгебра/9 класс/x.pdf")
check("чат главнее пути", h["subject"] == "physics", h)
h = hints.collect(path="Алгебра/9 класс/ДЗ-5.pdf")
check("путь даёт предмет и класс", (h["subject"], h["grade"]) == ("algebra", 9), h)
h = hints.collect(caption="домашнее задание", path="Алгебра/9 класс/x.pdf")
check("из подписи берём только то, что есть", (h["subject"], h["source"]) == ("algebra", "path"), h)

# --- тип по имени ----------------------------------------------------------------------
check("ДЗ → homework", name_rules.type_of("ДЗ-15.pdf")[0] == "homework")
check("КР → test", name_rules.type_of("КР-2 по геометрии.pdf")[0] == "test")
check("«Листок 16. Самостоятельная работа» → самостоятельная",
      name_rules.type_of("Листок 16. Самостоятельная работа.docx")[0] == "independent_work")
check("лист → worksheet", name_rules.type_of("лист 03.pdf")[0] == "worksheet")
check("«кружок» не становится контрольной", name_rules.type_of("кружок.txt")[0] is None)
check(".py → program", name_rules.type_of("script.py")[0] == "program")
check("безымянное фото — ничего", name_rules.type_of("photo_2026-10-18.jpg")[0] is None)

# --- маршрут меток ---------------------------------------------------------------------
calls = []


def spy(*a, **kw):
    calls.append(a[1] if len(a) > 1 else "")
    return {"subject": "physics", "type": "test", "lesson": 3, "date": None,
            "subject_conf": "high", "type_conf": "high", "confidence": "high", "engine": "spy"}


m = labels.label("Алгебра/9 класс/ДЗ-15.pdf", "ДЗ-15.pdf", "текст", classifier=spy)
check("подсказка + правило → без модели", calls == [], calls)
check("метки из подсказки и правила", (m["subject"], m["type"], m["grade"]) == ("algebra", "homework", 9), m)
check("input_mode heuristic", m["input_mode"] == "heuristic", m["input_mode"])

m = labels.label("Разное/скан.pdf", "скан.pdf", "", classifier=spy)
check("неизвестно → модель зовётся", len(calls) == 1, calls)
check("модель даёт тип", m["type"] == "test", m["type"])
check("input_mode model", m["input_mode"] == "model", m["input_mode"])

def spy_algebra(*a, **kw):
    return {"subject": "algebra", "type": "worksheet", "lesson": None, "date": None,
            "subject_conf": "high", "type_conf": "high", "confidence": "high", "engine": "spy"}


m = labels.label("Алгебра/9 класс/скан.pdf", "скан.pdf", "", caption="физика к пятнице",
                 classifier=spy_algebra)
check("подпись главнее модели по предмету", m["subject"] == "physics", m["subject"])
check("конфликт с моделью записан", m.get("subject_conflict") == "algebra", m.get("subject_conflict"))

# --- очередь ---------------------------------------------------------------------------
td = Path(tempfile.mkdtemp())
queue.QUEUE = td / "q.jsonl"
queue.LABELS = td / "l.jsonl"
check("постановка в очередь", queue.enqueue("f1", "ДЗ-1.pdf", path=None) is True)
check("повторно не ставится", queue.enqueue("f1", "ДЗ-1.pdf", path=None) is False)
check("в очереди один", len(queue.pending()) == 1, len(queue.pending()))

st = queue.process(limit=10, classifier=spy, with_text=False)
check("файл без пути → review", st["review"] == 1, st)
check("после обработки очередь пуста", queue.pending() == [])


def boom(*a, **kw):
    raise RuntimeError("модель упала")


queue.QUEUE = td / "q2.jsonl"
queue.LABELS = td / "l2.jsonl"
queue.enqueue("f2", "непонятное.pdf", path=None)
st = queue.process(limit=10, classifier=boom, with_text=False)
check("падение модели → failed, файл остаётся", st["failed"] == 1 and len(queue.pending()) == 1, (st, len(queue.pending())))
rows = [json.loads(l) for l in open(queue.LABELS, encoding="utf-8")]
check("попытка записана", rows[-1]["status"] == "failed" and rows[-1]["attempts"] == 1, rows[-1])

print(f"\n{'FAILED' if fails else 'all good'} — {fails} failure(s)")
sys.exit(1 if fails else 0)
