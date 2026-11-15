"""Подсказки: предмет и класс без модели.

Три источника, в порядке силы: подпись под вложением → название чата → путь папки.
Подпись сильнее всего («физика к пятнице §12» пишет человек, который знает), название
чата задаёт предмет и класс для всех файлов чата, путь папки — для архива и диска.

Возвращаем не только предмет, но и откуда он взялся: в строке события это `hint_source`,
чтобы потом было видно, сколько файлов разложено бесплатно, а сколько догадкой.
"""
from eduhub_agent.files import taxonomy


def from_text(text, source):
    """Подпись или название чата → (предмет, класс, источник)."""
    if not (text or "").strip():
        return None
    subject = taxonomy.code_for(text)
    grade = taxonomy.grade_from(text)
    if not subject and not grade:
        return None
    return {"subject": subject, "grade": grade, "source": source}


def from_path(path):
    """Путь папки: снизу вверх, первое совпадение. «Алгебра/9 класс/ДЗ-5.pdf» → algebra, 9."""
    parts = [p for p in str(path or "").split("/")[:-1] if p.strip()]
    subject = grade = None
    for part in reversed(parts):
        if subject is None:
            subject = taxonomy.code_for(part)
        if grade is None:
            grade = taxonomy.grade_from(part)
        if subject and grade:
            break
    if not subject and not grade:
        return None
    return {"subject": subject, "grade": grade, "source": "path"}


def collect(caption=None, chat=None, path=None, chat_subjects=None, chat_class=None):
    """Лучшая доступная подсказка.

    caption       — текст сообщения под вложением
    chat          — название чата («Физика 11б»)
    chat_subjects — уже разобранные предметы чата (из timetable.class_from_chat)
    chat_class    — уже разобранный класс («11Б»)
    path          — путь файла или папки
    """
    candidates = [from_text(caption, "caption")]
    if chat_subjects:
        candidates.append({"subject": taxonomy.code_for(chat_subjects[0]) if chat_subjects else None,
                           "grade": taxonomy.grade_from(chat_class or "") or taxonomy.grade_from(chat or ""),
                           "source": "chat"})
    else:
        candidates.append(from_text(chat, "chat"))
    candidates.append(from_path(path))

    subject = grade = None
    source = None
    for c in candidates:
        if not c:
            continue
        if subject is None and c.get("subject"):
            subject, source = c["subject"], c["source"]
        if grade is None and c.get("grade"):
            grade = c["grade"]
            source = source or c["source"]
    if subject is None and grade is None:
        return {"subject": None, "grade": None, "source": None}
    return {"subject": subject, "grade": grade, "source": source}


def merge(model, hint):
    """Свести подсказку и ответ модели.

    Правило: подсказка главнее по предмету и классу (файл положили в чат физики — значит
    физика), модель главнее по типу. Конфликт не прячем: он попадает в subject_conflict.
    """
    out = dict(model or {})
    conflict = None
    if hint.get("subject"):
        if out.get("subject") and out["subject"] != hint["subject"]:
            conflict = out["subject"]
        out["subject"] = hint["subject"]
        out["subject_conf"] = "high"
        out["subject_source"] = hint.get("source") or "hint"
    if hint.get("grade"):
        out["grade"] = hint["grade"]
    if conflict:
        out["subject_conflict"] = conflict
    return out
