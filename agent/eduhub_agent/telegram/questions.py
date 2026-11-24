"""Что спросили у бота в личке: вопрос о домашке, расписании, кабинете или новое ДЗ.

Сначала простые правила (мгновенно), а если правила не уверены — короткий запрос к модели. Модель только выбирает
намерение, день и предмет; отвечает бот сам, по расписанию бэкенда (dm.answer_question), поэтому придумать ДЗ
или кабинет модель не может.
Намерения: add (записать ДЗ), homework (что задали), schedule (какие уроки), next (где и когда урок),
day (уроки дня вместе с ДЗ: «что у нас в пятницу?» — только правила), other.
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


def typos(a, b):
    """Сколько опечаток между словами: замена, лишняя или пропущенная буква, две соседние буквы местами
    (расстояние Дамерау — Левенштейна). «раписание» ~ «расписание» = 1, «когад» ~ «когда» = 1."""
    prev2, prev = None, list(range(len(b) + 1))
    for i in range(1, len(a) + 1):
        cur = [i] + [0] * len(b)
        for j in range(1, len(b) + 1):
            cur[j] = min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + (a[i - 1] != b[j - 1]))
            if i > 1 and j > 1 and a[i - 1] == b[j - 2] and a[i - 2] == b[j - 1]:
                cur[j] = min(cur[j], prev2[j - 2] + 1)
        prev2, prev = prev, cur
    return prev[-1]


def allowed_typos(word):
    """Короткие слова (до 4 букв) не правим: «кода», «года» — настоящие слова. До 7 букв — одна опечатка, длиннее — две."""
    return 0 if len(word) <= 4 else 1 if len(word) <= 7 else 2


# слова, по которым правила узнают вопрос; опечатки в них правит fix_typos
KEYWORDS = ("расписание", "расписания", "расписанию", "расписании", "домашка", "домашку", "домашки", "домашке",
            "домашнее", "домашняя", "домашний", "задали", "задала", "задано", "задание", "задания", "кабинет",
            "кабинете", "кабинета", "следующий", "следующая", "следующее", "следующего", "когда", "сколько", "какие",
            "какой", "какая", "какое", "уроки", "урока", "уроков", "покажи", "сегодня", "завтра", "послезавтра")

REAL_WORDS = {"завтрак", "завтраке", "завтраку", "столько", "сколь"}  # настоящие слова рядом с ключевыми


def fix_typos(text):
    """Исправить опечатки в словах вопроса: «раписание на зватра» -> «расписание на завтра», «кабинте химии» ->
    «кабинет химии». Слово меняем, только если первая буква та же и до ключевого слова 1–2 опечатки. Слова, которые
    правила и так узнают («задачи», «номер», «решить»), не трогаем: «задачи» не должно стать «задали»."""
    def fix(m):
        w = m.group(0)
        low = w.lower()
        n = allowed_typos(low)
        if not n or low in KEYWORDS or low in REAL_WORDS or HOMEWORK_TEXT.search(low) or HW_WORDS.search(low) or NEXT_WORDS.search(low):
            return w
        best = min(KEYWORDS, key=lambda k: (typos(low, k), abs(len(k) - len(low))))
        return best if best[0] == low[0] and typos(low, best) <= n else w
    return re.sub(r"[а-яёА-ЯЁ]+", fix, text or "")


MONTHS = (("январ", 1), ("феврал", 2), ("март", 3), ("апрел", 4), ("ма", 5), ("июн", 6), ("июл", 7), ("август", 8),
          ("сентябр", 9), ("октябр", 10), ("ноябр", 11), ("декабр", 12))
MONTH_WORD = r"(январ\w*|феврал\w*|март\w*|апрел\w*|ма[яй]|июн\w*|июл\w*|август\w*|сентябр\w*|октябр\w*|ноябр\w*|декабр\w*)"
NUM_DOT = re.compile(r"(?<![\d.])(\d{1,2})[./](\d{1,2})(?:[./](?:20)?\d\d)?(?![\d])")  # 25.11, 25/11, 25.11.2026
NUM_MONTH = re.compile(r"\b(\d{1,2})(?:-?(?:го|е|ое))?\s+" + MONTH_WORD + r"(?![а-яё])", re.I)  # 25 ноября
# после числа идёт не день, а урок, номер задачи и т.п.: «на 5 урок», «к 3 уроку», «на 25 баллов»
NOT_DAY_WORDS = r"урок|класс|кабинет|каб\b|минут|мин\b|час|балл|раз\b|задач|номер|стр|страниц|параграф|упр"
NOT_DAY_AFTER = r"(?!\s*(?:-?(?:й|м|ый|ой)\s+)?(?:" + NOT_DAY_WORDS + r"|%|§|№))"
# у «25-го» окончание уже говорит, что это дата: после него «§5» или «№3» — само задание («к 25-му §5»)
NUM_SUFFIX = re.compile(r"\b(\d{1,2})-?(?:го|е|ое|ого|му|ому)\b(?!\s*(?:" + NOT_DAY_WORDS + r"))|\b(\d{1,2})\s+числ",
                        re.I)  # 25-го, 25е, 25 числа
NUM_PREP = re.compile(r"\b(?:на|к|ко|до)\s+(\d{1,2})\b(?![.,:/]\d|-?[а-яё])" + NOT_DAY_AFTER, re.I)  # на 25, к 25
NUM_BARE = re.compile(r"(?<![\d.,/:№§#-])\b(\d{1,2})\b(?![.,:/]\d|-?[а-яё])" + NOT_DAY_AFTER, re.I)
NOT_DAY_BEFORE = re.compile(r"(?:номер\w*|стр\w*|страниц\w*|упр\w*|каб\w*|урок\w*|параграф\w*|задач\w*|класс\w*|в|во)$")
IN_DAYS = re.compile(r"\bчерез\s+(\d{1,2})\s+(?:дн|день|дня)|\bчерез\s+(неделю)|\bчерез\s+(день)\b", re.I)  # через день = послезавтра
AFTER_BREAK = re.compile(r"\bпосле\s+каникул", re.I)  # день ищет dm по расписанию: первый учебный после перерыва


ORD_STEMS = {"перв": 1, "втор": 2, "трет": 3, "четверт": 4, "четвёрт": 4, "пят": 5, "шест": 6, "седьм": 7, "восьм": 8,
             "девят": 9, "десят": 10, "одиннадцат": 11, "двенадцат": 12, "тринадцат": 13, "четырнадцат": 14,
             "пятнадцат": 15, "шестнадцат": 16, "семнадцат": 17, "восемнадцат": 18, "девятнадцат": 19, "двадцат": 20,
             "тридцат": 30}
# «двадцать пятое», «первого», «третьему» — только средний род и родительный/дательный, как у дат:
# «пятый урок», «вторая задача» сюда не попадают
ORDINAL = re.compile(r"\b(?:(двадцать|тридцать)\s+)?(" + "|".join(sorted(ORD_STEMS, key=len, reverse=True)) +
                     r")(?:ое|ого|ому|ье|ьего|ьему)\b", re.I)


def words_to_numbers(text):
    """«на двадцать пятое» -> «на 25-е», «к первому» -> «к 1-е»: дальше дату разбирают как число."""
    def num(m):
        n = ORD_STEMS[m[2].lower()] + (20 if (m[1] or "").lower() == "двадцать" else 30 if m[1] else 0)
        return f"{n}-е" if n <= 31 and (not m[1] or ORD_STEMS[m[2].lower()] < 10) else m[0]
    return ORDINAL.sub(num, text or "")


WEEKEND = re.compile(r"\b(?:(следующ\w*|будущ\w*)\s+|эт\w+\s+)?выходн(?:ые|ых)\b", re.I)  # «на выходных», «в выходные»
WEEK = re.compile(r"(?<!через )\b(?:(следующ\w*|будущ\w*)|(прошл\w*)|(эт\w+)|(?<=на )|(?<=за ))\s*недел[юеия]\b", re.I)
WEEKDAY_ANY = r"(понедельник\w*|вторник\w*|сред[ауы]|четверг\w*|пятниц\w*|суббот\w*|воскресень\w*)"
NEXT_WEEKDAY = re.compile(r"\b(следующ\w*|эт\w+)\s+" + WEEKDAY_ANY, re.I)  # «в следующую среду», «в эту пятницу»


def parse_week(text, today):
    """«на неделю», «на этой неделе», «на следующей неделе», «на прошлой неделе» -> (первый день, последний) или None.
    «Эта неделя» — с сегодня до воскресенья (в сб и вс — уже следующая), «на неделю» — семь дней с сегодня."""
    t = (text or "").lower()
    monday = today - timedelta(days=today.weekday())
    m = WEEKEND.search(t)
    if m:  # сб и вс этой недели (в воскресенье — только сегодня), «следующие выходные» — через неделю
        sat = monday + timedelta(days=5 + (7 if m[1] else 0))
        return max(sat, today), sat + timedelta(days=1)
    m = WEEK.search(t)
    if not m:
        return None
    if m[1]:
        return monday + timedelta(days=7), monday + timedelta(days=13)
    if m[2]:
        return monday - timedelta(days=7), monday - timedelta(days=1)
    if m[3]:
        return (today, monday + timedelta(days=6)) if today.weekday() < 5 else \
            (monday + timedelta(days=7), monday + timedelta(days=13))
    return today, today + timedelta(days=6)


def month_day(n, today):
    """Число месяца без месяца («на 25») -> ближайшая такая дата: в этом месяце, если она не раньше чем три дня
    назад, иначе в следующем («на 3» 28.11 — это 3.12). Несуществующее («на 31» в ноябре) — следующий месяц с 31-м."""
    if not 1 <= n <= 31:
        return None
    for k in range(3):
        y, mo = today.year + (today.month - 1 + k) // 12, (today.month - 1 + k) % 12 + 1
        try:
            d = date(y, mo, n)
        except ValueError:
            continue
        if k or d >= today - timedelta(days=3):
            return d
    return None


def with_year(d, m, today):
    """День и месяц -> дата этого или следующего учебного года (дата больше месяца назад — следующий год)."""
    try:
        x = date(today.year, m, d)
    except ValueError:
        return None
    return x if x >= today - timedelta(days=30) else x.replace(year=today.year + 1)


def number_day(text, today, loose=False):
    """Дата, заданная числом: «25.11», «25 ноября», «25-го», «25 числа», «на 25», «к 25»; loose — ещё и просто «25»
    (только в вопросе: «какое расписание 25?»), если число в тексте одно и это не номер урока, задачи или кабинета."""
    t = (text or "").lower()
    m = NUM_DOT.search(t)
    if m:
        return with_year(int(m[1]), int(m[2]), today) if 1 <= int(m[2]) <= 12 else None
    m = NUM_MONTH.search(t)
    if m:
        mo = next(v for k, v in MONTHS if m[2].startswith(k))
        return with_year(int(m[1]), mo, today)
    m = NUM_SUFFIX.search(t) or NUM_PREP.search(t)
    if m:
        return month_day(int(m[1] or m[2]), today)
    if loose:
        nums = [m for m in NUM_BARE.finditer(t) if not NOT_DAY_BEFORE.search(t[:m.start()].rstrip())]
        if len(nums) == 1 and len(re.findall(r"\d+", t)) == 1:
            return month_day(int(nums[0][1]), today)
    return None


def suffix_day(text, today):
    """Только однозначные числа-даты: «к 25-му», «25-го», «25 числа» (для заданий в чатах, где «к 12» может быть
    и номером). «25.11» и «25 ноября» и так находят правила чатов."""
    m = NUM_SUFFIX.search((text or "").lower())
    return month_day(int(m[1] or m[2]), today) if m else None


def strip_dates(text):
    """Текст без дат-чисел: «дз на 25» -> «дз на». Нужно, чтобы дата не считалась номером задачи."""
    text = words_to_numbers(text)
    for rx in (NUM_DOT, NUM_MONTH, NUM_SUFFIX, NUM_PREP):
        text = rx.sub(" ", text)
    return text


def parse_day(text, today, loose=False):
    """«сегодня», «завтра», «послезавтра», «вчера», «в среду», «в следующую среду», «на пт», «20.10», «25 ноября»,
    «на 25», «25-го», «на двадцать пятое», «через 2 дня» -> date или None. День недели — ближайший такой, начиная
    с сегодня. loose — см. number_day."""
    t = words_to_numbers((text or "").lower())
    if re.search(r"\bпослезавтра", t):
        return today + timedelta(days=2)
    if re.search(r"\bзавтра(?:шн\w*)?\b", t):
        return today + timedelta(days=1)
    if re.search(r"\bсегодня", t):
        return today
    if re.search(r"\bпозавчера", t):
        return today - timedelta(days=2)
    if re.search(r"\bвчера(?:шн\w*)?\b", t):
        return today - timedelta(days=1)
    m = NEXT_WEEKDAY.search(t)
    if m:  # «в следующую среду» — среда следующей недели, «в эту среду» — этой
        wd = next(i for i, g in enumerate(WEEKDAY_WORD.match(m[2]).groups()) if g) if WEEKDAY_WORD.match(m[2]) else None
        if wd is not None:
            monday = today - timedelta(days=today.weekday()) + timedelta(days=7 if m[1].startswith("следующ") else 0)
            return monday + timedelta(days=wd)
    m = IN_DAYS.search(t)
    if m:
        return today + timedelta(days=7 if m[2] else 2 if m[3] else int(m[1]))
    d = number_day(t, today, loose)
    if d or re.search(r"\d", t) and NUM_DOT.search(t):
        return d
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


# «что у нас в пятницу?» сюда не входит: это может быть и расписание, и ДЗ — решает модель, как раньше
DAY_FILL = {"а", "и", "ну", "тогда", "на", "к", "ко", "в", "во", "до", "за", "с", "по", "числа", "число", "это", "эту",
            "этот", "эта", "этой", "следующую", "следующий", "следующей", "следующая", "прошлой", "прошлую", "неделю",
            "неделе", "неделя", "через", "дня", "дней", "день", "выходные", "выходных", "следующие", "следующих", "эти", "этих"}
# «что у нас в пятницу?», «что завтра?» — и уроки, и ДЗ: отвечаем тем и другим (намерение day), без модели
WHAT_DAY = re.compile(r"^(?:а\s+)?(?:что|чё|чо|че)\s+(?:(?:у\s+(?:нас|меня)|будет|там)\s+)*", re.I)


def only_day(text, today=None):
    """Сообщение — только день: «а на 26?», «25?», «а в пятницу?», «на следующей неделе». Такой вопрос продолжает
    предыдущий (что спрашивали — то и про новый день), а без предыдущего это вопрос о домашке."""
    today = today or date.today()
    t = words_to_numbers((text or "").lower())
    brk = AFTER_BREAK.search(t)
    t = AFTER_BREAK.sub(" ", t)
    if not (brk or parse_day(t, today, loose=True) or parse_week(t, today)):
        return False
    rest = [w for w in re.findall(r"[а-яё]+", NUM_DOT.sub(" ", NUM_MONTH.sub(" ", t)))
            if w not in DAY_FILL and not re.fullmatch(r"е|го|ое|му|ого|ому", w)]
    return all(parse_day(w, today) or re.match(MONTH_WORD, w) for w in rest)


QUESTION = re.compile(r"\?|^(что|чё|че|чо|какое|какая|какие|какой|где|когда|во сколько|покажи|скажи|подскажи|расскажи|"
                      r"пришли|скинь|кинь|дай|напиши|отправь|есть ли|а\s)", re.I)
GREETING = re.compile(r"^(?:привет\w*|здравствуй\w*|здрасьте|добр\w+\s+(?:утро|день|вечер)|хай|хей|ку)[\s,!.)]+", re.I)
# просьба показать домашку, а не новое ДЗ: «что мне надо сделать на 25?», «пришли все дз», «по всем предметам»
ASK_HW = re.compile(r"\b(?:что|чё|че|чо|чего)\b(?:\s+[а-яё]+){0,3}?\s+(?:сделать|делать|учить|выучить|готовить|"
                    r"подготовить|решать|решить|прочитать|читать)\b"
                    r"|\b(?:пришли|пришлите|скинь|скиньте|кинь|киньте|дай|дайте|покажи|покажите|напиши|напишите|отправь|"
                    r"выведи|подскажи|подскажите|расскажи|скажи|скажите)\b.{0,40}?(?:\bдз\b|д/з|домашк|домашн|задани|задал)"
                    r"|\b(?:все|всё|вся|всю)\s+(?:дз\b|домашк|домашн|задани)|\bпо\s+всем\s+предмет", re.I)
HELLO = re.compile(r"^(привет\w*|здравствуй\w*|хай|hi|hello|спасибо|спс|ок|окей|ага|понял\w*|ясно|пока|добрый \w+)[!.)\s]*$", re.I)
HW_WORDS = re.compile(r"задал|дз\b|домашк|домашн|задани", re.I)
SCHEDULE_WORDS = re.compile(r"расписан|какие уроки|что за уроки|сколько уроков|уроки (сегодня|завтра|в |на )", re.I)
NEXT_WORDS = re.compile(r"кабинет|\bкаб\b|где\b|куда\b|когда\b|во сколько|следующ(?!\w*\s+(?:недел|понедельник|вторник|"
                        r"сред|четверг|пятниц|суббот|воскресень|выходн))", re.I)  # «в следующую среду» — день, а не следующий урок
PO_SUBJECT = re.compile(r"^(а\s+)?(что\s+)?(задали\s+)?по\s+[а-яё.\-]+(\s+[а-яё]+)?\s*\??$", re.I)
HOMEWORK_TEXT = re.compile(r"§|№|\bномер|\bзадач|\bстр\b|\bстраниц|\bупр|параграф|выучить|решить|прочитать|сделать|написать|подготовить", re.I)


def rule_intent(text):
    """Намерение по простым признакам или None, если не уверены."""
    t = fix_typos((text or "").strip())
    if HELLO.match(t):
        return "other"
    t = GREETING.sub("", t)  # «Привет, скажи что задали» — дальше смотрим без приветствия
    m = WHAT_DAY.match(t)
    if m and m.end() < len(t) and only_day(t[m.end():]):
        return "day"
    if only_day(t):
        return "homework"  # «а на 26?» — dm продолжит прошлый вопрос, если он был только что
    if SCHEDULE_WORDS.search(t):
        return "schedule"
    if ASK_HW.search(t) and not re.search(r"[§№]", t):
        return "homework"  # «что надо сделать на 25?», «пришли все дз»: «сделать» тут не задание, а вопрос
    if not QUESTION.search(t) and re.search(r"\d", strip_dates(t)):
        return "add"  # «дз по алгебре 7, 9» — числа без вопроса (кроме дат: 20.10, на 25, 25-го) — это задание
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
day — день из сообщения, как написан («завтра», «вчера», «среда», «20.10», «25», «следующая неделя»), или null.
subject — предмет из сообщения, как написан, или null."""


def llm_intent(text):
    """Короткий запрос к модели: {"intent", "day", "subject"}. Исключение — модель не ответила."""
    body = {"model": MODEL, "stream": False, "format": SCHEMA, "options": {"temperature": 0, "num_ctx": 2048},
            "messages": [{"role": "system", "content": SYSTEM}, {"role": "user", "content": fix_typos(text)[:500]}]}
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
