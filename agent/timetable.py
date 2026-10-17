"""Расписание для агента: на каком уроке задали ДЗ и к какому уроку его сдавать.

Агент спрашивает расписание только через функции этого модуля (lessons_on, lesson_at, next_lesson, ...),
а откуда оно берётся, решает «источник»:
  FileSource    — timetable.json рядом со скриптом (сейчас: заглушка для 11Б с картинки);
  BackendSource — бэкенд EduHub, когда у Миши появится HTTP API (TIMETABLE_URL=https://... в окружении).
Оба источника отдают одно и то же: экземпляры уроков на дату, как lesson_instances в бэкенде
(docs/SCHEDULE.md, docs/OVERRIDES.md): {"date", "class", "num", "start", "end", "subject", "teacher", "room",
"group", "status": "scheduled" | "cancelled", "changed"}. Замена = отменённый урок + новый на его месте.
Поэтому, когда появится всё расписание школы, менять нужно только источник, а не агента.

timetable.json (все классы в одном файле, формат на будущее тот же):
  bells:   номер урока -> [начало, конец]: {"1": ["09:00", "09:45"], ...}
  classes: класс -> день недели -> уроки по номерам. Урок — строка «Физика», объект {"subject", "teacher", "room"},
           список таких объектов с "group" (урок по группам: тервер у одних, английский у других) или null (окно).
  chats:   часть названия чата -> {"class": "11Б", "subjects": ["Физика"]}; без subjects — общий чат класса,
           предмет тогда ищется в тексте ДЗ.
  teachers: учителя в чатах, чьи сообщения всегда идут в модель: ["Иван Иванов", "user123456789", ...] —
           имя, как в Telegram, или id (в JSON-экспорте from_id). Фамилии из расписания узнаются и без этого списка.
  groups:  в какой группе ученик по предмету, если урок по группам: {"11Б": {"Английский язык": 2}} (не обязательно).
  substitutions: замены на дату (здесь или отдельно в substitutions.json — тот же список): [{"date": "2026-10-20", "class": "11Б", "lesson": 3, "subject": "Алгебра" | null
           (null = урок отменён), "teacher": ..., "room": ..., "group": ... (если меняется только одна группа)}]
"""
import json
import os
import re
import urllib.request
from datetime import date, datetime, timedelta
from pathlib import Path

FILE = Path(__file__).parent / "timetable.json"
SUBS_FILE = Path(__file__).parent / "substitutions.json"  # замены меняются каждый день, поэтому отдельным файлом
WINDOW_MIN = 7  # сообщение в течение 7 минут после конца урока считаем написанным «на этом уроке»
DAYS = ["понедельник", "вторник", "среда", "четверг", "пятница", "суббота", "воскресенье"]

# сокращения из школьного расписания -> полные названия
ALIASES = {"англяз": "английский язык", "англ": "английский язык", "английский": "английский язык",
           "русяз": "русский язык", "русский": "русский язык", "инфор": "информатика", "инфа": "информатика",
           "общ": "обществознание", "общага": "обществознание", "геом": "геометрия", "тервер": "теория вероятностей",
           "матан": "математический анализ", "физра": "физкультура", "профиз": "физика",
           "важное": "разговоры о важном", "лит": "литература", "литра": "литература", "био": "биология"}


def norm(s):
    return re.sub(r"[^а-яёa-z0-9]", "", (s or "").lower())


def canon(s):
    return norm(ALIASES.get(norm(s), s))


def same_subject(a, b):
    """«Рус.яз» ~ «Русский язык», «Алгебра» ~ «Алгебра и начала анализа»: совпадение по началу названия.
    Уточнение в скобках должно совпасть: «Физика» и «Физика (профиль)» — разные уроки."""
    def split(x):
        x = ALIASES.get(norm(x), x)
        base, _, qual = x.partition("(")
        return norm(base), norm(qual)
    (a, qa), (b, qb) = split(a), split(b)
    return bool(a and b) and qa == qb and (a.startswith(b[:5]) or b.startswith(a[:5]))


def mentions(text, subject):
    """Упоминается ли предмет в тексте: «по химии», «на алгебру», «англ»."""
    c = canon(subject)
    stem = c[:5] if len(c) >= 6 else c[:4]
    for w in re.findall(r"[а-яёa-z]+", (text or "").lower()):
        if len(w) >= 4 and w.startswith(stem) or ALIASES.get(w) and canon(ALIASES[w]) == c:
            return True
    return False


# ---------- источники ----------

def _read(path, cache):
    """JSON-файл, перечитывается, если его поменяли, пока агент работает. Нет файла -> None."""
    try:
        mtime = path.stat().st_mtime
    except OSError:
        return None
    if cache.get("mtime") != mtime:
        try:
            cache["data"], cache["mtime"] = json.load(open(path, encoding="utf-8")), mtime
        except ValueError as e:
            print(f"{path.name} не читается: {e}")
            return cache.get("data")  # файл правят прямо сейчас: пока старая версия
    return cache["data"]


class FileSource:
    """timetable.json (+ substitutions.json с заменами)."""

    def __init__(self, path=FILE, subs_path=SUBS_FILE):
        self.path, self.subs_path, self.c1, self.c2 = Path(path), Path(subs_path), {}, {}

    def _load(self):
        return _read(self.path, self.c1)

    def substitutions(self):
        extra = _read(self.subs_path, self.c2) or []
        return (self._load() or {}).get("substitutions", []) + (extra.get("substitutions", []) if isinstance(extra, dict) else extra)

    def chats(self):
        return (self._load() or {}).get("chats", {})

    def groups(self, cls):
        return (self._load() or {}).get("groups", {}).get(cls, {})

    def classes(self):
        return list((self._load() or {}).get("classes", {}))

    def teachers(self):
        """Фамилии учителей из расписания + список teachers (имена в Telegram и id)."""
        tt = self._load() or {}
        names = {x.get("teacher") for days in tt.get("classes", {}).values() for day in days.values() for slot in day
                 for x in (slot if isinstance(slot, list) else [slot]) if isinstance(x, dict) and x.get("teacher")}
        return {"surnames": sorted(names), "accounts": tt.get("teachers", [])}

    def lessons(self, cls, day):
        tt = self._load()
        if not tt:
            return []
        bells = tt.get("bells", {})
        plan = tt.get("classes", {}).get(cls, {}).get(DAYS[day.weekday()], []) or []
        out = []
        for num, slot in enumerate(plan, 1):
            for x in slot if isinstance(slot, list) else [slot]:
                x = {"subject": x} if isinstance(x, str) else x
                if x and x.get("subject"):
                    out.append({"num": num, "subject": x["subject"], "teacher": x.get("teacher"), "room": x.get("room"),
                                "group": x.get("group"), "status": "scheduled", "changed": False})
        for s in self.substitutions():
            if s.get("class") != cls or s.get("date") != day.isoformat():
                continue
            n = int(s["lesson"])
            old = [x for x in out if x["num"] == n and (s.get("group") is None or x["group"] == s.get("group"))]
            for x in old:
                x["status"] = "cancelled"  # как в бэкенде: старый урок отменён, на его месте новый
            if s.get("subject"):
                out.append({"num": n, "subject": s["subject"], "teacher": s.get("teacher"),
                            "room": s.get("room") or next((x["room"] for x in old if x.get("room")), None),
                            "group": s.get("group"), "status": "scheduled", "changed": True})
        res = []
        for x in sorted(out, key=lambda x: x["num"]):
            if str(x["num"]) in bells:
                start, end = bells[str(x["num"])]
                res.append({**x, "date": day.isoformat(), "class": cls, "start": start, "end": end})
        return res


class BackendSource:
    """Бэкенд EduHub. Эндпоинтов пока нет: пути ниже — предложение для Миши, поправить, когда он их сделает.
    Ответы кешируются на 10 минут, чтобы не дёргать сервер на каждое сообщение."""

    def __init__(self, url):
        self.url, self.cache = url.rstrip("/"), {}

    def _get(self, path):
        hit = self.cache.get(path)
        if hit and hit[0] > datetime.now():
            return hit[1]
        with urllib.request.urlopen(self.url + path, timeout=10) as r:
            data = json.load(r)
        self.cache[path] = (datetime.now() + timedelta(minutes=10), data)
        return data

    def chats(self):
        return self._get("/agent/chats")  # {"название чата": {"class": ..., "subjects": [...]}}

    def groups(self, cls):
        return self._get(f"/agent/classes/{cls}/groups")

    def classes(self):
        return self._get("/agent/classes")

    def teachers(self):
        return self._get("/agent/teachers")  # {"surnames": [...], "accounts": [имя в Telegram или id, ...]}

    def lessons(self, cls, day):
        return self._get(f"/agent/classes/{cls}/lessons?date={day.isoformat()}")  # список экземпляров, формат выше


SOURCE = BackendSource(os.environ["TIMETABLE_URL"]) if os.environ.get("TIMETABLE_URL") else FileSource()


# ---------- то, чем пользуется агент ----------

def chat_info(chat):
    """{"class": ..., "subjects": [...]} по названию чата (самое длинное совпадение) или None."""
    best = None
    for name, info in SOURCE.chats().items():
        if name.lower() in chat.lower() and (best is None or len(name) > len(best[0])):
            best = (name, info)
    if not best:
        return None
    info = dict(best[1])
    info["subjects"] = info.get("subjects") or ([info["subject"]] if info.get("subject") else [])
    return info


def _words(s):
    return {w.replace("ё", "е") for w in re.findall(r"[а-яёa-z]+", (s or "").lower())}


# ---------- класс и предмет из названия чата ----------

# предметы, которые узнаём в названии чата (по началу слова); порядок важен: «физкульт» раньше «физик»
SUBJECT_STEMS = [("физкульт", "Физкультура"), ("физра", "Физкультура"), ("физик", "Физика"), ("профиз", "Физика"),
                 ("алгебр", "Алгебра"), ("геометр", "Геометрия"), ("геом", "Геометрия"), ("матан", "Математический анализ"),
                 ("тервер", "Теория вероятностей"), ("вероятн", "Теория вероятностей"), ("математ", "Математика"),
                 ("матем", "Математика"), ("химия", "Химия"), ("хими", "Химия"), ("биолог", "Биология"), ("био", "Биология"),
                 ("информат", "Информатика"), ("инфа", "Информатика"), ("истори", "История"),
                 ("обществ", "Обществознание"), ("общество", "Обществознание"), ("геогр", "География"),
                 ("литерат", "Литература"), ("литра", "Литература"), ("русск", "Русский язык"), ("русяз", "Русский язык"),
                 ("английск", "Английский язык"), ("англ", "Английский язык"), ("немецк", "Немецкий язык"),
                 ("французск", "Французский язык"), ("астроном", "Астрономия"), ("эконом", "Экономика"),
                 ("право", "Право"), ("обж", "ОБЗР"), ("обзр", "ОБЗР"), ("технолог", "Технология"), ("музык", "Музыка"),
                 ("изо", "ИЗО"), ("проект", "Индивидуальный проект"), ("важн", "Разговоры о важном")]
LATIN_LETTER = dict(zip("abvgdezijklmnoprstuf", "АБВГДЕЗИЙКЛМНОПРСТУФ"))  # «11B» латиницей = 11Б
SUBJECT_STEMS += [("english", "Английский язык"), ("physic", "Физика"), ("math", "Математика"),
                  ("chem", "Химия"), ("biolog", "Биология"), ("histor", "История")]


def school_year_end(on=None):
    """Год выпуска нынешних 11-х: в 2026/27 учебном году это 2027 (с сентября считаем новый год)."""
    on = on or date.today()
    return on.year + 1 if on.month >= 9 else on.year


def parse_chat_name(chat, on=None):
    """Класс и предметы по названию чата, без timetable.json: {"class": "11Б" | "11" | None, "subjects": [...]}.
    Понимает «11Б», «10 б», «11-Б», год выпуска «2027Б» и «27Б» (в 2026/27 это 11Б), буква перед годом «В27», «В-27»,
    «V27» (= 11В) и «вып 27», «выпуск 2027» (вся параллель, буква неизвестна, класс "11").
    on — дата сообщения, от неё считается учебный год."""
    name = chat or ""
    end = school_year_end(on)
    cls = None
    m = re.search(r"(?<![\wа-яё])(?:вып|выпуск)\s*[-–]?\s*(20)?(\d\d)(?![\dа-яё])", name, re.I)
    if m:
        grade = 11 - (2000 + int(m[2]) - end)
        cls = str(grade) if 1 <= grade <= 11 else None
        name = name[:m.start()] + " " + name[m.end():]
    if not cls:  # буква перед годом выпуска: «В27» = 11В
        m = re.search(r"(?<![\wа-яё])([а-яёa-z])\s*[-–]?\s*(20)?(\d\d)(?![\dа-яё])", name, re.I)
        if m:
            grade = 11 - (2000 + int(m[3]) - end)
            letter = LATIN_LETTER.get(m[1].lower(), m[1].upper())
            cls = f"{grade}{letter}" if 1 <= grade <= 11 else None
            if cls:
                name = name[:m.start()] + " " + name[m.end():]
    if not cls:
        m = re.search(r"(?<![\dа-яё])(20)?(\d{1,2})\s*[-–]?\s*([а-яёa-z])(?![а-яёa-z])", name, re.I)
        if m:
            n, letter = int(m[2]), LATIN_LETTER.get(m[3].lower(), m[3].upper())
            grade = n if not m[1] and n <= 11 else 11 - (2000 + n - end) if m[1] or n >= 20 else None
            cls = f"{grade}{letter}" if grade and 1 <= grade <= 11 else None
            if cls:
                name = name[:m.start()] + " " + name[m.end():]
    subjects = []
    for w in re.findall(r"[а-яёa-z]+", name.lower()):
        for stem, subj in SUBJECT_STEMS:
            if w.startswith(stem) and subj not in subjects:
                subjects.append(subj)
                break
    return {"class": cls, "subjects": subjects}


def chat_meta(chat, on=None):
    """Что передать модели о чате: класс и предметы. Сначала timetable.json ("chats"), чего там нет — из названия."""
    info = chat_info(chat) or {}
    parsed = parse_chat_name(chat, on)
    return {"class": info.get("class") or parsed["class"], "subjects": info.get("subjects") or parsed["subjects"]}


def is_teacher(author, author_id=None):
    """Учитель ли автор: id или имя из списка teachers, или в имени есть фамилия учителя из расписания
    («Иван Иванов»). Возвращает причину строкой или None."""
    try:
        t = SOURCE.teachers()
    except Exception as e:  # бэкенд недоступен: не роняем агента, просто без этой проверки
        print(f"список учителей недоступен: {e}")
        return None
    for acc in t.get("accounts", []):
        acc = str(acc).strip()
        if author_id and acc == str(author_id) or acc and acc.lower() == (author or "").strip().lower():
            return f"учитель из списка ({acc})"
    words = _words(author)
    for sur in t.get("surnames", []):
        sw = _words(sur)
        if sw and len(min(sw, key=len)) >= 4 and sw <= words:
            return f"фамилия учителя из расписания ({sur})"
    return None


def lessons_on(cls, day, subjects=None, statuses=("scheduled",)):
    """Уроки, которые состоятся (с заменами), при subjects — только этих предметов и своей группы.
    statuses=("scheduled", "cancelled") — вместе с отменёнными."""
    day = date.fromisoformat(day[:10]) if isinstance(day, str) else day
    mine = SOURCE.groups(cls)
    res = []
    for les in SOURCE.lessons(cls, day):
        if les["status"] not in statuses:
            continue
        if subjects and not any(same_subject(les["subject"], s) for s in subjects):
            continue
        g = next((v for k, v in mine.items() if same_subject(k, les["subject"])), None)
        if g is not None and les.get("group") is not None and les["group"] != g:
            continue
        res.append(les)
    return res


def subjects_for(info, text):
    """Предметы находки: из чата, а в общем чате класса или чате на два предмета — по тексту."""
    subs = info["subjects"]
    if len(subs) == 1:
        return subs
    pool = subs or sorted({x["subject"] for k in range(7)
                           for x in SOURCE.lessons(info["class"], date(2026, 1, 5) + timedelta(days=k))})
    hit = [s for s in pool if mentions(text, s)]
    return hit or subs


def _dt(day, hhmm):
    h, m = map(int, hhmm.split(":"))
    return datetime(day.year, day.month, day.day, h, m)


def lesson_at(cls, subjects, when, window=WINDOW_MIN):
    """Урок, во время которого или в первые window минут после которого написали. -> (урок, как нашли) или (None, None)."""
    for les in lessons_on(cls, when.date(), subjects):
        start, end = _dt(when.date(), les["start"]), _dt(when.date(), les["end"])
        if start <= when <= end:
            return les, "во время урока"
        if end < when <= end + timedelta(minutes=window):
            return les, f"через {int((when - end).total_seconds() // 60)} мин после урока"
    return None, None


def subject_days(cls, subjects, after, horizon=21, include_first=False):
    """Дни с уроками предмета после даты after: [(дата, первый урок этого дня)]. Сдвоенный урок — один день."""
    out = []
    for k in range(0 if include_first else 1, horizon):
        d = after + timedelta(days=k)
        les = lessons_on(cls, d, subjects)
        if les:
            out.append((d, les[0]))
    return out


def describe(les):
    if not les:
        return "—"
    extra = (f", каб. {les['room']}" if les.get("room") else "") + (" (замена)" if les.get("changed") else "")
    return f"{les['date']} {les['num']} урок {les['subject']} {les['start']}–{les['end']}{extra}"


THROUGH_LESSON = re.compile(r"\bчерез\s+(?:один\s+)?урок", re.I)


def link(it, text):
    """Привязка находки к урокам по расписанию. Меняет it и возвращает True, если чат есть в расписании
    (в "chats" timetable.json или по названию чата с классом, у которого есть расписание).
    ДЗ: lesson_given — на каком уроке задали (во время урока или в первые 7 минут после него);
    у ДЗ, контрольной и изменения урока due_lesson — к какому уроку, и due_kind — «на следующий урок»,
    «через урок», «через неделю» или «к дате»."""
    sent = datetime.fromisoformat(it["sent_at"][:19])
    info = chat_info(it["chat"])
    if not info:  # чата нет в "chats": класс и предмет из названия («Обществознание 11Б»), если у класса есть расписание
        info = parse_chat_name(it["chat"], sent.date())
        try:
            info = info if info["class"] in SOURCE.classes() else None
        except Exception:
            info = None
    if not info or it["type"] not in ("homework", "test", "lesson_change"):
        return False
    cls = info["class"]
    subjects = subjects_for(info, " ".join([text, it.get("summary", ""), it.get("lesson") or ""]))
    given = None
    if it["type"] == "homework":
        given, how = lesson_at(cls, subjects or None, sent)
        if given:
            it["lesson_given"] = {**given, "how": how}
            subjects = [given["subject"]]  # чат на два предмета: ДЗ к тому предмету, на котором задали
    if not subjects:
        return False  # общий чат класса, предмет не понять: остаётся старая логика
    it["class"], it["subject"] = cls, subjects[0] if len(subjects) == 1 else " / ".join(subjects)
    days = subject_days(cls, subjects, sent.date(), include_first=it["type"] == "lesson_change")
    if it["type"] == "lesson_change":  # «урок в 207» в день урока — про сегодняшний, если он ещё не кончился
        days = [(d, l) for d, l in days if d > sent.date() or _dt(d, lessons_on(cls, d, subjects)[-1]["end"]) > sent]
    from_text = it.get("date") and it.get("date_source", "текст").startswith(("текст", "из текста"))
    if from_text:
        due_day = date.fromisoformat(it["date"])
        due = next((l for d, l in days if d == due_day), None) or (lessons_on(cls, due_day, subjects) or [None])[0]
        if not due and it["type"] == "lesson_change":  # «завтра физики не будет»: урок уже отменён в заменах
            due = (lessons_on(cls, due_day, subjects, ("scheduled", "cancelled")) or [None])[0]
    elif it.get("long_term"):
        return True  # «к концу четверти» без даты: урока не знаем
    else:
        pick = 1 if THROUGH_LESSON.search(text) else 0
        if len(days) <= pick:
            return True
        due_day, due = days[pick]
        it["date"], it["date_source"] = due_day.isoformat(), "расписание"
    it["due_lesson"] = due
    if it["type"] == "lesson_change":
        return True  # у изменения урока важен сам урок, «через неделю» тут ни к чему
    n = next((i for i, (d, _) in enumerate(days, 1) if d == due_day), None)
    base = date.fromisoformat(given["date"]) if given else sent.date()
    weeks = (due_day - base).days // 7 if (due_day - base).days % 7 == 0 else 0
    if not due:
        it["due_kind"] = "к дате (урока по расписанию в этот день нет)"
    elif n == 1:
        it["due_kind"] = "на следующий урок" + (" (через неделю)" if weeks == 1 else "")
    elif n == 2:
        it["due_kind"] = "через урок" + (f" (через {weeks} недели)" if 1 < weeks < 5 else "")
    elif weeks:
        it["due_kind"] = "через неделю" if weeks == 1 else f"через {weeks} недели" if weeks < 5 else f"через {weeks} недель"
    else:
        it["due_kind"] = f"к дате ({n}-й урок предмета)" if n else "к дате"
    return True
