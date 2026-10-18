"""Разбор папки с материалами: что это за файлы и куда их потом класть.

  python3 -m eduhub_agent.ingest scan "data/takeout/3/Диск" --out out/takeout3
  python3 -m eduhub_agent.ingest scan data/math-b2027 --out out/math

scan пишет в --out:
  manifest.jsonl  по строке на файл: путь, размер, хеш, mime, тип текста, предмет, класс, тип, уверенность
  review.csv      только неуверенные — то, что человек должен посмотреть глазами
  report.md       сводка: сколько чего нашлось и что осталось непонятным

Ничего не удаляет, не переименовывает и не копирует: единственное, что меняется на диске —
папка --out. Файлы пока остаются там, где лежат; в манифесте хранится путь (ссылка).
"""
import argparse
import csv
import hashlib
import json
import mimetypes
import os
import time
from collections import Counter
from pathlib import Path

from eduhub_agent.classify import classify
from eduhub_agent.ingest.extract import extract
from eduhub_agent.ingest.grade import grade_of
from .source import open_source

HEAD = 200  # сколько текста храним в манифесте для быстрого просмотра


def mime_of(path, head):
    """MIME по сигнатуре: клиенту и расширению верить нельзя (переименованный .exe)."""
    if head.startswith(b"%PDF"):
        return "application/pdf"
    if head.startswith(b"PK\x03\x04"):
        return {"docx": "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                "pptx": "application/vnd.openxmlformats-officedocument.presentationml.presentation",
                "xlsx": "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                "zip": "application/zip"}.get(path.suffix.lower().lstrip("."), "application/zip")
    if head.startswith(b"\x89PNG"):
        return "image/png"
    if head.startswith(b"\xff\xd8"):
        return "image/jpeg"
    if head[:4] == b"GIF8":
        return "image/gif"
    if head.startswith(b"{\\rtf"):
        return "application/rtf"
    return mimetypes.guess_type(path.name)[0] or "application/octet-stream"


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def scan(source, out_dir, classifier=None):
    ok, why = source.available()
    if not ok:
        raise SystemExit(f"[!] источник недоступен: {why}")
    out = Path(out_dir)
    out.mkdir(parents=True, exist_ok=True)
    manifest_path = out / "manifest.jsonl"
    seen = {}                     # sha256 -> первый путь
    rows, t0 = [], time.time()
    printed = 0

    with open(manifest_path, "w", encoding="utf-8") as mf:
        for full, rel in source.files():
            try:
                st = full.stat()
                with open(full, "rb") as f:
                    head = f.read(64)
                digest = sha256(full)
                text, text_status = extract(full)
                row = {
                    "path": rel, "name": full.name, "ext": full.suffix.lower(),
                    "size": st.st_size, "mtime": int(st.st_mtime),
                    "sha256": digest, "mime": mime_of(full, head),
                    "text_status": text_status, "text_chars": len(text),
                }
                row.update((classifier or classify)(rel, full.name, text))
                row["grade"] = grade_of(rel, full.name)
                if digest in seen:
                    row["duplicate_of"] = seen[digest]
                else:
                    seen[digest] = rel
                row["text_head"] = text[:HEAD]
                mf.write(json.dumps(row, ensure_ascii=False) + "\n")
                rows.append(row)
                printed += 1
                if printed % 50 == 0:
                    print(f"   ... {printed} файлов", flush=True)
            except OSError as e:
                print(f"   пропуск {rel}: {e}")

    write_review(out / "review.csv", rows)
    write_report(out / "report.md", rows, source)
    return rows, time.time() - t0


def write_review(path, rows):
    unsure = [r for r in rows if r["subject_conf"] not in ("high", "medium")]
    with open(path, "w", encoding="utf-8", newline="") as f:
        w = csv.writer(f)
        w.writerow(["path", "subject", "type", "grade", "subject_conf", "type_conf",
                    "text_status", "text_head"])
        for r in sorted(unsure, key=lambda r: r["path"]):
            w.writerow([r["path"], r["subject"], r["type"], r["grade"] or "",
                        r["subject_conf"], r["type_conf"], r["text_status"],
                        r["text_head"].replace("\n", " ")[:80]])
    return len(unsure)


def write_report(path, rows, source):
    total = len(rows)
    size = sum(r["size"] for r in rows)
    by_conf = Counter(r["confidence"] for r in rows)
    by_subject = Counter(r["subject"] for r in rows)
    by_type = Counter(r["type"] for r in rows)
    by_status = Counter(r["text_status"] for r in rows)
    dupes = [r for r in rows if r.get("duplicate_of")]
    unknown_subj = [r for r in rows if r["subject_conf"] == "none"]

    def table(counter, title):
        lines = [f"### {title}", "", "| | файлов |", "|---|---:|"]
        for k, v in counter.most_common():
            lines.append(f"| {k} | {v} |")
        return "\n".join(lines) + "\n"

    md = [f"# Разбор папки\n",
          f"- источник: `{getattr(source, 'root', getattr(source, 'folder_id', '?'))}` ({source.kind})",
          f"- файлов: **{total}**, объём: **{size / 1e6:.1f} МБ**",
          f"- предмет ясен (high): **{by_conf.get('high', 0)}** "
          f"({100 * by_conf.get('high', 0) / max(total, 1):.0f}%)",
          f"- предмет ясен, тип нет (medium): **{by_conf.get('medium', 0)}**",
          f"- нужен человек, чтобы определить предмет: **{by_conf.get('low', 0) + by_conf.get('unknown', 0)}** (review.csv)",
          f"- точных копий (тот же sha256): **{len(dupes)}**",
          ""]
    md.append(table(by_subject, "Предметы"))
    md.append(table(by_type, "Типы"))
    md.append(table(by_status, "Текст внутри файла"))
    md.append(table(by_conf, "Уверенность"))
    if unknown_subj:
        md.append("### Без предмета — первые 20 (полный список в review.csv)\n")
        for r in unknown_subj[:20]:
            md.append(f"- `{r['path']}`")
        md.append("")
    Path(path).write_text("\n".join(md), encoding="utf-8")


def main():
    ap = argparse.ArgumentParser(prog="drive_ingest", description="Разбор папки с материалами")
    sub = ap.add_subparsers(dest="cmd", required=True)

    s = sub.add_parser("scan", help="пройти папку, распознать предмет/тип, написать манифест")
    s.add_argument("path", nargs="?", help="папка (Takeout, распакованный архив, диск учителя)")
    s.add_argument("--drive", help="id папки Google Drive (когда появится доступ)")
    s.add_argument("--out", default="out/ingest", help="куда писать manifest/review/report")

    a = ap.parse_args()
    source = open_source(getattr(a, "path", None), getattr(a, "drive", None))

    rows, dur = scan(source, a.out)
    print(f"\n[готово] {len(rows)} файлов за {dur:.1f} с -> {a.out}/report.md")


if __name__ == "__main__":
    main()
