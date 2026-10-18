"""Подтверждённые ДЗ и контрольные из чатов -> домашка в бэкенде EduHub (HTTP API из presentation/src/router.rs).

Как ищется урок. В homeworks нужен lesson_instance_id — конкретный урок в конкретный день. Класс находки («11Б» из названия
чата) ищем в GET /api/classes (там «2027б»: год выпуска + буква), берём его id и расписание класса со всеми группами:
GET /api/classes/schedule/day?date=...&class_id=... Из уроков на дату срока берём урок того же предмета
(если агент уже знает номер урока по timetable.py, то урок с тем же временем начала).

Как пишется. У одного урока одна домашка. Если её ещё нет — POST /api/homework, если есть (учитель уже записал
или агент нашёл второе задание к тому же уроку) — дописываем свой текст через PATCH /api/homework/{id}/text.
Что куда записали, помнит api_sent.json: когда в чате поправили срок, текст переезжает со старого урока на новый.
Если сервер недоступен или урок не нашёлся, находка ждёт в api_queue.json и уходит при следующей отправке
(или командой python3 backend_api.py retry — обёртка в корне agent/).

Настройки (.env):
  API_URL=http://localhost:8080          адрес бэкенда (SERVER_PORT из backend/.env)
  AGENT_USER_ID=<uuid>                   кто записывается автором домашки (created_by)
  AGENT_USER_ROLE=teacher                его роль: student / teacher / admin, как в таблице users
  AGENT_KEY=<секрет>                     тот же, что AGENT_KEY в backend/.env: по нему бэкенд отвечает, кто пишет боту
Без API_URL ничего не отправляется. ДЗ из личных сообщений боту (dm.py) записывается от имени того, кто прислал:
в находке есть author_user_id и author_role, тогда AGENT_USER_ID не нужен.
"""
import json
import sys
import urllib.error
import urllib.request
from datetime import date

from eduhub_agent.paths import data

from eduhub_agent.server import timetable
from eduhub_agent.config import env

SENT = data("api_sent.json")
QUEUE = data("api_queue.json")
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


def call(method, path, body=None, headers=None):
    req = urllib.request.Request(env("API_URL").rstrip("/") + path, method=method,
                                 data=None if body is None else json.dumps(body).encode(),
                                 headers={"Content-Type": "application/json", **(headers or {})})
    try:
        with urllib.request.urlopen(req, timeout=15) as r:
            return json.loads(r.read() or "null")
    except urllib.error.HTTPError as e:
        try:
            message = json.loads(e.read()).get("error", "")
        except Exception:
            message = e.reason
        raise ApiError(e.code, message)


_classes = {}
_names = {}  # id класса -> «11Б»


def _reload_classes():
    end = timetable.school_year_end()
    _classes.clear()
    _names.clear()
    for c in call("GET", "/api/classes")["classes"]:
        grade = 11 - (c["graduation_year"] - end)
        _classes[timetable.norm(f"{grade}{c['letter']}")] = c["id"]
        _classes[timetable.norm(c["name"])] = c["id"]
        _names[c["id"]] = f"{grade}{c['letter'].upper()}"


def class_id(cls):
    """«11Б» -> id класса из GET /api/classes (там год выпуска: в 2026/27 это «2027б»). None, если такого класса нет.
    Список классов запоминаем и перечитываем, когда класса в нём не нашлось (вдруг добавили)."""
    key = timetable.norm(cls or "")
    if key and key not in _classes:
        _reload_classes()
    return _classes.get(key)


def class_name(cid):
    """id класса -> «11Б» (обратное к class_id). None, если такого нет."""
    if cid not in _names:
        _reload_classes()
    return _names.get(cid)


def telegram_user(telegram_id):
    """Кто пишет боту: {"user_id", "login", "role", "class_id"} по Telegram id, если человек вошёл в основном боте
    (таблица telegram_bindings, статус active). None — не вошёл. Нужен AGENT_KEY в .env и в backend/.env."""
    try:
        return call("GET", f"/api/telegram/users/{int(telegram_id)}", headers={"X-Agent-Key": env("AGENT_KEY") or ""})
    except ApiError as e:
        if e.status == 404:
            return None
        raise


def class_of(it):
    if it.get("class"):
        return it["class"]
    try:
        return (timetable.parse_chat_name(it.get("chat", "")) or {}).get("class")
    except Exception:
        return None


def day_lessons(class_id, day):
    return call("GET", f"/api/classes/schedule/day?date={day}&class_id={class_id}")["lessons"]


def find_lesson(it):
    """-> (урок из /api/schedule/day, None) или (None, почему не нашли)."""
    cls = class_of(it)
    if not cls:
        return None, "непонятен класс"
    cid = class_id(cls)
    if not cid:
        return None, f"класса {cls} нет в /api/classes"
    if not it.get("date"):
        return None, "у находки нет даты"
    subjects = [s.strip() for s in (it.get("subject") or "").split(" / ") if s.strip()]
    if not subjects:
        return None, "непонятен предмет"
    day = sorted(day_lessons(cid, it["date"]), key=lambda l: l["start_time"])
    lessons = [l for l in day if any(timetable.same_subject(l["subject_name"], s) for s in subjects)]
    if not lessons:
        return None, f"{it['date']} в расписании нет урока «{' / '.join(subjects)}»"
    start = (it.get("due_lesson") or {}).get("start")
    lesson = next((l for l in lessons if l["start_time"] == start), lessons[0])
    for l in pair_of(day, lesson):
        l["_class"], l["_date"] = cid, it["date"]
    lesson["_pair"] = [l for l in pair_of(day, lesson) if l is not lesson]
    return lesson, None


def pair_of(day, lesson):
    """Сдвоенный урок: lesson и стоящие вплотную к нему уроки того же предмета (домашку пишем на каждый)."""
    i = day.index(lesson)
    lo, hi = i, i
    while lo > 0 and day[lo - 1]["subject_name"] == lesson["subject_name"]:
        lo -= 1
    while hi + 1 < len(day) and day[hi + 1]["subject_name"] == lesson["subject_name"]:
        hi += 1
    return day[lo:hi + 1]


def text_of(it):
    """Текст для ученика: что задано, слова учителя и откуда это."""
    head = ("Контрольная: " if it["type"] == "test" else "") + (it.get("summary") or it.get("quote") or "").strip()
    quote = (it.get("quote") or "").strip()
    lines = [head]
    if quote and quote != head:
        lines.append(f"«{quote}»")
    if it.get("via") == "dm":
        lines = [head]  # человек сам прислал задание боту: цитата и чат не нужны, автор записан в created_by
    elif it.get("chat"):
        lines.append(f"Из чата «{it['chat']}»")
    return "\n".join(lines)


def without(full, part):
    """Текст домашки без нашего куска (когда срок поправили и задание переезжает на другой урок)."""
    rest = [p for p in (full or "").split(SEP) if p.strip() and p.strip() != part.strip()]
    return SEP.join(rest) or None


def set_text(homework_id, text):
    call("PATCH", f"/api/homework/{homework_id}/text", {"text_content": text})


def author(it):
    """(created_by, created_by_role): кто прислал ДЗ боту, а для находок из чатов — пользователь агента."""
    if it.get("author_user_id"):
        return it["author_user_id"], it.get("author_role") or "student"
    return env("AGENT_USER_ID"), env("AGENT_USER_ROLE") or "teacher"


def add_to(lesson, text, by=None):
    """Кладёт text в домашку урока: создаёт её или дописывает. by — (created_by, created_by_role). -> id домашки."""
    hw = lesson.get("homework")
    by = by or author({})
    if not hw:
        try:
            return call("POST", "/api/homework", {
                "lesson_instance_id": lesson["id"], "created_by": by[0], "created_by_role": by[1],
                "text_content": text, "status": "published"})["id"]
        except ApiError as e:
            if e.status != 409:
                raise
            hw = next((l.get("homework") for l in day_lessons(lesson["_class"], lesson["_date"])
                       if l["id"] == lesson["id"]), None)  # кто-то успел записать раньше нас
            if not hw:
                raise
    if text.strip() not in [p.strip() for p in hw["text"].split(SEP)]:
        set_text(hw["id"], (hw["text"].rstrip() + SEP if hw["text"].strip() else "") + text)
    return hw["id"]


def remove_from(prev):
    """Убирает наш текст из домашки урока, куда записали раньше."""
    try:
        lesson = next((l for l in day_lessons(prev.get("class_id"), prev["date"]) if l["id"] == prev["lesson_id"]), None)
    except ApiError:
        lesson = None
    hw = lesson and lesson.get("homework")
    if hw and hw["id"] == prev["homework_id"]:
        set_text(hw["id"], without(hw["text"], prev["text"]))


def _send(key, it):
    """Одна находка. -> None, если записали, иначе причина (тогда находка ждёт в очереди).
    На сдвоенный урок пишем на оба: второй помним в api_sent.json под ключом «key+1»."""
    if not author(it)[0]:
        return "нет AGENT_USER_ID в .env"
    lesson, why = find_lesson(it)
    if not lesson:
        return why
    targets = [lesson] + lesson.pop("_pair")
    for n, l in enumerate(targets):
        _send_to(key if n == 0 else f"{key}+{n}", it, l)
    sent = load(SENT, {})
    for k in [k for k in sent if k.startswith(key + "+") and int(k.rsplit("+", 1)[1]) >= len(targets)]:
        remove_from(sent.pop(k))  # раньше урок был сдвоенный, теперь нет: убираем лишнюю копию
    save(SENT, sent)
    if ON_SENT:
        try:
            ON_SENT(key, it, lesson)
        except Exception as e:  # уведомление не должно мешать записи
            print("  в бэкенд: уведомление не ушло:", repr(e))
    return None


def _send_to(key, it, lesson):
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
        homework_id = add_to(lesson, text, author(it))
    sent[key] = {"homework_id": homework_id, "lesson_id": lesson["id"], "date": it["date"],
                 "class_id": lesson["_class"], "text": text}
    save(SENT, sent)
    print(f"  в бэкенд: {it['type']} «{it.get('summary', '')[:40]}» -> урок {it['date']} {lesson['start_time']} "
          f"{lesson['subject_name']}")
    return None


ON_SENT = None  # dm.py ставит сюда функцию: написать ученику, что его ДЗ записали


def send(key, it):
    """Подтверждённая находка из чата (moderator.send_to_server). key — постоянный id находки.
    -> None, если записали (или отправлять не нужно), иначе причина: находка осталась ждать в очереди."""
    if it.get("type") not in KINDS or not env("API_URL"):
        return None
    queue = [q for q in load(QUEUE, []) if q["key"] != key] + [{"key": key, "item": it}]
    return next((q["why"] for q in flush(queue) if q["key"] == key), None)


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
    return left


if __name__ == "__main__":
    if sys.argv[1:] == ["retry"]:
        retry()
    else:
        print(__doc__)
