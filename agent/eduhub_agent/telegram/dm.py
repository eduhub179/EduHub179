"""Домашка из личных сообщений боту.

Ученик или учитель пишет в личку боту-сборщику (BOT_TOKEN, тот же бот, что сидит в чатах): «физика к пятнице §12, № 5–7».
1. Кто это. Бот спрашивает бэкенд по Telegram id: GET /api/telegram/users/{id} (нужен AGENT_KEY). Связь Telegram ↔
   пользователь появляется, когда человек вошёл в основном боте EduHub179 (бот Яши: /start, школьная почта, код),
   таблица telegram_bindings. Не вошёл — бот просит сначала войти там. Свой вход бот-сборщик не делает.
2. Что задали. Текст разбирает та же модель, что сообщения в чатах (chat_agent.ask + refine). Класс ученика — из его
   профиля, учитель пишет класс в тексте («11Б»). Предмет — по тексту среди предметов класса в расписании бэкенда.
   Нет даты — ближайший урок предмета. Модель ничего не нашла — всё сообщение считается одним ДЗ.
3. Бот показывает, как понял, с кнопками «Записать» и «Отмена». Предмет непонятен или в этот день нет урока —
   кнопками выбирается предмет или день.
4. «Записать». Учитель: сразу в бэкенд от своего имени (created_by = он, created_by_role = teacher).
   Ученик: в бота-модератора, как находки из чатов; после подтверждения ДЗ записывается от имени ученика
   (роль student), а ученику приходит «записал». Без бота-модератора (--no-moderator) — сразу.
5. Кнопки внизу (для учеников): «ДЗ на завтра» (ближайший учебный день), «Расписание на сегодня»,
   «Где следующий урок» (предмет, время, кабинет), «Добавить ДЗ» (подсказка). Всё из расписания бэкенда.
6. Вопросы своими словами («что задали по химии?», «расписание на среду», «где алгебра?»): questions.py понимает,
   о чём спросили (правила, потом модель), а ответ собирается из расписания бэкенда, как у кнопок.
Черновики лежат в chats/dm_drafts.json (там тексты учеников, папка chats/ не коммитится).

Запускается из chat_agent.py live сам; в .env нужны API_URL и AGENT_KEY (тот же, что в backend/.env),
LOGIN_BOT=@имя_основного_бота — чтобы подсказать, где войти.
"""
import json
import re
import secrets
import threading
import time
import urllib.request
from datetime import date, datetime, timedelta
from pathlib import Path

from eduhub_agent.server import backend_api
from eduhub_agent.telegram import chat_moderation, questions
from eduhub_agent.server import timetable
from eduhub_agent.config import env

DRAFTS = "chats/dm_drafts.json"
HORIZON = 14  # на сколько дней вперёд ищем уроки предмета
MAX_ITEMS = 3  # больше находок из одного сообщения не показываем
WD = ["пн", "вт", "ср", "чт", "пт", "сб", "вс"]
KINDS = {"homework": "ДЗ", "test": "Контрольная"}

API = None  # https://api.telegram.org/bot<BOT_TOKEN>/
PARSE = None  # (m, chat) -> находки модели после refine; ставит chat_agent.run_live
MODERATE = False  # работает бот-модератор: ДЗ учеников идёт через него
_users = {}  # telegram id -> (пользователь или None, когда спросили)
_days = {}  # (class_id, дата) -> (уроки, когда спросили)
_menu_shown = set()  # кому уже показали кнопки внизу

B_HW, B_TODAY, B_NEXT, B_ADD = "📚 ДЗ на завтра", "📅 Расписание на сегодня", "🚪 Где следующий урок", "➕ Добавить ДЗ"
MENU = {"keyboard": [[{"text": B_HW}, {"text": B_TODAY}], [{"text": B_NEXT}, {"text": B_ADD}]],
        "resize_keyboard": True, "is_persistent": True}
NOW = datetime.now  # подменяется в тесте


def setup(api, parse, moderate):
    global API, PARSE, MODERATE
    API, PARSE, MODERATE = api, parse, moderate
    backend_api.ON_SENT = on_sent


def tg(method, **params):
    req = urllib.request.Request(API + method, data=json.dumps(params).encode(),
                                 headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=30) as r:
        return json.loads(r.read()).get("result")


def load():
    try:
        return json.load(open(DRAFTS, encoding="utf-8"))
    except FileNotFoundError:
        return {}


def save(drafts):
    Path(DRAFTS).parent.mkdir(exist_ok=True)
    json.dump(drafts, open(DRAFTS, "w", encoding="utf-8"), ensure_ascii=False, indent=1)


# ---------- кто пишет ----------

def user_of(telegram_id):
    hit = _users.get(telegram_id)
    if hit and time.time() - hit[1] < 300:
        return hit[0]
    u = backend_api.telegram_user(telegram_id)
    _users[telegram_id] = (u, time.time())
    return u


def login_hint():
    bot = env("LOGIN_BOT")
    return (f"Сначала войди в {bot}" if bot else "Сначала войди в основном боте EduHub179") + \
        " (/start и школьная почта), потом пришли задание сюда ещё раз."


CLASS_IN_TEXT = re.compile(r"(?<![\w№])(20\d\d|\d{1,2})\s*[-–]?\s*([а-яё])(?![а-яё])", re.I)


def class_in_text(text):
    """Учитель пишет класс в тексте: «11Б», «11-б», «2027Б». Берём первый, который есть в /api/classes."""
    for m in CLASS_IN_TEXT.finditer(text):
        if re.search(r"(?:№|стр\.?|упр\.?|с\.)\s*$", text[:m.start()], re.I):
            continue  # «№ 5 и 7», «стр. 4 а»
        n, letter = int(m[1]), m[2].upper()
        grade = 11 - (n - timetable.school_year_end()) if n > 2000 else n
        if 1 <= grade <= 11 and backend_api.class_id(f"{grade}{letter}"):
            return f"{grade}{letter}"
    return None


# ---------- расписание ----------

def lessons_on(cid, day):
    key = (cid, day.isoformat())
    hit = _days.get(key)
    if not hit or time.time() - hit[1] > 600:
        try:
            les = backend_api.day_lessons(cid, day.isoformat())
        except backend_api.ApiError as e:
            if e.status != 404:  # 404 — на эту неделю расписания нет (каникулы, ещё не загрузили)
                raise
            les = []
        hit = _days[key] = (les, time.time())
    return hit[0]


def subject_days(cid, subject, after, count=3):
    """Ближайшие дни после after, когда у класса есть урок subject: [(дата, урок)]."""
    out = []
    for k in range(1, HORIZON + 1):
        d = after + timedelta(days=k)
        les = next((l for l in lessons_on(cid, d) if timetable.same_subject(l["subject_name"], subject)), None)
        if les:
            out.append((d, les))
            if len(out) >= count:
                break
    return out


def class_subjects(cid, today):
    names = []
    for k in range(0, 8):
        for l in lessons_on(cid, today + timedelta(days=k)):
            if l["subject_name"] not in names:
                names.append(l["subject_name"])
    return names


def subjects_in(text, names):
    """Предметы класса, которые упоминаются в тексте: «по физике», «англ», «алгебра»."""
    hit = [s for s in names if timetable.mentions(text, s)]
    for s in timetable.parse_chat_name(text)["subjects"]:
        hit += [n for n in names if timetable.same_subject(n, s) and n not in hit]
    return hit


def resolve(d, it):
    """Урок для находки: предмет и дата есть — ищем урок; даты нет — ближайший урок предмета.
    Ставит it["lesson"] или it["problem"] (что спросить кнопками)."""
    it.pop("lesson", None)
    it.pop("problem", None)
    if not it.get("subject"):
        it["problem"] = "subject"
        return
    today = date.fromisoformat(d["posted"][:10])
    if it.get("date"):
        day = date.fromisoformat(it["date"])
        les = next((l for l in lessons_on(d["cid"], day) if timetable.same_subject(l["subject_name"], it["subject"])), None)
        if les:
            it["lesson"] = {"start": les["start_time"], "subject": les["subject_name"]}
            return
        it["problem"] = "date"
        it["choices"] = [x.isoformat() for x, _ in subject_days(d["cid"], it["subject"], today)]  # ближайшие уроки
        return
    nxt = subject_days(d["cid"], it["subject"], today, count=1)
    if not nxt:
        it["problem"] = "date"
        it["choices"] = []
        return
    it["date"], it["date_source"] = nxt[0][0].isoformat(), "ближайший урок"
    it["lesson"] = {"start": nxt[0][1]["start_time"], "subject": nxt[0][1]["subject_name"]}


# ---------- кнопки внизу: ДЗ, расписание, кабинет ----------

def lessons_now(cid, day):
    """Уроки дня без кэша: домашка могла появиться минуту назад."""
    _days.pop((cid, day.isoformat()), None)
    return sorted(lessons_on(cid, day), key=lambda l: l["start_time"])


def next_school_day(cid, day):
    """Ближайший день после day, когда у класса есть уроки: (дата, уроки) или (None, [])."""
    for k in range(1, 8):
        d = day + timedelta(days=k)
        les = lessons_now(cid, d)
        if les:
            return d, les
    return None, []


def lesson_line(l):
    room = f", каб. {l['cabinet']}" if l.get("cabinet") else ""
    return f"{l['start_time'][:5]} {l['subject_name']}{room}"


def when(day, today):
    return "сегодня" if day == today else "завтра" if day == today + timedelta(days=1) else day_label(day.isoformat())


def upcoming(cid, subject, now):
    """Ближайший ещё не начавшийся урок (предмета subject или любой): (дата, урок) или (None, None)."""
    today, hm = now.date(), now.strftime("%H:%M")
    for k in range(HORIZON + 1):
        d = today + timedelta(days=k)
        for l in sorted(lessons_on(cid, d), key=lambda l: l["start_time"]):
            if k == 0 and l["start_time"][:5] <= hm:
                continue
            if subject is None or timetable.same_subject(l["subject_name"], subject):
                fresh = next((x for x in lessons_now(cid, d) if x["id"] == l["id"]), l)  # свежая домашка
                return d, fresh
    return None, None


def hw_text(l):
    return ((l.get("homework") or {}).get("text") or "").strip()


def answer_question(intent, cid, day=None, subject=None):
    """Ответ из расписания бэкенда. intent: homework / schedule / next; day — date или None; subject — название
    предмета класса или None."""
    now = NOW()
    today = now.date()
    if intent == "schedule":
        d = day or today
        les = lessons_now(cid, d)
        if not les:
            return f"{when(d, today).capitalize()} ({day_label(d.isoformat())}) уроков нет." if d in (
                today, today + timedelta(days=1)) else f"В {day_label(d.isoformat())} уроков нет."
        return f"{when(d, today).capitalize()}, {day_label(d.isoformat())}:\n" + "\n".join(lesson_line(l) for l in les)
    if intent == "next":
        if day:
            l = next((x for x in lessons_now(cid, day) if subject is None or timetable.same_subject(x["subject_name"], subject)), None)
            d = day
        else:
            d, l = upcoming(cid, subject, now)
        if not l:
            return f"Не нашёл урок «{subject}»." if subject else "Ближайших уроков не нашёл."
        room = f"Кабинет {l['cabinet']}." if l.get("cabinet") else "Кабинет в расписании не указан."
        head = "Следующий урок: " if subject is None and not day else ""
        return f"{head}{l['subject_name']}, {when(d, today)} в {l['start_time'][:5]}. {room}"
    # homework
    if subject:
        if day:
            l = next((x for x in lessons_now(cid, day) if timetable.same_subject(x["subject_name"], subject)), None)
            d = day
            if not l:
                return f"{when(d, today).capitalize()} нет урока «{subject}»."
        else:
            d, l = upcoming(cid, subject, now)
            if not l:
                return f"Не нашёл ближайший урок «{subject}»."
        t = hw_text(l)
        return (f"{l['subject_name']} ({when(d, today)}): {t}" if t
                else f"{l['subject_name']} ({when(d, today)}): домашки пока нет.")
    if day:
        d, les = day, lessons_now(cid, day)
        if not les:
            return f"{when(d, today).capitalize()} уроков нет."
    else:
        d, les = next_school_day(cid, today)
        if not d:
            return "В ближайшую неделю уроков не нашёл."
    hw = [l for l in les if hw_text(l)]
    if not hw:
        return f"На {when(d, today)} домашки пока нет."
    return f"ДЗ на {when(d, today)}:\n\n" + "\n\n".join(f"{l['subject_name']}: {hw_text(l)}" for l in hw)


def answer_menu(button, cid):
    return answer_question({B_HW: "homework", B_TODAY: "schedule", B_NEXT: "next"}[button], cid)


CAN = ("Я умею:\n• записать ДЗ: «физика к пятнице §12, № 5–7»\n• сказать, что задали: «что по химии?», «дз на среду»\n"
       "• расписание: «какие уроки завтра?»\n• где и когда урок: «где алгебра?»\nИли нажми кнопку внизу.")


# ---------- сообщение ----------

def day_label(iso):
    d = date.fromisoformat(iso)
    return f"{WD[d.weekday()]} {d:%d.%m}"


def preview(d):
    """Текст и кнопки черновика."""
    lines = [f"Понял так ({d['cls']}):"]
    rows = []
    for i, it in enumerate(d["items"]):
        head = f"{i + 1}. {KINDS[it['type']]}"
        if it.get("subject"):
            head += f", {it['subject']}"
        if it.get("lesson"):
            head += f" — к {day_label(it['date'])}, урок в {it['lesson']['start']}"
            if it.get("date_source"):
                head += " (ближайший)"
        lines += [head, f"   {it['summary']}"]
        if it.get("problem") == "date":
            if it.get("date"):
                lines.append(f"   ⚠️ {day_label(it['date'])} нет урока «{it['subject']}». Выбери день:")
            else:
                lines.append(f"   ⚠️ не нашёл урок «{it['subject']}» в ближайшие две недели")
            rows.append([{"text": day_label(c), "callback_data": f"dm:{d['id']}:d:{i}:{c}"} for c in it.get("choices", [])])
    if any(it.get("problem") == "subject" for it in d["items"]):
        lines.append("⚠️ Не понял предмет. Выбери:")
        names = d["subjects"][:12]
        rows += [[{"text": n, "callback_data": f"dm:{d['id']}:s:{k}"} for k, n in enumerate(names)][j:j + 3]
                 for j in range(0, len(names), 3)]
    rows = [r for r in rows if r]
    if all(it.get("lesson") for it in d["items"]):
        who = "запишу от твоего имени" + (" после проверки" if d["user"]["role"] == "student" and MODERATE else "")
        lines.append(f"\nЗаписать? ({who})")
        rows.append([{"text": "✅ Записать", "callback_data": f"dm:{d['id']}:ok"},
                     {"text": "❌ Отмена", "callback_data": f"dm:{d['id']}:no"}])
    else:
        rows.append([{"text": "❌ Отмена", "callback_data": f"dm:{d['id']}:no"}])
    return "\n".join(lines), {"inline_keyboard": rows}


def understand(m, cls, subjects):
    """Находки ДЗ/контрольных из текста. Модель недоступна или ничего не нашла — всё сообщение одно ДЗ."""
    chat = f"Личные сообщения боту, {cls}" + (f" {subjects[0]}" if len(subjects) == 1 else "")
    try:
        items = [x for x in PARSE(m, chat) if x.get("type") in KINDS]
    except Exception as e:  # Ollama не отвечает и т.п.
        print("ЛС: модель не ответила:", repr(e))
        items = []
    if not items:
        items = [{"type": "homework", "quote": m["text"], "summary": m["text"]}]
    out = []
    for it in items[:MAX_ITEMS]:
        quote = (it.get("quote") or "").strip()
        text = quote if len(quote) >= len(it.get("summary") or "") / 2 else it.get("summary") or quote
        cand = subjects_in(quote, subjects) or subjects  # в одном сообщении могут быть два предмета
        out.append({"type": it["type"], "summary": (text or m["text"])[:500], "quote": quote,
                    "date": it.get("date"), "subject": cand[0] if len(cand) == 1 else None})
    return out


def on_message(msg):
    """Сообщение в личку боту-сборщику."""
    try:
        _on_message(msg)
    except (backend_api.ApiError, OSError) as e:
        print("ЛС: бэкенд не ответил:", e)
        tg("sendMessage", chat_id=msg["chat"]["id"], text="Сервер сейчас недоступен, пришли задание чуть позже.")


def _on_message(msg):
    chat_id, text = msg["chat"]["id"], (msg.get("text") or msg.get("caption") or "").strip()
    if not text or text.startswith("/") or text == B_ADD:
        _menu_shown.add(chat_id)
        tg("sendMessage", chat_id=chat_id, reply_markup=MENU, text=(
            "Пришли сюда домашку текстом, например: «физика к пятнице §12, № 5–7». Я найду урок в расписании и запишу"
            " задание от твоего имени. Учителю нужно написать и класс: «11Б физика к пятнице …»."
            + ("\nФайлы и фото пока не принимаю, только текст." if not text and (msg.get("document") or msg.get("photo")) else "")
            + "\nКнопки внизу: домашка на завтра, расписание на сегодня и где следующий урок."))
        return
    user = user_of(msg["from"]["id"])
    if not user:
        print(f"ЛС: Telegram id {msg['from']['id']} не привязан к пользователю (нет входа в основном боте)")
        tg("sendMessage", chat_id=chat_id, text=login_hint())
        return
    if text in (B_HW, B_TODAY, B_NEXT):
        _menu_shown.add(chat_id)
        if user["role"] != "student" or not user.get("class_id"):
            reply = "Эти кнопки пока работают только для учеников: расписание учителя бэкенд ещё не отдаёт."
        else:
            reply = answer_menu(text, user["class_id"])
        tg("sendMessage", chat_id=chat_id, text=reply, reply_markup=MENU)
        return
    if user["role"] == "student" and user.get("class_id") and _question(msg, chat_id, text, user["class_id"]):
        return
    if user["role"] == "student":
        cid = user.get("class_id")
        cls = cid and backend_api.class_name(cid)
        if not cls:
            tg("sendMessage", chat_id=chat_id, text="В твоём профиле не указан класс, напиши администратору школы.")
            return
    else:
        cls = class_in_text(text)
        if not cls:
            tg("sendMessage", chat_id=chat_id, text="Напиши в сообщении класс, например: «11Б физика к пятнице §12».")
            return
        cid = backend_api.class_id(cls)
    done = typing(chat_id)
    try:
        _draft(msg, chat_id, text, user, cls, cid)
    finally:
        done()


def _question(msg, chat_id, text, cid):
    """Вопрос, а не новое ДЗ: ответить и вернуть True. Новое ДЗ — False (дальше обычный черновик)."""
    q = questions.classify(text, llm=None if questions.rule_intent(text) else _llm_with_typing(chat_id))
    if q["intent"] == "add":
        return False
    _menu_shown.add(chat_id)
    if q["intent"] == "other":
        tg("sendMessage", chat_id=chat_id, text=CAN, reply_markup=MENU)
        return True
    names = class_subjects(cid, NOW().date())
    found = subjects_in(text, names) or (q.get("subject") and subjects_in(q["subject"], names)) or []
    if not found and q.get("subject"):
        found = [n for n in names if timetable.same_subject(n, q["subject"])]
    day = questions.parse_day(text, NOW().date()) or (q.get("day") and questions.parse_day(q["day"], NOW().date()))
    print(f"ЛС: вопрос {q['intent']} ({q['by']}), день {day}, предмет {found[:1]}")
    tg("sendMessage", chat_id=chat_id, text=answer_question(q["intent"], cid, day, found[0] if found else None),
       reply_markup=MENU)
    return True


def _llm_with_typing(chat_id):
    def ask(text):
        done = typing(chat_id)
        try:
            return questions.llm_intent(text)
        finally:
            done()
    return ask


def typing(chat_id):
    """«Печатает…» в Telegram гаснет через 5 секунд, а модель думает дольше: повторяем, пока не ответим."""
    stop = threading.Event()

    def loop():
        while not stop.is_set():
            try:
                tg("sendChatAction", chat_id=chat_id, action="typing")
            except Exception:
                pass
            stop.wait(4)
    threading.Thread(target=loop, daemon=True).start()
    return stop.set


def _draft(msg, chat_id, text, user, cls, cid):
    posted = datetime.fromtimestamp(msg["date"]).strftime("%Y-%m-%dT%H:%M:%S")
    names = class_subjects(cid, date.fromisoformat(posted[:10]))
    m = {"chat": "ЛС", "id": msg["message_id"], "date": posted, "author": "?", "author_id": None, "text": text,
         "links": [], "files": [], "reply_to": None, "forwarded": False, "message_ids": [msg["message_id"]]}
    d = {"id": secrets.token_hex(4), "tg": chat_id, "msg_id": msg["message_id"], "posted": posted,
         "user": user, "cls": cls, "cid": cid, "subjects": names, "status": "draft",
         "items": understand(m, cls, subjects_in(text, names))}
    for it in d["items"]:
        resolve(d, it)
    body, kb = preview(d)
    d["preview"] = tg("sendMessage", chat_id=chat_id, text=body, reply_markup=kb)["message_id"]
    drafts = load()
    drafts[d["id"]] = d
    save(drafts)
    if chat_id not in _menu_shown:  # человек сразу прислал ДЗ и ещё не видел кнопок внизу
        _menu_shown.add(chat_id)
        tg("sendMessage", chat_id=chat_id, reply_markup=MENU,
           text="Внизу кнопки: домашка на завтра, расписание на сегодня и где следующий урок.")


def on_button(cb):
    """Нажатие кнопки под черновиком: dm:<id>:ok | no | s:<предмет> | d:<находка>:<дата>."""
    parts = (cb.get("data") or "").split(":")
    if parts[0] != "dm" or len(parts) < 3:
        return False
    drafts = load()
    d = drafts.get(parts[1])
    if not d or d["tg"] != cb["from"]["id"] or d["status"] != "draft":
        tg("answerCallbackQuery", callback_query_id=cb["id"], text="Этот черновик уже не активен")
        return True
    act = parts[2]
    note = None
    if act == "no":
        d["status"] = "cancelled"
        tg("editMessageText", chat_id=d["tg"], message_id=d["preview"], text=preview(d)[0].split("\n\n")[0] + "\n\nОтменено.")
    elif act == "s":
        subject = d["subjects"][int(parts[3])]
        for it in d["items"]:
            if not it.get("subject"):
                it["subject"] = subject
                resolve(d, it)
    elif act == "d":
        it = d["items"][int(parts[3])]
        it["date"] = parts[4]
        it.pop("date_source", None)
        resolve(d, it)
    elif act == "ok" and all(it.get("lesson") for it in d["items"]):
        note = submit(d)
    if d["status"] == "draft":
        body, kb = preview(d)
        tg("editMessageText", chat_id=d["tg"], message_id=d["preview"], text=body, reply_markup=kb)
    tg("answerCallbackQuery", callback_query_id=cb["id"], **({"text": note} if note else {}))
    drafts[d["id"]] = d
    save(drafts)
    return True


def item_for(d, i, it):
    """Находка в том виде, в каком её ждут backend_api.send и бот-модератор."""
    u = d["user"]
    return {"type": it["type"], "summary": it["summary"], "quote": it["quote"], "date": it["date"],
            "class": d["cls"], "subject": it["lesson"]["subject"], "due_lesson": {"start": it["lesson"]["start"]},
            "via": "dm", "author_user_id": u["user_id"], "author_role": u["role"], "author_login": u["login"],
            "chat": f"личные сообщения боту ({u['role']} {u['login']})", "message_id": f"{d['msg_id']}.{i}",
            "sent_at": d["posted"], "posted": d["posted"][:10], "notify": d["tg"], "status": "new"}


def submit(d):
    """«Записать»: учитель — сразу в бэкенд, ученик — через бота-модератора. -> подсказка на кнопку."""
    global _quiet
    waiting = []
    for i, it in enumerate(d["items"]):
        item = item_for(d, i, it)
        if d["user"]["role"] == "student" and MODERATE:
            chat_moderation.push(item)
            continue
        _quiet = True  # о записанном сейчас человек узнает из черновика, отдельное «записал» не нужно
        try:
            why = backend_api.send(f"dm:{d['tg']}:{item['message_id']}", item)
        finally:
            _quiet = False
        if why:
            waiting.append(f"{i + 1}: {why}")
    head = preview(d)[0].split("\n\n")[0]
    if d["user"]["role"] == "student" and MODERATE:
        d["status"] = "moderation"
        text = head + "\n\nОтправил на проверку. Когда запишут, напишу."
    else:
        d["status"] = "sent"
        text = head + ("\n\nЗаписал ✅" if not waiting else
                       "\n\nПока не записалось (" + "; ".join(waiting) + "). Попробую ещё раз через 10 минут и напишу.")
    tg("editMessageText", chat_id=d["tg"], message_id=d["preview"], text=text)
    return "Отправлено" if d["status"] == "moderation" else ("Записано" if not waiting else "Ждёт сервера")


_quiet = False


def on_sent(key, it, lesson):
    """backend_api записал ДЗ из лички позже, чем нажали «Записать» (после проверки модератором или повторной
    попытки): пишем тому, кто прислал, что и куда записано."""
    if it.get("via") != "dm" or not it.get("notify") or not API or _quiet:
        return
    tg("sendMessage", chat_id=it["notify"],
       text=f"✅ Записал: {KINDS.get(it['type'], 'ДЗ')}, {lesson['subject_name']}, к {day_label(it['date'])}:\n{it['summary']}")
