"""Проверка dm.py (ДЗ из лички боту) на поддельном бэкенде и поддельном Telegram.
Запуск: python3 test_dm.py  (ни Telegram, ни Ollama, ни настоящий бэкенд не нужны).
Модель подменена: отвечает заранее заданными находками или «не отвечает»."""
import json
import os
import sys
import tempfile
import threading
import uuid
from datetime import date, datetime
from http.server import BaseHTTPRequestHandler, HTTPServer
from urllib.parse import parse_qs, urlparse

CLASS, STUDENT, TEACHER = str(uuid.uuid4()), str(uuid.uuid4()), str(uuid.uuid4())
GRAD = date.today().year + (1 if date.today().month >= 9 else 0)  # год выпуска нынешних 11-х
KEY = "test-agent-key"
USERS = {111: {"user_id": STUDENT, "login": "student.test", "role": "student", "class_id": CLASS},
         222: {"user_id": TEACHER, "login": "teacher.test", "role": "teacher", "class_id": None}}
LESSONS = {  # дата -> уроки 11Б; сообщения пишут в пн 2099-10-19
    "2099-10-20": [{"id": str(uuid.uuid4()), "start_time": "09:00", "end_time": "09:45", "subject_name": "Физика",
                    "cabinet": "305"},
                   {"id": str(uuid.uuid4()), "start_time": "09:55", "end_time": "10:40", "subject_name": "Алгебра"}],
    "2099-10-22": [{"id": str(uuid.uuid4()), "start_time": "11:00", "end_time": "11:45", "subject_name": "Физика"}],
    "2099-10-23": [{"id": str(uuid.uuid4()), "start_time": "10:00", "end_time": "10:45", "subject_name": "Алгебра"}],
}
HOMEWORK = {}  # lesson_instance_id -> {"id", "text", "created_by", "created_by_role"}


class Api(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def answer(self, code, body):
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(json.dumps(body).encode())

    def body(self):
        return json.loads(self.rfile.read(int(self.headers["Content-Length"])))

    def do_GET(self):
        u = urlparse(self.path)
        q = parse_qs(u.query)
        if u.path.startswith("/api/telegram/users/"):
            if self.headers.get("X-Agent-Key") != KEY:
                return self.answer(401, {"error": "Agent key required"})
            user = USERS.get(int(u.path.rsplit("/", 1)[1]))
            return self.answer(200, user) if user else self.answer(404, {"error": "Telegram account is not linked"})
        if u.path == "/api/classes":
            return self.answer(200, {"classes": [
                {"id": CLASS, "graduation_year": GRAD, "letter": "б", "name": f"{GRAD}б", "is_active": True}]})
        if u.path != "/api/classes/schedule/day" or q["class_id"] != [CLASS]:
            return self.answer(404, {"error": "Class not found"})
        day = q["date"][0]
        out = []
        for l in LESSONS.get(day, []):
            hw = HOMEWORK.get(l["id"])
            out.append({"cabinet": None, **l, "teachers": [], "homework": hw and {"id": hw["id"], "text": hw["text"]}})
        self.answer(200, {"date": day, "lessons": out})

    def do_POST(self):
        b = self.body()
        if b["lesson_instance_id"] in HOMEWORK:
            return self.answer(409, {"error": "Homework already exists for this lesson"})
        HOMEWORK[b["lesson_instance_id"]] = {"id": str(uuid.uuid4()), "text": b["text_content"],
                                             "created_by": b["created_by"], "created_by_role": b["created_by_role"]}
        self.answer(200, {"id": HOMEWORK[b["lesson_instance_id"]]["id"]})

    def do_PATCH(self):
        hid = self.path.split("/")[3]
        hw = next(h for h in HOMEWORK.values() if h["id"] == hid)
        hw["text"] = self.body()["text_content"] or ""
        self.answer(200, {"id": hid})


def main():
    server = HTTPServer(("127.0.0.1", 0), Api)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    os.environ.update(API_URL=f"http://127.0.0.1:{server.server_port}", AGENT_KEY=KEY, LOGIN_BOT="@EduHub179Bot")
    os.environ.pop("AGENT_USER_ID", None)
    here = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))  # корень agent/
    home = tempfile.mkdtemp()
    os.chdir(home)  # рабочие файлы — во временной папке
    os.environ["EDUHUB_AGENT_HOME"] = home  # черновики и очередь — во временной папке
    sys.path.insert(0, here)
    from eduhub_agent.telegram import chat_agent
    from eduhub_agent.telegram import dm

    sent = []  # что бот отправил в Telegram

    def tg(method, **p):
        sent.append((method, p))
        return {"message_id": 900 + len(sent)} if method == "sendMessage" else True
    dm.tg = tg
    answers = []  # что «скажет модель» на следующее сообщение; исключение — модель не ответила

    def parse(m, chat):
        a = answers.pop(0)
        if isinstance(a, Exception):
            raise a
        return chat_agent.refine(m, a)
    dm.setup("https://api.telegram.org/botTEST/", parse, moderate=False)
    pushed = []
    dm.chat_moderation.push = pushed.append

    ok = total = 0

    def check(name, cond):
        nonlocal ok, total
        total += 1
        print(("OK  " if cond else "FAIL") + " " + name)
        ok += bool(cond)

    ts = datetime(2099, 10, 19, 15, 0).timestamp()
    n = [0]

    def write(tg_id, text, answer=None):
        n[0] += 1
        sent.clear()
        if answer is not None:
            answers.append(answer)
        dm.on_message({"message_id": n[0], "date": ts, "chat": {"id": tg_id, "type": "private"},
                       "from": {"id": tg_id}, "text": text})
        return "\n".join(p.get("text", "") for _, p in sent)

    def press(tg_id, what, index=None):
        """Нажать кнопку последнего черновика: ok, no, s:<n>, d:<n>:<дата>; или кнопку по подписи."""
        d = list(dm.load().values())[-1]
        sent.clear()
        dm.on_button({"id": "cb", "from": {"id": tg_id}, "data": f"dm:{d['id']}:{what}"})
        return "\n".join(p.get("text", "") for _, p in sent if "text" in p)

    def buttons():
        kb = next((p["reply_markup"] for _, p in reversed(sent) if "inline_keyboard" in p.get("reply_markup", {})),
                  {"inline_keyboard": []})
        return [b["text"] for row in kb["inline_keyboard"] for b in row]

    phys20, alg20 = (l["id"] for l in LESSONS["2099-10-20"])
    phys22, alg23 = LESSONS["2099-10-22"][0]["id"], LESSONS["2099-10-23"][0]["id"]

    out = write(333, "физика к четвергу №5")
    check("не вошёл в основном боте: просим войти там", "@EduHub179Bot" in out and "войди" in out)

    out = write(111, "Физика к четвергу № 5–7 из задачника",
                [{"quote": "Физика к четвергу № 5–7 из задачника", "type": "homework", "summary": "№ 5–7", "date": "2099-10-22"}])
    check("ученик: понял предмет, день и урок", "Физика" in out and "чт 22.10" in out and "11:00" in out)
    check("... и спрашивает «Записать?»", "✅ Записать" in buttons() and "после проверки" not in out)
    out = press(111, "ok")
    hw = HOMEWORK.get(phys22, {})
    check("записано на урок физики в четверг", "№ 5–7" in hw.get("text", ""))
    check("автор — ученик, роль student", hw.get("created_by") == STUDENT and hw.get("created_by_role") == "student")
    check("в тексте нет «Из чата»", "Из чата" not in hw.get("text", ""))
    check("в черновике «Записал»", "Записал" in out)
    check("отдельного «✅ Записал» не пришло (уже видно в черновике)", not any(p.get("chat_id") == 111 and
          m == "sendMessage" for m, p in sent))

    out = write(111, "алгебра № 100 к следующему уроку", ConnectionError("Ollama"))
    check("модель не ответила: всё сообщение — одно ДЗ, к ближайшему уроку", "Алгебра" in out and "вт 20.10" in out
          and "ближайший" in out)
    press(111, "ok")
    check("... записано на алгебру во вторник", "№ 100" in HOMEWORK.get(alg20, {}).get("text", ""))

    out = write(111, "сделать упр. 5", [{"quote": "сделать упр. 5", "type": "homework", "summary": "упр. 5", "date": None}])
    check("предмет непонятен: кнопки с предметами класса", "Не понял предмет" in out and {"Физика", "Алгебра"} <= set(buttons())
          and "✅ Записать" not in buttons())
    d = list(dm.load().values())[-1]
    press(111, f"s:{d['subjects'].index('Физика')}")
    check("выбрал физику: ближайший урок вт 20.10", "вт 20.10" in sent[0][1]["text"] and "✅ Записать" in buttons())
    press(111, "ok")
    check("... записано", "упр. 5" in HOMEWORK.get(phys20, {}).get("text", ""))

    out = write(111, "физика к пятнице лабораторная 3",
                [{"quote": "физика к пятнице лабораторная 3", "type": "homework", "summary": "лаба 3", "date": "2099-10-23"}])
    check("в пятницу физики нет: предлагает ближайшие дни", "нет урока" in out and buttons()[:2] == ["вт 20.10", "чт 22.10"])
    press(111, "d:0:2099-10-22")
    press(111, "ok")
    t = HOMEWORK[phys22]["text"]
    check("выбрал четверг: дописано к уже записанному ДЗ", "№ 5–7" in t and "лабораторная 3" in t)

    out = write(222, "алгебра к пятнице стр. 40")
    check("учитель без класса в тексте: просим написать класс", "класс" in out and not dm.load().get("x"))
    out = write(222, "11Б алгебра к пятнице стр. 40, № 3",
                [{"quote": "алгебра к пятнице стр. 40, № 3", "type": "homework", "summary": "стр. 40, № 3", "date": "2099-10-23"}])
    check("учитель: класс из текста, урок алгебры в пятницу", "(11Б)" in out and "пт 23.10" in out)
    press(222, "ok")
    hw = HOMEWORK.get(alg23, {})
    check("автор — учитель, роль teacher", hw.get("created_by") == TEACHER and hw.get("created_by_role") == "teacher")

    out = write(111, "физика к четвергу выучить формулы",
                [{"quote": "физика к четвергу выучить формулы", "type": "homework", "summary": "формулы", "date": "2099-10-22"}])
    out = press(222, "ok")
    check("чужой черновик нажать нельзя", "не активен" in out and "формулы" not in HOMEWORK[phys22]["text"])

    dm.MODERATE = True
    out = write(111, "физика к четвергу выучить формулы",
                [{"quote": "физика к четвергу выучить формулы", "type": "homework", "summary": "формулы", "date": "2099-10-22"}])
    check("с ботом-модератором ученик видит «после проверки»", "после проверки" in out)
    out = press(111, "ok")
    check("ДЗ ученика ушло на проверку, в бэкенд пока нет", len(pushed) == 1 and "формулы" not in HOMEWORK[phys22]["text"]
          and "на проверку" in out)
    it = pushed[0]
    check("в карточке модератора есть всё для записи", it["author_user_id"] == STUDENT and it["class"] == "11Б"
          and it["subject"] == "Физика" and it["date"] == "2099-10-22")
    sent.clear()
    dm.backend_api.send("mod:1", it)  # модератор подтвердил (moderator.send_to_server)
    check("после подтверждения записано от имени ученика", "формулы" in HOMEWORK[phys22]["text"])
    check("... и ученику пришло «✅ Записал»", any(p.get("chat_id") == 111 and "✅ Записал" in p["text"] for _, p in sent))

    # кнопки внизу: ДЗ на завтра, расписание на сегодня, где следующий урок
    menu = lambda: any(p.get("reply_markup") == dm.MENU for _, p in sent)
    dm.NOW = lambda: datetime(2099, 10, 19, 15, 0)  # пн, уроков нет; завтра физика (каб. 305) и алгебра
    out = write(111, "/start")
    check("подсказка показывает кнопки внизу", menu() and "расписание" in out)
    out = write(111, dm.B_TODAY)
    check("расписание на сегодня: уроков нет", "уроков нет" in out and menu())
    HOMEWORK[alg20] = {"id": str(uuid.uuid4()), "text": "№ 101–104", "created_by": TEACHER, "created_by_role": "teacher"}
    out = write(111, dm.B_HW)
    check("ДЗ на завтра: алгебра с текстом", out.startswith("ДЗ на завтра") and "Алгебра: № 101–104" in out)
    out = write(111, dm.B_NEXT)
    check("следующий урок завтра: физика, кабинет 305", "Физика" in out and "09:00" in out and "305" in out)
    dm.NOW = lambda: datetime(2099, 10, 20, 9, 30)  # вт, идёт физика
    out = write(111, dm.B_TODAY)
    check("расписание на сегодня: оба урока с кабинетом", "09:00 Физика, каб. 305" in out and "09:55 Алгебра" in out)
    out = write(111, dm.B_NEXT)
    check("следующий урок сегодня: алгебра, кабинет не указан", "Алгебра, сегодня в 09:55" in out and "не указан" in out)
    out = write(111, dm.B_HW)
    check("ДЗ на ближайший учебный день (чт), если завтра уроков нет", out.startswith("ДЗ на чт 22.10") and "Физика:" in out)
    out = write(222, dm.B_HW)
    check("учителю кнопки пока не работают, так и говорим", "только для учеников" in out)
    out = write(111, dm.B_ADD)
    check("«Добавить ДЗ» — подсказка, черновик не создаётся", "Пришли сюда домашку" in out and "Записать" not in out)

    # вопросы своими словами: правила, а если не уверены — модель (подменена); ответ — из расписания
    dm.NOW = lambda: datetime(2099, 10, 19, 15, 0)  # пн
    asked = []

    def llm(text):
        asked.append(text)
        a = llm_answers.pop(0)
        if isinstance(a, Exception):
            raise a
        return a
    dm.questions.llm_intent = llm
    llm_answers = []
    out = write(111, "что задали по алгебре?")
    check("«что задали по алгебре?» — ДЗ ближайшей алгебры, без модели",
          out.startswith("Алгебра (завтра): № 101–104") and not asked)
    out = write(111, "расписание на четверг")
    check("«расписание на четверг» — уроки чт, дата один раз", out.strip().startswith("Чт 22.10:") and "11:00 Физика" in out
          and out.lower().count("22.10") == 1)
    out = write(111, "где физика?")
    check("«где физика?» — когда и кабинет", out.startswith("Физика, завтра в 09:00. Кабинет 305."))
    out = write(111, "когда алгерба")
    check("«когда алгерба» — опечатка, урок алгебры", out.startswith("Алгебра, завтра в 09:55"))
    out = write(111, "что по матеше?")
    check("«что по матеше?» — непонятный предмет: переспрашивает", "Не понял предмет «матеше»" in out and "Физика" in out)
    check("опечатка в сокращении: «физрв» = физкультура",
          dm.subjects_typo("когда физрв", ["Физика", "Физкультура"]) == ["Физкультура"])
    out = write(111, "дз на субботу")
    check("«дз на субботу» — уроков нет", "уроков нет" in out)
    out = write(111, "привет")
    check("«привет» — что умею, без черновика", out.startswith("Я умею") and "Записать" not in out)
    llm_answers.append({"intent": "schedule", "day": "пятница", "subject": None})
    out = write(111, "что у нас в пятницу?")
    check("непонятное правилам спрашиваем у модели: расписание пт", asked and "10:00 Алгебра" in out)
    llm_answers.append(ConnectionError("Ollama"))
    out = write(111, "а что у нас завтра?")
    check("модель не ответила — подсказка, не падаем", out.strip().startswith("Я умею"))
    answers.append([{"quote": "алгебра к пятнице № 7", "type": "homework", "summary": "№ 7", "date": "2099-10-23"}])
    out = write(111, "алгебра к пятнице № 7")
    check("задание без вопроса — по-прежнему черновик ДЗ", "✅ Записать" in buttons() and "алгебра к пятнице № 7" not in asked)
    out = write(111, "а по алгебре?")
    check("«а по алгебре?» — домашка по алгебре, без модели", out.strip().startswith("Алгебра (завтра)"))
    press(111, "no")

    # болтовня в личке: фильтр (подменён) — без модели, сразу подсказка; похожее на ДЗ — черновик
    dm.WORTH = lambda text: "главу" in text
    out = write(111, "ну и погодка сегодня жесть")
    check("болтовня без предмета и дня — подсказка, модель не вызывается", out.strip().startswith("Я умею") and not answers)
    answers.append([])
    out = write(111, "прочитай главу пять")
    check("фильтр счёл похожим на ДЗ — черновик с выбором предмета", "Не понял предмет" in out)
    press(111, "no")
    dm.WORTH = None
    answers.append([{"quote": "дз по алгебре номер 7", "type": "homework", "summary": "№ 7", "date": None}])
    out = write(111, "дз по алгебре номер 7")
    check("«дз по алгебре номер 7» — это новое ДЗ, а не вопрос", "✅ Записать" in buttons() and "Алгебра" in out)
    press(111, "no")

    # быстрый путь: короткое сообщение про один предмет — без модели, дату ставят правила
    dm.REFINE = chat_agent.refine
    dm.NOW = lambda: datetime(2099, 10, 19, 15, 0)
    out = write(111, "физика к четвергу выучить формулы 3-5")
    d = list(dm.load().values())[-1]
    check("быстрый путь: физика к чт без модели", not answers and d["items"][0]["subject"] == "Физика"
          and d["items"][0]["date"] == "2099-10-22" and "✅ Записать" in buttons())
    press(111, "no")
    out = write(111, "алгебра в пятницу контрольная")
    d = list(dm.load().values())[-1]
    check("быстрый путь: контрольная по алгебре в пт", d["items"][0]["type"] == "test" and d["items"][0]["date"] == "2099-10-23")
    press(111, "no")
    answers.append([{"quote": "физика §3", "type": "homework", "summary": "§3"},
                    {"quote": "алгебра №7", "type": "homework", "summary": "№7"}])
    out = write(111, "физика §3, алгебра №7")
    check("два предмета — всё-таки модель", not answers and len(list(dm.load().values())[-1]["items"]) == 2)
    press(111, "no")
    dm.REFINE = None

    # опечатки в дне и предмете
    dm.NOW = lambda: datetime(2099, 10, 19, 15, 0)
    dm.NOW = lambda: datetime(2099, 10, 21, 15, 0)  # ср: «завтра» был бы четверг
    out = write(111, "дз на вторний")
    check("«дз на вторний» — вторник 27.10, а не «завтра»", "В вт 27.10 уроков нет" in out)
    dm.NOW = lambda: datetime(2099, 10, 19, 15, 0)
    out = write(111, "что по алгерба?")
    check("«что по алгерба?» — алгебра", out.strip().startswith("Алгебра (завтра)"))
    out = write(111, "расписание на четвег")
    check("«расписание на четвег» — четверг", "чт 22.10" in out.lower())

    # сдвоенный урок: одно ДЗ на двух уроках показываем один раз
    real = dm.lessons_now
    dm.lessons_now = lambda cid, d: [{"id": "a", "start_time": "09:00", "subject_name": "Химия", "homework": {"text": "§5"}},
                                     {"id": "b", "start_time": "09:55", "subject_name": "Химия", "homework": {"text": "§5"}}]
    out = dm.answer_question("homework", CLASS, date(2099, 10, 20))
    check("сдвоенная химия — ДЗ один раз", out.count("Химия: §5") == 1)
    dm.lessons_now = real

    # один бот: основной бот (бот Яши) пересылает обновления агенту, тут — его JSON (из теста agent_forward.rs)
    import socket
    import urllib.error
    import urllib.request
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        port = s.getsockname()[1]
    inbox = chat_agent.listen(f"127.0.0.1:{port}")
    upd = {"message": {"chat": {"id": -1001, "is_forum": False, "title": "Физика 11Б", "type": "supergroup"},
                       "date": 1792400000, "entities": [], "from": {"first_name": "Тест", "id": 42, "is_bot": False},
                       "message_id": 5, "message_thread_id": None, "text": "дз к пятнице §12", "via_bot": None},
           "update_id": 10}

    def post(key):
        req = urllib.request.Request(f"http://127.0.0.1:{port}/telegram/update", data=json.dumps(upd).encode(),
                                     headers={"Content-Type": "application/json", "X-Agent-Key": key})
        try:
            return urllib.request.urlopen(req, timeout=5).status
        except urllib.error.HTTPError as e:
            return e.code
    check("чужой ключ: 403, в очередь не попало", post("wrong") == 403 and inbox.empty())
    check("пересланное обновление принято", post(KEY) == 200)
    got = chat_agent.next_updates(inbox, 1)
    m = got and chat_agent.from_bot(got[0]["message"])
    check("агент читает его как сообщение из чата", m and m["chat"] == "Физика 11Б" and m["text"] == "дз к пятнице §12"
          and m["author_id"] == "user42")

    print(f"{ok}/{total}")
    server.shutdown()
    raise SystemExit(0 if ok == total else 1)


if __name__ == "__main__":
    main()
