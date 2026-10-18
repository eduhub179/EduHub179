"""Прогоняет посты канала через локальную модель Ollama и сравнивает с ручной разметкой.

Запуск (Ollama должна быть запущена):
    python3 llm_extract.py                 # модель qwen2.5:7b
    python3 llm_extract.py qwen2.5:3b      # другая модель
    python3 llm_extract.py qwen2.5:7b 20   # только первые 20 постов, для пробы
Ответы сохраняются в llm_<модель>.json; при повторном запуске уже готовые посты пропускаются.
"""
import json
import os
import re
import sys
import time
import urllib.request
from datetime import date

from baseline_rules import extract as rules_extract

MODEL = sys.argv[1] if len(sys.argv) > 1 else "qwen2.5:7b"
LIMIT = int(sys.argv[2]) if len(sys.argv) > 2 else None
OUT = f"llm_{MODEL.replace(':', '_').replace('/', '_')}_v2.json"  # v2: события первыми, примеры в промпте
URL = os.environ.get("OLLAMA_URL", "http://localhost:11434") + "/api/chat"

SCHEMA = {
    "type": "object",
    "properties": {
        "events": {"type": "array", "items": {
            "type": "object",
            "properties": {
                "quote": {"type": "string"},
                "title": {"type": "string"},
                "date": {"type": ["string", "null"]},
                "end_date": {"type": ["string", "null"]},
                "time": {"type": ["string", "null"]},
                "place": {"type": ["string", "null"]},
                "audience": {"type": ["string", "null"]},
                "registration_deadline": {"type": ["string", "null"]},
            },
            "required": ["quote", "title", "date", "time", "place"],
        }},
    },
    "required": ["events"],
}

SYSTEM = """Ты разбираешь посты Telegram-канала школы 179 и выписываешь БУДУЩИЕ мероприятия.
Мероприятие: лекция, концерт, спектакль, матч, олимпиада или её тур, конференция, праздник, бал,
первое занятие кружка, 1 сентября, каникулы, срок регистрации куда-либо.
Не выписывай то, что уже прошло: итоги, результаты, поздравления, некрологи, фотоотчёты.
Если пост в основном про итоги, но в нём есть фраза о будущем событии («Сложный тур пройдёт 18 октября»), выпиши это событие.
Если событий несколько (график олимпиад на неделю), выпиши каждое отдельно. Если будущих событий нет, верни пустой список.

Поля события:
- quote: фраза из поста, где сказано про это событие (дословно).
- title: короткое название.
- date, end_date: YYYY-MM-DD. «Завтра», «в эту субботу» считай от даты поста. Нет точной даты — null.
- time: ЧЧ:ММ начала, если написано в тексте, иначе null (не пиши 00:00).
- place: кабинет, зал или адрес из текста, иначе null.
- audience: классы, для кого, иначе null.
- registration_deadline: YYYY-MM-DD, если указан срок регистрации.

Пример 1. Дата поста: 2026-10-04. Текст: «Базовый тур Турнира городов успешно состоялся! Участвовали более 100 человек. Сложный тур пройдёт через 2 недели, 18 октября!»
Ответ: {"events": [{"quote": "Сложный тур пройдёт через 2 недели, 18 октября!", "title": "Турнир городов, сложный тур", "date": "2026-10-18", "end_date": null, "time": null, "place": null, "audience": null, "registration_deadline": null}]}

Пример 2. Дата поста: 2026-05-02. Текст: «В Татарстане завершился заключительный этап олимпиады по обществознанию. Пётр Тестов стал победителем!»
Ответ: {"events": []}

Отвечай только JSON."""


def ask(post):
    body = {
        "model": MODEL,
        "stream": False,
        "format": SCHEMA,
        "options": {"temperature": 0, "num_ctx": 8192},
        "messages": [
            {"role": "system", "content": SYSTEM},
            {"role": "user", "content": f"Дата поста: {post['date'][:10]}\n\n{post['text']}"},
        ],
    }
    req = urllib.request.Request(URL, data=json.dumps(body).encode(), headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=300) as r:
        content = json.loads(r.read())["message"]["content"]
    out = json.loads(content)
    for e in out.get("events", []):
        if e.get("time") in ("00:00", "0:00"):
            e["time"] = None
    out["is_announcement"] = bool(out.get("events"))
    return out


def refine(post, out):
    """Гибрид: модель находит событие и цитату, а дату, время и кабинет берём регулярками из цитаты.
    Время, которого нет в посте, выбрасываем: маленькие модели его выдумывают."""
    for e in out.get("events", []):
        quote = e.get("quote") or ""
        if quote not in post["text"]:      # цитата выдумана — ей не доверяем, правим только по совпадению с постом
            quote = ""
        got = rules_extract({"text": quote, "date": post["date"]})
        if got["dates"] and (not e.get("date") or e["date"] not in {d.isoformat() for d in got["dates"]}):
            e["date"] = got["dates"][0].isoformat()
        in_post = rules_extract(post)["times"]
        t = norm_time(e.get("time"))
        e["time"] = t if t in got["times"] or t in in_post else (sorted(got["times"])[0] if got["times"] else None)
        if not e.get("place") and got["places"]:
            e["place"] = sorted(got["places"])[0]
    posted = post["date"][:10]
    events = [e for e in out.get("events", []) if not (e.get("date") and e["date"] < posted)]  # прошедшее — не анонс
    whole = rules_extract(post)
    if len(events) == 1 and not events[0].get("time") and len(whole["times"]) == 1:
        events[0]["time"] = next(iter(whole["times"]))  # одно событие и одно время в посте — скорее всего его
    out["events"] = events
    out["is_announcement"] = bool(events)
    return out


def norm_time(t):
    m = re.match(r"(\d{1,2})[:.](\d{2})", t or "")
    return f"{int(m.group(1))}:{m.group(2)}" if m else None


def evaluate(results, labels):
    tp = fp = fn = 0
    d_hit = d_all = t_hit = t_all = p_hit = p_all = 0
    extra_dates = 0
    misses, false_alarms = [], []
    for r in labels:
        if r.get("duplicate_of") or str(r["id"]) not in results:
            continue
        if r["category"] in ("series", "admissions"):  # кружки и набор: засчитываем любой ответ
            continue
        got = results[str(r["id"])]
        want = r["is_announcement"] and r["category"] in ("event", "schedule", "olympiad")
        is_ann = bool(got.get("is_announcement")) and bool(got.get("events"))
        if is_ann and want: tp += 1
        elif is_ann: fp += 1; false_alarms.append(r["id"])
        elif want: fn += 1; misses.append(r["id"])
        if not want:
            continue
        got_dates = {e.get("date") for e in got.get("events", [])}
        got_times = {norm_time(e.get("time")) for e in got.get("events", [])}
        got_places = " | ".join((e.get("place") or "").lower() for e in got.get("events", []))
        want_dates = {e["date"] for e in r["events"] if e.get("date")}
        extra_dates += len({d for d in got_dates if d} - want_dates)
        for e in r["events"]:
            if e.get("date"):
                d_all += 1; d_hit += e["date"] in got_dates
            if norm_time(e.get("time")):
                t_all += 1; t_hit += norm_time(e["time"]) in got_times
            if e.get("place") and e["place"] != "школа":
                p_all += 1
                key = re.search(r"\d{3}", e["place"])
                p_hit += (key.group(0) if key else e["place"].lower()[:8]) in got_places
    pct = lambda a, b: f"{a}/{b} ({a / b:.0%})" if b else "—"
    print(f"\nМодель {MODEL}, постов с ответом: {sum(1 for r in labels if str(r['id']) in results)}")
    print(f"Анонс или нет: верно {tp}, ложных {fp}, пропущено {fn} "
          f"(точность {tp / max(tp + fp, 1):.0%}, полнота {tp / max(tp + fn, 1):.0%})")
    print("Даты событий найдены:", pct(d_hit, d_all), f"| лишних дат: {extra_dates}")
    print("Время найдено:", pct(t_hit, t_all))
    print("Место найдено (кроме «школа»):", pct(p_hit, p_all))
    print("Пропущенные анонсы:", misses)
    print("Ложные срабатывания:", false_alarms)


def main():
    posts = json.load(open("posts.json", encoding="utf-8"))
    labels = json.load(open("labels.json", encoding="utf-8"))
    dup = {r["id"] for r in labels if r.get("duplicate_of")}
    todo = [p for p in posts if p["id"] not in dup][:LIMIT]
    try:
        results = json.load(open(OUT, encoding="utf-8"))
    except FileNotFoundError:
        results = {}
    started = time.time()
    for i, p in enumerate(todo, 1):
        if str(p["id"]) in results:
            continue
        try:
            results[str(p["id"])] = ask(p)
        except Exception as e:  # модель иногда отвечает невалидным JSON — пропускаем пост
            print(f"пост {p['id']}: ошибка {e}")
            continue
        n = len(results[str(p["id"])].get("events", []))
        print(f"[{i}/{len(todo)}] {p['id']}: событий {n}  ({time.time() - started:.0f} с)")
        json.dump(results, open(OUT, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
    by_id = {str(p["id"]): p for p in posts}
    print("\n=== Только модель ===")
    evaluate(results, labels)
    print("\n=== Модель + регулярки по цитате ===")
    import copy
    evaluate({k: refine(by_id[k], copy.deepcopy(v)) for k, v in results.items()}, labels)


if __name__ == "__main__":
    main()
