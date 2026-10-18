"""Класс (параллель) из пути или имени файла — отдельно от модели.

Модель в P2 не размечает класс: в промпте его нет, и выдумывать его она не должна.
Класс берётся из папки («9 класс») или из метки («8Б»), это правило, а не классификация.
"""
import re

GRADE_WORD = re.compile(r"(?<!\d)(\d{1,2})\s*(?:класс|кл\.?\b)", re.I)
GRADE_LETTER = re.compile(r"(?<![\d])(\d{1,2})\s*[-–]?\s*([А-ЯЁ])(?![А-Яа-яЁё])")


def _grade_in(text, letters=True):
    for rx in ([GRADE_WORD, GRADE_LETTER] if letters else [GRADE_WORD]):
        m = rx.search(text)
        if m:
            n = int(m.group(1))
            if 1 <= n <= 11:
                return n
    return None


def grade_of(path, name=""):
    """Сначала папки (надёжно), потом имя — и в имени только «N класс».

    «Матан/9 класс/лист12д--...pdf» — это 9 класс: номер листка не должен становиться
    параллелью.
    """
    for part in reversed(str(path).split("/")[:-1]):
        found = _grade_in(part)
        if found:
            return found
    return _grade_in(name, letters=False)
