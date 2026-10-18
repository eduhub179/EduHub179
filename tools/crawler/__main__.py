"""Краулер: собирает материалы из внешних источников в платформу.

  python3 -m tools.crawler sync "<папка>" --state state/drive.json --out out/sync
  python3 -m tools.crawler sync --fixture tools/crawler/fixtures/drive_first.json \\
      --folder-id PROOT --state state/fake.json --out out/fake     # Drive без ключей

Первый запуск = снимок источника, дальше — только изменения. Второй прогон по тем же
данным обязан дать ноль событий. Источники: папка на диске (сейчас), Google Drive
(на записанных ответах), дальше Classroom и Gmail — та же схема событий.
"""
import argparse
import json
import sys

from .sources.drive import DriveSource
from .sources.local import LocalFolderSource
from .sync import run, sync_folder
from .transport import FixtureTransport, UrllibTransport


def main():
    ap = argparse.ArgumentParser(prog="crawler", description="Синхронизация источника с платформой")
    sub = ap.add_subparsers(dest="cmd", required=True)

    s = sub.add_parser("sync", help="найти новое/изменённое/удалённое с прошлого запуска")
    s.add_argument("path", nargs="?", help="папка-источник (локальный диск, выгрузка Takeout)")
    s.add_argument("--drive", action="store_true", help="источник — Google Drive (нужен токен)")
    s.add_argument("--folder-id", action="append", default=[], help="id папки Drive (можно несколько)")
    s.add_argument("--fixture", help="вместо сети — записанные ответы API (для проверки без ключей)")
    s.add_argument("--state", default="state/sync.json", help="файл состояния (создаётся сам)")
    s.add_argument("--out", default="out/sync", help="куда писать events.jsonl и summary.md")

    a = ap.parse_args()

    if a.fixture:
        routes = json.loads(open(a.fixture, encoding="utf-8").read())
        source = DriveSource(FixtureTransport(routes), folder_ids=a.folder_id or ["PROOT"])
        rows, dur = run(source, a.state, a.out)
    elif a.drive:
        import os
        token = os.environ.get("GOOGLE_ACCESS_TOKEN")
        if not token:
            raise SystemExit("[!] нужен GOOGLE_ACCESS_TOKEN (или --fixture для проверки без ключей)")
        source = DriveSource(UrllibTransport(token), folder_ids=a.folder_id)
        rows, dur = run(source, a.state, a.out)
    else:
        if not a.path:
            raise SystemExit("нужен путь к папке, --drive или --fixture")
        rows, dur = sync_folder(a.path, a.state, a.out)

    kinds = {}
    for r in rows:
        kinds[r["kind"]] = kinds.get(r["kind"], 0) + 1
    print(f"[sync] {len(rows)} событий за {dur:.1f} с: "
          f"новых {kinds.get('created', 0)}, изменённых {kinds.get('updated', 0)}, "
          f"удалённых {kinds.get('deleted', 0)}")
    print(f"       сводка: {a.out}/summary.md")


if __name__ == "__main__":
    sys.exit(main())
