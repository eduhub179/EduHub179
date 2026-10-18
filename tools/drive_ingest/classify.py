"""Правила классификации: предмет, класс, тип материала.

Почему правила, а не модель: на 280 размеченных файлах архивного трека правила дают
~77% совместной точности, gemma3:4b — 73%, qwen2.5:3b — 71%, и это при 0.05 с против
10 с на файл. Гибрид (правила + модель только на неуверенных) — 88.6%, но модель пока
не нужна: неуверенные файлы дешевле показать человеку списком.

Правила читают путь (папки!), имя файла и, если есть, текст — в этом порядке.
"""
import re

SUBJECTS = {
    "algebra": ["алгебра", "алг"],
    "geometry": ["геометрия", "геом"],
    "calculus": ["матан", "мат. анализ", "математический анализ", "матанализ"],
    "digital_tech": ["цифровые технологии", "основы цифровых", "опцт", "ит"],
    "programming": ["python", "питон", "программирование", "информатика", "colab"],
    "physics": ["физика"],
    "chemistry": ["химия"],
    "biology": ["биология", "зоология", "анатомия"],
    "history": ["история"],
    "russian": ["русский"],
    "literature": ["литература", "чтение"],
    "english": ["английский", "english"],
    "geography": ["география"],
    "social": ["обществознание"],
    "other_subject": ["разное", "не в папках"],
}

TYPES = {
    "homework": ["дз", "домашнее задание", "домашка", "homework"],
    "test": ["контрольная", "кр", "зачёт", "зачет", "экзамен", "test"],
    "independent_work": ["самостоятельная", "ср", "отработка"],
    "notes": ["конспект", "урок", "лекция", "notes"],
    "worksheet": ["листок", "задачник", "тренировка", "worksheet", "карточк"],
    "presentation": ["презентация", "слайд", "presentation"],
    "textbook": ["учебник", "пособие"],
    "program": ["программа", "program", "скрипт", "project"],
}

# «9 класс», «9 кл.»
GRADE_WORD = re.compile(r"(?<!\d)(\d{1,2})\s*(?:класс|кл\.?\b)", re.I)
# «8Б», «10 А» — только заглавная буква и не часть слова (лист04д — номер листка, не класс)
GRADE_LETTER = re.compile(r"(?<![\d])(\d{1,2})\s*[-–]?\s*([А-ЯЁ])(?![А-Яа-яЁё])")

# папки, которые ничего не говорят о предмете
NOISE_DIRS = {"не в папках", "разное", "мой диск", "диск", "classroom", "корзина"}


def _tokens(s):
    return re.sub(r"[_\-–—.,()\[\]]+", " ", s.lower())


def _match(table, text):
    """Первое совпадение по таблице: возвращает (ключ, насколько уверенно)."""
    t = _tokens(text)
    best = (None, "none")
    for key, words in table.items():
        for w in words:
            if not w:
                continue
            if re.search(rf"(^|[\s/]){re.escape(w)}($|[\s/])", t) or w in t.split():
                return key, "high"
            if w in t and best[1] == "none":
                best = (key, "medium")        # вхождение внутри слова — слабее
    return best


def subject_of(path, name):
    """Предмет: сначала папки (снизу вверх), потом имя файла.

    «Разное» и «Не в папках» — не предмет, а свалка: файл оттуда получает other_subject
    и уходит человеку, даже если правила что-то нашли в имени.
    """
    noise = False
    for part in reversed(path.split("/")[:-1]):
        if part.lower() in NOISE_DIRS:
            noise = True
            continue
        key, conf = _match(SUBJECTS, part)
        if key and conf == "high" and key != "other_subject":
            return key, "high"
    key, conf = _match(SUBJECTS, name)
    if key and key != "other_subject":
        return key, conf
    for part in reversed(path.split("/")[:-1]):
        key, conf = _match(SUBJECTS, part)
        if key and key != "other_subject" and conf == "medium":
            return key, conf
    return ("other_subject", "none") if noise else ("unknown", "none")


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
    """Тип: имя файла важнее папки, расширение — последний аргумент."""
    key, conf = _match(TYPES, name)
    if key:
        return key, conf
    for part in reversed(path.split("/")[:-1]):
        key, conf = _match(TYPES, part)
        if key == "homework" and part.strip().lower() in ("дз",):
            return key, "high"
        if key:
            return key, conf
    ext = name.rsplit(".", 1)[-1].lower() if "." in name else ""
    if ext in ("pptx", "ppt"):
        return "presentation", "medium"
    if ext in ("py", "ipynb"):
        return "program", "high"
    if text:
        key, conf = _match(TYPES, text[:400])
        if key:
            return key, "medium"
    return "other", "low"


def confidence(subject_conf, type_conf):
    """Одна оценка: можно ли раскладывать файл без человека.

    Предмет важнее типа: без предмета файл некуда положить, а тип «other» — это нормально
    (листок с ребусами и правда не ДЗ). Поэтому «предмет ясен, тип нет» — это medium,
    а не провал.
    """
    if subject_conf == "high":
        return "high" if type_conf in ("high", "medium") else "medium"
    if subject_conf == "medium":
        return "medium"
    return "low" if type_conf != "none" else "unknown"


def classify(path, name, text=""):
    subject, s_conf = subject_of(path, name)
    kind, t_conf = type_of(path, name, text)
    return {
        "subject": subject,
        "type": kind,
        "grade": grade_of(path, name),
        "subject_conf": s_conf,
        "type_conf": t_conf,
        "confidence": confidence(s_conf, t_conf),
    }
