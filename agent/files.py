"""Вложения для агента: текст из PDF, DOCX, PPTX и TXT и скачивание файлов из Telegram в живом режиме.

Текст файла показывается модели вместе с сообщением, поэтому «ДЗ.pdf» без подписи теперь понятно по содержимому.
PDF читается библиотекой pypdf (один раз: .venv/bin/pip install pypdf) или, если её нет, программой pdftotext
(brew install poppler). DOCX и PPTX читаются без библиотек.
Фото распознаёт Tesseract (brew install tesseract; русский язык берётся из папки tessdata рядом со скриптом
или из brew install tesseract-lang). Он хорошо читает распечатанные листки и скриншоты, а рукописную доску плохо:
если распознался мусор, текст фото модели не показывается.
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
    """Текст с фото через Tesseract (русский + английский). Мелкие фото увеличиваем: так он ошибается реже."""
    exe = shutil.which("tesseract")
    if not exe:
        _warn_once("ocr", "Фото не распознаются: нужен Tesseract (brew install tesseract)")
        return None
    src, tmp = str(path), None
    try:
        from PIL import Image, ImageOps
        img = ImageOps.exif_transpose(Image.open(path)).convert("L")  # повёрнутые фото с телефона
        if max(img.size) < 1600:
            k = 1600 / max(img.size)
            img = img.resize((int(img.width * k), int(img.height * k)))
        tmp = str(Path(path).with_suffix(".ocr.png").name)
        tmp = os.path.join(os.environ.get("TMPDIR", "/tmp"), tmp)
        img.save(tmp)
        src = tmp
    except Exception:
        pass  # без Pillow отдаём файл как есть
    env = dict(os.environ)
    if (TESSDATA / "rus.traineddata").exists():
        env["TESSDATA_PREFIX"] = str(TESSDATA)
    try:
        r = subprocess.run([exe, src, "-", "-l", "rus+eng", "--psm", "3"], capture_output=True, timeout=120, env=env)
    finally:
        if tmp and os.path.exists(tmp):
            os.remove(tmp)
    if r.returncode:
        _warn_once("ocr-lang", "Tesseract не запустился (нет русского языка? brew install tesseract-lang): "
                   + r.stderr.decode("utf-8", "replace")[:200])
        return None
    text = r.stdout.decode("utf-8", "replace")
    return text if looks_like_text(text) or looks_like_table(text) else None


def looks_like_text(text):
    """Отсекаем мусор распознавания: нужно хотя бы 4 настоящих слова, и они должны быть основой текста."""
    words = re.findall(r"[А-Яа-яЁёA-Za-z]{3,}", text)
    letters = sum(len(w) for w in words)
    junk = len(re.sub(r"\s", "", text))
    return len(words) >= 4 and letters >= 0.5 * junk


def looks_like_table(text):
    """Распечатанная таблица ответов или данных: слов мало, зато много чисел («625 Дж», «1:3:5»). Без этого правила
    такие фото считались «без текста» и уходили в проверку доски, а модель для картинок принимала их за доску."""
    nums = len(re.findall(r"\d+(?:[.,:]\d+)*", text))
    words = len(re.findall(r"[А-Яа-яЁёA-Za-z]{3,}", text))
    meaningful = len(re.findall(r"[\dА-Яа-яЁёA-Za-z]", text))
    return nums >= 10 and nums > words and meaningful >= 0.6 * len(re.sub(r"\s", "", text))


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
