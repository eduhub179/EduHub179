"""Правила классификации: предмет и тип материала.

Это тот же классификатор, что в архивном треке P2 (subject 77%, type 54% у наивной версии;
эта версия доведена до чисел scoreboard). Метки: subject ∈ {algebra, geometry, calculus,
digital_tech, misc}, type ∈ {worksheet, homework, test, independent_work, notes, textbook,
program, other} — ровно те, что в gold.

Почему правила, а не модель: на 280 размеченных файлах правила дают ~77% совместной
точности, gemma3:4b — 73%, qwen2.5:3b — 71%, при 0.05 с против 10 с на файл. Гибрид
(правила + модель только на неуверенных) — 88.6%, и это следующий шаг, а не первый.
"""
import json
import os
import re
from pathlib import Path

DEFAULT_CATEGORIES = Path(__file__).with_name("categories.json")
_categories = None


def load_categories(path=None):
    """Категории из JSON. Новый предмет или тип — это строка в файле, а не правка кода.

    Формат: {"subjects": {код: [синонимы]}, "types": {...}, "fallback": {...}}.
    Порядок ключей в "types" важен: worksheet проверяется раньше остальных, чтобы
    «Листок 16. Самостоятельная работа» не стал листком (см. type_of).
    """
    global _categories
    p = Path(path or os.environ.get("EDUHUB_CATEGORIES") or DEFAULT_CATEGORIES)
    data = json.loads(p.read_text(encoding="utf-8"))
    for key in ("subjects", "types"):
        if not data.get(key):
            raise ValueError(f"{p}: пустой раздел {key}")
    data.setdefault("fallback", {"subject": "misc", "type": "other"})
    _categories = data
    return data


def categories():
    return _categories if _categories is not None else load_categories()


def set_categories(data):
    global _categories
    _categories = data


# «9 класс», «9 кл.»
GRADE_WORD = re.compile(r"(?<!\d)(\d{1,2})\s*(?:класс|кл\.?\b)", re.I)
# «8Б», «10 А» — только заглавная буква и не часть слова (лист04д — номер листка, не класс)
GRADE_LETTER = re.compile(r"(?<![\d])(\d{1,2})\s*[-–]?\s*([А-ЯЁ])(?![А-Яа-яЁё])")

PROGRAM_EXT = {".py", ".ipynb", ".js", ".ts", ".java", ".c", ".cpp"}
NOISE_DIRS = {"не в папках", "мой диск", "диск", "classroom", "корзина"}


def _norm(s):
    """Нижний регистр, ё→е, разделители → пробел: «Тр-ДЗ-001» и «тр дз 001» сравнимы."""
    s = s.lower().replace("ё", "е")
    return re.sub(r"[_\-–—.,()\[\]{}]+", " ", s)


def _hit(words, text):
    """Самое длинное совпадение из списка; возвращает (слово, уверенность)."""
    t = " " + _norm(text) + " "
    best = (None, "none")
    for w in words:
        n = _norm(w).strip()
        if not n:
            continue
        if re.search(rf"(?<![а-яa-z0-9]){re.escape(n)}(?![а-яa-z0-9])", t):
            if best[0] is None or len(n) > len(best[0]):
                best = (w, "high")
        elif n in t and best[1] != "high":
            best = (w, "medium")
    return best


def _type_order():
    """Порядок проверки: как в JSON, «other» последний, worksheet — после остальных.

    «Листок 16. Самостоятельная работа» должен стать самостоятельной работой, а не листком,
    поэтому самый общий тип проверяется последним среди содержательных.
    """
    fallback = categories()["fallback"]["type"]
    order = [k for k in categories()["types"] if k != fallback]
    if "worksheet" in order:
        order.remove("worksheet")
        order.append("worksheet")
    return order


def _type_words(kind):
    return categories()["types"].get(kind, [])


def subject_of(path, name):
    """Предмет: папки снизу вверх, потом имя. «Разное» — это misc, а не отсутствие предмета."""
    parts = [p for p in path.split("/")[:-1] if p.lower() not in NOISE_DIRS]
    subjects = categories()["subjects"]
    for part in reversed(parts):
        for key, words in subjects.items():
            if _hit(words, part)[1] == "high":
                return key, "high"
    for key, words in subjects.items():
        if _hit(words, name)[1] == "high":
            return key, "high"
    for part in reversed(parts):
        for key, words in subjects.items():
            if _hit(words, part)[0]:
                return key, "medium"
    return "unknown", "none"


def _grade_in(text, letters=True):
    for rx in ([GRADE_WORD, GRADE_LETTER] if letters else [GRADE_WORD]):
        m = rx.search(text)
        if m:
            n = int(m.group(1))
            if 1 <= n <= 11:                 # 12-й параллели не бывает
                return n
    return None


def grade_of(path, name):
    """Класс: сначала папки (надёжно), потом имя — и в имени только «N класс».

    Порядок важен: «Матан/9 класс/лист12д--...pdf» — это 9 класс, а не 12-й.
    """
    for part in reversed(path.split("/")[:-1]):
        found = _grade_in(part)
        if found:
            return found
    return _grade_in(name, letters=False)


def type_of(path, name, text=""):
    """Тип: имя файла важнее папки, расширение — последний аргумент.

    Порядок проверок не случаен: «Листок 16. Самостоятельная работа» — это самостоятельная,
    а не листок, поэтому worksheet идёт последним из «содержательных» типов.
    """
    order = _type_order()
    for kind in order:
        hit, conf = _hit(_type_words(kind), name)
        if hit:
            return kind, conf
    # вложения сохранённых страниц (*_files/) — не материал, а картинки и скрипты страницы
    if any(part.endswith("_files") for part in path.split("/")[:-1]):
        return "other", "high"
    for part in reversed(path.split("/")[:-1]):
        for kind in order:
            hit, conf = _hit(_type_words(kind), part)
            if hit:
                return kind, conf
    ext = ("." + name.rsplit(".", 1)[-1].lower()) if "." in name else ""
    if ext in PROGRAM_EXT:
        return "program", "high"
    if text:
        for kind in order:
            if _hit(_type_words(kind), text[:400])[0]:
                return kind, "medium"
    return "other", "low"


def confidence(subject_conf, type_conf):
    """Можно ли раскладывать файл без человека.

    Предмет важнее типа: без предмета файл некуда положить, а тип «other» — нормальный
    ответ (листок с ребусами и правда не ДЗ). Поэтому «предмет ясен, тип нет» — medium.
    """
    if subject_conf == "high":
        return "high" if type_conf in ("high", "medium") else "medium"
    if subject_conf == "medium":
        return "medium"
    return "low" if type_conf != "none" else "unknown"


def classify(path, name, text=""):
    subject, s_conf = subject_of(path, name)
    kind, t_conf = type_of(path, name, text)
    # «Основы цифровых технологий» — папка целиком из листков и учебных материалов;
    # если по имени тип не виден, это листок, а не «other».
    if kind == "other" and subject == "digital_tech":
        kind, t_conf = "worksheet", "medium"
    return {
        "subject": subject,
        "type": kind,
        "grade": grade_of(path, name),
        "subject_conf": s_conf,
        "type_conf": t_conf,
        "confidence": confidence(s_conf, t_conf),
    }
