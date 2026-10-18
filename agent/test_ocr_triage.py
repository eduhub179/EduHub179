"""Проверка чтения фото (ocr_triage.py, ocr_providers.py) без чатов и без модели:
python3 test_ocr_triage.py

Что закрепляем:
  * порог уверенности отделяет печатный текст от рукописного;
  * правило таблиц чисел продолжает работать (иначе таблицы ответов снова станут «фото без текста»);
  * пустое фото не попадает в очередь для будущего движка, рукописное — попадает;
  * повторное фото берётся из кеша, а не распознаётся заново;
  * облачный OCR и модель для картинок объявлены, но выключены (ключей и сети нет).
"""
import json
import sys
import tempfile
from pathlib import Path

import ocr_triage as T
import ocr_providers as P
from ocr_providers import Reading

fails = 0


def check(name, cond, detail=""):
    global fails
    print(f"{'OK  ' if cond else 'FAIL'} {name}{(' :: ' + detail) if detail else ''}")
    fails += not cond


# --- правила приёма ---------------------------------------------------------------
print_text = Reading(text="Первый сплав содержит 10% меди", words=42, numbers=2, mean_conf=91.0, psm=3, ms=500, engine="t")
handwritten = Reading(text="Яомемикее joparwue", words=45, numbers=3, mean_conf=41.0, psm=3, ms=900, engine="t")
table = Reading(text="625 12 1:3:5 407", words=2, numbers=14, mean_conf=52.0, psm=3, ms=400, engine="t")

check("печатный текст принимается", T.accept(print_text)[0])
check("рукописное отклоняется", not T.accept(handwritten)[0], T.accept(handwritten)[1])
check("таблица чисел принимается без высокого порога", T.accept(table)[0], T.accept(table)[1])
check("пустое не принимается", not T.accept(Reading("", 0, 0, 0.0, 3, 10, "t"))[0])
check("граница порога: 70 берём, 69 нет",
      T.accept(Reading("текст", 10, 0, 70.0, 3, 1, "t"))[0] and not T.accept(Reading("текст", 10, 0, 69.9, 3, 1, "t"))[0])

# --- разбор TSV ---------------------------------------------------------------------
TSV = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\tw\th\tconf\ttext\n" + \
      "5\t1\t1\t1\t1\t1\t0\t0\t10\t10\t96.0\tДомашнее\n" + \
      "5\t1\t1\t1\t1\t2\t0\t0\t10\t10\t88.0\tзадание\n" + \
      "5\t1\t1\t1\t2\t1\t0\t0\t10\t10\t0.0\t-\n"
text, words, numbers, conf = P._parse_tsv(TSV)
check("TSV: слова и строки", words == 2 and text.count("\n") == 1, repr(text))
check("TSV: уверенность", abs(conf - ((96 + 88 + 0) / 3)) < 0.1, str(conf))
check("TSV: числа считаются", P._parse_tsv(TSV.replace("задание", "625"))[2] == 1)

# --- провайдеры ----------------------------------------------------------------------
ok, why = P.get("local_tesseract").available()
check("Tesseract доступен", ok, why)
check("облачный OCR выключен", not P.get("cloud_ocr").available()[0])
check("модель для картинок не пишет текст", not P.get("local_vlm").available()[0])

# --- сквозная проверка на картинке ---------------------------------------------------
if not ok:
    print("SKIP сквозная проверка: нет Tesseract")
else:
    from PIL import Image, ImageDraw, ImageFont

    tmpdir = Path(tempfile.mkdtemp())
    T.CHATS = tmpdir
    T.QUEUE = tmpdir / "ocr_queue.jsonl"
    T.CACHE = tmpdir / "ocr_cache.json"
    T._cache = None

    def picture(name, draw_text=None):
        img = Image.new("L", (1400, 900), 255)
        if draw_text:
            d = ImageDraw.Draw(img)
            try:
                font = ImageFont.load_default(size=56)
            except TypeError:  # старый Pillow
                font = ImageFont.load_default()
            d.multiline_text((70, 90), draw_text, fill=0, font=font, spacing=24)
        path = tmpdir / name
        img.save(path)
        return path

    printed = picture("printed.png", "HOMEWORK 5\nAlgebra tasks\nSolve problems 12 and 15")
    res = T.read_image(printed)
    check("печатная картинка прочитана", res.route == "tesseract", f"{res.route}/{res.reason} conf={res.conf}")
    check("текст непустой", bool(res.text and "HOMEWORK" in res.text.upper()), repr((res.text or "")[:60]))

    again = T.read_image(printed)
    check("повтор берётся из кеша", again.engine.startswith("cache:") and again.ms == 0, again.engine)

    blank = picture("blank.png")
    res2 = T.read_image(blank)
    check("пустое фото не читается", res2.route == "empty", f"{res2.route}/{res2.reason}")
    queued = T.queue_size()
    check("пустое фото не в очереди", queued == 0, f"в очереди {queued}")

    # рукописное имитируем уверенностью: картинка есть, но правило должно отклонить
    check("очередь пишется только для рукописного", T.accept(handwritten)[0] is False)
    T._queue(printed, handwritten, "рукописное")
    check("очередь: запись появилась", T.queue_size() == 1, str(T.queue_size()))
    T._queue(printed, handwritten, "рукописное")
    check("очередь: повтор не дублируется", T.queue_size() == 1, str(T.queue_size()))
    rows = [json.loads(l) for l in open(T.QUEUE, encoding="utf-8")]
    check("очередь: форма записи", rows[0]["shape"] == "handwriting" and rows[0]["conf"] == 41.0, rows[0]["shape"])

print(f"\n{'FAILED' if fails else 'all good'} — {fails} failure(s)")
sys.exit(1 if fails else 0)
