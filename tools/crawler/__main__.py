"""Краулер: собирает материалы из внешних источников в платформу.

  python3 -m tools.crawler sync "<папка>" --state state/drive.json --out out/sync
  python3 -m tools.crawler sync "<папка>" --state state/drive.json --out out/sync   # второй раз: 0 событий

Источники: сейчас папка на диске (выгрузка Drive, архив учителя). Когда появятся доступы —
Drive, Classroom и Gmail, и для них меняется только `scan_local` на соответствующий API.
"""
import argparse
import sys

from .sync import sync


def main():
    ap = argparse.ArgumentParser(prog="crawler", description="Синхронизация источника с платформой")
    sub = ap.add_subparsers(dest="cmd", required=True)

    s = sub.add_parser("sync", help="найти новое/изменённое/удалённое с прошлого запуска")
    s.add_argument("path", help="папка-источник")
    s.add_argument("--state", default="state/sync.json", help="файл состояния (создаётся сам)")
    s.add_argument("--out", default="out/sync", help="куда писать events.jsonl и summary.md")
    s.add_argument("--no-text", action="store_true", help="не извлекать текст (быстрее)")

    a = ap.parse_args()
    rows, dur = sync(a.path, a.state, a.out, with_text=not a.no_text)
    kinds = {}
    for r in rows:
        kinds[r["kind"]] = kinds.get(r["kind"], 0) + 1
    print(f"[sync] {len(rows)} событий за {dur:.1f} с: "
          f"новых {kinds.get('created', 0)}, изменённых {kinds.get('updated', 0)}, "
          f"удалённых {kinds.get('deleted', 0)}")
    print(f"       сводка: {a.out}/summary.md")


if __name__ == "__main__":
    sys.exit(main())
