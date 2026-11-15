"""Вложения Telegram: чтение через общий read.py и скачивание файлов в живом режиме.

Текст даёт общий читатель `files/read.py` — тот же, что у папки и у Drive: PDF (с OCR для сканов),
DOCX, PPTX, XLSX, TXT и фото через `ocr_triage` (берётся только то, в чём Tesseract уверен).
Здесь остаётся только то, что относится к Telegram: скачивание файла и подпись «распознано с фото».
Скачанные файлы лежат в chats/files/<чат>/ на этом компьютере и никуда не отправляются.
"""
import json
import os
import re
import shutil
import urllib.request
from pathlib import Path

from eduhub_agent.files.read import read


MAX_CHARS = 2000  # столько текста файла видит модель (начало файла: там обычно номер листка, задания и срок)
MAX_PAGES = 5
TG_LIMIT = 20 * 1024 * 1024  # бот может скачать файл до 20 МБ
_warned = set()


def _warn_once(key, text):
    if key not in _warned:
        _warned.add(key)
        print(text)


IMAGES = {".jpg", ".jpeg", ".png", ".webp", ".bmp", ".tif", ".tiff"}


def extract_text(path):
    """Начало текста файла (до MAX_CHARS) или None, если текста нет или формат нечитаемый."""
    r = read(path, max_chars=MAX_CHARS)
    if r.status not in ("native", "ocr"):
        return None
    return r.text if len(r.text) >= 20 else None


def read_many(paths):
    """[(путь, Reading)] — один проход по вложениям, статус и текст вместе."""
    return [(p, read(p)) for p in (paths or [])]


def texts_from(readings, names=None):
    """[(имя, текст)] из уже прочитанных файлов. У фото в имени помечено, что это OCR."""
    out = []
    for i, (p, r) in enumerate(readings or []):
        if not r.text or r.status not in ("native", "ocr"):
            continue
        name = (names[i] if names and i < len(names) else None) or Path(p).name
        if r.status == "ocr":
            name += ", распознано с фото, возможны ошибки"
        out.append((name, r.text))
    return out


def texts_for(paths, names=None):
    """Текст вложений. Фото читаются всегда — переключателя «не распознавать» больше нет."""
    return texts_from(read_many(paths), names)


def safe_name(s):
    return re.sub(r"[^\w.\- ]+", "_", s).strip(" ._")[:80] or "file"


def download(api, file_id, dest_dir, name):
    """Скачать файл из Telegram по file_id в dest_dir. api — "https://api.telegram.org/bot<токен>/".
    Возвращает путь или None (файл больше 20 МБ, нет сети)."""
    try:
        with urllib.request.urlopen(api + "getFile?file_id=" + file_id, timeout=30) as r:
            info = json.loads(r.read())["result"]
        if info.get("file_size", 0) > TG_LIMIT:
            print(f"   файл {name} больше 20 МБ, бот его скачать не может")
            return None
        dest_dir = Path(dest_dir)
        dest_dir.mkdir(parents=True, exist_ok=True)
        ext = Path(info["file_path"]).suffix
        dest = dest_dir / (safe_name(name) if Path(name).suffix else safe_name(name) + ext)
        url = api.replace("/bot", "/file/bot", 1) + info["file_path"]
        with urllib.request.urlopen(url, timeout=120) as r, open(dest, "wb") as f:
            shutil.copyfileobj(r, f)
        return dest
    except Exception as e:
        print(f"   не смог скачать {name}: {e}")
        return None
