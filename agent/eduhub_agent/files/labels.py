"""Метки файла: подсказки → правила по имени → модель только на остаток.

Порядок зафиксирован Максом 18.10:

  1. подсказки (подпись, название чата, путь папки) — 0 секунд;
  2. правила по имени файла (ДЗ / КР / лист / конспект) — 0 секунд;
  3. модель — только если предмет или тип остались неизвестны ИЛИ уверенность ниже high.

Если подсказка и правила закрыли оба поля уверенно, модель не зовётся вовсе:
`input_mode = "heuristic"`. Иначе `"model"`.

Подсказка главнее модели по предмету и классу, модель главнее по типу. Конфликт не
замалчивается: он остаётся в `subject_conflict`, чтобы потом посчитать, как часто модель
и папка расходятся.
"""
from eduhub_agent.classify import classify
from eduhub_agent.files import hints, name_rules
from eduhub_agent.ingest.grade import grade_of


def _conf(*pairs):
    """Общая уверенность пары «предмет + тип»: high только если оба high."""
    vals = [c for _, c in pairs if c]
    if not vals or any(v == "none" for v in vals):
        return "none"
    return "high" if all(v == "high" for v in vals) else "medium"


def label(path, name="", text="", caption=None, chat=None, chat_subjects=None, chat_class=None,
          classifier=None, model=True, timeout=300):
    """Метки одного файла. Модель зовётся, только если бесплатные слои не справились."""
    hint = hints.collect(caption=caption, chat=chat, path=path,
                         chat_subjects=chat_subjects, chat_class=chat_class)
    subject = hint["subject"]
    s_conf = "high" if subject else "none"
    s_src = hint["source"]
    kind, t_conf, t_src = name_rules.type_of(name)
    grade = hint["grade"] or grade_of(path, name)

    got = {}
    need_model = model and (subject is None or kind is None or s_conf != "high" or t_conf != "high")
    if need_model:
        got = (classifier or classify)(path, name, text, timeout=timeout)
        if subject is None and got.get("subject") and got["subject"] != "unknown":
            subject, s_conf, s_src = got["subject"], got.get("subject_conf") or "high", "model"
        if kind is None or t_conf != "high":
            if got.get("type") and got["type"] != "other":
                kind, t_conf, t_src = got["type"], got.get("type_conf") or "high", "model"
        conflict = None
        if hint.get("subject") and got.get("subject") and got["subject"] not in ("unknown", hint["subject"]):
            conflict = got["subject"]
    else:
        conflict = None

    out = {
        "subject": subject or "unknown",
        "type": kind or "other",
        "grade": grade,
        "lesson": got.get("lesson"),
        "date": got.get("date"),
        "subject_conf": s_conf,
        "type_conf": t_conf,
        "confidence": _conf((subject, s_conf), (kind, t_conf)),
        "input_mode": "model" if need_model else "heuristic",
        "subject_source": s_src or ("model" if subject else None),
        "type_source": t_src or ("model" if kind else None),
        "engine": got.get("engine"),
    }
    if conflict:
        out["subject_conflict"] = conflict
    return out
