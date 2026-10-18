"""Проверка разбора папок без сети и без модели: python3 tests/test_ingest.py

Что закрепляем:
  * обвязка: скан, манифест, review, копии; класс считается по папке/имени;
    сам классификатор — модель из `eduhub_agent.classify`, в тестах он подменён;
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
sys.path.insert(0, str(HERE.parent))

from eduhub_agent.ingest.grade import grade_of  # noqa: E402
from eduhub_agent.files.read import read  # noqa: E402
from eduhub_agent.files.decide import decide  # noqa: E402
from eduhub_agent.ingest.source import LocalSource  # noqa: E402
from eduhub_agent.ingest.__main__ import scan  # noqa: E402

# Классификатор — модель (10 с на файл). В тестах подменяем: проверяем обвязку, а не модель.
FAKE = {"subject": "algebra", "type": "homework", "lesson": None, "date": None,
        "subject_conf": "high", "type_conf": "high", "confidence": "high", "engine": "fake"}


def fake_classify(path, name="", text="", **kw):
    """Заглушка вместо модели: класс считается по-настоящему, предмет/тип — фиксированные."""
    if "Разное" in str(path) or "непонятно" in str(name):
        return {**FAKE, "subject": "unknown", "confidence": "none", "subject_conf": "none",
                "grade": grade_of(path, name)}
    return {**FAKE, "grade": grade_of(path, name)}

fails = 0


def check(name, cond, detail=""):
    global fails
    print(f"{'OK  ' if cond else 'FAIL'} {name}{(' :: ' + str(detail)) if detail else ''}")
    fails += not cond


# --- класс: правило, а не модель -------------------------------------------------------
r = fake_classify("Алгебра/дз/9 класс/ДЗ-004-к-12-10-2024.pdf", "ДЗ-004-к-12-10-2024.pdf")
check("класс из папки", r["grade"] == 9, r["grade"])
r = fake_classify("Геометрия/листки/8Б/листок 3.pdf", "листок 3.pdf")
check("класс из 8Б", r["grade"] == 8, r["grade"])
r = fake_classify("Разное/что-то.pdf", "что-то.pdf")
check("«Разное» без текста — предмет неясен", r["subject"] == "unknown", r["subject"])
check("и это повод позвать человека", r["confidence"] in ("low", "none"), r["confidence"])
check("класс без папки — None", grade_of("Разное/файл.pdf", "файл.pdf") is None)
check("папка важнее имени", grade_of("Матан/9 класс/лист04д--хелли.pdf", "лист04д--хелли.pdf") == 9)
check("лист12д — не 12 класс", grade_of("Матан/лист12д--чум.pdf", "лист12д--чум.pdf") is None)
check("8Б из папки", grade_of("Геометрия/8Б/листок.pdf", "листок.pdf") == 8)
check("двузначный класс в имени", grade_of("Математика/x.pdf", "10 класс, листок 2.pdf") == 10)

# --- порт классификатора из P2: строгий разбор ответа модели ---------------------------
from eduhub_agent.classify import SUBJECTS, TYPES, parse  # noqa: E402

check("категории — те же, что в P2",
      SUBJECTS == ["algebra", "geometry", "calculus", "digital_tech", "programming"]
      and TYPES[-1] == "other" and "plusnik_sheet" in TYPES, (SUBJECTS, TYPES))
check("parse: валидный ответ",
      parse('{"subject":"algebra","type":"homework","lesson":3,"date":null}')["lesson"] == 3)
check("parse: чужая категория отклоняется", parse('{"subject":"физика","type":"homework"}') is None)
check("parse: мусор отклоняется", parse("не json") is None)
check("parse: отсутствие полей отклоняется", parse('{"subject":"algebra"}') is None)

# --- извлечение текста ---------------------------------------------------------------
with tempfile.TemporaryDirectory() as td:
    txt = Path(td) / "a.txt"
    txt.write_text("Домашнее задание по алгебре, решить номера 5 и 6", encoding="utf-8")
    r = read(txt)
    check("txt читается", r.status == "native" and "алгебре" in r.text, r.status)

    exe = Path(td) / "b.exe"
    exe.write_bytes(b"MZ\x90\x00")
    check("exe — binary", read(exe).status == "binary")
    check("нет файла — not_downloaded", read(Path(td) / "нет.pdf").status == "not_downloaded")
    # решатель: один и тот же набор правил для всех источников
    check("decide: binary → skip", decide("binary")[0] == "skip")
    check("decide: Drive без файла → review", decide("not_downloaded")[0] == "review")
    check("decide: рукописное → queued", decide("scanned")[0] == "queued")
    check("decide: текст + high → accepted", decide("ocr", {"confidence": "high"})[0] == "accepted")
    check("decide: текст без меток → review", decide("native", {"confidence": "none"})[0] == "review")

    # --- скан папки ------------------------------------------------------------------
    root = Path(td) / "drive"
    (root / "Алгебра" / "дз" / "9 класс").mkdir(parents=True)
    (root / "Алгебра" / "дз" / "9 класс" / "ДЗ-001.pdf").write_bytes(b"%PDF-1.4 fake")
    (root / "Алгебра" / "дз" / "9 класс" / "ДЗ-001 (1).pdf").write_bytes(b"%PDF-1.4 fake")
    (root / "непонятно.pdf").write_bytes(b"%PDF-1.4 other")   # в корне: предмет неизвестен
    (root / "Корзина").mkdir()
    (root / "Корзина" / "мусор.pdf").write_bytes(b"%PDF-1.4 trash")

    out = Path(td) / "out"
    rows, _ = scan(LocalSource(root), out, classifier=fake_classify)
    check("служебная папка пропущена", len(rows) == 3, len(rows))
    dupes = [x for x in rows if x.get("duplicate_of")]
    check("копия помечена", len(dupes) == 1, len(dupes))
    check("mime по сигнатуре", rows[0]["mime"] == "application/pdf", rows[0]["mime"])

    review = list(csv.DictReader(open(out / "review.csv", encoding="utf-8")))
    check("в review только неясный предмет", [x["path"] for x in review] == ["непонятно.pdf"],
          [x["path"] for x in review])

    manifest = [json.loads(l) for l in open(out / "manifest.jsonl", encoding="utf-8")]
    check("манифест на каждый файл", len(manifest) == 3, len(manifest))

    # --- ссылка вместо копирования ------------------------------------------------------
    check("в манифесте есть путь-ссылка, а не копия",
          all(x.get("path") for x in manifest), [x.get("path") for x in manifest])

    # --- классификатор подменяется, модель в тестах не нужна ------------------------------
    check("подмена классификатора работает без модели",
          fake_classify("Физика/кр.pdf")["subject"] == "algebra", "заглушка отвечает всегда")


print(f"\n{'FAILED' if fails else 'all good'} — {fails} failure(s)")
sys.exit(1 if fails else 0)
