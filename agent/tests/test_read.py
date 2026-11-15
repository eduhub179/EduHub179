"""Матрица чтения: один и тот же read() для папки, Drive и Telegram.

python3 tests/test_read.py

Проверяем не качество OCR, а то, что каждый формат даёт честный статус и текст:
  txt/docx/pptx/xlsx → native, картинка с печатью → ocr, пустое → empty,
  старый .doc и exe → binary, отсутствующий файл → not_downloaded,
  PDF без текстового слоя → уходит в рендер + OCR (scanned, если читать нечего).
"""
import sys
import tempfile
import zipfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))

from eduhub_agent.files.read import read  # noqa: E402

fails = 0


def check(name, cond, detail=""):
    global fails
    print(f"{'OK  ' if cond else 'FAIL'} {name}{(' :: ' + str(detail)) if detail else ''}")
    fails += not cond


td = Path(tempfile.mkdtemp())

# --- простые форматы ------------------------------------------------------------------
txt = td / "a.txt"
txt.write_text("Домашнее задание по алгебре, номера 5 и 6", encoding="utf-8")
r = read(txt)
check("txt → native", (r.status, r.engine) == ("native", "text"), (r.status, r.engine))

docx = td / "b.docx"
with zipfile.ZipFile(docx, "w") as z:
    z.writestr("word/document.xml",
               "<w:document><w:body><w:p><w:r><w:t>Домашнее задание по геометрии</w:t>"
               "</w:r></w:p></w:body></w:document>")
r = read(docx)
check("docx → native", r.status == "native" and "геометрии" in r.text, (r.status, r.text[:30]))

pptx = td / "c.pptx"
with zipfile.ZipFile(pptx, "w") as z:
    z.writestr("ppt/slides/slide1.xml", "<p:sld><a:p><a:r><a:t>Презентация к уроку</a:t></a:r></a:p></p:sld>")
r = read(pptx)
check("pptx → native", r.status == "native" and "Презентация" in r.text, (r.status, r.text[:30]))

exe = td / "d.exe"
exe.write_bytes(b"MZ\x90\x00")
check("exe → binary", read(exe).status == "binary")
check("нет файла → not_downloaded", read(td / "нет.pdf").status == "not_downloaded")

old_doc = td / "e.doc"
old_doc.write_bytes(b"\xd0\xcf\x11\xe0")
check("старый .doc → binary", read(old_doc).status == "binary")

empty = td / "f.txt"
empty.write_text("", encoding="utf-8")
check("пустой текстовый → empty", read(empty).status == "empty")

# --- картинки: OCR ---------------------------------------------------------------------
try:
    from PIL import Image, ImageDraw, ImageFont
    img = Image.new("L", (1200, 400), 255)
    d = ImageDraw.Draw(img)
    try:
        font = ImageFont.load_default(size=52)
    except TypeError:
        font = ImageFont.load_default()
    # в картинке должно быть не меньше 4 настоящих слов: ocr_triage отбрасывает мелочь
    d.multiline_text((40, 40), "HOMEWORK 5\nALGEBRA TASKS\nSOLVE PROBLEMS 12 15", fill=0, font=font,
                     spacing=18)
    printed = td / "g.png"
    img.save(printed)
    r = read(printed)
    check("картинка с печатью → ocr", r.status == "ocr" and "HOMEWORK" in r.text.upper(),
          (r.status, r.text[:30], r.ocr_confidence))
    check("у OCR есть уверенность", isinstance(r.ocr_confidence, float), r.ocr_confidence)

    blank = td / "h.png"
    Image.new("L", (600, 400), 255).save(blank)
    check("пустая картинка → empty", read(blank).status == "empty", read(blank).status)
except ImportError:
    print("SKIP картинки: нет Pillow")

# --- PDF без текстового слоя → рендер + OCR ---------------------------------------------


def make_blank_pdf(path):
    """Минимальный валидный PDF на одну страницу, без текста. Без внешних библиотек."""
    objs = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Contents 4 0 R >>",
        b"<< /Length 0 >>\nstream\n\nendstream",
    ]
    out = bytearray(b"%PDF-1.4\n")
    offsets = []
    for i, body in enumerate(objs, 1):
        offsets.append(len(out))
        out += f"{i} 0 obj\n".encode() + body + b"\nendobj\n"
    xref = len(out)
    out += f"xref\n0 {len(objs) + 1}\n".encode() + b"0000000000 65535 f \n"
    for off in offsets:
        out += f"{off:010d} 00000 n \n".encode()
    out += f"trailer\n<< /Size {len(objs) + 1} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n".encode()
    Path(path).write_bytes(bytes(out))


pdf = td / "i.pdf"
make_blank_pdf(pdf)
r = read(pdf)
check("скан PDF: не молчит, а идёт в OCR", (r.status, r.engine, r.pages) == ("scanned", "tesseract", 1),
      (r.status, r.engine, r.pages))

print(f"\n{'FAILED' if fails else 'all good'} — {fails} failure(s)")
sys.exit(1 if fails else 0)
