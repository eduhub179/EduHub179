"""Проверка backend_api.py на поддельном сервере (те же адреса и ответы, что в backend/crates/presentation/src/router.rs).
Запуск: python3 test_backend_api.py  (ни Telegram, ни Ollama, ни настоящий бэкенд не нужны)."""
import json
import os
import tempfile
import threading
import uuid
from http.server import BaseHTTPRequestHandler, HTTPServer
from datetime import date
from urllib.parse import parse_qs, urlparse

CLASS, TEACHER = str(uuid.uuid4()), str(uuid.uuid4())
GRAD = date.today().year + (1 if date.today().month >= 9 else 0)  # год выпуска нынешних 11-х
LESSONS = {  # дата -> уроки 11Б
    "2099-10-22": [{"id": str(uuid.uuid4()), "start_time": "09:00", "end_time": "09:45", "subject_name": "Алгебра"},
                   {"id": str(uuid.uuid4()), "start_time": "09:55", "end_time": "10:40", "subject_name": "Физика"}],
    "2099-10-23": [{"id": str(uuid.uuid4()), "start_time": "11:00", "end_time": "11:45", "subject_name": "Физика"}],
}
HOMEWORK = {}  # lesson_instance_id -> {"id", "text", "created_by", "created_by_role", "status"}
DOWN = [False]


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
        if DOWN[0]:
            return self.answer(500, {"error": "Internal server error"})
        u = urlparse(self.path)
        q = parse_qs(u.query)
        if u.path == "/api/classes":
            return self.answer(200, {"classes": [
                {"id": CLASS, "graduation_year": GRAD, "letter": "б", "name": f"{GRAD}б", "is_active": True},
                {"id": str(uuid.uuid4()), "graduation_year": GRAD, "letter": "в", "name": f"{GRAD}в", "is_active": True}]})
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
        assert b["created_by"] == TEACHER and b["created_by_role"] == "teacher" and b["status"] == "published", b
        HOMEWORK[b["lesson_instance_id"]] = {"id": str(uuid.uuid4()), "text": b["text_content"]}
        self.answer(200, {"id": HOMEWORK[b["lesson_instance_id"]]["id"]})

    def do_PATCH(self):
        hid = self.path.split("/")[3]
        hw = next(h for h in HOMEWORK.values() if h["id"] == hid)
        hw["text"] = self.body()["text_content"] or ""
        self.answer(200, {"id": hid})


def main():
    server = HTTPServer(("127.0.0.1", 0), Api)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    os.environ.update(API_URL=f"http://127.0.0.1:{server.server_port}", AGENT_USER_ID=TEACHER,
                      AGENT_USER_ROLE="teacher")
    here = os.path.dirname(os.path.abspath(__file__))
    os.chdir(tempfile.mkdtemp())  # api_sent.json и api_queue.json — во временной папке
    import sys
    sys.path.insert(0, here)
    import backend_api as b

    ok = 0
    phys1, phys2 = LESSONS["2099-10-22"][1]["id"], LESSONS["2099-10-23"][0]["id"]

    def check(name, cond):
        nonlocal ok
        print(("OK  " if cond else "FAIL") + " " + name)
        ok += bool(cond)

    hw = {"type": "homework", "chat": "Физика 11Б", "class": "11Б", "subject": "Физика", "date": "2099-10-22",
          "summary": "№5 и №7 из задачника", "quote": "к четвергу №5 и №7 из задачника",
          "due_lesson": {"start": "09:55"}}
    b.send("a1", hw)
    check("новое ДЗ создаёт домашку на уроке физики", "№5 и №7" in HOMEWORK.get(phys1, {}).get("text", ""))
    check("на алгебру ничего не записалось", LESSONS["2099-10-22"][0]["id"] not in HOMEWORK)

    b.send("a1", hw)
    check("повторная отправка не дублирует текст", HOMEWORK[phys1]["text"].count("Из чата") == 1)

    test = {**hw, "type": "test", "summary": "контрольная по кинематике", "quote": "в четверг контрольная"}
    b.send("a2", test)
    t = HOMEWORK[phys1]["text"]
    check("вторая находка к тому же уроку дописывается", "№5 и №7" in t and "Контрольная: контрольная по кинематике" in t)

    b.send("a1", {**hw, "date": "2099-10-23", "due_lesson": {"start": "11:00"}})
    check("поправили срок: ДЗ ушло со старого урока", "№5 и №7" not in HOMEWORK[phys1]["text"]
          and "кинематике" in HOMEWORK[phys1]["text"])
    check("... и появилось на новом", "№5 и №7" in HOMEWORK.get(phys2, {}).get("text", ""))

    HOMEWORK[LESSONS["2099-10-22"][0]["id"]] = {"id": str(uuid.uuid4()), "text": "Учитель: стр. 40"}
    b.send("a3", {**hw, "chat": "Алгебра 11Б", "subject": "Алгебра", "summary": "№100", "quote": "№100",
                  "due_lesson": None})
    t = HOMEWORK[LESSONS["2099-10-22"][0]["id"]]["text"]
    check("домашка учителя уже есть: дописали, его текст не тронули", t.startswith("Учитель: стр. 40") and "№100" in t)

    b.send("a4", {**hw, "subject": "Химия", "summary": "химия"})
    q = b.load(b.QUEUE, [])
    check("урока нет в расписании: находка ждёт в очереди", [x["key"] for x in q] == ["a4"])

    DOWN[0] = True
    b.send("a5", {**hw, "date": "2099-10-23", "summary": "лаба 3", "quote": "лаба 3", "due_lesson": None})
    check("сервер лежит: находка в очереди", {x["key"] for x in b.load(b.QUEUE, [])} == {"a4", "a5"})
    DOWN[0] = False
    b.retry()
    check("сервер поднялся: retry отправил", "лаба 3" in HOMEWORK[phys2]["text"]
          and {x["key"] for x in b.load(b.QUEUE, [])} == {"a4"})

    b.send("a6", {**hw, "type": "info"})
    check("инфо в бэкенд не идёт", len(b.load(b.SENT, {})) == 4)

    b.send("a7", {**hw, "class": None, "chat": "Физика 10А"})
    check("класса нет в /api/classes: в очереди с причиной",
          any(x["key"] == "a7" and "/api/classes" in x["why"] for x in b.load(b.QUEUE, [])))

    b.send("a8", {**hw, "class": None, "chat": f"Физика {GRAD}Б", "summary": "лаба 4", "quote": "лаба 4"})
    check("класс по году выпуска из названия чата", "лаба 4" in HOMEWORK[phys1]["text"])

    total = 13
    print(f"{ok}/{total}")
    server.shutdown()
    raise SystemExit(0 if ok == total else 1)


if __name__ == "__main__":
    main()
