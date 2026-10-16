"""Разбор канала семинара «Кружочек» (@kruzhochek179): анонсы лекций и записи к ним.

У канала строгий формат анонса, поэтому модель не нужна:
    [среда 14 октября, 16:15, ауд.302] Сергей Локтев, "Коды Рида-Соломона"
Заголовок может стоять и не в начале поста («завтра будет ещё доклад... [понедельник 20 апреля, ...]»).
Запись — пост со ссылкой на YouTube/VK/Rutube; привязываем её к лекции, прошедшей не больше 3 дней назад,
иначе оставляем «без лекции» (например, видео Дня математика).

Проверка на скачанных постах:  python3 kruzhochek.py kruzhochek.json
"""
import json
import re
import sys
import urllib.parse
import urllib.request
from datetime import date, timedelta

MONTHS = {"янв": 1, "фев": 2, "мар": 3, "апр": 4, "май": 5, "мая": 5, "июн": 6, "июл": 7,
          "авг": 8, "сен": 9, "окт": 10, "ноя": 11, "дек": 12}
HEADER = re.compile(
    r"\[\s*(?P<wd>[а-яё]+)\s+(?P<day>\d{1,2})\s+(?P<month>[а-яё]+)\s*,\s*(?P<time>\d{1,2}[:.]\d{2})\s*,?\s*(?P<place>[^\]]*)\]"
    r"\s*(?P<speaker>.+?),\s*[«\"“](?P<title>.+?)[»\"”]", re.I)
VIDEO = re.compile(r"https?://(?:www\.)?(?:youtube\.com/watch\S+|youtu\.be/\S+|vk\.com/video\S+|vkvideo\.ru/\S+|rutube\.ru/\S+)")


def lecture_date(day, month_word, posted):
    m = MONTHS.get(month_word.lower()[:3])
    if not m:
        return None
    d = date(posted.year, m, int(day))
    if d < posted - timedelta(days=60):   # анонс в декабре про январь
        d = date(posted.year + 1, m, int(day))
    elif d > posted + timedelta(days=120):  # повтор старого анонса («25 декабря» в посте от февраля)
        d = date(posted.year - 1, m, int(day))
    return d


def classify(post):
    """-> ("lecture", {...}) | ("recording", [ссылки]) | ("other", None)"""
    text = post["text"]
    posted = date.fromisoformat(post["date"][:10])
    m = HEADER.search(text)
    if m:
        d = lecture_date(m["day"], m["month"], posted)
        place = m["place"].strip() or None
        if place and re.search(r"дистанц|онлайн|zoom", place, re.I):
            place = "онлайн"
        return "lecture", {
            "title": m["title"].strip(),
            "speaker": m["speaker"].strip(),
            "date": d.isoformat() if d else None,
            "time": m["time"].replace(".", ":"),
            "place": place,
            "source": post["url"],
            "no_recording": "без записи" in text.lower() or None,
        }
    videos = VIDEO.findall(text) + [l for l in post.get("links", []) if VIDEO.match(l)]
    if videos:
        return "recording", sorted(set(videos))
    return "other", None


def video_title(url):
    """Название ролика через открытый oEmbed YouTube (без ключа). None, если не вышло."""
    if "youtu" not in url:
        return None
    try:
        api = "https://www.youtube.com/oembed?format=json&url=" + urllib.parse.quote(url, safe="")
        with urllib.request.urlopen(api, timeout=10) as r:
            return json.loads(r.read())["title"]
    except Exception:
        return None


def words(s):
    return {w for w in re.findall(r"[а-яёa-z]{5,}", s.lower())}


def match_recording(rec, candidates):
    """Выбирает лекцию для записи. Возвращает (лекция или None, уверенность)."""
    if not candidates:
        return None, None
    titles = [t for t in (video_title(u) for u in rec["links"]) if t]
    if titles:
        rec["video_titles"] = titles
        tw = words(" ".join(titles))
        best = max(candidates, key=lambda l: len(tw & words(l["title"] + " " + l["speaker"])))
        if tw & words(best["title"] + " " + best["speaker"]):
            return best, "по названию видео"
        return None, None  # видео про другое (например, День математика)
    target = max(candidates, key=lambda l: l["date"])
    bare = VIDEO.sub("", rec["text"]).strip() == ""
    return target, "предположительно" if bare else "по тексту поста"


def process(posts, lectures=None):
    """Проходит посты по порядку. Возвращает список лекций (с записями) и записи без лекции."""
    lectures = lectures if lectures is not None else []
    loose = []
    for p in sorted(posts, key=lambda p: p["id"]):
        kind, data = classify(p)
        if kind == "lecture":
            old = next((x for x in lectures if x["date"] == data["date"] and x["title"] == data["title"]), None)
            if old:  # напоминание или повтор анонса
                old.setdefault("also_in", []).append(p["url"])
                for k in ("time", "place"):  # время/кабинет могли поменять
                    if data[k] and data[k] != old.get(k):
                        old.setdefault("changes", []).append({k: data[k], "source": p["url"]})
                        old[k] = data[k]
            else:
                data = {k: v for k, v in data.items() if v is not None}
                data["recordings"] = []
                lectures.append(data)
        elif kind == "recording":
            posted = p["date"][:10]
            past = [x for x in lectures if x["date"] and x["date"] <= posted
                    and date.fromisoformat(posted) - date.fromisoformat(x["date"]) <= timedelta(days=3)]
            rec = {"links": data, "source": p["url"], "text": p["text"][:120]}
            past = [x for x in past if not x.get("no_recording")]
            target, how = match_recording(rec, past)
            if target:
                rec["match"] = how
                target["recordings"].append(rec)
            else:
                loose.append(rec)
    return lectures, loose


if __name__ == "__main__":
    posts = json.load(open(sys.argv[1] if len(sys.argv) > 1 else "kruzhochek.json", encoding="utf-8"))
    lectures, loose = process(posts)
    kinds = [classify(p)[0] for p in posts]
    print(f"постов {len(posts)}: анонсов {kinds.count('lecture')}, с записями {kinds.count('recording')}, прочих {kinds.count('other')}")
    print(f"разных лекций: {len(lectures)}, из них с записью: {sum(1 for l in lectures if l['recordings'])}, записей без лекции: {len(loose)}\n")
    for l in sorted(lectures, key=lambda l: l["date"] or ""):
        rec = " | запись: " + ", ".join(f"{r['links'][0]} ({r['match']})" for r in l["recordings"]) if l["recordings"] else ""
        ch = f" | изменения: {l['changes']}" if l.get("changes") else ""
        print(f"{l['date']} {l['time']} {l['place']} — {l['speaker']}, «{l['title']}» (повторов {len(l.get('also_in', []))}){rec}{ch}")
    print("\nЗаписи без лекции:")
    for r in loose:
        print(" ", r["links"][0], "←", r["source"], "|", r["text"][:70].replace("\n", " "))
