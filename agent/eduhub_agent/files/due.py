"""Дата сдачи файла по расписанию — та же логика, что у находок из чатов.

В чате дата берётся из текста («к пятнице») или из расписания: ДЗ, написанное на уроке,
почти всегда к следующему уроку предмета (`server/timetable.py: link()`, правила Саши:
«на следующий урок» / «через урок» / «через неделю», окно 7 минут после звонка).
У файла из папки или Drive текста нет, зато есть время появления файла: `seen_at` — mtime
локального файла или `modifiedTime` у Drive.

Правило: кандидаты — уроки этого предмета, которые на момент появления файла **ещё не
начались**. Тот, что уже идёт, не годится: листок, положенный в 11:20, не домашнее задание
к уроку, который начался в 11:00. «Ближайший следующий» — по умолчанию; `pick=1` берёт
следующий за ним (файл в пятницу вечером почти всегда к уроку через неделю).
"""
from datetime import date, datetime, timedelta

from eduhub_agent.server import backend_api, timetable

HORIZON = 28  # дней вперёд: дальше уроков предмета не ищем, честнее отдать в review
THROUGH = ("через урок", "через один урок")


def _dt(day, hhmm):
    h, m = map(int, str(hhmm)[:5].split(":"))
    return datetime(day.year, day.month, day.day, h, m)


def seen_at(row):
    """Когда файл появился: mtime/Drive modifiedTime, иначе время постановки в очередь."""
    raw = row.get("seen_at")
    if raw:
        try:
            return datetime.fromisoformat(str(raw)[:19])
        except ValueError:
            pass
    return datetime.fromtimestamp(row.get("ts") or 0)


def candidates(class_name, subject, when, horizon=HORIZON):
    """[(дата, урок)] — уроки предмета после `when`, которые ещё не начались. В порядке времени."""
    cid = backend_api.class_id(class_name)
    if not cid:
        return []
    out = []
    for k in range(horizon):
        day = when.date() + timedelta(days=k)
        for les in sorted(backend_api.day_lessons(cid, day.isoformat()), key=lambda l: l["start_time"]):
            if not timetable.same_subject(les["subject_name"], subject):
                continue
            if _dt(day, les["start_time"]) <= when:
                continue          # урок уже идёт или прошёл: это не «к следующему»
            out.append((day, les))
            break                 # сдвоенный урок — одна дата
    return out


def due_for(row, class_name=None, subject=None, pick=0, horizon=HORIZON):
    """(дата, урок, почему) для файла. (None, None, причина), если поставить некуда."""
    cls = class_name or row.get("class")
    if not cls:
        return None, None, "нет класса"
    subj = subject or row.get("subject")
    if not subj:
        return None, None, "предмет не опознан"
    text = " ".join(str(row.get(k) or "") for k in ("name", "caption")).lower()
    pick = 1 if any(t in text for t in THROUGH) else pick
    days = candidates(cls, subj, seen_at(row), horizon)
    if len(days) <= pick:
        return None, None, f"в расписании нет уроков «{subj}» после появления файла"
    day, les = days[pick]
    return day, les, f"по расписанию: {'через урок' if pick else 'ближайший урок'} {day.isoformat()}"


def describe(day, lesson):
    return f"{day.isoformat()} {lesson['start_time'][:5]} {lesson['subject_name']}" if lesson else "—"
