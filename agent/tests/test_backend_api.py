"""Проверка backend_api.py на поддельном сервере (те же адреса и ответы, что в backend/crates/presentation/src/router.rs).
Запуск: python3 test_backend_api.py  (ни Telegram, ни Ollama, ни настоящий бэкенд не нужны)."""
import json
import os
import re
import tempfile
import threading
import uuid
from http.server import BaseHTTPRequestHandler, HTTPServer
from datetime import date
from urllib.parse import parse_qs, urlparse

CLASS, TEACHER = str(uuid.uuid4()), str(uuid.uuid4())
KEY = "test-agent-key"  # AGENT_KEY: без него бэкенд отвечает 401
GRAD = date.today().year + (1 if date.today().month >= 9 else 0)  # год выпуска нынешних 11-х
LESSONS = {  # дата -> уроки 11Б
    "2099-10-22": [{"id": str(uuid.uuid4()), "start_time": "09:00", "end_time": "09:45", "subject_name": "Алгебра"},
                   {"id": str(uuid.uuid4()), "start_time": "09:55", "end_time": "10:40", "subject_name": "Физика"}],
    "2099-10-23": [{"id": str(uuid.uuid4()), "start_time": "11:00", "end_time": "11:45", "subject_name": "Физика"}],
}
HOMEWORK = {}  # lesson_instance_id -> {"id", "text", "created_by", "created_by_role", "status"}
DOWN = [False]
FILES = {}  # file_id -> {"lesson", "name", "mime", "bytes"}
STORAGE = ["on"]  # "on" / "off" (503, хранилище выключено) / "broken" (500)


class Api(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def answer(self, code, body):
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(json.dumps(body).encode())

    def denied(self):
        if self.headers.get("X-Agent-Key") == KEY:
            return False
        self.answer(401, {"error": "Authentication required"})
        return True

    def body(self):
        return json.loads(self.rfile.read(int(self.headers["Content-Length"])))

    def do_GET(self):
        if DOWN[0]:
            return self.answer(500, {"error": "Internal server error"})
        if self.denied():
            return
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
        if self.denied():
            return
        if self.path.startswith("/api/lessons/"):
            return self.attach(self.path.split("/")[3])
        assert self.headers.get("X-Agent-On-Behalf-Of") == TEACHER, self.headers
        b = self.body()
        if b["lesson_instance_id"] in HOMEWORK:
            return self.answer(409, {"error": "Homework already exists for this lesson"})
        assert b["created_by"] == TEACHER and b["created_by_role"] == "teacher" and b["status"] == "published", b
        HOMEWORK[b["lesson_instance_id"]] = {"id": str(uuid.uuid4()), "text": b["text_content"]}
        self.answer(200, {"id": HOMEWORK[b["lesson_instance_id"]]["id"]})

    def attach(self, lesson):
        raw = self.rfile.read(int(self.headers["Content-Length"]))
        if STORAGE[0] != "on":
            return self.answer(503 if STORAGE[0] == "off" else 500, {"error": "File storage is not configured"})
        boundary = re.search(r"boundary=(\S+)", self.headers["Content-Type"]).group(1).encode()
        part = raw.split(b"--" + boundary)[1]
        head, data = part.split(b"\r\n\r\n", 1)
        name = re.search(rb'name="file"; filename="([^"]*)"', head).group(1).decode()
        mime = re.search(rb"Content-Type: (\S+)", head).group(1).decode()
        fid = str(uuid.uuid4())
        FILES[fid] = {"lesson": lesson, "name": name, "mime": mime, "bytes": data[:-2]}
        created = lesson not in HOMEWORK
        if created:
            HOMEWORK[lesson] = {"id": str(uuid.uuid4()), "text": ""}
        self.answer(200, {"homework_id": HOMEWORK[lesson]["id"], "homework_created": created,
                          "file": {"id": fid, "file_name": name, "mime_type": mime, "size_bytes": len(data) - 2}})

    def do_DELETE(self):
        if self.denied():
            return
        fid = self.path.split("/")[4]
        if FILES.pop(fid, None) is None:
            return self.answer(404, {"error": "File not found"})
        self.answer(204, {})

    def do_PATCH(self):
        if self.denied():
            return
        hid = self.path.split("/")[3]
        hw = next(h for h in HOMEWORK.values() if h["id"] == hid)
        hw["text"] = self.body()["text_content"] or ""
        self.answer(200, {"id": hid})


def main():
    server = HTTPServer(("127.0.0.1", 0), Api)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    os.environ.update(API_URL=f"http://127.0.0.1:{server.server_port}", AGENT_USER_ID=TEACHER,
                      AGENT_USER_ROLE="teacher", AGENT_KEY=KEY)
    here = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))  # корень agent/
    home = tempfile.mkdtemp()
    os.chdir(home)  # рабочие файлы — во временной папке
    os.environ["EDUHUB_AGENT_HOME"] = home  # api_sent.json и api_queue.json — во временной папке
    import sys
    sys.path.insert(0, here)
    from eduhub_agent.server import backend_api as b

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
    check("вторая находка к тому же уроку дописывается", "№5 и №7" in t and "Контрольная по кинематике" in t and "Контрольная: контрольная" not in t)

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

    # вложения: файл из чата грузится к тому же уроку
    def files_of(lesson):
        return sorted(f["name"] for f in FILES.values() if f["lesson"] == lesson)

    os.makedirs("chats/files/Физика 11Б", exist_ok=True)
    pdf = "chats/files/Физика 11Б/9_задачи.pdf"  # относительный путь, как пишет chat_agent.py
    open(pdf, "wb").write(b"%PDF-1.4 tasks")
    alg = LESSONS["2099-10-22"][0]["id"]
    fhw = {**hw, "chat": "Алгебра 11Б", "subject": "Алгебра", "summary": "задачи из файла", "quote": "задачи в файле",
           "due_lesson": None, "files": ["задачи.pdf"], "file_paths": [pdf]}
    b.send("f1", fhw)
    f = next((x for x in FILES.values() if x["lesson"] == alg), {})
    check("файл из чата загружен к уроку", f.get("name") == "9_задачи.pdf" and f.get("bytes") == b"%PDF-1.4 tasks"
          and f.get("mime") == "application/pdf" and "задачи из файла" in HOMEWORK[alg]["text"])
    b.send("f1", fhw)
    check("повторная отправка не грузит файл второй раз", files_of(alg) == ["9_задачи.pdf"])

    b.send("f1", {**fhw, "date": "2099-10-23", "subject": "Физика"})
    check("поправили срок: файл ушёл со старого урока и появился на новом",
          files_of(alg) == [] and files_of(phys2) == ["9_задачи.pdf"])

    STORAGE[0] = "off"
    b.send("f2", {**fhw, "summary": "ещё задачи", "quote": "ещё задачи"})
    check("хранилище выключено (503): текст записан, в очереди не висит",
          "ещё задачи" in HOMEWORK[alg]["text"] and files_of(alg) == []
          and "f2" not in {x["key"] for x in b.load(b.QUEUE, [])})
    STORAGE[0] = "on"
    b.send("f2", {**fhw, "summary": "ещё задачи", "quote": "ещё задачи"})
    check("хранилище включили: файл догрузился", files_of(alg) == ["9_задачи.pdf"])

    STORAGE[0] = "broken"
    b.send("f3", {**fhw, "date": "2099-10-23", "subject": "Физика", "summary": "лаба 5", "quote": "лаба 5",
                  "file_paths": [pdf, "chats/files/нет такого.pdf"]})
    q = {x["key"]: x["why"] for x in b.load(b.QUEUE, [])}
    check("ошибка сервера на файле: текст записан, находка ждёт в очереди",
          "лаба 5" in HOMEWORK[phys2]["text"] and "файл" in q.get("f3", ""))
    STORAGE[0] = "on"
    b.retry()
    check("retry догрузил файл, пропавший файл пропущен, очередь чистая",
          files_of(phys2) == ["9_задачи.pdf", "9_задачи.pdf"] and "f3" not in {x["key"] for x in b.load(b.QUEUE, [])}
          and HOMEWORK[phys2]["text"].count("лаба 5") == 1)

    total = 20
    print(f"{ok}/{total}")
    server.shutdown()
    raise SystemExit(0 if ok == total else 1)


if __name__ == "__main__":
    main()
