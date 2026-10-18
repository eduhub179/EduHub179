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

import backend_api
import chat_moderation
import timetable
from moderator import env

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
    if not text or text.startswith("/"):
        tg("sendMessage", chat_id=chat_id, text=(
            "Пришли сюда домашку текстом, например: «физика к пятнице §12, № 5–7». Я найду урок в расписании и запишу"
            " задание от твоего имени. Учителю нужно написать и класс: «11Б физика к пятнице …»."
            + ("\nФайлы и фото пока не принимаю, только текст." if not text and (msg.get("document") or msg.get("photo")) else "")))
        return
    user = user_of(msg["from"]["id"])
    if not user:
        print(f"ЛС: Telegram id {msg['from']['id']} не привязан к пользователю (нет входа в основном боте)")
        tg("sendMessage", chat_id=chat_id, text=login_hint())
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
