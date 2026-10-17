"""Находки из чатов по предметам идут в того же бота-модератора, что события из каналов (moderator.py).

Бот один (MODERATOR_BOT_TOKEN в .env). Ручная модерация чатов и каналов переключается отдельно:
/manual_chats и /manual_channels (/manual показывает, что как настроено), /list присылает всё, что ждёт решения.
Без ручной модерации найденное сразу уходит на сервер, а в бот приходит уведомление.

Очередь лежит в chats/moderation_<тип>.json (по файлу на тип находки: homework, test, lesson_change, material, info).
Там цитаты из чатов, поэтому папка chats/ никуда не копируется и не коммитится.
Полная находка агента лежит в поле item, её и получит сервер после подтверждения (moderator.send_to_server).

Запускается из chat_agent.py live:
    python3 chat_agent.py live              # чаты + бот-модератор
    python3 chat_agent.py live --channel    # то же и ещё проверка каналов (вместо отдельного watch_channel.py loop)
    python3 chat_agent.py live --no-moderator   # без бота: находки только копятся в chats/live_items.json
Два процесса не могут читать один бот (ошибка 409), поэтому watch_channel.py loop отдельно при этом не запускать:
каналы проверяет --channel.
"""
import json
import sys
import threading
import time
from pathlib import Path

import backend_api
import moderator

TYPES = ("homework", "test", "lesson_change", "material", "info")
RETRY_EVERY = 600  # раз в столько секунд повторяем то, что не ушло в бэкенд
SYNC_EVERY = 30  # раз в столько секунд новые находки уходят модератору карточками
LOCK = threading.RLock()  # файлы очереди пишут и агент (новые находки), и бот (решения)


def path(kind):
    return f"chats/moderation_{kind}.json"


def load(p):
    try:
        return json.load(open(p, encoding="utf-8"))
    except FileNotFoundError:
        return []


def save(p, data):
    Path(p).parent.mkdir(exist_ok=True)
    json.dump(data, open(p, "w", encoding="utf-8"), ensure_ascii=False, indent=1)


def fields(it):
    """Что показать на карточке: название, срок, урок, кабинет, класс, цитата, откуда."""
    dl = it.get("due_lesson") or {}
    when = it.get("time") or (f"{dl['num']} урок, {dl['start']}" if dl.get("num") and dl.get("start") else None)
    room = it.get("room") or dl.get("room")
    who = ", ".join(x for x in (it.get("class"), it.get("subject")) if x)
    return {"title": it["summary"], "date": it.get("date"), "time": when, "place": room and f"каб. {room}",
            "audience": who or None, "quote": it.get("quote"),
            "source": f"чат «{it['chat']}», сообщение #{it['message_id']}"}


def ref(it):
    return f"{it['chat']}#{it['message_id']}"


def add(it):
    p = path(it["type"])
    with LOCK:
        data = load(p)
        data.append({**fields(it), "ref": ref(it), "status": "new", "item": json.loads(json.dumps(it))})
        save(p, data)


def update(it):
    """Находке поменяли дату (поправка в чате или ответ на старое сообщение): карточка снова ждёт решения."""
    p = path(it["type"])
    with LOCK:
        data = load(p)
        x = next((x for x in data if x.get("ref") == ref(it) and x["item"]["summary"] == it["summary"]), None)
        if not x:
            data.append({**fields(it), "ref": ref(it), "status": "new", "item": json.loads(json.dumps(it))})
            save(p, data)
            return
        new = fields(it)
        diff = {k: [x.get(k), new[k]] for k in ("date", "time", "place") if new[k] != x.get(k)}
        x.update(new)
        x["item"] = json.loads(json.dumps(it))
        if diff and x.get("status") != "rejected":  # отклонённое (не задание) заново не спрашиваем
            x.setdefault("changes", []).append({"fields": diff, "at": time.strftime("%Y-%m-%d %H:%M"),
                                                "source": f"сообщение #{(it.get('updated_by') or ['?'])[-1]}"})
            if x.get("tg_msg") or x.get("status") not in moderator.WAITING:
                moderator.reopen(x)  # старая карточка помечается устаревшей, придёт новая
        save(p, data)


def push(it):
    """Вызывается из chat_agent.send_to_server: status new — новая находка, updated — изменилась."""
    if it.get("type") not in TYPES:
        return
    (update if it.get("status") == "updated" else add)(it)


def _locked(f):
    def run(*a, **kw):
        with LOCK:
            return f(*a, **kw)
    return run


def start(channel=False):
    """Регистрирует очереди чатов в moderator.py и запускает бота в фоне (и проверку каналов, если channel)."""
    for kind in TYPES:
        moderator.EXTRA[kind] = path(kind)
    moderator.handle = _locked(moderator.handle)  # нажатие кнопки не должно совпасть с записью новой находки

    def worker():
        channels = channel
        if channels:
            # llm_extract и fetch_channel читают sys.argv при импорте (там аргументы chat_agent.py), прячем их
            argv, sys.argv = sys.argv, sys.argv[:1]
            try:
                import watch_channel
            except Exception as e:  # без каналов бот-модератор для чатов всё равно работает
                print("каналы: не запустились, проверяю только чаты:", repr(e))
                channels = False
            finally:
                sys.argv = argv
        next_check = next_retry = 0
        while True:
            if channels and time.time() >= next_check:
                try:
                    watch_channel.check_once()
                except Exception as e:  # нет сети или Ollama — попробуем в следующий раз
                    print("каналы: ошибка", e)
                next_check = time.time() + watch_channel.INTERVAL
            if time.time() >= next_retry:  # то, что не ушло в бэкенд (сервер лежал, урока не было в расписании)
                try:
                    with LOCK:
                        backend_api.retry()
                except Exception as e:
                    print("в бэкенд: ошибка", repr(e))
                next_retry = time.time() + RETRY_EVERY
            try:
                with LOCK:
                    moderator.sync()
            except Exception as e:
                print("бот-модератор: ошибка", e)
            moderator.poll(SYNC_EVERY)

    if not moderator.TOKEN:
        print("Бот-модератор: нет MODERATOR_BOT_TOKEN в .env, карточки не приходят"
              + (" (ручная модерация выключена — находки сразу уходят на сервер)" if not manual() else ""))
    threading.Thread(target=worker, daemon=True).start()


def manual():
    return moderator.is_manual("chats")
