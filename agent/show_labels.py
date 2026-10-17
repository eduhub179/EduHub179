"""Собирает chats/разметка.html: все размеченные сообщения с фильтром по типу. Открывать в браузере."""
import html
import json

NAMES = {"h": "домашка", "t": "контрольная", "l": "изменение урока", "m": "материал",
         "i": "инфо", "p": "уточнение", "n": "болтовня"}
COLORS = {"h": "#2b7", "t": "#d63", "l": "#c90", "m": "#37c", "i": "#86c", "p": "#999", "n": "#ccc"}

rows = [json.loads(l) for l in open("chats/dataset.jsonl", encoding="utf-8")]
codes = {}
for line in open("chat_labels.txt", encoding="utf-8"):
    if not line.startswith("#"):
        for tok in line.split():
            codes[int(tok.rstrip("htlmip"))] = tok.lstrip("0123456789")
out = []
for i, r in enumerate(rows):
    c = codes.get(i, "n")
    tags = "".join(f'<b style="background:{COLORS[k]}">{NAMES[k]}</b>' for k in c)
    extra = (f'<div class=f>📎 {html.escape(", ".join(r["files"]))}</div>' if r["files"] else "")
    reply = (f'<div class=r>↪ {html.escape(r["reply_to"][:150])}</div>' if r["reply_to"] else "")
    kw = "" if r["keyword_filter"] or c == "n" else '<i title="ключевые слова это пропускают">не ловится словами</i>'
    out.append(f'<div class="m c-{" c-".join(c)}"><div class=h>{tags} {html.escape(r["chat"])} · {r["date"]} · '
               f'{html.escape(r["author"])} {kw}</div>{reply}<div>{html.escape(r["text"])}</div>{extra}</div>')
buttons = "".join(f'<button onclick="f(\'{k}\')">{v} ({sum(k in codes.get(i, "n") for i in range(len(rows)))})</button>'
                  for k, v in NAMES.items())
page = f"""<!doctype html><meta charset=utf-8><title>Разметка чатов</title><style>
body{{font:14px system-ui;max-width:900px;margin:auto;padding:12px}} .m{{border-bottom:1px solid #eee;padding:6px 0;white-space:pre-wrap}}
.h{{color:#777;font-size:12px}} b{{color:#fff;border-radius:4px;padding:1px 5px;margin-right:3px}} .r,.f{{color:#888;font-size:12px}}
i{{color:#d33;font-size:12px}} button{{margin:2px}}</style>
<p>{buttons}<button onclick="f('')">все</button></p>{''.join(out)}
<script>function f(k){{document.querySelectorAll('.m').forEach(e=>e.style.display=!k||e.classList.contains('c-'+k)?'':'none')}}f('h')</script>"""
open("chats/разметка.html", "w", encoding="utf-8").write(page)
print(f"готово: chats/разметка.html ({len(rows)} сообщений)")
