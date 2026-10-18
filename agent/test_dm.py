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
    "2099-10-20": [{"id": str(uuid.uuid4()), "start_time": "09:00", "end_time": "09:45", "subject_name": "Физика"},
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
            out.append({**l, "teachers": [], "cabinet": None, "homework": hw and {"id": hw["id"], "text": hw["text"]}})
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
    here = os.path.dirname(os.path.abspath(__file__))
    os.chdir(tempfile.mkdtemp())  # черновики и очередь — во временной папке
    sys.path.insert(0, here)
    import chat_agent
    import dm

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
        kb = next((p["reply_markup"] for _, p in reversed(sent) if p.get("reply_markup")), {"inline_keyboard": []})
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
