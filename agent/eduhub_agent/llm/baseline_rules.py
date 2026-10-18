"""Извлечение мероприятий без модели: регулярки + ключевые слова.
Запуск: python3 -m eduhub_agent.llm.baseline_rules  (posts.json и labels.json — в корне agent/)
Печатает, насколько правила совпадают с ручной разметкой.
"""
import json
import re
from datetime import date, timedelta

from eduhub_agent.paths import data

MONTHS = {"январ": 1, "феврал": 2, "март": 3, "апрел": 4, "ма": 5, "июн": 6, "июл": 7,
          "август": 8, "сентябр": 9, "октябр": 10, "ноябр": 11, "декабр": 12}
WEEKDAYS = {"понедельник": 0, "вторник": 1, "сред": 2, "четверг": 3, "пятниц": 4, "суббот": 5, "воскресень": 6}

FUTURE = re.compile(r"состоится|пройд[её]т|пройдут|приходите|приглаша|начн[её]тся|начало в|ждём|ждем|"
                    r"регистрац|не пропустите|стартует|будет сыгран|прочитает|прочтёт|придёт", re.I)
PAST = re.compile(r"итоги|состоялся|состоялась|состоялось|прош[её]л|прошла|прошло|завершил|"
                  r"поздравля|ушёл из жизни|ушла из жизни|стал[аи]? (?:обладател|победител|призёр)", re.I)

DATE_RE = re.compile(r"(\d{1,2})(?:\s*[-–]\s*\d{1,2})?\s+(январ\w*|феврал\w*|март\w*|апрел\w*|ма[яй]\w*|июн\w*|июл\w*|"
                     r"август\w*|сентябр\w*|октябр\w*|ноябр\w*|декабр\w*)", re.I)
NUM_DATE_RE = re.compile(r"\b(\d{1,2})\.(\d{2})(?:\.(\d{4}))?\b(?!\s*[-–:]?\s*\d{2}\b)")
TIME_RE = re.compile(r"\b([01]?\d|2[0-3])[:.]([0-5]\d)\b")
PLACE_RE = re.compile(r"(?:каб(?:инет[а-я]*|\.)?|ауд(?:итори[а-я]*|\.)?)\s*(\d{3})|актов\w+ зал\w*", re.I)


def month_of(word):
    w = word.lower()
    for k, v in MONTHS.items():
        if w.startswith(k):
            return v


def with_year(m, d, posted):
    y = posted.year
    try:
        dt = date(y, m, d)
    except ValueError:
        return None
    if dt < posted - timedelta(days=60):   # «15 февраля» в посте от декабря — это следующий год
        dt = date(y + 1, m, d)
    return dt


def extract(post):
    text = post["text"]
    posted = date.fromisoformat(post["date"][:10])
    dates = set()
    for m in DATE_RE.finditer(text):
        dt = with_year(month_of(m.group(2)), int(m.group(1)), posted)
        if dt:
            dates.add(dt)
    for m in NUM_DATE_RE.finditer(text):
        if int(m.group(2)) <= 12:
            dt = with_year(int(m.group(2)), int(m.group(1)), posted)
            if dt:
                dates.add(dt)
    low = text.lower()
    if re.search(r"\bзавтра\b", low):
        dates.add(posted + timedelta(days=1))
    if re.search(r"\bсегодня\b", low):
        dates.add(posted)
    for m in re.finditer(r"(?:в эт[уо]т?|в ближайш\w+|в следующ\w+)\s+(понедельник|вторник|сред\w*|четверг|пятниц\w*|суббот\w*|воскресень\w*)", low):
        wd = next(v for k, v in WEEKDAYS.items() if m.group(1).startswith(k))
        delta = (wd - posted.weekday()) % 7 or 7
        if "следующ" in m.group(0) and delta < 7:
            delta += 7 if wd <= posted.weekday() else 0
        dates.add(posted + timedelta(days=delta))
    future = {d for d in dates if d >= posted}
    times = {f"{int(h)}:{mm}" for h, mm in TIME_RE.findall(text) if 7 <= int(h) <= 22}
    places = {("каб. " + m.group(1)) if m.group(1) else "актовый зал" for m in PLACE_RE.finditer(text)}
    score = len(FUTURE.findall(text)) - len(PAST.findall(text[:300]))
    is_ann = bool(future) and score > 0
    return {"is_announcement": is_ann, "dates": sorted(future), "times": times, "places": places}


def norm_time(t):
    m = re.match(r"(\d{1,2})[:.](\d{2})", t or "")
    return f"{int(m.group(1))}:{m.group(2)}" if m else None


def main():
    posts = {p["id"]: p for p in json.load(open(data("posts.json"), encoding="utf-8"))}
    labels = [r for r in json.load(open(data("labels.json"), encoding="utf-8")) if not r.get("duplicate_of")]
    tp = fp = fn = 0
    d_hit = d_all = t_hit = t_all = p_hit = p_all = 0
    misses, false_alarms = [], []
    for r in labels:
        got = extract(posts[r["id"]])
        want = r["is_announcement"] and r["category"] in ("event", "schedule", "olympiad")
        if got["is_announcement"] and want: tp += 1
        elif got["is_announcement"]: fp += 1; false_alarms.append(r["id"])
        elif want: fn += 1; misses.append(r["id"])
        if not want:
            continue
        for e in r["events"]:
            if e.get("date"):
                d_all += 1
                d_hit += date.fromisoformat(e["date"]) in got["dates"]
            if norm_time(e.get("time")):
                t_all += 1
                t_hit += norm_time(e["time"]) in got["times"]
            if e.get("place") and e["place"] != "школа":
                p_all += 1
                p_hit += any(pl.split()[-1] in e["place"] or pl in e["place"] for pl in got["places"])
    print(f"Анонс или нет: верно найдено {tp}, ложных {fp}, пропущено {fn} "
          f"(точность {tp/(tp+fp):.0%}, полнота {tp/(tp+fn):.0%})")
    print(f"Даты событий найдены: {d_hit}/{d_all} ({d_hit/d_all:.0%})")
    print(f"Время найдено: {t_hit}/{t_all} ({t_hit/t_all:.0%})")
    print(f"Место найдено (кроме «школа»): {p_hit}/{p_all} ({p_hit/p_all:.0%})")
    print("Пропущенные анонсы:", misses)
    print("Ложные срабатывания:", false_alarms)


if __name__ == "__main__":
    main()
