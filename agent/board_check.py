"""Проверка распознавания доски на фото из экспортов: python3 board_check.py [сколько фото, по умолчанию 60]
Берёт фото без подписи и без печатного текста (те, к которым применяется правило «доска → ДЗ»), спрашивает
модель (board.py) и делает страницу chats/доски-проверка.html: слева «доска», справа «не доска».
Открой её в браузере и посчитай ошибки. Страница и фото остаются на этом компьютере.
"""
import html
import random
import sys
import time
from pathlib import Path

import board
import chat_agent as ca
import files

N = int(sys.argv[1]) if len(sys.argv) > 1 else 60
photos = []
for folder in sorted(Path("chats").glob("ChatExport_*")):
    data = ca.load_export(folder)
    for m in ca.merge_bursts([ca.from_export(x, data["name"]) for x in data["messages"] if x.get("type") == "message"]):
        paths = [folder / p for p in m.get("file_paths", [])]
        if paths and all(p.suffix.lower() in files.IMAGES for p in paths) and len(m["text"].strip()) <= 3:
            photos.append(paths[0])
random.seed(1)
sample = random.sample(photos, min(N, len(photos)))
print(f"Фото без подписи в экспортах: {len(photos)}, проверяю {len(sample)} моделью {board.BOARD_MODEL}")
res, t0, skipped = {True: [], False: []}, time.time(), 0
for i, p in enumerate(sample, 1):
    if files.extract_text(p):  # есть печатный текст — правило не применяется
        skipped += 1
        continue
    v = board.is_board(p)
    if v is None:
        sys.exit("Модель для фото недоступна")
    res[v].append(p)
    if i % 10 == 0:
        print(f"  {i}/{len(sample)}, {time.time() - t0:.0f} с")
n = len(res[True]) + len(res[False])
print(f"Доска: {len(res[True])}, не доска: {len(res[False])}, с печатным текстом (пропущены): {skipped}; "
      f"{(time.time() - t0) / max(1, n):.1f} с на фото")


def col(title, ps):
    imgs = "".join(f'<img src="{html.escape(p.resolve().as_uri())}">' for p in ps)
    return f"<div><h2>{title} ({len(ps)})</h2>{imgs}</div>"


out = Path("chats/доски-проверка.html")
out.write_text("<meta charset=utf-8><style>body{display:flex;gap:20px;font-family:sans-serif}div{flex:1}"
               "img{width:180px;margin:3px;border:1px solid #ccc}</style>"
               + col("Модель: доска → ДЗ", res[True]) + col("Модель: не доска → пропуск", res[False]), encoding="utf-8")
print(f"Открой {out.resolve()} в браузере и посмотри, где модель ошиблась")
