"""Тип материала по имени файла — бесплатно и без модели.

Покрывает то, что реально пишут в именах: ДЗ-15, КР-2, лист 16, конспект 3, самостоятельная.
Если имя ничего не говорит — возвращаем None и «none», дальше решает модель. Порядок проверок
важен: «Листок 16. Самостоятельная работа» — это самостоятельная, а не листок.
"""
import re

RULES = [
    ("homework", ["дз", "домашнее задание", "домашка", "домашняя работа"]),
    ("test", ["контрольная", "контрольная работа", "кр", "зачет", "зачёт", "экзамен"]),
    ("independent_work", ["самостоятельная", "самостоятельная работа", "самост", "отработка"]),
    ("notes", ["конспект", "лекция", "урок"]),
    ("textbook", ["учебник", "пособие", "сборник задач"]),
    ("worksheet", ["лист", "листок", "плюсник", "задачник", "вариант", "worksheet"]),
    ("presentation", ["презентация", "слайд", "presentation"]),
]
PROGRAM_EXT = {".py", ".ipynb", ".js", ".ts", ".java", ".c", ".cpp"}

# «кр» и «дз» короткие: ищем их только отдельным словом, иначе «кружок» станет контрольной
SHORT = {"дз", "кр", "ср"}


def _norm(s):
    return re.sub(r"\s+", " ", (s or "").lower().replace("ё", "е")).strip()


def type_of(name):
    """(тип, уверенность, источник). None — имя ничего не говорит."""
    n = _norm(name)
    if not n:
        return None, "none", None
    for kind, words in RULES:
        for w in words:
            if w in SHORT:
                if re.search(rf"(?<![а-яa-z0-9]){w}(?![а-яa-z0-9])", n):
                    return kind, "high", "name"
            elif w in n:
                return kind, "high", "name"
    ext = "." + name.rsplit(".", 1)[-1].lower() if "." in name else ""
    if ext in PROGRAM_EXT:
        return "program", "high", "ext"
    return None, "none", None
