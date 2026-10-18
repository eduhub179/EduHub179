"""Что на фото: печатный текст, таблица чисел, рукописное или ничего.

Правило простое и без моделей: Tesseract пытается прочитать фото, а дальше решает не
«похоже на текст», а **уверенность самого Tesseract** — он отдаёт её по каждому слову.

    печатный листок      ~90 уверенности   -> текст берём
    рукописная доска     ~40 уверенности   -> текста нет, картинка уходит в board.py
    график, фото, скрин  0-2 слова          -> текста нет, в очередь не ставим

Порог 70 проверен на архиве учителя (402 документа без текстового слоя): принято 37 при
средней уверенности 82, отклонено 286 при медиане 43. Распределение двугорбое — печатное
даёт 80-95, рукописное 30-50, — а в полосе 60-79 нет ни одного домашнего задания, только
графики и скриншоты установщика. Порог поэтому стоит в пустоте, а не на глаз.

Распознанное не отправляется никуда: фото и текст остаются на этом компьютере.
Всё, что прочитать не удалось, но текстом быть может, пишется в `chats/ocr_queue.jsonl` —
это очередь для будущего движка (облачный OCR или модель), см. ocr_providers.py.
"""
import hashlib
import json
import os
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path

from eduhub_agent.paths import CHATS as PATHS_CHATS
from eduhub_agent.media import ocr_providers as providers

# Порог: 70. Проверено на архиве (402 документа): в полосе 60-79 нет ни одного домашнего
# задания — только графики и скриншоты установщика. Печатный листок даёт 85-95.
CONF_TEXT = float(os.environ.get("OCR_MIN_CONF", 70))
MIN_WORDS = 4           # как в прежнем правиле files.py («хотя бы 4 настоящих слова»)
MIN_NUMBERS = 10        # и в правиле для таблиц ответов
MIN_TABLE_WORDS = 2     # таблица ответов — это числа плюс подписи («625 Дж»), а не голая ось графика
CONF_TABLE = 60.0       # печатная таблица читается уверенно; мусор с цифрами — нет
MAX_SIDE = 2600         # больше не нужно: телефонное фото 4000 px читается дольше и не точнее
UPSCALE_TO = 1600       # мелкие фото увеличиваем, так Tesseract ошибается реже

CHATS = PATHS_CHATS
QUEUE = CHATS / "ocr_queue.jsonl"
CACHE = CHATS / "ocr_cache.json"
_cache = None


@dataclass
class Triage:
    text: object          # str или None
    route: str            # "tesseract" | "queued" | "empty"
    reason: str
    conf: float
    words: int
    numbers: int
    psm: int
    ms: int
    engine: str


def _sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def _load_cache():
    global _cache
    if _cache is None:
        try:
            _cache = json.load(open(CACHE, encoding="utf-8"))
        except (OSError, ValueError):
            _cache = {}
    return _cache


def _save_cache(key, row):
    cache = _load_cache()
    cache[key] = row
    try:
        CHATS.mkdir(exist_ok=True)
        json.dump(cache, open(CACHE, "w", encoding="utf-8"), ensure_ascii=False)
    except OSError:
        pass


def _queue(path, reading, reason):
    """Записать фото в очередь для будущего движка. Повторы (то же фото переслали ещё раз) не дублируем."""
    if not reading:
        return
    try:
        CHATS.mkdir(exist_ok=True)
        row = {"sha256": _sha256(path), "name": Path(path).name, "reason": reason,
               "conf": reading.mean_conf, "words": reading.words, "numbers": reading.numbers,
               "shape": "handwriting" if reading.words >= MIN_WORDS else "no_text",
               "ts": int(time.time())}
        seen = set()
        if QUEUE.exists():
            for line in open(QUEUE, encoding="utf-8"):
                try:
                    seen.add(json.loads(line)["sha256"])
                except (ValueError, KeyError):
                    continue
        if row["sha256"] not in seen:
            with open(QUEUE, "a", encoding="utf-8") as f:
                f.write(json.dumps(row, ensure_ascii=False) + "\n")
    except OSError:
        pass


def looks_like_text(reading):
    """Печатный текст: настоящих слов хватает. Качество чтения проверяет уверенность, не эта функция."""
    return reading.words >= MIN_WORDS


def looks_like_table(reading):
    """Распечатанная таблица чисел: слов мало, зато много чисел («625 Дж», «1:3:5»).

    Три ограничителя, без которых правило принимает мусор: подписи оси графика (монотонный
    ряд чисел), картинку без единого слова и неразборчивое фото с цифрами.
    """
    return (reading.numbers >= MIN_NUMBERS and reading.numbers > reading.words
            and reading.words >= MIN_TABLE_WORDS and reading.mean_conf >= CONF_TABLE
            and not reading.axis_like)


def accept(reading):
    """(берём ли текст, почему). Уверенность решает для текста, структура — для таблиц."""
    if not reading:
        return False, "не прочитано"
    if reading.axis_like:
        return False, "подписи оси графика"
    if looks_like_table(reading):
        return True, "таблица чисел"
    if looks_like_text(reading) and reading.mean_conf >= CONF_TEXT:
        return True, "печатный текст"
    if reading.words < MIN_WORDS:
        return False, "текста нет"
    return False, "рукописное или плохое фото"


def _prepare(path):
    """Повернуть по EXIF, привести к серому, поправить масштаб. Без Pillow — как есть."""
    try:
        from PIL import Image, ImageOps
    except ImportError:
        return str(path), None
    try:
        img = ImageOps.exif_transpose(Image.open(path)).convert("L")
        side = max(img.size)
        if side < UPSCALE_TO:
            k = UPSCALE_TO / side
            img = img.resize((int(img.width * k), int(img.height * k)), Image.LANCZOS)
        elif side > MAX_SIDE:
            k = MAX_SIDE / side
            img = img.resize((int(img.width * k), int(img.height * k)), Image.LANCZOS)
        tmp = os.path.join(tempfile.gettempdir(), f"ocr_{os.getpid()}_{Path(path).stem[:40]}.png")
        img.save(tmp)
        return tmp, tmp
    except Exception:
        return str(path), None


def read_image(path):
    """Разобрать одно фото. Возвращает Triage; текст в `.text` (или None)."""
    path = Path(path)
    key = _sha256(path)
    cached = _load_cache().get(key)
    if cached:
        return Triage(text=cached.get("text"), route=cached["route"], reason=cached.get("reason", "кеш"),
                      conf=cached.get("conf", 0.0), words=cached.get("words", 0), numbers=cached.get("numbers", 0),
                      psm=cached.get("psm", 0), ms=0, engine="cache:" + str(cached.get("engine", "?")))

    src, tmp = _prepare(path)
    try:
        engine = providers.get("local_tesseract")
        ok, why = engine.available()
        if not ok:
            print(f"   фото не распознать: {why}")
            return Triage(None, "queued", why, 0.0, 0, 0, 0, 0, "none")

        first = engine.read(src, psm=3)
        best, route, reason = first, "empty", "текста нет"
        if first:
            ok, why = accept(first)
            if ok:
                route, reason = "tesseract", why
            else:
                # Второй режим разметки: на части листков psm 3 не разбирает ни строки.
                second = engine.read(src, psm=6)
                if second and (second.mean_conf, second.words) > (first.mean_conf, first.words):
                    best = second
                ok2, why2 = accept(best)
                if ok2:
                    route, reason = "tesseract", why2
                elif best.words >= MIN_WORDS or best.numbers >= MIN_NUMBERS:
                    route, reason = "queued", why   # текст есть, но не читается — ждёт движок
                else:
                    route, reason = "empty", "текста нет"  # график, фото без записей — читать нечего
    finally:
        if tmp and os.path.exists(tmp):
            os.remove(tmp)

    text = best.text.strip() if (best and route == "tesseract") else None
    result = Triage(text=text, route=route, reason=reason, conf=best.mean_conf if best else 0.0,
                    words=best.words if best else 0, numbers=best.numbers if best else 0,
                    psm=best.psm if best else 0, ms=best.ms if best else 0,
                    engine=best.engine if best else "none")
    if route == "queued":
        _queue(path, best, reason)
    _save_cache(key, {"text": text, "route": route, "reason": reason, "conf": result.conf,
                      "words": result.words, "numbers": result.numbers, "psm": result.psm,
                      "engine": result.engine})
    return result


def image_text(path):
    """Текст с фото или None — то, что нужно files.extract_text()."""
    return read_image(path).text


def queue_size():
    """Сколько фото ждёт движок, которого пока нет."""
    if not QUEUE.exists():
        return 0
    n = 0
    for line in open(QUEUE, encoding="utf-8"):
        if line.strip():
            n += 1
    return n
