"""Текст из файла — без тяжёлых зависимостей.

PDF: pypdf, если стоит, иначе pdftotext, иначе считаем, что текста нет (скан).
DOCX/PPTX/XLSX: обычный zip + вырезание XML.
Прочее: читаем как текст, если расширение текстовое.

Возвращает (текст, статус), где статус — одно из:
  "native"   текстовый слой есть
  "scanned"  PDF без текстового слоя (скан или фото внутри)
  "binary"   картинка/архив/исполняемый — текста нет и не будет
  "empty"    файл пустой или не читается
"""
import re
import shutil
import subprocess
import zipfile
from html import unescape
from pathlib import Path

TEXT_EXT = {".txt", ".md", ".csv", ".json", ".py", ".ipynb", ".tex", ".html", ".htm",
            ".js", ".ts", ".java", ".c", ".cpp", ".h", ".sh", ".bat", ".ps1", ".css"}
IMAGE_EXT = {".png", ".jpg", ".jpeg", ".gif", ".bmp", ".tif", ".tiff", ".webp", ".heic"}
BINARY_EXT = {".mp4", ".avi", ".mov", ".mkv", ".mp3", ".wav", ".zip", ".rar", ".7z",
              ".gz", ".exe", ".dll", ".so", ".dylib", ".djvu", ".psd", ".stl"}
MAX_CHARS = 20000


def _clean(text):
    text = re.sub(r"[ \t\u00a0]+", " ", text)
    text = re.sub(r"\n\s*\n+", "\n", text)
    return text.strip()


def _xml(xml, para_tag):
    xml = re.sub(rf"</{para_tag}>", "\n", xml)
    return unescape(re.sub(r"<[^>]+>", "", xml))


def _pdf(path):
    try:
        from pypdf import PdfReader
    except ImportError:
        PdfReader = None
    if PdfReader:
        try:
            reader = PdfReader(str(path))
            return "\n".join((p.extract_text() or "") for p in reader.pages[:40])
        except Exception:
            return ""
    if shutil.which("pdftotext"):
        try:
            r = subprocess.run(["pdftotext", "-l", "40", "-enc", "UTF-8", str(path), "-"],
                               capture_output=True, timeout=120)
            return r.stdout.decode("utf-8", "replace")
        except (subprocess.TimeoutExpired, OSError):
            return ""
    return ""


def _docx(path):
    with zipfile.ZipFile(path) as z:
        return _xml(z.read("word/document.xml").decode("utf-8", "replace"), "w:p")


def _pptx(path):
    with zipfile.ZipFile(path) as z:
        slides = sorted((n for n in z.namelist() if re.match(r"ppt/slides/slide\d+\.xml$", n)),
                        key=lambda n: int(re.search(r"\d+", n.rsplit("/", 1)[1])[0]))
        return "\n".join(_xml(z.read(n).decode("utf-8", "replace"), "a:p") for n in slides)


def _xlsx(path):
    """Только текстовые ячейки: numbers в sharedStrings тоже строки, этого достаточно для классификации."""
    with zipfile.ZipFile(path) as z:
        try:
            shared = z.read("xl/sharedStrings.xml").decode("utf-8", "replace")
        except KeyError:
            return ""
        return _xml(shared, "si")


def extract(path):
    """(текст, статус). Никогда не бросает: битый файл — это "empty", а не падение прогона."""
    path = Path(path)
    ext = path.suffix.lower()
    if ext in BINARY_EXT:
        return "", "binary"
    if ext in IMAGE_EXT:
        return "", "scanned"
    try:
        if ext == ".pdf":
            text = _pdf(path)
            text = _clean(text)
            return text[:MAX_CHARS], ("native" if len(text) >= 40 else "scanned")
        if ext == ".docx":
            return _clean(_docx(path))[:MAX_CHARS], "native"
        if ext == ".pptx":
            return _clean(_pptx(path))[:MAX_CHARS], "native"
        if ext == ".xlsx":
            text = _clean(_xlsx(path))
            return text[:MAX_CHARS], ("native" if text else "empty")
        if ext in TEXT_EXT:
            text = _clean(path.read_text(encoding="utf-8", errors="ignore"))
            return text[:MAX_CHARS], ("native" if text else "empty")
        if ext in (".doc", ".ppt", ".xls"):
            return "", "binary"          # старый формат: нужен libreoffice, пока не читаем
        return "", "binary"
    except (OSError, zipfile.BadZipFile, KeyError):
        return "", "empty"
