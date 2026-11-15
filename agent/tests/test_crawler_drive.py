"""Проверка Drive-источника на записанных ответах — без сети.

python3 tests/test_crawler_drive.py

Что закрепляем (это и есть критерий готовности шага):
  1. первый запуск: startPageToken + обход → события created, курсор сохранён;
  2. повторный прогон на тех же данных → 0 событий;
  3. изменился md5 → updated;
  4. удалённый файл (removed=true) → deleted;
  5. две страницы changes → один курсор, ровно два запроса;
  6. падение на второй странице → курсор не сдвинулся;
  7. путь собирается из parents: Алгебра/9 класс/ДЗ-15.pdf → algebra, 9 класс;
  8. событие из Drive по схеме совпадает с событием из папки;
     ссылка на файл сохраняется (файлы не копируются);
  9. Google Doc без md5: изменилась version → updated;
 10. файл перенесли в другую папку (содержимое и имя те же) → updated с новым путём.
"""
import json
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))

from eduhub_agent.crawler.sources.drive import DriveSource  # noqa: E402
from eduhub_agent.crawler.sources.local import LocalFolderSource  # noqa: E402
from eduhub_agent.crawler.state import SyncState  # noqa: E402
from eduhub_agent.crawler.sync import event_row, run  # noqa: E402
from eduhub_agent.crawler.transport import FixtureTransport  # noqa: E402

# модель не зовём: тест про токены, пути и события, а не про качество классификации
FAKE = {"subject": "algebra", "type": "homework", "type_conf": "high",
        "confidence": "high", "lesson": None, "date": None, "engine": "fake"}


def fake_classify(*a, **k):
    return dict(FAKE)

fails = 0


def check(name, cond, detail=""):
    global fails
    print(f"{'OK  ' if cond else 'FAIL'} {name}{(' :: ' + str(detail)) if detail else ''}")
    fails += not cond


FIX = HERE / "fixtures" / "drive"


def fixture(name):
    return json.loads((FIX / name).read_text(encoding="utf-8"))


with tempfile.TemporaryDirectory() as td:
    td = Path(td)
    state_path = td / "state.json"
    out = td / "out"

    # --- 1. первый запуск ------------------------------------------------------------
    t = FixtureTransport(fixture("drive_first.json"))
    src = DriveSource(t, folder_ids=["PROOT"])
    rows, _ = run(src, state_path, out, classifier=fake_classify)
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
    check("ссылка из webViewLink", dz["link"] == "https://drive.google.com/file/d/F1/view?usp=drivesdk",
          dz["link"])
    kn = [r for r in rows if r["name"] == "конспект.docx"][0]
    check("ссылка строится, если Drive её не дал",
          kn["link"].startswith("https://drive.google.com/file/d/F2/"), kn["link"])

    # --- 2. повтор без изменений -----------------------------------------------------
    t2 = FixtureTransport({"changes:T0": {"changes": [], "newStartPageToken": "T1"}})
    rows2, _ = run(DriveSource(t2, folder_ids=["PROOT"]), state_path, out, classifier=fake_classify)
    check("повтор: ноль событий", rows2 == [], len(rows2))
    check("курсор сдвинулся на newStartPageToken", SyncState(state_path).cursor == "T1",
          SyncState(state_path).cursor)

    # --- 3/4. изменение и удаление ---------------------------------------------------
    t3 = FixtureTransport(fixture("drive_changes.json") | {"changes:T1": fixture("drive_changes.json")["changes:T0"]})
    state = SyncState(state_path)
    state.cursor = "T0"
    state.save()
    rows3, _ = run(DriveSource(t3, folder_ids=["PROOT"]), state_path, out, classifier=fake_classify)
    got = sorted((r["kind"], r["name"]) for r in rows3)
    check("новый файл + изменённый + удалённый",
          got == [("created", "ДЗ-16.pdf"), ("deleted", "конспект.docx"), ("updated", "ДЗ-15.pdf")], got)
    check("удаление убрано из состояния", SyncState(state_path).get("F2") is None)

    # --- 5. две страницы --------------------------------------------------------------
    t5 = FixtureTransport(fixture("drive_paged.json"))
    state = SyncState(state_path)
    state.cursor = "T1"
    state.save()
    rows5, _ = run(DriveSource(t5, folder_ids=["PROOT"]), state_path, out, classifier=fake_classify)
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

    # --- 9. Google Doc без md5: изменилась только version ------------------------------
    folders = {"file:P9": {"id": "P9", "name": "9 класс", "parents": ["PALG"]},
               "file:P8": {"id": "P8", "name": "8 класс", "parents": ["PALG"]},
               "file:PALG": {"id": "PALG", "name": "Алгебра", "parents": ["PROOT"]}}
    doc_state = td / "doc_state.json"
    st = SyncState(doc_state)
    st.put("F6", {"hash": "v1", "size": 100, "mtime": "2026-10-18T10:00:00Z",
                  "name": "конспект", "path": "Алгебра/9 класс/конспект"})
    st.cursor = "T0"
    st.save()
    doc_routes = {"changes:T0": {"changes": [{"changeType": "file", "fileId": "F6", "file": {
        "id": "F6", "name": "конспект", "mimeType": "application/vnd.google-apps.document",
        "modifiedTime": "2026-10-19T10:00:00Z", "version": "2", "parents": ["P9"]}}],
        "newStartPageToken": "T1"}}
    doc_rows, _ = run(DriveSource(FixtureTransport({**doc_routes, **folders}), ["PROOT"]),
                      doc_state, td / "out_doc")
    check("Google Doc: version изменилась → updated",
          [(r["kind"], r["name"]) for r in doc_rows] == [("updated", "конспект")],
          [(r["kind"], r["name"]) for r in doc_rows])

    # --- 10. файл перенесли в другую папку --------------------------------------------
    move_state = td / "move_state.json"
    st = SyncState(move_state)
    st.put("F1", {"hash": "aaa", "size": 10, "mtime": "2026-10-18T10:00:00Z",
                  "name": "ДЗ-15.pdf", "path": "Алгебра/9 класс/ДЗ-15.pdf"})
    st.cursor = "T0"
    st.save()
    move_routes = {"changes:T0": {"changes": [{"changeType": "file", "fileId": "F1", "file": {
        "id": "F1", "name": "ДЗ-15.pdf", "mimeType": "application/pdf", "size": "10",
        "modifiedTime": "2026-10-18T10:00:00Z", "md5Checksum": "aaa", "parents": ["P8"]}}],
        "newStartPageToken": "T1"}}
    move_rows, _ = run(DriveSource(FixtureTransport({**move_routes, **folders}), ["PROOT"]),
                       move_state, td / "out_move")
    check("перенос в другую папку → updated с новым путём",
          [(r["kind"], r["path"]) for r in move_rows] == [("updated", "Алгебра/8 класс/ДЗ-15.pdf")],
          [(r["kind"], r["path"]) for r in move_rows])

    # --- 8. схема события одинаковая ---------------------------------------------------
    drive_row = rows[0]
    local_row = event_row("created", "local", {"id": "x/ДЗ.pdf", "name": "ДЗ.pdf", "path": "x/ДЗ.pdf",
                                               "size": 1, "mtime": 1, "hash": "h"},
                          classifier=fake_classify)
    check("схема Drive и папки совпадает", set(drive_row) == set(local_row),
          sorted(set(drive_row) ^ set(local_row)))

    # --- локальный источник не сломался ------------------------------------------------
    root = td / "drive"
    (root / "Алгебра" / "9 класс").mkdir(parents=True)
    (root / "Алгебра" / "9 класс" / "ДЗ-1.pdf").write_bytes(b"%PDF-1.4 x")
    rows_local, _ = run(LocalFolderSource(root), td / "st2.json", td / "out2", classifier=fake_classify)
    check("локальный источник всё ещё работает",
          [r["kind"] for r in rows_local] == ["created"] and rows_local[0]["subject"] == "algebra",
          rows_local[0].get("subject"))

print(f"\n{'FAILED' if fails else 'all good'} — {fails} failure(s)")
sys.exit(1 if fails else 0)
