"""Модерация: найденные события и лекции ждут подтверждения в Telegram-боте.

Каждое новое или изменённое событие (events.json), лекция и запись к лекции (lectures.json)
приходит в бота карточкой с кнопками «Опубликовать» / «Отклонить».
На сервер (send_to_server) уходит только то, что подтвердили.

Настройка (один раз):
  1. В Telegram у @BotFather: /newbot, получить токен. Нужен ОТДЕЛЬНЫЙ бот, не бот Яши:
     два процесса не могут читать getUpdates одного токена (ошибка 409).
  2. В папке channel-agent создать файл .env со строкой
         MODERATOR_BOT_TOKEN=123456:ABC...
     Токен никуда не копировать и не коммитить (.env и moderator.json — только у себя).
  3. Запустить python3 watch_channel.py loop и написать боту /start.
     Первый, кто напишет /start, становится модератором (его chat_id сохраняется в moderator.json).
     Чтобы сменить модератора, удалить moderator.json или прописать MODERATOR_CHAT_ID в .env.

Команды бота: /start — стать модератором, /list — прислать заново всё, что ждёт решения,
/manual_channels — вкл/выкл ручную модерацию каналов (события и лекции),
/manual_chats — вкл/выкл ручную модерацию чатов по предметам (ДЗ, контрольные, изменения уроков...),
/manual — показать, что сейчас включено. Каждая из двух команд — переключатель.
Когда ручная модерация выключена, найденное сразу уходит на сервер, а в бот приходит
карточка без кнопок с пометкой «опубликовано автоматически».
То же из терминала: python3 moderator.py manual_channels | manual_chats | manual
Отдельно (без проверки каналов) бота можно запустить так: python3 moderator.py
"""
import hashlib
import html
import json
import os
import time
import urllib.request

from eduhub_agent.config import env
from eduhub_agent.paths import data

EVENTS = data("events.json")
LECTURES = data("lectures.json")
STATE = data("moderator.json")
WAITING = ("new", "changed")


def load(path, default):
    try:
        return json.load(open(path, encoding="utf-8"))
    except FileNotFoundError:
        return default


def save(path, data):
    json.dump(data, open(path, "w", encoding="utf-8"), ensure_ascii=False, indent=1)


TOKEN = env("MODERATOR_BOT_TOKEN")


def api(method, **params):
    req = urllib.request.Request(f"https://api.telegram.org/bot{TOKEN}/{method}",
                                 data=json.dumps(params).encode(), headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=params.get("timeout", 0) + 15) as r:
        return json.loads(r.read())["result"]


def send_to_server(item):
    """Сюда попадает только подтверждённое. ДЗ и контрольные из чатов уходят в бэкенд (backend_api.py),
    для событий и лекций эндпоинтов пока нет."""
    if item.get("item"):
        try:
            from eduhub_agent.server import backend_api
            backend_api.send(item["id"], item["item"])
        except Exception as e:  # ошибка отправки не должна ронять бота
            print("в бэкенд: ошибка", repr(e))


# ---------- что ждёт решения ----------

def item_id(*parts):
    return hashlib.sha1("|".join(str(p) for p in parts).encode()).hexdigest()[:10]


# Другие агенты (например, чаты по предметам) добавляют сюда свой файл: {вид: путь}.
# Файл — список словарей с полями title, date, time, place, quote, source и status ("new" / "changed").
EXTRA = {}


def items():
    """Все карточки: (вид, объект, лекция-родитель для записи). Заодно раздаём id тем, у кого его нет.
    Возвращает (файлы {путь: данные}, карточки) — после изменений файлы сохраняет save_all()."""
    files = {EVENTS: load(EVENTS, []), LECTURES: load(LECTURES, [])}
    out = []
    for e in files[EVENTS]:
        e.setdefault("id", item_id("e", e.get("source"), e["title"], e.get("date")))
        out.append(("event", e, None))
    for l in files[LECTURES]:
        l.setdefault("id", item_id("l", l.get("date"), l["title"]))
        out.append(("lecture", l, None))
        for i, r in enumerate(l.get("recordings", [])):
            r.setdefault("id", item_id("r", l["id"], i))
            if "status" in r:  # старые записи (до модерации) без статуса не трогаем
                out.append(("recording", r, l))
    for kind, path in EXTRA.items():
        files[path] = load(path, [])
        for x in files[path]:
            x.setdefault("id", item_id(kind, x.get("source"), x.get("title"), x.get("date")))
            out.append((kind, x, None))
    return files, out


def save_all(files):
    for path, data in files.items():
        save(path, data)


def card(kind, x, parent):
    h = html.escape
    when = lambda o: " ".join(v for v in (o.get("date") or "дата не указана", o.get("time") or "") if v)
    if kind == "recording":
        lines = [f"🎬 <b>Запись лекции</b> «{h(parent['title'])}» ({h(when(parent))})",
                 *[h(u) for u in x.get("links", [])[:2]],
                 f"Как привязана: {h(x.get('match', '?'))}"]
        if x.get("video_titles"):
            lines.append(f"Видео: {h(x['video_titles'][0])}")
    else:
        head = {"lecture": "🎓 Лекция кружочка", "event": "📅 Событие", "homework": "📚 ДЗ",
                "test": "📝 Контрольная", "lesson_change": "🔄 Изменение урока", "material": "📎 Материал",
                "info": "ℹ️ Инфо"}.get(kind, f"📌 {kind}")
        if x.get("status") == "changed":
            head = "✏️ Изменилось: " + head.split(" ", 1)[1].lower()
        title = x["title"] + (f" — {x['speaker']}" if x.get("speaker") else "")
        lines = [f"{head}\n<b>{h(title)}</b>", f"Когда: {h(when(x))}"]
        for k, name in (("place", "Где"), ("audience", "Для кого"), ("registration_deadline", "Регистрация до")):
            if x.get(k):
                lines.append(f"{name}: {h(str(x[k]))}")
        if x.get("status") == "changed" and x.get("changes"):
            last = x["changes"][-1]
            if "fields" in last:  # события: {"fields": {"date": [было, стало]}}
                diff = ", ".join(f"{k} {a} → {b}" for k, (a, b) in last["fields"].items())
            else:  # лекции кружочка: {"time": новое, "source": ...}
                diff = ", ".join(f"{k} → {v}" for k, v in last.items() if k != "source")
            lines.append("Изменение: " + h(diff))
        if x.get("quote"):
            lines.append(f"«{h(x['quote'][:300])}»")
        src = x.get("source") or x.get("url") or (x.get("posts") or [None])[0]
        if src:
            lines.append(h(src))
    return "\n".join(lines)


def buttons(x):
    return {"inline_keyboard": [[{"text": "✅ Опубликовать", "callback_data": f"ok:{x['id']}"},
                                 {"text": "❌ Отклонить", "callback_data": f"no:{x['id']}"}]]}


def sync(resend=False):
    """Шлёт модератору карточки для всего, что ждёт решения и ещё не отправлено.
    Если дата события прошла, а решения нет, карточка закрывается как «прошло»."""
    state = load(STATE, {})
    chat = env("MODERATOR_CHAT_ID") or state.get("chat_id")
    if all(is_manual(g, state) for g in GROUPS) and (not TOKEN or not chat):
        return
    files, all_items = items()
    today = time.strftime("%Y-%m-%d")
    sent = 0
    for kind, x, parent in all_items:
        if x.get("status") not in WAITING:
            continue
        date = (parent or x).get("date")
        if kind != "recording" and date and date < today:
            x["status"] = "expired"
            if x.get("tg_msg"):
                close(chat, x["tg_msg"], card(kind, x, parent), "⌛ Прошло без решения")
            continue
        if not is_manual(group(kind), state):  # ручная модерация выключена — публикуем сразу
            text = card(kind, x, parent)
            approve(kind, x, parent, auto=True)
            verdict = "🤖 Опубликовано автоматически"
            if TOKEN and chat:
                if x.get("tg_msg"):
                    close(chat, x["tg_msg"], text, verdict)
                else:
                    api("sendMessage", chat_id=chat, text=f"{text}\n\n<b>{verdict}</b>", parse_mode="HTML",
                        disable_web_page_preview=True)
            continue
        if x.get("tg_msg") and not resend or not TOKEN or not chat:
            continue
        x["tg_msg"] = api("sendMessage", chat_id=chat, text=card(kind, x, parent), parse_mode="HTML",
                          disable_web_page_preview=True, reply_markup=buttons(x))["message_id"]
        sent += 1
    save_all(files)
    if sent:
        print(f"  модератору отправлено карточек: {sent}")


def close(chat, msg_id, text, verdict):
    try:
        api("editMessageText", chat_id=chat, message_id=msg_id, text=f"{text}\n\n<b>{verdict}</b>",
            parse_mode="HTML", disable_web_page_preview=True)
    except Exception as e:  # сообщение могли удалить — не страшно
        print("  не удалось обновить карточку:", e)


def reopen(x):
    """Событие или лекцию исправили в канале: снова ждёт решения, старая карточка помечается устаревшей."""
    chat = env("MODERATOR_CHAT_ID") or load(STATE, {}).get("chat_id")
    if x.get("tg_msg") and TOKEN and chat and x.get("status") in WAITING:
        try:
            api("editMessageText", chat_id=chat, message_id=x["tg_msg"],
                text="Устарело: в канале исправили пост, ниже новая карточка.")
        except Exception as e:
            print("  не удалось обновить карточку:", e)
    x.pop("tg_msg", None)
    x["status"] = "changed"


def public(x):
    return {k: v for k, v in x.items() if k not in ("tg_msg", "recordings")} | (
        {"recordings": [public(r) for r in x["recordings"] if r.get("status") != "rejected"]} if "recordings" in x else {})


def approve(kind, x, parent, auto=False):
    x["status"] = "approved"
    x["decided_at"] = time.strftime("%Y-%m-%d %H:%M")
    if auto:
        x["auto"] = True
    if kind == "recording":
        send_to_server({"kind": "recording", "lecture": parent["id"], "recording": public(x)})
    else:
        send_to_server({"kind": kind, **public(x)})


GROUPS = {"channels": "каналов (события, лекции)", "chats": "чатов по предметам"}


def group(kind):
    return "channels" if kind in ("event", "lecture", "recording") else "chats"


def is_manual(g, state=None):
    state = state if state is not None else load(STATE, {})
    return state.get(f"manual_{g}", state.get("manual", True))  # "manual" — старая общая настройка


def toggle_manual(g):
    """Переключает ручную модерацию для группы channels / chats. Возвращает новое значение (True = вручную)."""
    state = load(STATE, {})
    state[f"manual_{g}"] = not is_manual(g, state)
    save(STATE, state)
    return state[f"manual_{g}"]


def manual_text(g, on):
    if on:
        return f"Ручная модерация {GROUPS[g]} ВКЛЮЧЕНА: найденное ждёт твоей кнопки."
    return f"Ручная модерация {GROUPS[g]} ВЫКЛЮЧЕНА: найденное сразу уходит на сервер, сюда приходит только уведомление."


def manual_status():
    return "\n".join(f"{'✋ вручную' if is_manual(g) else '🤖 автоматически'} — {GROUPS[g]}" for g in GROUPS) + \
        "\nПереключить: /manual_channels, /manual_chats"


def decide(cb):
    state = load(STATE, {})
    chat = env("MODERATOR_CHAT_ID") or state.get("chat_id")
    if str(cb["from"]["id"]) != str(chat) and str(cb["message"]["chat"]["id"]) != str(chat):
        api("answerCallbackQuery", callback_query_id=cb["id"], text="Решает только модератор")
        return
    action, _, xid = cb["data"].partition(":")
    files, all_items = items()
    found = next(((k, x, p) for k, x, p in all_items if x["id"] == xid), None)
    if not found:
        api("answerCallbackQuery", callback_query_id=cb["id"], text="Не нашёл, возможно уже удалено")
        return
    kind, x, parent = found
    if x.get("status") not in WAITING:
        api("answerCallbackQuery", callback_query_id=cb["id"], text="Уже решено")
        return
    text = card(kind, x, parent)
    if action == "ok":
        approve(kind, x, parent)
        verdict = "✅ Опубликовано"
    else:
        x["status"] = "rejected"
        x["decided_at"] = time.strftime("%Y-%m-%d %H:%M")
        verdict = "❌ Отклонено"
    save_all(files)
    api("answerCallbackQuery", callback_query_id=cb["id"], text=verdict)
    close(cb["message"]["chat"]["id"], cb["message"]["message_id"], text, verdict)
    print(f"  {verdict}: {x.get('title') or parent['title']}")


def handle(upd):
    if "callback_query" in upd:
        decide(upd["callback_query"])
        return
    msg = upd.get("message") or {}
    text, chat = msg.get("text", ""), msg.get("chat", {}).get("id")
    if not chat:
        return
    state = load(STATE, {})
    owner = env("MODERATOR_CHAT_ID") or state.get("chat_id")
    if text.startswith("/start"):
        if owner and str(owner) != str(chat):
            api("sendMessage", chat_id=chat, text="У этого бота уже есть модератор.")
            return
        state["chat_id"] = chat
        save(STATE, state)
        api("sendMessage", chat_id=chat, text="Готово, ты модератор. Сюда будут приходить найденные события.")
        sync()
    elif text.startswith(("/manual_channels", "/manual_chats")) and str(owner) == str(chat):
        g = "channels" if text.startswith("/manual_channels") else "chats"
        on = toggle_manual(g)
        waiting = sum(1 for k, x, _ in items()[1] if x.get("status") in WAITING and group(k) == g)
        extra = f" Ждавшие решения ({waiting}) сейчас опубликуются." if not on and waiting else ""
        api("sendMessage", chat_id=chat, text=manual_text(g, on) + extra)
        sync()
    elif text.startswith("/manual") and str(owner) == str(chat):
        api("sendMessage", chat_id=chat, text=manual_status())
    elif text.startswith("/list") and str(owner) == str(chat):
        n = sum(1 for _, x, _ in items()[1] if x.get("status") in WAITING)
        api("sendMessage", chat_id=chat, text=f"Ждут решения: {n}")
        sync(resend=True)


MENU_SET = False


def poll(seconds):
    """Отвечает на кнопки в течение seconds секунд (вместо простого sleep между проверками каналов)."""
    if not TOKEN:
        print("  нет MODERATOR_BOT_TOKEN в .env — бот-модератор выключен")
        time.sleep(seconds)
        return
    global MENU_SET
    if not MENU_SET:  # меню команд у кнопки «/» в Telegram
        try:
            api("setMyCommands", commands=[{"command": "manual_channels", "description": "Вкл/выкл модерацию каналов"},
                                           {"command": "manual_chats", "description": "Вкл/выкл модерацию чатов"},
                                           {"command": "manual", "description": "Что сейчас включено"},
                                           {"command": "list", "description": "Что ждёт решения"}])
            MENU_SET = True
        except Exception:
            pass
    state = load(STATE, {})
    end = time.time() + seconds
    while time.time() < end:
        try:
            ups = api("getUpdates", offset=state.get("offset", 0), timeout=int(min(50, max(1, end - time.time()))),
                      allowed_updates=["message", "callback_query"])
        except Exception as e:  # нет сети — подождём
            print("  бот: ошибка сети", e)
            time.sleep(10)
            continue
        for u in ups:
            try:
                handle(u)
            except Exception as e:
                print("  бот: ошибка при обработке", e)
            state = load(STATE, {})  # handle мог записать chat_id
            state["offset"] = u["update_id"] + 1
            save(STATE, state)


if __name__ == "__main__":
    import sys
    if sys.argv[1:] in (["manual_channels"], ["manual_chats"]):
        g = sys.argv[1].split("_")[1]
        print(manual_text(g, toggle_manual(g)))
        sys.exit()
    if sys.argv[1:] == ["manual"]:
        print(manual_status())
        sys.exit()
    print("Бот-модератор запущен, Ctrl+C чтобы остановить")
    sync()
    while True:
        poll(3600)
