"""Подтверждённые ДЗ и контрольные из чатов -> домашка в бэкенде EduHub (HTTP API из presentation/src/router.rs).

Как ищется урок. В homeworks нужен lesson_instance_id — конкретный урок в конкретный день. Расписание API отдаёт
только «глазами ученика»: GET /api/schedule/day?date=...&student_id=..., поэтому на каждый класс нужен id одного
ученика этого класса (API_STUDENTS). Из уроков на дату срока берём урок того же предмета
(если агент уже знает номер урока по timetable.py, то урок с тем же временем начала).

Как пишется. У одного урока одна домашка. Если её ещё нет — POST /api/homework, если есть (учитель уже записал
или агент нашёл второе задание к тому же уроку) — дописываем свой текст через PATCH /api/homework/{id}/text.
Что куда записали, помнит api_sent.json: когда в чате поправили срок, текст переезжает со старого урока на новый.
Если сервер недоступен или урок не нашёлся, находка ждёт в api_queue.json и уходит при следующей отправке
(или командой python3 backend_api.py retry).

Настройки (.env):
  API_URL=http://localhost:8080          адрес бэкенда (SERVER_PORT из backend/.env)
  AGENT_USER_ID=<uuid>                   кто записывается автором домашки (created_by)
  AGENT_USER_ROLE=teacher                его роль: student / teacher / admin, как в таблице users
  API_STUDENTS=11Б:<uuid>,11В:<uuid>     по ученику на класс, чьими глазами смотреть расписание
Без API_URL ничего не отправляется.
"""
import json
import sys
import urllib.error
import urllib.request
from datetime import date

import timetable
from moderator import env

SENT = "api_sent.json"
QUEUE = "api_queue.json"
KINDS = ("homework", "test")
SEP = "\n\n"


def load(path, default):
    try:
        return json.load(open(path, encoding="utf-8"))
    except FileNotFoundError:
        return default


def save(path, data):
    json.dump(data, open(path, "w", encoding="utf-8"), ensure_ascii=False, indent=1)


class ApiError(Exception):
    def __init__(self, status, message):
        super().__init__(f"{status}: {message}")
        self.status = status


def call(method, path, body=None):
    req = urllib.request.Request(env("API_URL").rstrip("/") + path, method=method,
                                 data=None if body is None else json.dumps(body).encode(),
                                 headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=15) as r:
            return json.loads(r.read() or "null")
    except urllib.error.HTTPError as e:
        try:
            message = json.loads(e.read()).get("error", "")
        except Exception:
            message = e.reason
        raise ApiError(e.code, message)


def students():
    """API_STUDENTS=11Б:uuid,11В:uuid -> {"11Б": uuid}. Класс сравнивается без пробелов и дефисов."""
    out = {}
    for pair in (env("API_STUDENTS") or "").split(","):
        cls, _, uid = pair.partition(":")
        if cls.strip() and uid.strip():
            out[timetable.norm(cls)] = uid.strip()
    return out


def class_of(it):
    if it.get("class"):
        return it["class"]
    try:
        return (timetable.parse_chat_name(it.get("chat", "")) or {}).get("class")
    except Exception:
        return None


def day_lessons(student, day):
    return call("GET", f"/api/schedule/day?date={day}&student_id={student}")["lessons"]


def find_lesson(it):
    """-> (урок из /api/schedule/day, None) или (None, почему не нашли)."""
    cls = class_of(it)
    student = students().get(timetable.norm(cls or ""))
    if not student:
        return None, f"нет ученика класса {cls or '?'} в API_STUDENTS"
    if not it.get("date"):
        return None, "у находки нет даты"
    subjects = [s.strip() for s in (it.get("subject") or "").split(" / ") if s.strip()]
    if not subjects:
        return None, "непонятен предмет"
    lessons = [l for l in day_lessons(student, it["date"])
               if any(timetable.same_subject(l["subject_name"], s) for s in subjects)]
    if not lessons:
        return None, f"{it['date']} в расписании нет урока «{' / '.join(subjects)}»"
    start = (it.get("due_lesson") or {}).get("start")
    return next((l for l in lessons if l["start_time"] == start), lessons[0]), None


def text_of(it):
    """Текст для ученика: что задано, слова учителя и откуда это."""
    head = ("Контрольная: " if it["type"] == "test" else "") + (it.get("summary") or it.get("quote") or "").strip()
    quote = (it.get("quote") or "").strip()
    lines = [head]
    if quote and quote != head:
        lines.append(f"«{quote}»")
    if it.get("chat"):
        lines.append(f"Из чата «{it['chat']}»")
    return "\n".join(lines)


def without(full, part):
    """Текст домашки без нашего куска (когда срок поправили и задание переезжает на другой урок)."""
    rest = [p for p in (full or "").split(SEP) if p.strip() and p.strip() != part.strip()]
    return SEP.join(rest) or None


def set_text(homework_id, text):
    call("PATCH", f"/api/homework/{homework_id}/text", {"text_content": text})


def add_to(lesson, text):
    """Кладёт text в домашку урока: создаёт её или дописывает. -> id домашки."""
    hw = lesson.get("homework")
    if not hw:
        try:
            return call("POST", "/api/homework", {
                "lesson_instance_id": lesson["id"], "created_by": env("AGENT_USER_ID"),
                "created_by_role": env("AGENT_USER_ROLE") or "teacher",
                "text_content": text, "status": "published"})["id"]
        except ApiError as e:
            if e.status != 409:
                raise
            hw = next((l.get("homework") for l in day_lessons(lesson["_student"], lesson["_date"])
                       if l["id"] == lesson["id"]), None)  # кто-то успел записать раньше нас
            if not hw:
                raise
    if text.strip() not in [p.strip() for p in hw["text"].split(SEP)]:
        set_text(hw["id"], (hw["text"].rstrip() + SEP if hw["text"].strip() else "") + text)
    return hw["id"]


def remove_from(prev):
    """Убирает наш текст из домашки урока, куда записали раньше."""
    try:
        lesson = next((l for l in day_lessons(prev["student"], prev["date"]) if l["id"] == prev["lesson_id"]), None)
    except ApiError:
        lesson = None
    hw = lesson and lesson.get("homework")
    if hw and hw["id"] == prev["homework_id"]:
        set_text(hw["id"], without(hw["text"], prev["text"]))


def _send(key, it):
    """Одна находка. -> None, если записали, иначе причина (тогда находка ждёт в очереди)."""
    if not env("AGENT_USER_ID"):
        return "нет AGENT_USER_ID в .env"
    lesson, why = find_lesson(it)
    if not lesson:
        return why
    lesson["_student"], lesson["_date"] = students()[timetable.norm(class_of(it))], it["date"]
    text = text_of(it)
    sent = load(SENT, {})
    prev = sent.get(key)
    if prev and prev["lesson_id"] == lesson["id"] and prev["text"] == text:
        return None  # уже записано
    hw = lesson.get("homework")
    if prev and prev["lesson_id"] == lesson["id"] and hw and hw["id"] == prev["homework_id"]:
        rest = without(hw["text"], prev["text"])  # тот же урок, поменялся текст: заменяем свой кусок
        set_text(hw["id"], (rest + SEP if rest else "") + text)
        homework_id = hw["id"]
    else:
        if prev:
            remove_from(prev)
        homework_id = add_to(lesson, text)
    sent[key] = {"homework_id": homework_id, "lesson_id": lesson["id"], "date": it["date"],
                 "student": lesson["_student"], "text": text}
    save(SENT, sent)
    print(f"  в бэкенд: {it['type']} «{it.get('summary', '')[:40]}» -> урок {it['date']} {lesson['start_time']} "
          f"{lesson['subject_name']}")
    return None


def send(key, it):
    """Подтверждённая находка из чата (moderator.send_to_server). key — постоянный id находки."""
    if it.get("type") not in KINDS or not env("API_URL"):
        return
    queue = [q for q in load(QUEUE, []) if q["key"] != key] + [{"key": key, "item": it}]
    flush(queue)


def retry():
    if env("API_URL"):
        flush(load(QUEUE, []))


def flush(queue):
    today = date.today().isoformat()
    left = []
    for q in queue:
        if (q["item"].get("date") or today) < today:
            print(f"  в бэкенд: «{q['item'].get('summary', '')[:40]}» не отправлено, срок прошёл ({q.get('why')})")
            continue
        try:
            why = _send(q["key"], q["item"])
        except (ApiError, OSError) as e:
            why = f"сервер: {e}"
        if why:
            print(f"  в бэкенд: «{q['item'].get('summary', '')[:40]}» ждёт: {why}")
            left.append({**q, "why": why})
    save(QUEUE, left)


if __name__ == "__main__":
    if sys.argv[1:] == ["retry"]:
        retry()
    else:
        print(__doc__)
