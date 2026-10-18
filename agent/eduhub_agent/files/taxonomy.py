"""Предметы школы: коды, русские синонимы и список из расписания.

Коды — то, что видит модель и база. Синонимы — то, что пишут люди в подписи, в названии
чата и в папках. Список из `timetable.json` (настоящий, лежит у Макса) главнее встроенного:
если в расписании есть предмет, которого нет в синонимах, он честно остаётся `unknown`,
а не превращается в ближайший похожий.

Новый предмет добавляется в двух местах: `SUBJECTS` здесь и список в промпте модели —
схема не даст модели вернуть что-то вне `SUBJECTS`.
"""
import json
import re
from pathlib import Path

SUBJECTS = [
    "algebra", "geometry", "calculus", "digital_tech", "programming",
    "physics", "chemistry", "biology", "history", "russian",
    "literature", "english", "geography", "social", "informatics",
    "obzh", "pe", "misc",
]

SYNONYMS = {
    "algebra": ["алгебра"],
    "geometry": ["геометрия", "планиметрия", "стереометрия"],
    "calculus": ["матан", "матанализ", "математический анализ", "мат анализ"],
    "digital_tech": ["цифровые технологии", "основы цифровых технологий", "опцт", "технология"],
    "programming": ["программирование", "python", "питон", "colab"],
    "physics": ["физика"],
    "chemistry": ["химия"],
    "biology": ["биология", "зоология", "анатомия"],
    "history": ["история", "всеобщая история"],
    "russian": ["русский язык", "русский"],
    "literature": ["литература", "чтение"],
    "english": ["английский язык", "английский", "english"],
    "geography": ["география"],
    "social": ["обществознание", "общество"],
    "informatics": ["информатика", "икт"],
    "obzh": ["обж", "основы безопасности жизнедеятельности"],
    "pe": ["физкультура", "физическая культура"],
    "misc": ["разное", "классный час", "прочее"],
}

GRADE = re.compile(r"(?<!\d)(\d{1,2})\s*(?:класс|кл\.?\b)", re.I)
GRADE_LETTER = re.compile(r"(?<![\d])(\d{1,2})\s*[-–]?\s*([А-ЯЁ])(?![А-Яа-яЁё])")


def _norm(s):
    return re.sub(r"\s+", " ", (s or "").lower().replace("ё", "е")).strip()


def code_for(name):
    """«Физика 11б» → physics. None, если предмет не опознан: угадывать нельзя."""
    n = _norm(name)
    if not n:
        return None
    for code, words in SYNONYMS.items():
        for w in words:
            if re.search(rf"(?<![а-яa-z0-9]){re.escape(w)}(?![а-яa-z0-9])", n):
                return code
    return None


def grade_from(text):
    for rx in (GRADE, GRADE_LETTER):
        m = rx.search(text or "")
        if m:
            n = int(m.group(1))
            if 1 <= n <= 11:
                return n
    return None


def subjects_from_timetable(path):
    """{сырое название: код} из timetable.json: чаты и уроки. Битый файл — пустой словарь."""
    try:
        data = json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}
    raw = set()
    for info in (data.get("chats") or {}).values():
        raw.update(info.get("subjects") or [])
    for cls in (data.get("classes") or {}).values():
        for lessons in (cls or {}).values():
            for lesson in lessons or []:
                if isinstance(lesson, dict) and lesson.get("subject"):
                    raw.add(lesson["subject"])
    return {name: code_for(name) for name in sorted(raw)}


def unknown_subjects(timetable_path):
    """Предметы расписания, которых нет в синонимах — их надо добавить руками."""
    return [name for name, code in subjects_from_timetable(timetable_path).items() if code is None]
