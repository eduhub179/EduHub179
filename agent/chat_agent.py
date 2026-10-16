"""Агент чатов по предметам: домашка, контрольные, переносы и отмены уроков, смена кабинета, материалы.

Два режима, разбор у них общий (process_message):
    python3 chat_agent.py chats/result.json            # экспорт Telegram Desktop (JSON, с фото и файлами)
    python3 chat_agent.py "chats/ChatExport_..."       # папка экспорта: JSON или HTML, оба формата понимаются
    python3 chat_agent.py chats/result.json --all      # не отсеивать болтовню
(Болтовню отсеивает chats/filter_model.pkl из train_classifier.py, а без него ключевые слова.)
Домашке без даты ставится срок «к следующему уроку». Если чат есть в timetable.json (см. timetable.py), срок и уроки
берутся из расписания с заменами: lesson_given — на каком уроке задали (во время урока или в первые 7 минут после),
due_lesson и due_kind — к какому уроку («на следующий урок», «через урок», «через неделю», «к дате»).
Без расписания дни уроков берутся из lessons.json, например
{"Русский язык": ["вторник", "пятница"], "Физика": ["вторник", "четверг"]} (часть названия чата -> дни),
а без него угадываются по срокам прошлых домашек. Откуда срок, видно в поле date_source.
У каждой находки есть posted (когда написали) и date: срок домашки, день контрольной или изменения урока
(без даты в тексте — следующий урок), у материалов и инфо — дата сообщения. У домашки ещё assigned,
а у долгих заданий («на месяц», «к концу четверти») long_term.
    python3 chat_agent.py chats/result.json --limit 50 # только первые 50 подходящих сообщений, для пробы
    python3 chat_agent.py live                         # наш бот в чатах (privacy mode выключен в BotFather),
                                                       # токен — строка BOT_TOKEN=... в .env (или BOT_TOKEN=... перед командой)
    (live печатает каждое сообщение, решение фильтра и ответ модели; --quiet — только находки;
     для экспорта то же включает --verbose)
    Модерация (chat_moderation.py): в live каждая находка приходит карточкой в бота-модератора — того же, что у
    каналов (MODERATOR_BOT_TOKEN в .env, должен отличаться от BOT_TOKEN). На сервер уходит только подтверждённое.
    Ручную модерацию чатов одной командой включает и выключает /manual_chats в боте или python3 chat_agent.py manual
    (каналы отдельно: /manual_channels; /manual показывает, что как настроено).
    live --channel — заодно проверять каналы (тогда watch_channel.py loop отдельно не запускать),
    live --no-moderator — без бота.

Экспорт: Telegram Desktop → чат → ⋮ → «Экспорт истории чата» → JSON → папка chats/.
Галочки «Фото» и «Файлы» оставить (ДЗ бывает файлом), видео, голосовые и стикеры снять.
Результат: chats/<имя чата>_items.json со статусом new у каждой находки (в live подтверждает модератор).
Ответы модели кешируются в chats/<имя чата>_llm.json, повторный запуск продолжает с места остановки.
Переписка одноклассников остаётся на этом компьютере: файлы из chats/ никуда не копируются.
"""
import difflib
import hashlib
import html
import json
import os
import re
import sys
import time
import urllib.request
from datetime import date, timedelta
from pathlib import Path

import board
import chat_moderation
import files as attachments
import timetable
from baseline_rules import WEEKDAYS, extract as rules_extract

MODEL = os.environ.get("MODEL", "qwen2.5:7b")  # другую модель: MODEL=qwen2.5:14b python3 ...
URL = os.environ.get("OLLAMA_URL", "http://localhost:11434") + "/api/chat"
CONTEXT = 4  # сколько предыдущих сообщений показывать модели
# Фото: "stub" (по умолчанию, для беты) — фото без подписи сразу считается ДЗ и привязывается к уроку по расписанию,
# без распознавания; "board" — распознавание текста на фото (Tesseract) и проверка «доска или нет» (board.py),
# как было 16.10. Включить: PHOTO_MODE=board python3 chat_agent.py ... Разбор этого варианта: фото-разбор.md
PHOTO_MODE = os.environ.get("PHOTO_MODE", "stub")

# Что-то из этого почти всегда есть в полезном сообщении; остальное (мемы, «ок», «спасибо») модель не видит.
USEFUL = re.compile(
    r"\bдз\b|домашк|домашн|задан|задали|сделать|решить|выучить|прочитать|прочесть|написать|сдать|сдаём|сдаем|"
    r"\bстр\b|страниц|№|\bномер|упр|параграф|§|конспект|презентац|листок|листочк|"
    r"контрольн|\bкр\b|самостоят|\bср\b|проверочн|тест|зач[её]т|диктант|сочинени|экзамен|"
    r"перенос|отмен|не будет|заболел|замен|кабинет|\bкаб\b|\bв \d{3}\b|урок|пар[аыу]\b|"
    r"завтра|послезавтра|понедельник|вторник|сред[уыа]|четверг|пятниц|суббот|"
    r"дедлайн|до \d|к \d|https?://|файл|ссылк|материал|учебник|classroom|диск", re.I)

SCHEMA = {
    "type": "object",
    "properties": {
        "items": {"type": "array", "items": {
            "type": "object",
            "properties": {
                "quote": {"type": "string"},
                "type": {"type": "string", "enum": ["homework", "test", "lesson_change", "material", "info"]},
                "summary": {"type": "string"},
                "date": {"type": ["string", "null"]},
                "time": {"type": ["string", "null"]},
                "room": {"type": ["string", "null"]},
                "lesson": {"type": ["string", "null"]},
            },
            "required": ["quote", "type", "summary", "date"],
        }},
    },
    "required": ["items"],
}

SYSTEM = """Ты разбираешь сообщения школьного Telegram-чата «ЧАТ» и выписываешь то, что ученику важно не пропустить.
Имён авторов нет: «учитель» — сообщение учителя, «участник 1», «участник 2»... — другие участники чата
(одинаковый номер — один и тот же человек). Задание может написать и участник, например переслать ДЗ от учителя.
Разбирай ТОЛЬКО сообщение после строки «СООБЩЕНИЕ:». Предыдущие сообщения даны лишь для понимания (например, на какой вопрос это ответ).
Следующие сообщения и ответы на него нужны для уточнений: если там исправляют дату или детали («а не послезавтра», «перенесли
на пятницу», «ой, не 245, а 254»), выпиши находку уже с исправлением, а quote возьми дословно из исправляющего сообщения.
Новые задания из следующих сообщений не выписывай — их разберут отдельно.

Типы находок:
- homework: домашнее задание (страницы, номера, параграфы, «выучить», «прислать до»). Голый список номеров задач
  без слова «ДЗ» («Кирик 102Д2 103Д6», «№ 245, 247», «упр. 176») — тоже homework.
- test: контрольная, самостоятельная, проверочная, тест, зачёт, диктант, сочинение, защита проекта или реферата,
  опрос («спрошу у доски», «будет опрос по теории») — когда будет.
- lesson_change: обычный урок этого класса перенесён, отменён, заменён, идёт в другом кабинете или в другое время.
- material: ссылка или файл с материалами к уроку (презентация, листок, учебник, конспект).
- info: другое важное для всех (что принести, сдать работы, консультация). Олимпиады, регистрации, экскурсии, кружки,
  пересдачи и дополнительные занятия — всегда info, даже если там есть срок или номер урока.

Не выписывай: вопросы учеников («а что задали?», «кто скинет дз?» — это не задание, а просьба), шутки, благодарности, обсуждение уже прошедшего, догадки («кажется, будет кр»), если никто их не подтвердил.
Файл или фото без текста (или с подписью вроде «вот») — это homework, если по имени файла или по предыдущим сообщениям видно, что это задание
(«ДЗ_12.pdf», «листок 5», ответ на «что задали?»), иначе material. В summary напиши «файл: <имя>».
Если в сообщении ничего такого нет, верни пустой список. Если несколько находок (ДЗ и дата контрольной), выпиши каждую.

Поля:
- quote: фраза из сообщения дословно.
- summary: коротко, своими словами («стр. 45, № 3–7», «контрольная по теме „Логарифмы“»).
- date: YYYY-MM-DD, к какому дню ДЗ или когда событие. «Завтра», «к четвергу» считай от даты сообщения. Не знаешь — null. «На следующий урок» — null, напиши это в summary.
- time: ЧЧ:ММ, только если написано. room: кабинет, только если написан. lesson: номер урока («3 урок»), если написан.

Пример. Дата сообщения: 2026-10-13 (вторник). СООБЩЕНИЕ: «Дз к четвергу: §12, № 245, 247. В пятницу самостоятельная по параграфам 10–12»
Ответ: {"items": [{"quote": "Дз к четвергу: §12, № 245, 247", "type": "homework", "summary": "§12, № 245, 247", "date": "2026-10-15", "time": null, "room": null, "lesson": null}, {"quote": "В пятницу самостоятельная по параграфам 10–12", "type": "test", "summary": "самостоятельная по §10–12", "date": "2026-10-16", "time": null, "room": null, "lesson": null}]}

Ещё примеры, только типы (сообщение → находки):
«Подготовьте сообщение о Чехове к пятнице» → homework
«Повторить формулы приведения к понедельнику» → homework
«Принесите на урок циркуль и калькулятор» → info
«Учебник Мякишева лежит в закрепе» → material
«Презентация с урока: https://…» → material
«Я думаю, кр будет сложная» → ничего (мнение ученика)
«Кажется, завтра тест?» → ничего (догадка)
«Работы проверю к среде» → ничего (учитель о своих делах, ученику делать нечего)
«Оценки за контрольную уже в журнале» → info
«Физики завтра не будет» → lesson_change
«Зачёт по стихам переносится на 30-е» → test
«Кто сделал пятый номер?» → ничего (вопрос)
«Олимпиада по физике, регистрация до 20.10» → info
«Сочинение пишем во вторник, а к четвергу прочитать „Грозу“» → test и homework
«Эссе по рассказу сдать к понедельнику» → homework (пишут дома)
«Онлайн-тест на сайте, пройти до субботы» → homework (тест дома — это задание)
«На следующем уроке спрошу стихотворение, выучите» → homework и test

Отвечай только JSON."""
# ответы модели в кеше годятся, только пока не менялись промпт и модель: метка в имени файла кеша
PROMPT_VERSION = "3"  # поднять, если меняется запрос к модели вне SYSTEM (ask, chat_header, roles): иначе подтянется старый кеш
CACHE_TAG = "_" + hashlib.md5((MODEL + SYSTEM + json.dumps(SCHEMA) + PROMPT_VERSION).encode()).hexdigest()[:6]

ROOM = re.compile(r"\b(\d{3})\s*(?:-?м\s*)?каб|каб\w*\.?\s*(?:№\s*)?(\d{3})\b", re.I)
WD_NAMES = ["понедельник", "вторник", "среда", "четверг", "пятница", "суббота", "воскресенье"]


# ---------- сообщения: из экспорта и из Bot API в один формат ----------

def export_text(m):
    """В экспорте text — строка или список кусков; ссылки лежат в text_entities."""
    parts = m.get("text_entities") or ([{"type": "plain", "text": m["text"]}] if isinstance(m.get("text"), str)
                                       else [p if isinstance(p, dict) else {"type": "plain", "text": p} for p in m.get("text", [])])
    text = m["text"] if isinstance(m.get("text"), str) else "".join(p.get("text", "") for p in parts)
    links = [p.get("href") or p.get("text") for p in parts if p.get("type") in ("link", "text_link")]
    return text, links


def from_export(m, chat):
    text, links = export_text(m)
    # Экспорт с медиа: в file/photo лежит путь внутри папки экспорта; без медиа — «(File not included...)»
    paths = [m[k] for k in ("file", "photo") if m.get(k) and not m[k].startswith("(")]
    files = []
    if m.get("file") or m.get("file_name"):
        files.append(m.get("file_name") or (Path(m["file"]).name if not m["file"].startswith("(") else "файл"))
    if m.get("photo"):
        files.append("фото")
    return {"chat": chat, "id": m["id"], "date": m["date"], "author": m.get("from") or "?", "author_id": m.get("from_id"),
            "text": text, "links": links, "files": files, "file_paths": paths, "reply_to": m.get("reply_to_message_id"),
            "forwarded": bool(m.get("forwarded_from"))}


def from_bot(msg):
    text = msg.get("text") or msg.get("caption") or ""
    ents = msg.get("entities") or msg.get("caption_entities") or []
    raw = text.encode("utf-16-le")  # смещения в Bot API считаются в UTF-16
    links = [e.get("url") or raw[e["offset"] * 2:(e["offset"] + e["length"]) * 2].decode("utf-16-le")
             for e in ents if e["type"] in ("url", "text_link")]
    files = [msg["document"].get("file_name", "файл")] if msg.get("document") else []
    file_ids = [msg["document"]["file_id"]] if msg.get("document") else []
    if msg.get("photo"):
        files.append("фото")
        file_ids.append(msg["photo"][-1]["file_id"])  # самый крупный размер; бот потом перешлёт файл по file_id
    who = msg.get("from", {})
    return {"chat": msg["chat"].get("title") or str(msg["chat"]["id"]), "id": msg["message_id"],
            "date": time.strftime("%Y-%m-%dT%H:%M:%S", time.localtime(msg["date"])),
            "author": " ".join(filter(None, [who.get("first_name"), who.get("last_name")])) or "?",
            "author_id": f"user{who['id']}" if who.get("id") else None,  # как from_id в экспорте
            "text": text, "links": links, "files": files, "file_ids": file_ids,
            "reply_to": (msg.get("reply_to_message") or {}).get("message_id"),
            "forwarded": "forward_origin" in msg}


def merge_bursts(msgs, gap=120):
    """Подряд идущие сообщения одного автора с разницей до 2 минут — одно высказывание:
    альбом из 9 фото, «фото» + «Дз на чт» + «Переписать в тетрадь...». Модель видит их вместе."""
    out = []
    for m in msgs:
        prev = out[-1] if out else None
        if (prev and prev["author"] == m["author"] and not m["reply_to"]
                and 0 <= (_ts(m) - _ts(prev["last"])) <= gap):
            prev["text"] = "\n".join(filter(None, [prev["text"], m["text"]]))
            for k in ("links", "files", "file_paths"):
                prev[k] = prev.get(k, []) + m.get(k, [])
            prev["message_ids"].append(m["id"])
            prev["last"] = m
        else:
            out.append({**m, "message_ids": [m["id"]], "last": m})
    for m in out:
        del m["last"]
    return out


def _ts(m):
    return time.mktime(time.strptime(m["date"][:19], "%Y-%m-%dT%H:%M:%S"))


_filter = None


def load_filter():
    """Классификатор из train_classifier.py (chats/filter_model.pkl); если его нет, остаются ключевые слова."""
    global _filter
    if _filter is None:
        path = Path(__file__).parent / "chats" / "filter_model.pkl"
        try:
            import pickle
            with open(path, "rb") as f:
                _filter = pickle.load(f)
            print(f"Фильтр болтовни: классификатор {path.name} (порог {_filter['threshold']:.2f})")
        except (OSError, ImportError) as e:
            _filter = False
            print(f"Фильтр болтовни: ключевые слова ({path.name} не загружен: {e.__class__.__name__})")
    return _filter


# страховка классификатора: такие слова почти всегда по делу (на разметке +0,2% нагрузки)
STRONG = re.compile(r"\b(?:дз|д/з|домашк|выучи|конспект|закреп|контрольн|самостоятельн|проверочн|зач[её]т|диктант|"
                    r"test|homework|exercise|ex\.|кр\b|ср\b|присла\w*|прислать)|\bа не\b|\bне на\b|перен[оеё]с|ошиб|исправ|точнее|поправк", re.I)
ROOM_BARE = re.compile(r"\bв(?:о)?\s+(\d{3})\b(?!\s*(?:раз|%|г\b|км|кг|м\b|с\b|руб|год))", re.I)
VERBOSE = False  # печатать ход мысли: решение фильтра, ответ модели, что отброшено (live и --verbose)


def say(*a):
    if VERBOSE:
        print(*a, flush=True)


# «меня/нас не будет» — ученик предупреждает, что не придёт; «урока не будет» ловится и так
NET_EXTRA = re.compile(r"\bп\.\s*\d|вопрос\w*\s+\d|вместо|заболе|отмен|(?<!меня )(?<!нас )не будет|реферат|проект|доклад|эссе|фото доски|"
                       r"на месяц|на (?:две|три|\d) недел", re.I)


def safety_net(text):
    """Короткие ДЗ и замены фильтр иногда режет («№ 318, 320», «П. 23, вопросы 1–4», «вместо физики алгебра»):
    такие слова пропускаем в модель всегда. Вопросы — нет, это болтовня учеников."""
    if "?" in text:
        return False
    return bool(STRONG.search(text) or TASK_LIST.search(text) or DO_TASK.search(text.strip()) or NET_EXTRA.search(text))


def worth_asking(m):
    if m["files"]:  # файлы и фото всегда смотрит модель
        say("   фильтр: есть файл → в модель")
        return True
    teacher = timetable.is_teacher(m.get("author"), m.get("author_id"))
    if teacher and m["text"].strip():  # сообщения учителя всегда смотрит модель
        say(f"   фильтр: пишет учитель, {teacher} → в модель")
        return True
    f = load_filter()
    if f:
        text = m["text"].lower() + (" ⟨ссылка⟩" if m["links"] else "")  # как text() в train_classifier.py
        p = f["model"].predict_proba([text])[0, 1]
        if p < f["threshold"] and safety_net(m["text"]):
            say(f"   фильтр: вероятность {p:.2f} ниже порога, но есть слово вроде «дз»/«конспект» → в модель")
            return True
        say(f"   фильтр: вероятность полезного {p:.2f} (порог {f['threshold']:.2f}) → {'в модель' if p >= f['threshold'] else 'болтовня, пропускаю'}")
        return p >= f["threshold"]
    ok = bool(m["links"]) or (len(m["text"]) >= 6 and USEFUL.search(m["text"]))
    say(f"   фильтр (ключевые слова): {'в модель' if ok else 'болтовня, пропускаю'}")
    return bool(ok)


# ---------- модель + правила ----------

AFTER = 3  # сколько следующих сообщений и ответов на сообщение показывать модели


def roles():
    """Вместо имён модель видит «учитель» или «участник N» (номер свой в каждом запросе, одинаковый у одного человека):
    имена одноклассников и учителей в модель не уходят. С пометкой «ученик» у всех модель 16.10 стала пропускать задания."""
    seen = {}

    def role(x):
        if timetable.is_teacher(x.get("author"), x.get("author_id")):
            return "учитель"
        return seen.setdefault(x.get("author") or "?", f"участник {len(seen) + 1}")
    return role


def chat_header(chat, posted):
    """Чат для первой строки SYSTEM: название, предмет и класс (из timetable.json или из названия: «11Б», «2027Б», «В27»).
    16.10 пробовали отдельные строки «Чат: / Предмет: / Класс:» перед сообщением: модель стала пропускать находки
    (132/150 против 145 без них), поэтому чат упоминается одной фразой, как было раньше с названием чата."""
    meta = timetable.chat_meta(chat, posted)
    about = [f"предмет: {', '.join(meta['subjects'])}"] if meta["subjects"] else []
    if meta["class"]:
        about.append(f"класс {meta['class']}" + (", вся параллель" if meta["class"].isdigit() else ""))
    return f"{chat}» ({'; '.join(about)})" if about else chat + "»"


def ask(m, history, chat, after=()):
    posted = date.fromisoformat(m["date"][:10])
    by_id = {i: h for h in history for i in h.get("message_ids", [h["id"]])}
    role = roles()
    ctx = [f"[{h['date'][11:16]}] {role(h)}: {h['text'][:300]}" for h in history[-CONTEXT:] if h["text"]]
    reply = by_id.get(m["reply_to"])
    if reply and reply["text"]:
        ctx.append(f"(сообщение — ответ на: {role(reply)}: «{reply['text'][:300]}»)")
    extra = (f"\nВложения: {', '.join(m['files'])}" if m["files"] else "") + \
            (f"\nСсылки: {' '.join(m['links'])}" if m["links"] else "") + \
            "".join(f"\nТекст вложения «{name}» (начало):\n{text}" for name, text in m.get("file_texts", []))
    ids = set(m.get("message_ids", [m["id"]]))
    me = role(m)  # номер автора сообщения — после предыдущих, как они идут в запросе
    nxt = [f"[{a['date'][11:16]}] {role(a)}" + (" (ответ на это сообщение)" if a.get("reply_to") in ids else "")
           + f": {a['text'][:300]}" for a in after if a["text"]]
    user = (f"Дата сообщения: {posted} ({WD_NAMES[posted.weekday()]})\n\n"
            + ("Предыдущие сообщения:\n" + "\n".join(ctx) + "\n\n" if ctx else "")
            + f"СООБЩЕНИЕ: {me}: «{m['text']}»{extra}"
            + ("\n\nСледующие сообщения (только для уточнений):\n" + "\n".join(nxt) if nxt else ""))
    body = {"model": MODEL, "stream": False, "format": SCHEMA,
            "options": {"temperature": 0, "num_ctx": 8192},
            "messages": [{"role": "system", "content": SYSTEM.replace("ЧАТ»", chat_header(chat, posted), 1)},
                         {"role": "user", "content": user}]}
    req = urllib.request.Request(URL, data=json.dumps(body).encode(), headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=300) as r:
        return json.loads(json.loads(r.read())["message"]["content"]).get("items", [])


def weekday_date(text, posted):
    """«к четвергу», «на пятницу», «в среду», «до понедельника» -> ближайший такой день после даты сообщения."""
    en = re.search(r"\b(?:next|on|by)\s+(monday|tuesday|wednesday|thursday|friday|saturday)", text.lower())
    if en:
        wd = ["monday", "tuesday", "wednesday", "thursday", "friday", "saturday"].index(en.group(1))
        return posted + timedelta(days=(wd - posted.weekday()) % 7 or 7)
    m = re.search(r"\b(?:к|на|в|во|до)\s+(?:следующ\w+\s+)?(понедельник|вторник|сред|четверг|пятниц|суббот|воскресень)\w*", text.lower())
    if not m:
        return None
    wd = next(v for k, v in WEEKDAYS.items() if m.group(1).startswith(k))
    return posted + timedelta(days=(wd - posted.weekday()) % 7 or 7)


DO_TASK = re.compile(r"^(?:[А-ЯЁA-Z][\w ]{2,20}:\s*)?(?:(?:прочита|сдела|досда|сда)(?:ть|йте)|выучи(?:ть|те)?\b|реши(?:ть|те)?\b|"
                     r"напиш(?:и|ите)\b|написать|повтори(?:ть|те)?\b|подготов(?:ить|ьте|иться|ьтесь)|законспектир\w*|"
                     r"learn|read|do\b|write|p\.\s*\d)", re.I)  # повелительное/инфинитив: «сделала таблицу» — не задание
GUESS = re.compile(r"\b(?:кажется|говорят|вроде|наверное|не точно|может быть)\b", re.I)
WISH = re.compile(r"\b(?:удачи|поздравля|молодцы|спасибо)\b", re.I)
TEST_WORDS = re.compile(r"контрольн|самостоятельн|проверочн|зач[её]т|диктант|защит\w* (?:проект|реферат)|опрос (?:по|у доски)|\bтест\b|спрошу|"
                        r"лабораторн\w* работ|лабораторк|\bлаб[аеуы]\b|практикум", re.I)
LAB = re.compile(r"лабораторн\w* работ|лабораторк|\bлаб[аеуы]\b|практикум", re.I)
CHANGE_WORDS = re.compile(r"перен[оеё]с|отмен|замен|не будет|нет \w+ урок|без урок|вместо|кабинет|каб\.|\bв(?:о)? \d{3}\b|сокращ|урок\w* в|зал|zoom|зум|telemost", re.I)
ONLINE = re.compile(r"zoom\.us|telemost|meet\.google|зум", re.I)


_DAY = r"(?:послезавтра|завтра|сегодня|понедельник\w*|вторник\w*|сред\w*|четверг\w*|пятниц\w*|суббот\w*|воскресень\w*|\d{1,2}(?:\.\d{1,2}|\s+[а-я]{3,}))"
NEG_DATE = re.compile(r"(?:\bа\s+)?\b(?:не|вместо)\s+(?:на\s+|в\s+|во\s+|к\s+|до\s+|с\s+)?" + _DAY, re.I)
CORRECTION = re.compile(r"\bа нет\b|^\s*нет[,!.\s]|\bой\b|\bа не\b|\bне на\b|\bне в\b|перен[оеё]с|ошиб|исправ|точнее|поправк|оказывается|на самом деле|сдвин", re.I)


REFERS = re.compile(r"^\s*(?:она|он|оно|они|её|ее|его|их|перенес\w*|будет|будут|сдвинул\w*)\b", re.I)
DATED = ("homework", "test", "lesson_change")


def day_words(text):
    """«к завтрашнему уроку» → «к завтра уроку»: правила дат знают только «завтра/послезавтра/сегодня»."""
    return re.sub(r"\b(послезавтра|завтра|сегодня)шн\w*", r"\1", text, flags=re.I)


def correction_date(text, when):
    """Дата из поправки. «не в среду, а в четверг» — дату после «не» выкидываем;
    но «а не(т), послезавтра» значит «ой, послезавтра»."""
    text = day_words(text)
    tries = [NEG_DATE.sub(" ", text)]
    oops = re.match(r"\s*(?:а\s+нет?|ой|нет)\b[\s,!.:—-]*(.*)", text, re.I | re.S)
    if oops:
        tries.append(oops.group(1))
    day = date.fromisoformat(when[:10])
    for qd in tries:
        got = rules_extract({"text": qd, "date": when})
        if re.search(r"\bпослезавтра\b", qd.lower()):
            got["dates"] = [day + timedelta(days=2)]
        d = weekday_date(qd, day) or (got["dates"][0] if got["dates"] else None)
        if d:
            return d
    return None


def is_reply_correction(a):
    """Ответ на сообщение, похожий на поправку: «а нет, в четверг», «она будет в четверг», короткое «в четверг»."""
    return "?" not in a["text"] and bool(CORRECTION.search(a["text"]) or REFERS.search(a["text"])
                                         or len(re.findall(r"\w+", a["text"])) <= 8)


def corrected_date(it, m, after, posted):
    """Поправка в следующих сообщениях («а нет, во вторник», ответ «не завтра, а в понедельник») от того же автора
    или ответом на сообщение: берём дату из последней такой поправки. Модель это часто пропускает."""
    if it["type"] not in DATED:
        return None
    ids = set(m.get("message_ids", [m["id"]]))
    for a in reversed(list(after)):
        same = a["author"] == m["author"] and CORRECTION.search(a["text"])
        if not (same or (a.get("reply_to") in ids and is_reply_correction(a))):
            continue
        d = correction_date(a["text"], a["date"])
        if d:
            return d.isoformat(), a
    return None


def item_ids(x):
    return x.get("message_ids") or [x["message_id"]]


def reply_update(m, found):
    """Ответ на старое сообщение с находкой («она будет в четверг» через день после «кр во вторник»)
    меняет дату той находки. Работает без модели и фильтра: такие ответы часто короткие."""
    if not m.get("reply_to") or not is_reply_correction(m):
        return []
    targets = [x for x in found if x["chat"] == m["chat"] and x["type"] in DATED and m["reply_to"] in item_ids(x)]
    if len(targets) > 1:  # на сообщение с ДЗ и контрольной: по словам ответа
        kind = "test" if TEST_WORDS.search(m["text"]) or re.search(r"\bк/?р\b|\bср\b", m["text"], re.I) else \
               "homework" if re.search(r"\bд/?з\b|домашк|задани", m["text"], re.I) else \
               "lesson_change" if re.search(r"урок|пар[аыу]\b", m["text"], re.I) else None
        targets = [x for x in targets if x["type"] == kind]
    d = correction_date(m["text"], m["date"]) if len(targets) == 1 else None
    if not d or targets[0].get("date") == d.isoformat():
        return []
    x = targets[0]
    say(f"   ответ на #{m['reply_to']} меняет дату: {x.get('date')} → {d}")
    x.setdefault("changed_from", []).append(x.get("date"))
    x["date"] = d.isoformat()
    x.pop("date_source", None)
    relink(x)
    x.setdefault("updated_by", []).append(m["id"])
    x["status"] = "updated"
    print("  ОБНОВЛЕНО" + show(x)[1:])
    send_to_server(x)
    return [x]
    ids = set(m.get("message_ids", [m["id"]]))
    for a in reversed(list(after)):
        if not (a["author"] == m["author"] or a.get("reply_to") in ids) or not CORRECTION.search(a["text"]):
            continue
        # «не в среду, а в четверг» — дату после «не» выкидываем; но «а не(т), послезавтра» значит «ой, послезавтра»
        tries = [NEG_DATE.sub(" ", a["text"])]
        oops = re.match(r"\s*(?:а\s+нет?|ой|нет)\b[\s,!.:—-]*(.*)", a["text"], re.I | re.S)
        if oops:
            tries.append(oops.group(1))
        for qd in tries:
            got = rules_extract({"text": qd, "date": a["date"]})
            if re.search(r"\bпослезавтра\b", qd.lower()):
                got["dates"] = [date.fromisoformat(a["date"][:10]) + timedelta(days=2)]
            d = weekday_date(qd, date.fromisoformat(a["date"][:10])) or (got["dates"][0] if got["dates"] else None)
            if d:
                return d.isoformat(), a
    return None


def clean_quote(quote, text):
    """Модель иногда цитирует с обёрткой из промпта: «учитель: «...»». Снимаем её и проверяем, что фраза есть в тексте."""
    q = (quote or "").strip()
    for _ in range(2):
        if q in text:
            return q
        q = re.sub(r"^[^:«]{1,30}:\s*", "", q).strip().strip("«»\"'").strip()
    return q if q and q in text else ""


def fix_type(it, quote, m):
    """Поправки типа поверх модели: то, что модель путает стабильно, и что легко проверить по словам."""
    t, q = it["type"], quote or m["text"]
    if t in ("test", "lesson_change") and GUESS.search(q):
        return None, "догадка, а не объявление"
    if t == "test" and WISH.search(q) and not TEST_WORDS.search(WISH.sub("", q).replace("контрольной", "")):
        return None, "пожелание, а не объявление"
    if t == "test" and re.search(r"результат|оценк|провер(?:ен|ил)", q, re.I):
        return "info", "результаты, а не сама контрольная"
    if t == "test" and re.search(r"пересда", q, re.I):
        return "info", "пересдача — это инфо"
    if t == "homework" and LAB.search(q) and not (DO_TASK.search(q) or TASK_LIST.search(re.sub(r"(?:работ\w*|практикум\w*)\s*№\s*\d+", "", q))):
        return "test", "лабораторная или практикум — это проверочная работа"
    if t in ("info", "material") and it.get("long_term_hint") and re.search(r"проект|реферат|доклад|эссе|сочинени|презентаци", q, re.I):
        return "homework", "долгое задание"
    if t == "info" and TEST_WORDS.search(q) and not m["links"] and \
            not re.search(r"олимпиад|регистрац|пересда|результат|консультац|перед контрольн|запис", q, re.I):
        return "test", "слово про проверочную работу"
    if t == "lesson_change" and not CHANGE_WORDS.search(q):
        if TEST_WORDS.search(q):
            return "test", "нет слов про перенос, зато есть проверочная работа"
        return "info", "нет слов про перенос, отмену или кабинет"
    if t == "material" and not (m["links"] or m["files"] or re.search(r"закреп|выложил|прикрепил|фото|скрин|в группе", q, re.I)):
        if ONLINE.search(q):
            return "lesson_change", "ссылка на онлайн-урок"
        if DO_TASK.search(q) or TASK_LIST.search(q):
            return "homework", "без ссылки и файла, но с заданием"
        return "info", "без ссылки и файла"
    if t == "material" and ONLINE.search(q):
        return "lesson_change", "ссылка на онлайн-урок"
    return t, None


TASK_LIST = re.compile(r"(?:кирик|№|упр\w*\.?|задач\w*|параграф\w*|§|стр\.?)\s*\d|\b\d{1,3}\s?[ДдВвСсНн]\s?\d", re.I)


def refine(m, items, after=()):
    """Как в агенте канала: модель находит и цитирует, а дату, время и кабинет перепроверяем регулярками по цитате."""
    posted = date.fromisoformat(m["date"][:10])
    out = []
    text = m["text"].strip()
    head = re.sub(r"^(?:дз|д/з|домашк\w*|задание)\s*[:\-—]?\s*", "", text, flags=re.I)
    if not items and "?" not in text and len(text) < 200:
        if TASK_LIST.match(head) or DO_TASK.search(head):
            say("   правило: модель промолчала, но это задание (номера задач или «прочитать/выучить…») → домашка")
            items = [{"quote": text, "type": "homework", "summary": text[:80]}]
        elif re.search(r"плюсник|таблиц\w* с плюс", text, re.I):
            say("   правило: модель промолчала, но это про плюсник → инфо")
            items = [{"quote": text, "type": "info", "summary": text[:80]}]
    for it in items:
        quote = clean_quote(it.get("quote"), m["text"]) or clean_quote(it.get("quote"), "\n".join(a["text"] for a in after))
        if not quote and len(items) == 1 and len(text) < 300:
            quote = text  # одна находка, цитата неточная: проверяем по всему сообщению
        if quote:
            it["quote"] = quote
        if long_term(quote, posted):
            it["long_term_hint"] = True
        new_type, why = fix_type(it, quote, m)
        it.pop("long_term_hint", None)
        if new_type is None:
            say(f"   отброшено ({why}): {quote}")
            continue
        if new_type != it["type"]:
            say(f"   правило: {it['type']} → {new_type} ({why})")
            it["type"] = new_type
        if quote.rstrip().endswith("?") and it.get("type") in ("homework", "test", "lesson_change"):
            say(f"   отброшено (это вопрос, а не объявление): {quote}")
            continue
        qd = NEG_DATE.sub(" ", day_words(quote))  # «завтра, а не послезавтра»: даты после «не»/«вместо» — отменённые
        got = rules_extract({"text": qd, "date": m["date"]})
        if re.search(r"\bпослезавтра\b", qd.lower()):
            got["dates"] = [posted + timedelta(days=2)]
        wd = weekday_date(qd, posted)
        rule_dates = [wd] if wd else got["dates"]
        lt = long_term(quote, posted)
        if lt is None and it["type"] == "homework" and sum(x.get("type") == "homework" for x in items) == 1:
            lt = long_term(m["text"], posted)  # условие могло остаться вне цитаты
        if lt:
            if it["type"] in ("homework", "info"):
                it["long_term"] = lt[1]  # отдельная пометка: задание или срок на долгое время («сдать долги до конца четверти»)
            if lt[0] and not rule_dates:  # «через неделю контрольная» — тоже дата
                it["date"], it["date_source"] = lt[0], "из текста: " + lt[1]
        if rule_dates:
            it["date"] = rule_dates[0].isoformat()
        cd = corrected_date(it, m, after, posted)
        if cd and cd[0] != it.get("date"):
            say(f"   поправка в следующем сообщении ({cd[1]['text'][:50]!r}): дата {it.get('date')} → {cd[0]}")
            if it.get("date"):
                it["changed_from"] = [it["date"]]
            it["date"] = cd[0]
            it.pop("date_source", None)
        elif NEXT_LESSON.search(quote):
            it["date"] = None  # «к следующему уроку»: срок поставит fill_due по дням предмета
        elif it.get("date") and not quote:
            it["date"] = None  # дату без опоры в тексте модель могла придумать
        if "вместо" in quote.lower():  # «в 14:10 вместо 13:20»: время после «вместо» — старое
            old = quote.lower().split("вместо", 1)[1]
            got["times"] = {x for x in got["times"] if x not in old and x.zfill(5) not in old} or got["times"]
        t = it.get("time")
        it["time"] = t if t and (t in got["times"] or t.lstrip("0") in got["times"]) else (sorted(got["times"])[0] if got["times"] else None)
        room = ROOM.search(quote)
        bare = ROOM_BARE.search(quote.lower().split("вместо")[0].split(", а не")[0].split(" а не ")[0])
        if room:
            it["room"] = "каб. " + (room.group(1) or room.group(2))
        elif bare and it["type"] in ("lesson_change", "info", "test"):
            it["room"] = "каб. " + bare.group(1)  # «завтра в 406» — тоже кабинет
        elif not it.get("room") and got["places"]:
            it["room"] = sorted(got["places"])[0]
        if it.get("date") and it["date"] < posted.isoformat() and it["type"] in ("test", "lesson_change", "homework"):
            say(f"   отброшено (дата {it['date']} уже прошла): {it.get('summary')}")
            continue  # про прошлое
        out.append({k: v for k, v in it.items() if v is not None})
    return out


NUM = {"одн": 1, "две": 2, "два": 2, "дв": 2, "тр": 3, "четыр": 4, "пят": 5, "шест": 6}
LONG = re.compile(r"\b(?:(через|на|в течение|за|срок\s*[:—–-]?)\s+(\d+|одн\w*|дв\w*|тр\w*|четыр\w*|пят\w*|шест\w*)?\s*"
                  r"(недел\w*|месяц\w*|дн(?:я|ей)|день)"
                  r"|(?:к|до)\s+(?:конц\w*|окончани\w*)\s+(четверт\w*|полугоди\w*|триместр\w*|месяц\w*|года)"
                  r"|(?:до|к)\s+каникул)", re.I)


def long_term(text, posted):
    """Долгие задания: «на месяц», «через две недели», «к концу четверти». -> (срок или None, фраза)."""
    m = LONG.search(text)
    if not m:
        return None
    phrase = m.group(0)
    if m.group(3):
        n = m.group(2) or "1"
        n = int(n) if n.isdigit() else next((v for k, v in NUM.items() if n.lower().startswith(k)), 1)
        unit = m.group(3).lower()
        days = n * (30 if unit.startswith("месяц") else 7 if unit.startswith("недел") else 1)
        if days < 4:
            return None  # «на два дня» и т.п. — обычная домашка
        return (posted + timedelta(days=days)).isoformat(), phrase
    return None, phrase  # конец четверти/каникулы: даты четвертей агент не знает


LESSONS_FILE = Path(__file__).parent / "lessons.json"
NEXT_LESSON = re.compile(r"\b(?:к|на|до)\s+следующ\w+\s+(?:урок|занят|пар)", re.I)


def lesson_days(chat, found):
    """Дни недели (0 = пн), когда идёт предмет. Сначала lessons.json, иначе угадываем по срокам прошлых находок."""
    try:
        table = json.load(open(LESSONS_FILE, encoding="utf-8"))
    except (OSError, ValueError):
        table = {}
    for name, days in table.items():
        if name.lower() in chat.lower():
            return sorted(next(i for i, w in enumerate(WD_NAMES) if d.lower().startswith(w[:3])) for d in days), "расписание"
    dated = [date.fromisoformat(x["date"]).weekday() for x in found
             if x["chat"] == chat and x.get("date_source", "текст") == "текст" and x.get("date")
             and x["type"] in ("homework", "test")]
    counts = {d: dated.count(d) for d in set(dated)}
    days = sorted(d for d, c in counts.items() if c >= (2 if len(dated) >= 6 else 1) and c >= 0.15 * len(dated) and d < 6)
    return days, "по прошлым срокам"


def relink(it):
    """После смены даты (поправка, ответ на старое сообщение) заново ищем урок, к которому срок."""
    for k in ("due_lesson", "due_kind"):
        it.pop(k, None)
    timetable.link(it, it.get("quote") or "")


def fill_due(it, found, text=""):
    """Находке без даты ставим дату и пишем в date_source, откуда она взялась.
    Если чат есть в timetable.json, дата и уроки берутся из расписания с заменами (timetable.link):
    на каком уроке задали ДЗ и к какому уроку срок. Иначе домашка, контрольная, изменение урока ->
    следующий день предмета из lessons.json или по прошлым срокам; материал и инфо -> дата сообщения."""
    dated = it.get("date") and it.get("date_source", "текст").startswith(("текст", "из текста"))
    if it["type"] in ("material", "info"):
        if not dated:
            it["date"], it["date_source"] = it["sent_at"][:10], "дата сообщения"
        return
    if timetable.link(it, (it.get("quote") or "") + " " + text):
        return
    if dated:
        return
    if it.get("long_term"):  # у долгого задания свой срок (или «к концу четверти» без даты)
        return
    posted = date.fromisoformat(it["sent_at"][:10])
    days, source = lesson_days(it["chat"], found)
    if not days:  # про предмет ничего не знаем: ближайший учебный день
        days, source = [0, 1, 2, 3, 4], "следующий учебный день"
    # «урок в 207» в день урока — про сегодня; срок домашки и контрольная — не раньше следующего урока
    today_ok = it["type"] == "lesson_change"
    shift = min((d - posted.weekday()) % 7 or (0 if today_ok else 7) for d in days)
    it["date"] = (posted + timedelta(days=shift)).isoformat()
    it["date_source"] = source


def find_corrected(found, it, m):
    """Поправка («кр не завтра, а послезавтра», «перенесли на пятницу»): та же находка того же чата за последние
    3 дня, но с другой датой. Возвращает старую находку, которую надо обновить, или None."""
    if not it.get("date") or not CORRECTION.search(m["text"]):
        return None
    words = lambda x: set(re.findall(r"[а-яёa-z]{4,}", (x.get("summary", "") + " " + x.get("quote", "")).lower()))
    now = _ts(m)
    best, score = None, 0
    for x in reversed(found):
        if x["chat"] != m["chat"] or x["type"] != it["type"] or x.get("date") == it["date"]:
            continue
        if not 0 <= now - _ts({"date": x["sent_at"]}) <= 3 * 86400:
            continue
        a, b = words(x), words(it)
        sim = len(a & b) / max(1, min(len(a), len(b)))
        if sim > score:
            best, score = x, sim
    # одно и то же слово темы («химии», «логарифмам») или только одна такая находка за 3 дня
    same_kind = [x for x in found if x["chat"] == m["chat"] and x["type"] == it["type"]
                 and 0 <= now - _ts({"date": x["sent_at"]}) <= 3 * 86400]
    return best if best and (score >= 0.3 or len(same_kind) == 1) else None


def same_item(a, b):
    if a["type"] != b["type"] or a.get("date") != b.get("date"):
        return False
    return difflib.SequenceMatcher(None, a["summary"].lower(), b["summary"].lower()).ratio() > 0.6


MODERATE = False  # live: находки идут в бота-модератора (chat_moderation.py), на сервер — после подтверждения


def send_to_server(item):
    """Новая или изменённая находка. В live — в очередь модерации; на сервер её отправит moderator.py после кнопки
    (или сразу, если ручная модерация выключена командой /manual)."""
    if MODERATE:
        chat_moderation.push(item)


def after_of(msgs, i):
    """3 следующих сообщения и до 3 ответов на сообщение msgs[i] (ответы могут прийти и позже)."""
    ids = set(msgs[i]["message_ids"])
    nxt = msgs[i + 1:i + 1 + AFTER]
    replies = [x for x in msgs[i + 1:] if x.get("reply_to") in ids and x not in nxt][:AFTER]
    return nxt + replies


def is_photo(name):
    return name == "фото" or Path(name).suffix.lower() in attachments.IMAGES or Path(name).name.startswith("photo_")


def photo_rule(m):
    """Фото без подписи. PHOTO_MODE=stub: всегда ДЗ (срок и урок — по расписанию, как у любой домашки).
    PHOTO_MODE=board: фото без печатного текста — ДЗ, только если это доска с записями, иначе ничего.
    None — правило не подходит (подпись, распознанный текст, не фото), решает модель как обычно."""
    names = m.get("file_paths") or m.get("files") or []
    if not names or not all(is_photo(p) for p in names):
        return None
    if len(re.sub(r"[^а-яёa-z0-9]", "", m["text"].lower())) > 3 or m.get("file_texts"):
        return None  # есть подпись или распознанный текст — пусть разбирается модель
    if PHOTO_MODE != "board":
        return True
    paths = m.get("file_paths") or []
    verdicts = [board.is_board(p) for p in paths if Path(p).exists()]
    if not verdicts or None in verdicts:
        return None  # модели для фото нет — по-старому
    return any(verdicts)


def process_message(m, history, chat, found, cache=None, after=()):
    """Одно сообщение -> находки в found (со склейкой повторов). cache: ответы модели по id сообщения.
    after: следующие сообщения и ответы — по ним модель уточняет дату и детали."""
    key = str(m["id"]) + ("|после:" + ",".join(str(a["id"]) for a in after) if after else "") + \
        ("|текст файла" if m.get("file_texts") else "")  # старые ответы модели видели только имя файла
    verdict = photo_rule(m)
    if verdict is not None:
        if PHOTO_MODE == "board":
            say(f"   фото без подписи: {'похоже на доску → ДЗ' if verdict else 'не доска → пропускаю'}")
            raw = [{"quote": m["text"].strip() or "фото доски", "type": "homework", "summary": "фото доски с записями",
                    "date": None}] if verdict else []
        else:
            say("   фото без подписи → ДЗ (заглушка для беты, PHOTO_MODE=stub)")
            raw = [{"quote": m["text"].strip() or "фото", "type": "homework", "summary": "фото с заданием", "date": None}]
    elif cache is not None and key in cache:
        raw = cache[key]
        say("   модель (из кеша):", json.dumps(raw, ensure_ascii=False))
    else:
        t0 = time.time()
        raw = ask(m, history, chat, after)
        say(f"   модель ({time.time() - t0:.1f} с):", json.dumps(raw, ensure_ascii=False))
        if not raw:
            say("   модель ничего не нашла")
        if cache is not None:
            cache[key] = raw
    new = []
    fields = set(SCHEMA["properties"]["items"]["items"]["properties"])  # старые кеши могли запомнить наши поля
    raw = [{k: v for k, v in x.items() if k in fields} for x in raw]
    for it in refine(m, json.loads(json.dumps(raw)), after):  # копия: refine меняет находки, а в кеше должен остаться ответ модели
        it.update({"chat": m["chat"], "message_id": m["id"], "sent_at": m["date"], "author": m["author"]})
        meta = timetable.chat_meta(m["chat"], date.fromisoformat(m["date"][:10]))
        if meta["class"]:
            it["class"] = meta["class"]  # timetable.link ниже уточнит класс и предмет по расписанию, если сможет
        if len(meta["subjects"]) == 1:
            it["subject"] = meta["subjects"][0]
        it["posted"] = m["date"][:10]  # когда написали в чат; дата события или срок лежит в date
        if it["type"] == "homework":
            it["assigned"] = it["posted"]  # когда задали
        fill_due(it, found, m["text"])
        if m["links"]:
            it["links"] = m["links"]
        if len(m.get("message_ids", [])) > 1:
            it["message_ids"] = m["message_ids"]
        for k in ("files", "file_paths", "file_ids"):
            if m.get(k):
                it[k] = m[k]
        fix = find_corrected(found, it, m)
        if fix:
            say(f"   поправка к #{fix['message_id']}: дата {fix.get('date')} → {it.get('date')}")
            fix.setdefault("changed_from", []).append(fix.get("date"))
            fix["date"] = it.get("date")
            if it.get("date_source"):
                fix["date_source"] = it["date_source"]
            else:
                fix.pop("date_source", None)
            for k in ("due_lesson", "due_kind"):
                if it.get(k):
                    fix[k] = it[k]
                else:
                    fix.pop(k, None)
            fix.setdefault("updated_by", []).append(m["id"])
            fix["status"] = "updated"
            print("  ОБНОВЛЕНО" + show(fix)[1:])
            send_to_server(fix)
            continue
        old = next((x for x in found if same_item(x, it)), None)
        if not old and it.get("date") and (CORRECTION.search(m["text"]) or m.get("reply_to")):
            # поправка, которую мы уже применили к прошлому сообщению по контексту: та же дата — не новая находка
            old = next((x for x in reversed(found) if x["chat"] == m["chat"] and x["type"] == it["type"]
                        and x.get("date") == it["date"] and (m.get("reply_to") in item_ids(x) or CORRECTION.search(m["text"])
                                                             and 0 <= _ts(m) - _ts({"date": x["sent_at"]}) <= 3 * 86400)), None)
        if old:
            say(f"   повтор уже найденного (#{old['message_id']}): {it.get('summary')}")
            old.setdefault("also_in", []).append(m["id"])
            for k, v in it.items():
                old.setdefault(k, v)
            continue
        it["status"] = "new"
        found.append(it)
        new.append(it)
        send_to_server(it)
    return new


SOURCE_WORDS = {"расписание": "по расписанию",
                "по прошлым срокам": "в тексте даты нет, угадано по дням прошлых сроков в этом чате",
                "следующий учебный день": "в тексте даты нет, поставлен следующий учебный день"}


def day(d):
    """«2026-10-22» -> «чт 22.10»."""
    d = date.fromisoformat(d[:10])
    return f"{['пн', 'вт', 'ср', 'чт', 'пт', 'сб', 'вс'][d.weekday()]} {d:%d.%m}"


def show(it):
    """Находка в несколько строк: что это, когда написали/задали и когда срок или событие."""
    labels = {"homework": "ДЗ", "test": "КОНТРОЛЬНАЯ", "lesson_change": "УРОК", "material": "МАТЕРИАЛ", "info": "ИНФО"}
    lines = [f"  {labels.get(it['type'], it['type'])}: {it['summary']} ← #{it['message_id']}"]
    written = "задано" if it["type"] == "homework" else "написано"
    if it.get("posted"):
        lg = it.get("lesson_given")
        lines.append(f"     {written}: {day(it['posted'])}"
                     + (f", на {lg['num']} уроке ({lg['subject']}, {lg['how']})" if lg else ""))
    src = it.get("date_source") or ""
    if it["type"] in ("material", "info") and src == "дата сообщения":
        src = None  # дата события — просто день сообщения, вторую дату не печатаем
    elif it.get("date") or it.get("long_term"):
        word = {"homework": "сдать", "test": "когда", "lesson_change": "когда"}.get(it["type"], "дата")
        parts = [day(it["date"]) if it.get("date") else "без даты"]
        dl = it.get("due_lesson")
        if dl:
            parts.append(f"{dl['num']} урок {dl['subject']} {dl['start']}" + (f" каб. {dl['room']}" if dl.get("room") else "")
                         + (" (замена)" if dl.get("changed") else "") + (" (отменён)" if dl.get("status") == "cancelled" else ""))
        parts += [x for x in (it.get("time") and "в " + it["time"], it.get("lesson"), not dl and it.get("room")) if x]
        how = [x for x in (it.get("due_kind"), it.get("long_term") and f"долгое задание «{it['long_term']}»",
                           SOURCE_WORDS.get(src, src) if src and src != "текст" else None) if x]
        lines.append(f"     {word}: " + ", ".join(parts) + (f" ({'; '.join(how)})" if how else ""))
    elif it.get("room") or it.get("time"):
        lines.append("     " + ", ".join(x for x in (it.get("time"), it.get("room")) if x))
    return "\n".join(lines)


# ---------- HTML-экспорт (если в Telegram Desktop выбран формат HTML, а не JSON) ----------

EN_MONTHS = {m: i for i, m in enumerate(["january", "february", "march", "april", "may", "june", "july", "august",
                                         "september", "october", "november", "december"], 1)}


RU_MONTHS = {m: i for i, m in enumerate(["янв", "фев", "мар", "апр", "мая", "июн", "июл", "авг", "сен", "окт", "ноя", "дек"], 1)}
RU_MONTHS["май"] = 5


def html_date(title):
    """«22 February 2025, 13:25:08» или «22.02.2025 13:25:08 UTC+03:00» -> 2025-02-22T13:25:08"""
    m = re.match(r"(\d{1,2})\.(\d{2})\.(\d{4})\s+(\d{1,2}:\d{2}:\d{2})", title)
    if m:
        return f"{m[3]}-{m[2]}-{int(m[1]):02d}T{int(m[4][:-6]):02d}{m[4][-6:]}"
    m = re.match(r"(\d{1,2})\s+([A-Za-zА-Яа-яЁё]+)\s+(\d{4}),?\s+(\d{1,2}):(\d{2}):(\d{2})", title)
    month = m and (EN_MONTHS.get(m[2].lower()) or RU_MONTHS.get(m[2].lower()[:3]))  # Telegram на русском: «5 июня 2023»
    if month:
        return f"{m[3]}-{month:02d}-{int(m[1]):02d}T{int(m[4]):02d}:{m[5]}:{m[6]}"
    raise ValueError(f"не понял дату {title!r}")


def html_clean(fragment):
    fragment = re.sub(r"<br\s*/?>", "\n", fragment)
    return html.unescape(re.sub(r"<[^>]+>", "", fragment)).strip()


def load_html_export(folder):
    """Папка ChatExport_... с messages.html, messages2.html, ... -> тот же вид, что у JSON-экспорта."""
    pages = sorted(folder.glob("messages*.html"), key=lambda f: int(re.sub(r"\D", "", f.stem) or 1))
    msgs, author = [], "?"
    for page in pages:
        for b in re.split(r'(?=<div class="message (?:default|service))', page.read_text(encoding="utf-8"))[1:]:
            mid = re.search(r'id="message(\d+)"', b[:200])
            if not mid or 'class="message service' in b[:40]:
                continue
            name = re.search(r'<div class="from_name">\s*(.*?)\s*</div>', b, re.S)
            if name:
                author = html_clean(re.sub(r'<span class="date details".*', "", name[1], flags=re.S))
            when = re.search(r'class="pull_right date details" title="([^"]+)"', b)
            text = re.search(r'<div class="text">(.*?)</div>', b, re.S)
            reply = re.search(r"GoToMessage\((\d+)\)", b)
            m = {"id": int(mid[1]), "type": "message", "date": html_date(when[1]) if when else None,
                 "from": author, "text": html_clean(text[1]) if text else "",
                 "text_entities": [{"type": "link", "text": html.unescape(h)} for h in
                                   re.findall(r'href="(https?://[^"]+)"', text[1])] if text else []}
            if text:
                m["text_entities"].insert(0, {"type": "plain", "text": m["text"]})
            if reply:
                m["reply_to_message_id"] = int(reply[1])
            if 'class="forwarded_from' in b:
                m["forwarded_from"] = True
            f = re.search(r'class="media clearfix pull_left block_link media_file" href="([^"]+)"', b)
            if f:
                title = re.search(r'<div class="title bold">\s*(.*?)\s*</div>', b, re.S)
                m["file"] = html.unescape(f[1])
                m["file_name"] = html_clean(title[1]) if title else Path(m["file"]).name
            elif re.search(r'class="media clearfix pull_left (?:media_file)?"[^>]*>|class="media clearfix pull_left block_link media_file"', b):
                m["file"] = "(File not included)"  # файл есть в чате, но не выгружен
                title = re.search(r'<div class="title bold">\s*(.*?)\s*</div>', b, re.S)
                if title:
                    m["file_name"] = html_clean(title[1])
            p = re.search(r'class="photo_wrap[^"]*" href="([^"]+)"', b)
            if p:
                m["photo"] = html.unescape(p[1])
            if m["date"]:
                msgs.append(m)
    return {"name": re.sub(r"_\d+$", "", folder.name.removeprefix("ChatExport_")), "messages": msgs}  # «…_2» — повторный экспорт


def load_export(path):
    path = Path(path)
    if path.is_dir():
        if (path / "result.json").exists():
            path = path / "result.json"
        else:
            return load_html_export(path)
    if path.suffix == ".html":
        return load_html_export(path.parent)
    return json.load(open(path, encoding="utf-8"))


# ---------- режимы ----------

def run_export(path, use_all, limit):
    data = load_export(path)
    chat = data.get("name") or Path(path).stem
    stem = re.sub(r"[^\w-]+", "_", chat).strip("_") or "chat"
    out_dir = Path(path) if Path(path).is_dir() else Path(path).parent
    stem += CACHE_TAG
    items_path, cache_path = out_dir / f"{stem}_items.json", out_dir / f"{stem}_llm.json"
    cache = json.load(open(cache_path, encoding="utf-8")) if cache_path.exists() else {}
    raw_msgs = [from_export(m, chat) for m in data["messages"] if m.get("type") == "message"]
    msgs = merge_bursts(raw_msgs)
    todo = [m for m in msgs if (use_all and (m["text"] or m["files"])) or worth_asking(m)][:limit]
    todo_ids = {m["id"] for m in todo}
    for m in todo:  # пути в экспорте — относительно папки экспорта
        if m.get("file_paths"):
            m["file_paths"] = [str(out_dir / p) for p in m["file_paths"]]
            m["file_texts"] = attachments.texts_for(m["file_paths"], ocr=PHOTO_MODE == "board")
    with_text = sum(1 for m in todo if m.get("file_texts"))
    if with_text:
        print(f"Текст достали из вложений {with_text} сообщений")
    print(f"Чат «{chat}»: сообщений {len(raw_msgs)} (подряд от одного автора склеено в {len(msgs)}), отдаём модели {len(todo)}"
          + ("" if use_all else " (остальные отсеяны как болтовня; --all, чтобы взять все)"))
    pos = {m["id"]: i for i, m in enumerate(msgs)}
    found, started = [], time.time()
    n = 0
    for m in msgs[:pos[todo[-1]["id"]] + 1] if todo else []:
        reply_update(m, found)  # «она в четверг» ответом на старое сообщение (фильтр такое часто отсеивает)
        if m["id"] not in todo_ids:
            continue
        n += 1
        i = pos[m["id"]]
        history = msgs[max(0, i - CONTEXT):i]
        reply = next((x for x in msgs[:i] if m["reply_to"] in x["message_ids"]), None)
        if reply and reply not in history:
            history = [reply] + history
        try:
            new = process_message(m, history, chat, found, cache, after_of(msgs, i))
        except Exception as e:  # невалидный JSON от модели и т.п. — пропускаем сообщение
            print(f"#{m['id']}: ошибка {e}")
            continue
        if new or n % 20 == 0:
            print(f"[{n}/{len(todo)}] {time.time() - started:.0f} с")
        for it in new:
            print(show(it))
        json.dump(cache, open(cache_path, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
    for it in found:  # к концу прогона дни предмета известны лучше, пересчитываем угаданные сроки
        if it.get("date_source") and it["date_source"] != "расписание":  # по расписанию уже точно
            fill_due(it, found)
    json.dump(found, open(items_path, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
    guessed = sum(1 for x in found if x.get("date_source") and x["type"] not in ("material", "info"))
    if guessed:
        print(f"Дата «к следующему уроку» поставлена {guessed} находкам без даты в тексте"
              + ("" if LESSONS_FILE.exists() or timetable.chat_info(chat) else "; точнее будет с timetable.json (расписание)"))
    counts = {t: sum(1 for x in found if x["type"] == t) for t in ("homework", "test", "lesson_change", "material", "info")}
    print(f"\nГотово: {len(found)} находок {counts} → {items_path}")


# live: сколько секунд ждать продолжения от того же автора (его сообщения склеиваются в одно)
BURST = int(os.environ.get("BURST", 20))
# live: сколько секунд максимум ждать 3 следующих сообщения и ответы, прежде чем разбирать
AFTER_WAIT = int(os.environ.get("AFTER_WAIT", 300))


def run_live():
    token = chat_moderation.moderator.env("BOT_TOKEN")  # из переменной окружения или из файла .env
    if not token:
        sys.exit("Нужен токен бота, который сидит в чатах: строка BOT_TOKEN=... в файле .env рядом со скриптом\n"
                 "(это не бот-модератор, у него свой MODERATOR_BOT_TOKEN)")
    if "--no-moderator" not in sys.argv:
        if token == chat_moderation.moderator.TOKEN:
            sys.exit("BOT_TOKEN и MODERATOR_BOT_TOKEN — один и тот же бот, так нельзя (ошибка 409): нужны два разных")
        global MODERATE
        MODERATE = True
        chat_moderation.start(channel="--channel" in sys.argv)
    api = f"https://api.telegram.org/bot{token}/"
    items_path = Path("chats/live_items.json")
    items_path.parent.mkdir(exist_ok=True)
    found = json.load(open(items_path, encoding="utf-8")) if items_path.exists() else []
    history, offset = {}, None  # последние сообщения каждого чата, для контекста
    pending = {}  # chat -> склеиваемая серия сообщений одного автора («завтра кр» + «а не послезавтра»)
    waiting = {}  # chat -> серии, которые ждут 3 следующих сообщения и ответы на себя
    global VERBOSE
    VERBOSE = "--quiet" not in sys.argv
    print("Слушаю чаты (Ctrl+C — остановить)" + ("" if VERBOSE else ", без подробностей"))

    def enqueue(m):
        """Серия закончена: она становится «следующим сообщением» или ответом для ждущих и сама встаёт в очередь."""
        h = history.setdefault(m["chat"], [])
        for w in waiting.get(m["chat"], []):
            if m.get("reply_to") in w["m"]["message_ids"] and len(w["replies"]) < AFTER:
                w["replies"].append(m)
            elif len(w["after"]) < AFTER:
                w["after"].append(m)
        waiting.setdefault(m["chat"], []).append({"m": m, "history": list(h), "after": [], "replies": [], "since": time.time()})
        h.append(m)
        del h[:-50]
        say(f"   (серия {m['author']} закончена: {m['text'][:60]!r}; жду {AFTER} следующих сообщения"
            f" или {AFTER_WAIT} с, потом разберу)")

    def ready():
        for chat, ws in waiting.items():
            while ws and (len(ws[0]["after"]) >= AFTER or time.time() - ws[0]["since"] > AFTER_WAIT):
                w = ws.pop(0)
                handle(w["m"], w["history"], sorted(w["after"] + w["replies"], key=lambda x: x["id"]))

    def handle(m, h, after):
        if after:
            say(f"\n[{m['chat']}] разбираю сообщение {m['author']}, следом пришло ещё {len(after)}:")
            for a in after:
                say(f"   ↳ {a['author']}: {a['text'][:150]}")
        if len(m["message_ids"]) > 1:
            say(f"\n[{m['chat']}] {m['author']}, {len(m['message_ids'])} сообщения подряд, разбираю вместе:\n   {m['text'][:400]}")
        else:
            say(f"\n[{m['chat']}] {m['author']}: разбираю")
        if m.get("reply_to") and not any(m["reply_to"] in x.get("message_ids", [x["id"]]) for x in h):
            old = next((x for x in reversed(found) if x["chat"] == m["chat"] and m["reply_to"] in item_ids(x)), None)
            if old:  # отвечают на давнее сообщение: показываем модели его по цитате из находки
                h = [{"id": m["reply_to"], "message_ids": [m["reply_to"]], "author": old.get("author", "?"),
                      "text": old.get("quote") or old["summary"], "date": old["sent_at"]}] + h
        if m.get("file_ids"):  # скачиваем вложения и достаём из них текст
            names = m.get("files", [])
            dest = Path("chats/files") / attachments.safe_name(m["chat"])
            paths = []
            for i, fid in enumerate(m["file_ids"]):
                name = names[i] if i < len(names) and names[i] != "фото" else f"photo_{m['message_ids'][min(i, len(m['message_ids']) - 1)]}"
                p = attachments.download(api, fid, dest, f"{m['id']}_{name}")
                if p:
                    paths.append(str(p))
            m["file_paths"] = paths
            m["file_texts"] = attachments.texts_for(paths, ocr=PHOTO_MODE == "board")
            say(f"   скачано файлов: {len(paths)} из {len(m['file_ids'])}, текст есть в {len(m['file_texts'])}")
        changed = reply_update(m, found)
        if changed:
            json.dump(found, open(items_path, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
        if worth_asking(m):
            try:
                for it in process_message(m, h, m["chat"], found, after=after):
                    print(show(it))
            except Exception as e:  # Ollama не отвечает и т.п.
                print(f"#{m['id']}: ошибка {e}")
            json.dump(found, open(items_path, "w", encoding="utf-8"), ensure_ascii=False, indent=1)

    while True:
        try:
            wait = 5 if pending or any(waiting.values()) else 50  # есть недосказанное — опрашиваем чаще, чтобы вовремя разобрать
            q = f"getUpdates?timeout={wait}&allowed_updates=%5B%22message%22%2C%22edited_message%22%5D" + (f"&offset={offset}" if offset else "")
            with urllib.request.urlopen(api + q, timeout=wait + 20) as r:
                updates = json.loads(r.read())["result"]
        except Exception as e:  # нет сети — подождём
            print("ошибка:", e)
            time.sleep(10)
            updates = []
        for u in updates:
            offset = u["update_id"] + 1
            msg = u.get("message") or u.get("edited_message")
            if not msg or msg["chat"]["type"] not in ("group", "supergroup"):
                say(f"\n(пропускаю: не сообщение из группы — {msg['chat']['type'] if msg else list(u)})")
                continue
            if u.get("edited_message"):
                say("\n(сообщение отредактировано, разбираю заново)")
            m = from_bot(msg)
            say(f"\n[{m['chat']}] {m['author']}: {m['text'][:300]}" + (f"  📎 {', '.join(m['files'])}" if m["files"] else ""))
            p = pending.get(m["chat"])
            if p and p["author"] == m["author"] and not m["reply_to"] and time.time() - p["last_seen"] <= BURST:
                p["text"] = "\n".join(filter(None, [p["text"], m["text"]]))
                for k in ("links", "files", "file_ids"):
                    p[k] = p.get(k, []) + m.get(k, [])
                p["message_ids"].append(m["id"])
                p["last_seen"] = time.time()
            else:
                if p:  # написал кто-то другой — прошлая серия закончена
                    enqueue(pending.pop(m["chat"]))
                pending[m["chat"]] = {**m, "message_ids": [m["id"]], "last_seen": time.time()}
            p = pending[m["chat"]]
            say(f"   (склеиваю сообщения {m['author']}: уже {len(p['message_ids'])}; серия закончится через {BURST} с тишины"
                " или когда напишет другой человек)")
        for chat in [c for c, p in pending.items() if time.time() - p["last_seen"] > BURST]:
            enqueue(pending.pop(chat))
        ready()


if __name__ == "__main__":
    args = sys.argv[1:]
    if not args:
        sys.exit(__doc__)
    if args[0] == "live":
        run_live()
    elif args[0] == "manual":  # то же, что /manual_chats в боте-модераторе (только чаты; каналы — /manual_channels)
        m = chat_moderation.moderator
        print(m.manual_text("chats", m.toggle_manual("chats")))
    else:
        lim = int(args[args.index("--limit") + 1]) if "--limit" in args else None
        VERBOSE = "--verbose" in args
        run_export(args[0], "--all" in args, lim)
