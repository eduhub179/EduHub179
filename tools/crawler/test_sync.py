"""Проверка синхронизации: второй прогон не должен находить «новых» файлов.

python3 tools/crawler/test_sync.py
"""
import json
import os
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent.parent))

from tools.crawler.sync import SyncState, diff, scan_local, sync  # noqa: E402

fails = 0


def check(name, cond, detail=""):
    global fails
    print(f"{'OK  ' if cond else 'FAIL'} {name}{(' :: ' + str(detail)) if detail else ''}")
    fails += not cond


with tempfile.TemporaryDirectory() as td:
    root = Path(td) / "drive"
    (root / "Алгебра" / "дз" / "9 класс").mkdir(parents=True)
    f1 = root / "Алгебра" / "дз" / "9 класс" / "ДЗ-001.pdf"
    f1.write_bytes(b"%PDF-1.4 one")
    (root / "Корзина").mkdir()
    (root / "Корзина" / "мусор.pdf").write_bytes(b"%PDF-1.4 trash")

    state_path = Path(td) / "state.json"
    out = Path(td) / "out"

    rows, _ = sync(root, state_path, out)
    kinds = [r["kind"] for r in rows]
    check("первый прогон: новый файл", kinds.count("created") == 1, kinds)
    check("служебная папка пропущена", all("мусор" not in r["id"] for r in rows))
    check("классификация приклеена", rows[0]["subject"] == "algebra" and rows[0]["type"] == "homework",
          (rows[0].get("subject"), rows[0].get("type")))

    rows2, _ = sync(root, state_path, out)
    check("второй прогон: ноль событий", rows2 == [], len(rows2))

    # изменение содержимого
    f1.write_bytes(b"%PDF-1.4 one CHANGED")
    rows3, _ = sync(root, state_path, out)
    check("изменение замечено", [r["kind"] for r in rows3] == ["updated"], [r["kind"] for r in rows3])

    # касание без изменения содержимого — события нет
    os.utime(f1, (1700000000, 1700000000))
    rows4, _ = sync(root, state_path, out)
    check("mtime без изменения — без события", rows4 == [], [r["kind"] for r in rows4])

    # удаление
    f1.unlink()
    rows5, _ = sync(root, state_path, out)
    check("удаление замечено", [r["kind"] for r in rows5] == ["deleted"], [r["kind"] for r in rows5])

    # состояние переживает перезапуск
    st = SyncState(state_path)
    check("состояние сохранено на диск", st.path.exists() and "items" in st.data)
    check("после удаления в состоянии пусто", st.data["items"] == {}, st.data["items"])

print(f"\n{'FAILED' if fails else 'all good'} — {fails} failure(s)")
sys.exit(1 if fails else 0)
