"""Агент каналов: @school179moscow -> events.json (модель + правила),
@kruzhochek179 -> lectures.json (анонсы лекций и записи к ним, по строгому формату, без модели),
              а прочие его посты (лекции у коллег, регистрации) -> через модель в тот же events.json.

Запуск (Ollama должна работать):
    python3 watch_channel.py          # одна проверка
    python3 watch_channel.py loop     # проверять каждые 30 минут, пока не остановишь Ctrl+C
Первый запуск берёт только посты за последние 14 дней. Что уже обработано, хранится в seen.json.
Посты за последние 7 дней перечитываются: если текст исправили, событие обновляется (status "changed").
События, которые уже прошли на сегодня, не добавляются и не рассылаются.
Новые и изменённые события, лекции и записи ждут подтверждения в Telegram-боте (moderator.py):
на сервер уходит только то, что ты подтвердил кнопкой. Без MODERATOR_BOT_TOKEN в .env бот выключен,
события просто копятся в events.json / lectures.json со статусом "new".
"""
import difflib
import hashlib
import json
import sys
import time
from datetime import date, timedelta

import kruzhochek
import llm_extract
import moderator
from fetch_channel import complete, get, parse

CHANNEL = "school179moscow"
MODEL = "qwen2.5:7b"
EVENTS = "events.json"
SEEN = "seen.json"
FIRST_RUN_DAYS = 14
EDIT_WINDOW_DAYS = 7
INTERVAL = 30 * 60


def load(path, default):
    try:
        return json.load(open(path, encoding="utf-8"))
    except FileNotFoundError:
        return default


def save(path, data):
    json.dump(data, open(path, "w", encoding="utf-8"), ensure_ascii=False, indent=1)


def text_hash(p):
    return hashlib.sha1(p["text"].encode("utf-8")).hexdigest()[:12]


def get_posts(channel):
    return sorted(complete(parse(get(f"https://t.me/s/{channel}"))), key=lambda p: p["id"])


def similar(a, b):
    return difflib.SequenceMatcher(None, a.lower(), b.lower()).ratio() > 0.5


def same_event(a, b):
    """Повтор или обновление: та же дата и похожее название."""
    return a.get("date") == b.get("date") and similar(a["title"], b["title"])


def add_llm_events(p, events, edited=False):
    """Пост без строгого формата: модель + правила, результат в events.json со склейкой повторов.
    edited=True — пост исправили: ищем его старые события и обновляем дату/время/место."""
    llm_extract.MODEL = MODEL
    out = llm_extract.refine(p, llm_extract.ask(p))
    today = date.today().isoformat()
    for e in out["events"]:
        e = {k: v for k, v in e.items() if v is not None}
        e["source"] = p["url"]
        if edited:
            mine = [x for x in events if p["url"] in [x.get("source")] + x.get("also_in", [])]
            old = next((x for x in mine if similar(x["title"], e["title"])), None)
            if old:
                diff = {k: [old.get(k), e.get(k)] for k in ("date", "time", "place") if e.get(k) and e.get(k) != old.get(k)}
                if diff:
                    for k, (_, new) in diff.items():
                        old[k] = new
                    old.setdefault("changes", []).append({"fields": diff, "at": time.strftime("%Y-%m-%d %H:%M"), "source": p["url"]})
                    moderator.reopen(old)  # снова ждёт подтверждения
                    print(f"  ИЗМЕНЕНО: {old['title']}: " + ", ".join(f"{k} {a} → {b}" for k, (a, b) in diff.items()))
                continue
        if e.get("date") and e["date"] < today:
            print(f"  прошло, пропускаю: {e['title']} ({e['date']})")
            continue
        old = next((x for x in events if same_event(x, e)), None)
        if old:
            if p["url"] != old.get("source") and p["url"] not in old.get("also_in", []):
                old.setdefault("also_in", []).append(p["url"])
            for k, v in e.items():  # уточняем: время, место, если раньше не было
                old.setdefault(k, v)
            print(f"  повтор: {e['title']} ({e.get('date')}) ← {p['url']}")
        else:
            e["status"] = "new"  # ждёт модератора: approved / rejected
            events.append(e)
            print(f"  НОВОЕ: {e['title']} | {e.get('date') or 'дата не указана'} {e.get('time', '')} {e.get('place', '')} ← {p['url']}")


def check_school():
    seen_all = load(SEEN, {})
    last = seen_all.get("last_id")
    hashes = seen_all.setdefault("hashes", {})
    events = load(EVENTS, [])
    posts = get_posts(CHANNEL)
    if last is None:
        since = (date.today() - timedelta(days=FIRST_RUN_DAYS)).isoformat()
        new = [p for p in posts if p["date"][:10] >= since]
    else:
        new = [p for p in posts if p["id"] > last]
    edit_since = (date.today() - timedelta(days=EDIT_WINDOW_DAYS)).isoformat()
    edited = [p for p in posts if last is not None and p["id"] <= last and p["date"][:10] >= edit_since
              and str(p["id"]) in hashes and hashes[str(p["id"])] != text_hash(p)]
    for p in edited:
        print(f"  пост исправлен: {p['url']}")
        add_llm_events(p, events, edited=True)
    for p in new:
        add_llm_events(p, events)
        hashes[str(p["id"])] = text_hash(p)
        seen_all["last_id"] = max(seen_all.get("last_id") or 0, p["id"])
        save(EVENTS, events)
        save(SEEN, seen_all)
    for p in posts:  # запоминаем тексты всех постов с страницы, чтобы потом заметить правки
        hashes[str(p["id"])] = text_hash(p)
    if posts and seen_all.get("last_id") is None:
        seen_all["last_id"] = posts[-1]["id"]
    save(EVENTS, events)
    save(SEEN, seen_all)
    print(f"{time.strftime('%H:%M')} школа: новых постов {len(new)}, исправленных {len(edited)}, событий в базе {len(events)}")


def check_kruzhochek():
    seen_all = load(SEEN, {})
    last = seen_all.get("kruzhochek179")
    lectures = load("lectures.json", [])
    events = load(EVENTS, [])
    posts = get_posts("kruzhochek179")
    new = [p for p in posts if last is None or p["id"] > last]
    known = {(l["date"], l["title"]): (len(l["recordings"]), len(l.get("changes", []))) for l in lectures}
    kruzhochek.process(new, lectures)
    # прочие посты (лекции у коллег, регистрации на школы и т.п.) — через модель
    since = (date.today() - timedelta(days=FIRST_RUN_DAYS)).isoformat()
    for p in new:
        if kruzhochek.classify(p)[0] == "other" and len(p["text"]) >= 40 and (last is not None or p["date"][:10] >= since):
            add_llm_events(p, events)
    save(EVENTS, events)
    today = date.today().isoformat()
    for l in lectures:
        key = (l["date"], l["title"])
        if key not in known:
            if l["date"] and l["date"] < today:
                l["status"] = "past"  # храним, чтобы привязывать записи, но не рассылаем
                continue
            l["status"] = "new"  # ждёт модератора
            print(f"  ЛЕКЦИЯ: {l['date']} {l['time']} {l.get('place', '')} — {l['speaker']}, «{l['title']}»")
            continue
        recs, changes = known[key]
        for r in l["recordings"][recs:]:
            r["status"] = "new"  # запись тоже ждёт модератора
            print(f"  ЗАПИСЬ к «{l['title']}»: {r['links'][0]} ({r['match']})")
        if len(l.get("changes", [])) > changes and l.get("status") != "past":
            moderator.reopen(l)
            print(f"  ЛЕКЦИЯ ИЗМЕНЕНА: «{l['title']}»: {l['changes'][-1]}")
    if posts:
        seen_all["kruzhochek179"] = max(p["id"] for p in posts)
    save("lectures.json", lectures)
    save(SEEN, seen_all)
    print(f"{time.strftime('%H:%M')} кружочек: новых постов {len(new)}, лекций в базе {len(lectures)}")


def check_once():
    check_school()
    check_kruzhochek()


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "loop":
        while True:
            try:
                check_once()
                moderator.sync()
            except Exception as e:  # нет сети или Ollama — попробуем в следующий раз
                print("ошибка:", e)
            moderator.poll(INTERVAL)  # между проверками бот отвечает на кнопки
    else:
        check_once()
        moderator.sync()
