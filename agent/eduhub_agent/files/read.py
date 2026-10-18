"""Чтение файла: одна точка для папки, Google Drive и Telegram.

Раньше читалок было две (`files/files.py` и `ingest/extract.py`) с разными лимитами, и обе
останавливались на pypdf — то есть сканы не читались нигде. Здесь один вход:

    read(path) -> Reading(text, status, engine, pages, ocr_confidence)

status:
  native          — текстовый слой есть (PDF, DOCX, PPTX, XLSX, TXT)
  ocr             — текста не было, прочитали с картинки через ocr_triage
  scanned         — текста нет и OCR не уверен (рукописное, плохое фото)
  not_downloaded  — файла нет на диске (Drive в режиме «только ссылки»)
  binary          — формат, из которого текст не достаём
  empty           — пусто или битый файл

Сканы PDF: если текстового слоя меньше MIN_PDF_TEXT символов, страницы рендерятся
(`pdftoppm`) и уходят в ocr_triage. Ограничение — MAX_OCR_PAGES страниц: OCR дорогой,
а для классификации хватает начала документа.
"""
import re
import shutil
import subprocess
import tempfile
import zipfile
from dataclasses import dataclass
from html import unescape
from pathlib import Path

from eduhub_agent.files import ocr_triage

MAX_CHARS = 20000
MAX_DOC_PAGES = 40
MAX_OCR_PAGES = 3
MIN_PDF_TEXT = 40         # меньше — считаем, что текстового слоя нет
OCR_DPI = 200

TEXT_EXT = {".txt", ".md", ".csv", ".json", ".py", ".ipynb", ".tex", ".html", ".htm",
            ".js", ".ts", ".java", ".c", ".cpp", ".h", ".sh", ".bat", ".ps1", ".css"}
IMAGE_EXT = {".png", ".jpg", ".jpeg", ".gif", ".bmp", ".tif", ".tiff", ".webp"}
BINARY_EXT = {".mp4", ".avi", ".mov", ".mkv", ".mp3", ".wav", ".zip", ".rar", ".7z",
              ".gz", ".exe", ".dll", ".so", ".dylib", ".djvu", ".psd", ".stl",
              ".doc", ".ppt", ".xls"}          # старые форматы: нужен LibreOffice


@dataclass
class Reading:
    text: str = ""
    status: str = "empty"
    engine: str = "none"
    pages: int = 0
    ocr_confidence: float = None


def _clean(text):
    text = re.sub(r"[ \t\u00a0]+", " ", text)
    text = re.sub(r"\n\s*\n+", "\n", text)
    return text.strip()


def _xml(xml, para_tag):
    xml = re.sub(rf"</{para_tag}>", "\n", xml)
    return unescape(re.sub(r"<[^>]+>", "", xml))


def _pdf_text(path, max_pages):
    try:
        from pypdf import PdfReader
    except ImportError:
        PdfReader = None
    if PdfReader:
        try:
            reader = PdfReader(str(path))
            pages = reader.pages[:max_pages]
            return "\n".join((p.extract_text() or "") for p in pages), len(pages), "pypdf"
        except Exception:
            return "", 0, "pypdf"
    if shutil.which("pdftotext"):
        try:
            r = subprocess.run(["pdftotext", "-l", str(max_pages), "-enc", "UTF-8", str(path), "-"],
                               capture_output=True, timeout=120)
            return r.stdout.decode("utf-8", "replace"), 0, "pdftotext"
        except (subprocess.TimeoutExpired, OSError):
            return "", 0, "pdftotext"
    return "", 0, "none"


def render_pdf(path, limit=MAX_OCR_PAGES, dpi=OCR_DPI):
    """Страницы PDF → PNG. Возвращает (список путей, временная папка или None)."""
    if not shutil.which("pdftoppm"):
        return [], None
    td = tempfile.mkdtemp(prefix="read_")
    try:
        subprocess.run(["pdftoppm", "-r", str(dpi), "-gray", "-png", "-f", "1", "-l", str(limit),
                        str(path), f"{td}/p"], capture_output=True, timeout=300, check=False)
        pages = sorted(Path(td).glob("p-*.png")) or sorted(Path(td).glob("p*.png"))
        return pages, td
    except (subprocess.TimeoutExpired, OSError):
        return [], td


def _pdf(path, ocr, max_pages, max_ocr_pages):
    text, pages, engine = _pdf_text(path, max_pages)
    text = _clean(text)
    if len(text) >= MIN_PDF_TEXT or not ocr:
        return Reading(text[:MAX_CHARS], "native" if text else "scanned", engine, pages, None)

    # текстового слоя нет — пробуем прочитать картинки страниц
    page_images, td = render_pdf(path, max_ocr_pages)
    try:
        if not page_images:
            return Reading(text, "scanned", engine, pages, None)
        parts, confs, engine = [], [], "tesseract"
        for img in page_images:
            r = ocr_triage.read_image(img)
            if r.text:
                parts.append(r.text)
                confs.append(r.conf)
        joined = _clean("\n\n".join(parts))
        if not joined:
            return Reading("", "scanned", engine, len(page_images), None)
        return Reading(joined[:MAX_CHARS], "ocr", engine, len(page_images),
                       round(sum(confs) / len(confs), 1) if confs else None)
    finally:
        if td:
            shutil.rmtree(td, ignore_errors=True)


def _image(path):
    r = ocr_triage.read_image(path)
    if r.route == "tesseract" and r.text:
        return Reading(_clean(r.text)[:MAX_CHARS], "ocr", r.engine, 1, r.conf)
    if r.route == "queued":
        return Reading("", "scanned", r.engine, 1, r.conf)
    return Reading("", "empty", r.engine, 1, r.conf)


def _docx(path):
    with zipfile.ZipFile(path) as z:
        return _xml(z.read("word/document.xml").decode("utf-8", "replace"), "w:p"), "docx"


def _pptx(path):
    with zipfile.ZipFile(path) as z:
        slides = sorted((n for n in z.namelist() if re.match(r"ppt/slides/slide\d+\.xml$", n)),
                        key=lambda n: int(re.search(r"\d+", n.rsplit("/", 1)[1])[0]))
        return "\n".join(_xml(z.read(n).decode("utf-8", "replace"), "a:p") for n in slides), "pptx"


def _xlsx(path):
    with zipfile.ZipFile(path) as z:
        try:
            shared = z.read("xl/sharedStrings.xml").decode("utf-8", "replace")
        except KeyError:
            return "", "xlsx"
        return _xml(shared, "si"), "xlsx"


def read(path, ocr=True, max_pages=MAX_DOC_PAGES, max_ocr_pages=MAX_OCR_PAGES,
         max_chars=MAX_CHARS):
    """Текст из файла и честный статус того, откуда он взялся. Никогда не бросает."""
    path = Path(path)
    ext = path.suffix.lower()
    if not path.exists():
        return Reading("", "not_downloaded", "none", 0, None)
    if ext in BINARY_EXT:
        return Reading("", "binary", "none", 0, None)
    try:
        if ext == ".pdf":
            return _pdf(path, ocr, max_pages, max_ocr_pages)
        if ext in IMAGE_EXT:
            return _image(path) if ocr else Reading("", "scanned", "none", 1, None)
        if ext == ".docx":
            text, engine = _docx(path)
        elif ext == ".pptx":
            text, engine = _pptx(path)
        elif ext == ".xlsx":
            text, engine = _xlsx(path)
        elif ext in TEXT_EXT:
            text, engine = path.read_text(encoding="utf-8", errors="ignore"), "text"
        else:
            return Reading("", "binary", "none", 0, None)
        text = _clean(text)[:max_chars]
        return Reading(text, "native" if text else "empty", engine, 1, None)
    except (OSError, zipfile.BadZipFile, KeyError, UnicodeDecodeError):
        return Reading("", "empty", "none", 0, None)
