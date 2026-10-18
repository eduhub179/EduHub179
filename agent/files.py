"""Вложения для агента: текст из PDF, DOCX, PPTX и TXT и скачивание файлов из Telegram в живом режиме.

Текст файла показывается модели вместе с сообщением, поэтому «ДЗ.pdf» без подписи теперь понятно по содержимому.
PDF читается библиотекой pypdf (один раз: .venv/bin/pip install pypdf) или, если её нет, программой pdftotext
(brew install poppler). DOCX и PPTX читаются без библиотек.
Фото распознаёт Tesseract (brew install tesseract; русский язык берётся из папки tessdata рядом со скриптом
или из brew install tesseract-lang) через ocr_triage: текст берётся, только если Tesseract уверен в прочитанном.
Рукописную доску он читает с уверенностью ~40 и текст не отдаёт — такое фото уходит в board.py.
Скачанные файлы лежат в chats/files/<чат>/ на этом компьютере и никуда не отправляются.
"""
import json
import os
import re
import shutil
import subprocess
import urllib.request
import zipfile
from html import unescape
from pathlib import Path

MAX_CHARS = 2000  # столько текста файла видит модель (начало файла: там обычно номер листка, задания и срок)
MAX_PAGES = 5
TG_LIMIT = 20 * 1024 * 1024  # бот может скачать файл до 20 МБ
_warned = set()


def _warn_once(key, text):
    if key not in _warned:
        _warned.add(key)
        print(text)


def _clean(text):
    text = re.sub(r"[ \t ]+", " ", text)
    text = re.sub(r"\n\s*\n+", "\n", text)
    return text.strip()


def _pdf(path):
    try:
        from pypdf import PdfReader
    except ImportError:
        PdfReader = None
    if PdfReader:
        try:
            reader = PdfReader(str(path))
            return "\n".join((p.extract_text() or "") for p in reader.pages[:MAX_PAGES])
        except Exception as e:  # битый или зашифрованный PDF
            print(f"   PDF {Path(path).name} не читается: {e}")
            return None
    if shutil.which("pdftotext"):
        r = subprocess.run(["pdftotext", "-l", str(MAX_PAGES), "-enc", "UTF-8", str(path), "-"],
                           capture_output=True, timeout=60)
        return r.stdout.decode("utf-8", "replace")
    _warn_once("pdf", "Текст из PDF не читается: нужна pypdf (.venv/bin/pip install pypdf) или pdftotext (brew install poppler)")
    return None


TESSDATA = Path(__file__).parent / "tessdata"
IMAGES = {".jpg", ".jpeg", ".png", ".webp", ".bmp", ".tif", ".tiff"}


def _image(path):
    """Текст с фото через ocr_triage (Tesseract + порог уверенности). None — если текста на фото нет."""
    try:
        import ocr_triage
    except ImportError as e:  # ocr_triage.py лежит рядом
        _warn_once("ocr-triage", f"Фото не распознаются: не найден ocr_triage.py ({e})")
        return None
    try:
        return ocr_triage.image_text(path)
    except Exception as e:
        print(f"   не смог распознать фото {Path(path).name}: {e}")
        return None


def _xml_text(xml, para_tag):
    xml = re.sub(rf"</{para_tag}>", "\n", xml)
    return unescape(re.sub(r"<[^>]+>", "", xml))


def _docx(path):
    with zipfile.ZipFile(path) as z:
        return _xml_text(z.read("word/document.xml").decode("utf-8", "replace"), "w:p")


def _pptx(path):
    with zipfile.ZipFile(path) as z:
        slides = sorted((n for n in z.namelist() if re.match(r"ppt/slides/slide\d+\.xml$", n)),
                        key=lambda n: int(re.search(r"\d+", n.rsplit("/", 1)[1])[0]))
        return "\n".join(_xml_text(z.read(n).decode("utf-8", "replace"), "a:p") for n in slides)


READERS = {**{e: _image for e in IMAGES}, ".pdf": _pdf, ".docx": _docx, ".pptx": _pptx,
           ".txt": lambda p: Path(p).read_text(encoding="utf-8", errors="replace"),
           ".md": lambda p: Path(p).read_text(encoding="utf-8", errors="replace")}


def extract_text(path):
    """Начало текста файла (до MAX_CHARS) или None, если такой формат не читаем или текста нет (скан, фото)."""
    path = Path(path)
    reader = READERS.get(path.suffix.lower())
    if not reader or not path.exists():
        return None
    try:
        text = reader(path)
    except Exception as e:
        print(f"   не смог прочитать {path.name}: {e}")
        return None
    text = _clean(text or "")
    return text[:MAX_CHARS] if len(text) >= 20 else None  # почти пусто — скорее всего скан


def texts_for(paths, names=None, ocr=True):
    """[(имя файла, текст)] для читаемых вложений. У фото в имени помечено, что текст распознан и может быть с ошибками.
    ocr=False — фото не распознавать (заглушка для беты: chat_agent.py, PHOTO_MODE=stub)."""
    out = []
    for i, p in enumerate(paths or []):
        if not ocr and Path(p).suffix.lower() in IMAGES:
            continue
        t = extract_text(p)
        if t:
            name = (names[i] if names and i < len(names) else None) or Path(p).name
            if Path(p).suffix.lower() in IMAGES:
                name += ", распознано с фото, возможны ошибки"
            out.append((name, t))
    return out


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
