"""Проверка Drive-источника на записанных ответах — без сети.

python3 tools/crawler/test_drive_source.py

Что закрепляем (это и есть критерий готовности шага):
  1. первый запуск: startPageToken + обход → события created, курсор сохранён;
  2. повторный прогон на тех же данных → 0 событий;
  3. изменился md5 → updated;
  4. удалённый файл (removed=true) → deleted;
  5. две страницы changes → один курсор, ровно два запроса;
  6. падение на второй странице → курсор не сдвинулся;
  7. путь собирается из parents: Алгебра/9 класс/ДЗ-15.pdf → algebra, 9 класс;
  8. событие из Drive по схеме совпадает с событием из папки.
"""
import json
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent.parent))

from tools.crawler.sources.drive import DriveSource  # noqa: E402
from tools.crawler.sources.local import LocalFolderSource  # noqa: E402
from tools.crawler.state import SyncState  # noqa: E402
from tools.crawler.sync import event_row, run  # noqa: E402
from tools.crawler.transport import FixtureTransport  # noqa: E402

fails = 0


def check(name, cond, detail=""):
    global fails
    print(f"{'OK  ' if cond else 'FAIL'} {name}{(' :: ' + str(detail)) if detail else ''}")
    fails += not cond


FIX = HERE / "fixtures"


def fixture(name):
    return json.loads((FIX / name).read_text(encoding="utf-8"))


with tempfile.TemporaryDirectory() as td:
    td = Path(td)
    state_path = td / "state.json"
    out = td / "out"

    # --- 1. первый запуск ------------------------------------------------------------
    t = FixtureTransport(fixture("drive_first.json"))
    src = DriveSource(t, folder_ids=["PROOT"])
    rows, _ = run(src, state_path, out)
    kinds = [r["kind"] for r in rows]
    check("первый запуск: два created", kinds == ["created", "created"], kinds)
    check("курсор сохранён из startPageToken", SyncState(state_path).cursor == "T0",
          SyncState(state_path).cursor)
    check("start + список, остальное — папки для пути",
          t.hits == ["start", "files:", "file:P9", "file:PALG"], t.hits)

    # --- 7. путь и классификация -----------------------------------------------------
    dz = [r for r in rows if r["name"] == "ДЗ-15.pdf"][0]
    check("путь собран из parents", dz["path"] == "Алгебра/9 класс/ДЗ-15.pdf", dz["path"])
    check("классификатор: algebra", dz["subject"] == "algebra", dz["subject"])
    check("классификатор: 9 класс", dz["grade"] == 9, dz["grade"])
    check("классификатор: homework", dz["type"] == "homework", dz["type"])

    # --- 2. повтор без изменений -----------------------------------------------------
    t2 = FixtureTransport({"changes:T0": {"changes": [], "newStartPageToken": "T1"}})
    rows2, _ = run(DriveSource(t2, folder_ids=["PROOT"]), state_path, out)
    check("повтор: ноль событий", rows2 == [], len(rows2))
    check("курсор сдвинулся на newStartPageToken", SyncState(state_path).cursor == "T1",
          SyncState(state_path).cursor)

    # --- 3/4. изменение и удаление ---------------------------------------------------
    t3 = FixtureTransport(fixture("drive_changes.json") | {"changes:T1": fixture("drive_changes.json")["changes:T0"]})
    state = SyncState(state_path)
    state.cursor = "T0"
    state.save()
    rows3, _ = run(DriveSource(t3, folder_ids=["PROOT"]), state_path, out)
    got = sorted((r["kind"], r["name"]) for r in rows3)
    check("новый файл + изменённый + удалённый",
          got == [("created", "ДЗ-16.pdf"), ("deleted", "конспект.docx"), ("updated", "ДЗ-15.pdf")], got)
    check("удаление убрано из состояния", SyncState(state_path).get("F2") is None)

    # --- 5. две страницы --------------------------------------------------------------
    t5 = FixtureTransport(fixture("drive_paged.json"))
    state = SyncState(state_path)
    state.cursor = "T1"
    state.save()
    rows5, _ = run(DriveSource(t5, folder_ids=["PROOT"]), state_path, out)
    check("две страницы: два файла", len(rows5) == 2, len(rows5))
    check("ровно два запроса changes", t5.hits == ["changes:T1", "changes:PAGE2"], t5.hits)
    check("курсор — новый стартовый токен", SyncState(state_path).cursor == "T2",
          SyncState(state_path).cursor)

    # --- 6. падение на второй странице ------------------------------------------------
    broken = fixture("drive_paged.json")
    del broken["changes:PAGE2"]                      # страница «не пришла»
    state = SyncState(state_path)
    state.cursor = "T1"
    state.save()
    try:
        run(DriveSource(FixtureTransport(broken), folder_ids=["PROOT"]), state_path, out)
        crashed = False
    except AssertionError:
        crashed = True
    check("падение на второй странице замечено", crashed)
    check("курсор не сдвинулся после падения", SyncState(state_path).cursor == "T1",
          SyncState(state_path).cursor)

    # --- 8. схема события одинаковая ---------------------------------------------------
    drive_row = rows[0]
    local_row = event_row("created", "local", {"id": "x/ДЗ.pdf", "name": "ДЗ.pdf", "path": "x/ДЗ.pdf",
                                               "size": 1, "mtime": 1, "hash": "h"})
    check("схема Drive и папки совпадает", set(drive_row) == set(local_row),
          sorted(set(drive_row) ^ set(local_row)))

    # --- локальный источник не сломался ------------------------------------------------
    root = td / "drive"
    (root / "Алгебра" / "9 класс").mkdir(parents=True)
    (root / "Алгебра" / "9 класс" / "ДЗ-1.pdf").write_bytes(b"%PDF-1.4 x")
    rows_local, _ = run(LocalFolderSource(root), td / "st2.json", td / "out2")
    check("локальный источник всё ещё работает",
          [r["kind"] for r in rows_local] == ["created"] and rows_local[0]["subject"] == "algebra",
          rows_local[0].get("subject"))

print(f"\n{'FAILED' if fails else 'all good'} — {fails} failure(s)")
sys.exit(1 if fails else 0)
