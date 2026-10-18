"""Проверка разбора папок без сети и без модели: python3 tools/drive_ingest/test_drive_ingest.py

Что закрепляем:
  * предмет берётся из папки, тип — из имени файла; «Разное» предметом не считается;
  * класс вытаскивается из «9 класс» и из «7Б»;
  * точные копии помечаются, но не удаляются;
  * в review.csv попадают только файлы с неясным предметом (это работа для человека);
  * copy раскладывает по <класс>/<предмет>/<тип> и не копирует копии.
"""
import csv
import json
import os
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent.parent))

from tools.drive_ingest.classify import classify, grade_of  # noqa: E402
from tools.drive_ingest.extract import extract  # noqa: E402
from tools.drive_ingest.source import LocalSource  # noqa: E402
from tools.drive_ingest.__main__ import copy_tree, scan  # noqa: E402

fails = 0


def check(name, cond, detail=""):
    global fails
    print(f"{'OK  ' if cond else 'FAIL'} {name}{(' :: ' + str(detail)) if detail else ''}")
    fails += not cond


# --- классификация ------------------------------------------------------------------
r = classify("Алгебра/дз/9 класс/ДЗ-004-к-12-10-2024.pdf", "ДЗ-004-к-12-10-2024.pdf")
check("предмет из папки", r["subject"] == "algebra", r["subject"])
check("тип из имени файла", r["type"] == "homework", r["type"])
check("класс из папки", r["grade"] == 9, r["grade"])
check("уверенность высокая", r["confidence"] == "high", r["confidence"])

r = classify("Геометрия/листки/8Б/листок 3.pdf", "листок 3.pdf")
check("листок — worksheet", r["type"] == "worksheet", r["type"])
check("класс из 8Б", r["grade"] == 8, r["grade"])

r = classify("Разное/что-то.pdf", "что-то.pdf")
check("«Разное» — это misc", r["subject"] == "misc", r["subject"])
check("misc не требует человека", r["confidence"] in ("high", "medium"), r["confidence"])
r = classify("файл-в-корне.pdf", "файл-в-корне.pdf")
check("файл без папки — unknown", r["subject"] == "unknown", r["subject"])
check("файл без папки — в review", r["confidence"] in ("low", "unknown"), r["confidence"])

r = classify("Матан/конспекты/лекция 3.pdf", "лекция 3.pdf")
check("матан — calculus", r["subject"] == "calculus", r["subject"])
check("конспект — notes", r["type"] == "notes", r["type"])

check("класс без папки — None", grade_of("Разное/файл.pdf", "файл.pdf") is None)
# регрессия: номер листка в имени не должен перебивать класс из папки
check("папка важнее имени", grade_of("Матан/9 класс/лист04д--хелли.pdf", "лист04д--хелли.pdf") == 9)
check("лист12д — не 12 класс", grade_of("Матан/лист12д--чум.pdf", "лист12д--чум.pdf") is None)
check("8Б из папки", grade_of("Геометрия/8Б/листок.pdf", "листок.pdf") == 8)
check("двузначный класс в имени", grade_of("Математика/x.pdf", "10 класс, листок 2.pdf") == 10)

# --- извлечение текста ---------------------------------------------------------------
with tempfile.TemporaryDirectory() as td:
    txt = Path(td) / "a.txt"
    txt.write_text("Домашнее задание по алгебре, решить номера 5 и 6", encoding="utf-8")
    text, status = extract(txt)
    check("txt читается", status == "native" and "алгебре" in text, status)

    exe = Path(td) / "b.exe"
    exe.write_bytes(b"MZ\x90\x00")
    check("exe — binary", extract(exe)[1] == "binary")
    check("png — картинка без текста", extract(Path(td) / "c.png")[1] == "scanned")

    # --- скан папки ------------------------------------------------------------------
    root = Path(td) / "drive"
    (root / "Алгебра" / "дз" / "9 класс").mkdir(parents=True)
    (root / "Алгебра" / "дз" / "9 класс" / "ДЗ-001.pdf").write_bytes(b"%PDF-1.4 fake")
    (root / "Алгебра" / "дз" / "9 класс" / "ДЗ-001 (1).pdf").write_bytes(b"%PDF-1.4 fake")
    (root / "непонятно.pdf").write_bytes(b"%PDF-1.4 other")   # в корне: предмет неизвестен
    (root / "Корзина").mkdir()
    (root / "Корзина" / "мусор.pdf").write_bytes(b"%PDF-1.4 trash")

    out = Path(td) / "out"
    rows, _ = scan(LocalSource(root), out)
    check("служебная папка пропущена", len(rows) == 3, len(rows))
    dupes = [x for x in rows if x.get("duplicate_of")]
    check("копия помечена", len(dupes) == 1, len(dupes))
    check("mime по сигнатуре", rows[0]["mime"] == "application/pdf", rows[0]["mime"])

    review = list(csv.DictReader(open(out / "review.csv", encoding="utf-8")))
    check("в review только неясный предмет", [x["path"] for x in review] == ["непонятно.pdf"],
          [x["path"] for x in review])

    manifest = [json.loads(l) for l in open(out / "manifest.jsonl", encoding="utf-8")]
    check("манифест на каждый файл", len(manifest) == 3, len(manifest))

    # --- раскладка -------------------------------------------------------------------
    staging = Path(td) / "staging"
    copied, skipped = copy_tree(LocalSource(root), staging, rows)
    check("копии не копируются", copied == 2 and skipped == 1, f"{copied}/{skipped}")
    target = staging / "9 класс" / "algebra" / "homework" / "ДЗ-001.pdf"
    check("путь полки <класс>/<предмет>/<тип>", target.exists(), str(target))

print(f"\n{'FAILED' if fails else 'all good'} — {fails} failure(s)")
sys.exit(1 if fails else 0)
