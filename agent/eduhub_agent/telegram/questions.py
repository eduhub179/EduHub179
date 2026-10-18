"""Что спросили у бота в личке: вопрос о домашке, расписании, кабинете или новое ДЗ.

Сначала простые правила (мгновенно), а если правила не уверены — короткий запрос к модели. Модель только выбирает
намерение, день и предмет; отвечает бот сам, по расписанию бэкенда (dm.answer_question), поэтому придумать ДЗ
или кабинет модель не может.
Намерения: add (записать ДЗ), homework (что задали), schedule (какие уроки), next (где и когда урок), other.
"""
import json
import os
import re
from difflib import SequenceMatcher
import urllib.request
from datetime import date, timedelta

MODEL = os.environ.get("MODEL", "qwen2.5:7b")
URL = os.environ.get("OLLAMA_URL", "http://localhost:11434") + "/api/chat"

WEEKDAY_WORD = re.compile(r"^(?:(понедельник\w*)|(вторник\w*)|(сред[ауы])|(четверг\w*)|(пятниц\w*)|(суббот\w*)|(воскресень\w*))$")
NOT_DAY = re.compile(r"^(?:четверт|средн|среди|сред$|пятер|пятёр|второ|завтрак)")  # похожие слова, но не дни
SHORT = {"пн": 0, "вт": 1, "ср": 2, "чт": 3, "пт": 4, "сб": 5, "вс": 6}


# слова для сравнения с опечатками: «вторний», «пятницк», «завра», «сегодян»
FUZZY_DAYS = {"сегодня": "today", "завтра": 1, "послезавтра": 2, "понедельник": 0, "вторник": 1, "среда": 2, "среду": 2,
              "четверг": 3, "пятница": 4, "пятницу": 4, "суббота": 5, "субботу": 5, "воскресенье": 6}


def close(word, target, cutoff=0.75):
    """Похожи ли слова с учётом опечатки (одна-две буквы)."""
    return len(word) >= 4 and SequenceMatcher(None, word, target).ratio() >= cutoff


def fuzzy_day(word, today):
    for target, v in FUZZY_DAYS.items():
        if close(word, target, 0.8 if len(target) <= 6 else 0.75):
            if v == "today":
                return today
            if target in ("завтра", "послезавтра"):
                return today + timedelta(days=v)
            return today + timedelta(days=(v - today.weekday()) % 7)
    return None


def parse_day(text, today):
    """«сегодня», «завтра», «послезавтра», «в среду», «на пт», «20.10» -> date или None.
    День недели — ближайший такой, начиная с сегодня."""
    t = (text or "").lower()
    if re.search(r"\bпослезавтра", t):
        return today + timedelta(days=2)
    if re.search(r"\bзавтра(?:шн\w*)?\b", t):
        return today + timedelta(days=1)
    if re.search(r"\bсегодня", t):
        return today
    m = re.search(r"\b(\d{1,2})\.(\d{1,2})\b", t)
    if m:
        try:
            d = date(today.year, int(m[2]), int(m[1]))
        except ValueError:
            return None
        return d if d >= today - timedelta(days=30) else d.replace(year=today.year + 1)
    words = re.findall(r"[а-яё]+", t)
    for w in words:
        m = WEEKDAY_WORD.match(w)
        wd = next(i for i, g in enumerate(m.groups()) if g) if m else SHORT.get(w)
        if wd is not None:
            return today + timedelta(days=(wd - today.weekday()) % 7)
    for w in words:  # опечатки: «на вторний», «в пятницк», «завра»
        d = None if NOT_DAY.match(w) else fuzzy_day(w, today)
        if d:
            return d
    return None


QUESTION = re.compile(r"\?|^(что|чё|че|какое|какая|какие|какой|где|когда|во сколько|покажи|скажи|есть ли|а\s)", re.I)
HELLO = re.compile(r"^(привет\w*|здравствуй\w*|хай|hi|hello|спасибо|спс|ок|окей|ага|понял\w*|ясно|пока|добрый \w+)[!.)\s]*$", re.I)
HW_WORDS = re.compile(r"задал|дз\b|домашк|домашн|задани", re.I)
SCHEDULE_WORDS = re.compile(r"расписан|какие уроки|что за уроки|сколько уроков|уроки (сегодня|завтра|в |на )", re.I)
NEXT_WORDS = re.compile(r"кабинет|\bкаб\b|где\b|куда\b|следующ|когда\b|во сколько", re.I)
PO_SUBJECT = re.compile(r"^(а\s+)?(что\s+)?(задали\s+)?по\s+[а-яё.\-]+(\s+[а-яё]+)?\s*\??$", re.I)
HOMEWORK_TEXT = re.compile(r"§|№|\bномер|\bзадач|\bстр\b|\bстраниц|\bупр|параграф|выучить|решить|прочитать|сделать|написать|подготовить", re.I)


def rule_intent(text):
    """Намерение по простым признакам или None, если не уверены."""
    t = (text or "").strip()
    if HELLO.match(t):
        return "other"
    if SCHEDULE_WORDS.search(t):
        return "schedule"
    if not QUESTION.search(t) and re.search(r"\d", re.sub(r"\b\d{1,2}\.\d{1,2}\b", "", t)):
        return "add"  # «дз по алгебре 7, 9» — числа без вопроса (кроме даты 20.10) — это задание
    if HOMEWORK_TEXT.search(t):
        return "add"  # есть само задание (§, №, «решить») — записываем, запись подтверждается кнопкой
    if PO_SUBJECT.match(t):
        return "homework"  # «а по алгебре?», «что по химии» — в школьном боте это про домашку
    if HW_WORDS.search(t):
        return "homework"  # «что задали по химии», «дз на завтра»
    if NEXT_WORDS.search(t):
        return "next"
    if not QUESTION.search(t):
        return "add"  # утверждение без вопроса — новое ДЗ, как раньше
    return None


SCHEMA = {"type": "object", "properties": {
    "intent": {"type": "string", "enum": ["add", "homework", "schedule", "next", "other"]},
    "day": {"type": ["string", "null"]},
    "subject": {"type": ["string", "null"]}}, "required": ["intent", "day", "subject"]}
SYSTEM = """Ученик пишет школьному боту. Определи, чего он хочет. Ответ — JSON.
intent:
- add — сообщает задание, чтобы бот его записал («физика к пятнице §12»);
- homework — спрашивает, что задали («что по химии?», «а по алгебре?», «есть дз на завтра?»);
- schedule — спрашивает расписание или какие уроки («что у нас в среду?»);
- next — спрашивает, где или когда урок, кабинет («где алгебра?», «когда физра?»);
- other — всё остальное.
day — день из сообщения, как написан («завтра», «среда», «20.10»), или null.
subject — предмет из сообщения, как написан, или null."""


def llm_intent(text):
    """Короткий запрос к модели: {"intent", "day", "subject"}. Исключение — модель не ответила."""
    body = {"model": MODEL, "stream": False, "format": SCHEMA, "options": {"temperature": 0, "num_ctx": 2048},
            "messages": [{"role": "system", "content": SYSTEM}, {"role": "user", "content": text[:500]}]}
    req = urllib.request.Request(URL, data=json.dumps(body).encode(), headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=120) as r:
        out = json.loads(json.loads(r.read())["message"]["content"])
    return out if out.get("intent") in SCHEMA["properties"]["intent"]["enum"] else {"intent": "other"}


def classify(text, llm=None):
    """-> {"intent", "day", "subject", "by"}; day и subject — как написано (их разбирает dm)."""
    intent = rule_intent(text)
    if intent:
        return {"intent": intent, "day": None, "subject": None, "by": "rules"}
    try:
        out = (llm or llm_intent)(text)
    except Exception as e:  # Ollama не отвечает
        print("ЛС: модель не ответила на вопрос:", repr(e))
        return {"intent": "other", "day": None, "subject": None, "by": "fallback"}
    return {"intent": out.get("intent", "other"), "day": out.get("day"), "subject": out.get("subject"), "by": "model"}


if __name__ == "__main__":  # проверка на настоящей модели: python3 -m eduhub_agent.telegram.questions "что у нас в пятницу?" "а по химии?"
    import sys
    for t in sys.argv[1:]:
        print(t, "->", classify(t), "->", parse_day(t, date.today()))
