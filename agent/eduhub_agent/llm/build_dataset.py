"""Собирает сообщения из всех экспортов чатов в один файл для разметки и обучения классификатора.

Запуск:  python3 -m eduhub_agent.llm.build_dataset   # все папки и result.json в chats/
Результат: chats/dataset.jsonl, по строке на сообщение (подряд идущие сообщения одного автора склеены).
Разметка chat_labels.txt ссылается на номера строк, поэтому старые строки остаются на своих местах,
а сообщения из новых чатов дописываются в конец.
Имена авторов заменены на «участник N» (свои номера в каждом чате), телефоны и почты вырезаны.
Метку (label) потом ставит разметчик: none / homework / test / lesson_change / material / info.
"""
import json
import re
from pathlib import Path

from eduhub_agent.telegram import chat_agent as ca

PHONE = re.compile(r"\+?\d[\d\s()-]{9,}\d")
EMAIL = re.compile(r"\S+@\S+\.\w+")


def scrub(text, names):
    text = PHONE.sub("[телефон]", EMAIL.sub("[почта]", text))
    for name, alias in names.items():  # упоминания по имени тоже заменяем
        for part in {name, *name.split()}:
            if len(part) >= 3:
                text = re.sub(rf"\b{re.escape(part)}\b", alias, text)
    return text


def keyword_ok(m):
    """Старый фильтр по ключевым словам (для сравнения с классификатором в train_classifier.py)."""
    return bool(m["files"] or m["links"] or (len(m["text"]) >= 6 and ca.USEFUL.search(m["text"])))


def exports(root):
    for p in sorted(root.iterdir()):
        if p.is_dir() and ((p / "result.json").exists() or list(p.glob("messages*.html"))):
            yield p
        elif p.suffix == ".json" and p.name not in ("dataset.json",) and not p.stem.endswith(("_items", "_llm")):
            yield p


def main():
    root = Path("chats")
    rows = []
    for path in exports(root):
        data = ca.load_export(path)
        if not isinstance(data, dict) or "messages" not in data:  # chats/live_items*.json и прочие не-экспорты
            continue
        chat = data.get("name") or path.stem
        msgs = ca.merge_bursts([ca.from_export(m, chat) for m in data["messages"] if m.get("type") == "message"])
        names = {}
        for m in msgs:
            names.setdefault(m["author"], f"участник {len(names) + 1}")
        for i, m in enumerate(msgs):
            if not (m["text"] or m["files"]):
                continue
            reply = next((x for x in msgs[:i] if m["reply_to"] in x["message_ids"]), None)
            rows.append({
                "key": f"{chat}#{m['id']}",
                "chat": chat,
                "date": m["date"][:16],
                "author": names[m["author"]],
                "text": scrub(m["text"], names),
                "files": m["files"],
                "links": len(m["links"]),
                "reply_to": scrub(reply["text"][:200], names) if reply else None,
                "prev": [scrub(x["text"][:150], names) for x in msgs[max(0, i - 2):i] if x["text"]],
                "keyword_filter": keyword_ok(m),
                "label": None,
            })
        print(f"{chat}: {len(msgs)} сообщений")
    # одинаковые чаты из повторных экспортов (как «…11Б» и «…11Б_2») не дублируем
    out = root / "dataset.jsonl"
    uniq = {}
    if out.exists():  # старые строки не трогаем и не переставляем: на их номера ссылается разметка
        for line in open(out, encoding="utf-8"):
            r = json.loads(line)
            uniq[r["key"]] = r
    old = len(uniq)
    for r in rows:
        uniq.setdefault(r["key"], r)
    with open(out, "w", encoding="utf-8") as f:
        for r in uniq.values():
            f.write(json.dumps(r, ensure_ascii=False) + "\n")
    print(f"\nВсего {len(uniq)} сообщений → {out} (новых {len(uniq) - old}, они с номера {old})")


if __name__ == "__main__":
    main()
