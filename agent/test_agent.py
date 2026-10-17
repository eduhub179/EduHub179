"""Прогоняет test_messages.tsv через агента (фильтр + модель + правила) без Telegram и считает ошибки.

Запуск (Ollama запущена, .venv активирован):  python3 test_agent.py
Ответы модели кешируются в test_<метка промпта>_llm.json, повторный запуск быстрый; после правки промпта кеш новый сам.
"""
import json
import sys
from pathlib import Path

import chat_agent as ca

NAMES = {"h": "homework", "t": "test", "l": "lesson_change", "m": "material", "i": "info"}
POSTED = "2026-10-15T12:00:00"


def main():
    # свой кеш на каждую модель и версию промпта (CACHE_TAG), удалять руками не нужно
    cache_path = Path((sys.argv[1].rsplit(".", 1)[0] if len(sys.argv) > 1 else "test") + ca.CACHE_TAG + "_llm.json")
    ca.VERBOSE = True  # печатаем всё: фильтр, ответ модели, итог
    cache = json.load(open(cache_path, encoding="utf-8")) if cache_path.exists() else {}
    tests = sys.argv[1] if len(sys.argv) > 1 else "test_messages.tsv"
    rows = [l.rstrip("\n").split("\t") for l in open(tests, encoding="utf-8") if l.strip() and not l.startswith("#")]
    ok = bad = 0
    fails = []
    for n, (text, *rest) in enumerate(rows, 1):
        want, want_date = (rest + ["", ""])[:2]
        want_long = "long" in rest
        want_fields = dict(kv.split("=", 1) for c in rest if "=" in c for kv in c.split(";"))
        options = [{NAMES[c] for c in alt if c != " "} for alt in want.split("|")] if want else [set()]
        m = {"id": n, "chat": "Физика тест", "date": POSTED, "author": "учитель", "text": text,
             "links": [w for w in text.split() if w.startswith("http")], "files": [], "reply_to": None}
        print(f"\n[{n}] {text}")
        items = ca.process_message(m, [], m["chat"], [], cache) if ca.worth_asking(m) else []
        json.dump(cache, open(cache_path, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
        got = {it["type"] for it in items}
        first = NAMES.get((want.split("|")[0].split() or [""])[0])  # дата в тесте — про первую находку («t h» → контрольная)
        main_item = next((it for it in items if it["type"] == first), None) or \
            next((it for it in items if it["type"] in options[0] or any(it["type"] in o for o in options)), None)
        problems = []
        if not any(got == o or (o and o <= got) for o in options):
            problems.append(f"тип {sorted(got) or 'болтовня'}, ждали {want or 'болтовня'}")
        if want_date and main_item and main_item.get("date") != want_date:
            problems.append(f"дата {main_item.get('date')}, ждали {want_date}")
        if want and main_item and not main_item.get("date") and not want_long:
            problems.append("нет даты")
        if want_long and main_item and not main_item.get("long_term"):
            problems.append("не помечено как долгое")
        for k, v in want_fields.items():  # кабинет сравниваем по цифрам, время без ведущего нуля
            got_v = str((main_item or {}).get(k) or "")
            norm = (lambda x: "".join(ch for ch in x if ch.isdigit())) if k == "room" else (lambda x: x.lstrip("0"))
            if norm(got_v) != norm(v):
                problems.append(f"{k} {got_v or 'нет'}, ждали {v}")
        for it in items:
            print(ca.show(it))
        if problems:
            bad += 1
            fails.append(f"[{n}] {text}: {'; '.join(problems)}")
            print(f"✗ ОШИБКА: {'; '.join(problems)}")
        else:
            ok += 1
            print("✓ верно")
    print(f"\n===== Верно {ok} из {ok + bad} =====")
    for f in fails:
        print("✗", f)


if __name__ == "__main__":
    main()
