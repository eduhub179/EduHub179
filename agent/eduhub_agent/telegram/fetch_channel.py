"""Скачивает посты публичного Telegram-канала через веб-превью t.me/s/<канал>.

Запуск:  python3 fetch_channel.py school179moscow 200
        python3 fetch_channel.py kruzhochek179 200 kruzhochek.json   # другой канал в другой файл
Результат: posts.json (новые сверху), без входа в Telegram и без зависимостей.
"""
import html
import json
import re
import sys
import time
import urllib.request

from eduhub_agent.paths import data

CHANNEL = sys.argv[1] if len(sys.argv) > 1 else "school179moscow"
LIMIT = int(sys.argv[2]) if len(sys.argv) > 2 else 200
OUT = sys.argv[3] if len(sys.argv) > 3 else str(data("posts.json"))
UA = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36"


def get(url):
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=30) as r:
        return r.read().decode("utf-8")


def clean(fragment):
    fragment = re.sub(r"<br\s*/?>", "\n", fragment)
    fragment = re.sub(r"<[^>]+>", "", fragment)
    return html.unescape(fragment).strip()


def parse(page):
    posts = []
    for block in page.split('class="tgme_widget_message_wrap')[1:]:
        m = re.search(r'data-post="([^"/]+)/(\d+)"', block)
        if not m:
            continue
        text = re.search(r'class="tgme_widget_message_text[^"]*"[^>]*>(.*?)</div>', block, re.S)
        date = re.search(r'<time[^>]*datetime="([^"]+)"', block)
        photos = re.findall(r"tgme_widget_message_photo_wrap[^>]*background-image:url\('([^']+)'\)", block)
        links = re.findall(r'class="tgme_widget_message_text[^"]*".*?</div>', block, re.S)
        hrefs = re.findall(r'href="(https?://[^"]+)"', links[0]) if links else []
        posts.append({
            "id": int(m.group(2)),
            "url": f"https://t.me/{m.group(1)}/{m.group(2)}",
            "date": date.group(1) if date else None,
            "text": clean(text.group(1)) if text else "",
            "photos": photos,
            "links": hrefs,
            "edited": "tgme_widget_message_meta" in block and ">edited<" in block,
        })
    return posts


def full_text(channel, post_id):
    """Полный текст одного поста со страницы встраивания (в ленте t.me/s/ длинные посты обрезаны на «…»)."""
    page = get(f"https://t.me/{channel}/{post_id}?embed=1&mode=tme")
    m = re.search(r'class="tgme_widget_message_text[^"]*"[^>]*>(.*?)</div>', page, re.S)
    return clean(m.group(1)) if m else None


def complete(posts):
    """Догружает полный текст постов, обрезанных в ленте. Если не вышло — оставляет как есть."""
    for p in posts:
        if p["text"].endswith("…"):
            channel, post_id = p["url"].rsplit("/", 2)[-2:]
            try:
                text = full_text(channel, post_id)
            except Exception:
                text = None
            if text and len(text) > len(p["text"]) - 1:
                p["text"] = text
                p["completed"] = True
    return posts


def main():
    seen, url = {}, f"https://t.me/s/{CHANNEL}"
    while len(seen) < LIMIT:
        batch = parse(get(url))
        new = [p for p in batch if p["id"] not in seen]
        if not new:
            break
        for p in new:
            seen[p["id"]] = p
        url = f"https://t.me/s/{CHANNEL}?before={min(p['id'] for p in batch)}"
        print(f"постов: {len(seen)}")
        time.sleep(1.5)
    posts = sorted(seen.values(), key=lambda p: -p["id"])[:LIMIT]
    with open(OUT, "w", encoding="utf-8") as f:
        json.dump(posts, f, ensure_ascii=False, indent=1)
    with_photo = sum(1 for p in posts if p["photos"])
    print(f"Сохранено {len(posts)} постов в {OUT} ({with_photo} с картинками), "
          f"с {posts[-1]['date'] if posts else '-'} по {posts[0]['date'] if posts else '-'}")


if __name__ == "__main__":
    main()
