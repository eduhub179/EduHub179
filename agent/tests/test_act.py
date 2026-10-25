"""Проверка files/act.py: вердикт классификатора действительно доходит до бэкенда.

Поддельный бэкенд — тот же, что в test_backend_api.py (классы, расписание, POST файла к уроку).
Проверяем ровно то, что обещает act.publish(): грузим только accepted (или подтверждённые
человеком), без даты и класса — в review, повторный прогон не грузит второй раз, 413 не
повторяем, dry-run ничего не отправляет.

Запуск: python3 tests/test_act.py  (ни Telegram, ни Ollama, ни настоящий бэкенд не нужны).
"""
import json
import os
import sys
import tempfile
import threading
from datetime import date
from http.server import HTTPServer

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))
sys.path.insert(0, HERE)

import test_backend_api as fake  # noqa: E402  — поддельный сервер и его состояние

fake.STORAGE[0] = "on"


def main():
    server = HTTPServer(("127.0.0.1", 0), fake.Api)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    os.environ.update(API_URL=f"http://127.0.0.1:{server.server_port}", AGENT_USER_ID=fake.TEACHER,
                      AGENT_USER_ROLE="teacher", AGENT_KEY=fake.KEY)
    home = tempfile.mkdtemp()
    os.chdir(home)
    os.environ["EDUHUB_AGENT_HOME"] = home

    from eduhub_agent.files import act, queue, taxonomy
    from eduhub_agent.server import backend_api as b

    ok = fails = 0

    def check(name, cond):
        nonlocal ok, fails
        print(("OK  " if cond else "FAIL") + " " + name)
        ok += bool(cond)
        fails += not cond

    def reset(*paths):
        for p in paths:
            if p.exists():
                p.unlink()

    def label(fid, **kw):
        row = {"id": fid, "name": f"{fid}.pdf", "path": f"{home}/{fid}.pdf", "status": "done",
               "route": "accepted", "confidence": "high", "subject": "physics", "type": "homework",
               "class": "11Б", "date": "2099-10-22", "chat": "ЛС"}
        row.update(kw)
        act._write(act.LABELS, row)
        open(row["path"], "wb").write(b"%PDF-1.4 tasks")
        return row

    reset(act.LABELS, act.ACTIONS, act.REVIEW)

    # 1. dry-run: показывает, но не грузит
    label("f1")
    st = act.publish(class_name="11Б", dry_run=True)
    check("dry-run: файл намечен", st["planned"] == 1 and st["uploaded"] == 0)
    check("dry-run: бэкенд не тронут", not fake.FILES)

    # 2. боевой прогон: файл уходит к уроку физики
    alg, phys = (l["id"] for l in fake.LESSONS["2099-10-22"])
    st = act.publish(dry_run=False)
    check("accepted: файл загружен", st["uploaded"] == 1)
    check("accepted: урок найден по предмету и дате",
          [f["lesson"] for f in fake.FILES.values()] == [phys])

    # 3. повторный прогон не дублирует
    st = act.publish(dry_run=False)
    check("повтор: второй раз не грузим", st["uploaded"] == 0 and st["skipped"] == 1 and len(fake.FILES) == 1)

    # 4. нет даты — в review, бэкенд не тронут
    label("f2", date=None)
    st = act.publish(dry_run=False)
    kinds = {r["id"]: r["why"] for r in act.review_rows()}
    check("без даты и без уроков в расписании: review",
          st["review"] == 1 and "расписании" in kinds["f2"])
    check("без даты: файл не загружен", len(fake.FILES) == 1)

    # 5. medium/review без подтверждения — не грузим; с confirmed — грузим (человек посмотрел)
    label("f3", route="review", confidence="medium", route_reason="метки неполные")
    st = act.publish(dry_run=False)
    check("review: ждёт человека", st["review"] == 1 and st["uploaded"] == 0 and len(fake.FILES) == 1)
    st = act.publish(dry_run=False, confirmed=("f3",))
    check("review + подтверждено: грузим", st["uploaded"] == 1 and len(fake.FILES) == 2)

    # 6. skip и queued не трогаем вовсе
    label("f4", route="skip")
    label("f5", route="queued")
    st = act.publish(dry_run=False)
    check("skip/queued: passthrough", st["uploaded"] == 0 and st["skipped"] >= 3 and len(fake.FILES) == 2)

    # 7. 413 — бэкенд не примет никогда: refused, второго раза нет
    label("f6")
    real = b.upload
    b.upload = lambda *a, **k: (_ for _ in ()).throw(b.ApiError(413, "too large"))
    try:
        st = act.publish(dry_run=False)
        check("413: помечен refused", st["refused"] == 1)
        st = act.publish(dry_run=False)
        check("413: второй раз не пробуем", st["refused"] == 0 and st["skipped"] >= 1)
    finally:
        b.upload = real

    # 8. 503 — хранилище выключено: файл ждёт, повторяем позже
    label("f7")
    real = b.upload
    b.upload = lambda *a, **k: (_ for _ in ()).throw(b.ApiError(503, "no storage"))
    try:
        st = act.publish(dry_run=False)
        check("503: ждёт следующего прогона", st["waiting"] == 1)
        st = act.publish(dry_run=False)
        check("503: повторяем", st["waiting"] == 1)
    finally:
        b.upload = real

    # 8b. даты нет, но есть класс, предмет и время появления файла: расписание решает
    from datetime import datetime
    seen = datetime(2099, 10, 20, 15, 0).timestamp()   # вторник, физика была бы 22-го
    row = label("f8", date=None)
    os.utime(row["path"], (seen, seen))
    from eduhub_agent.files import due, queue as q2
    q2.enqueue("f8", "f8.pdf", source="folder", path=row["path"], chat_class="11Б")
    check("queue: время появления файла взято из mtime",
          q2._rows(q2.QUEUE)[0]["seen_at"].startswith("2099-10-20"))
    it, why = act.target({**row, "path": row["path"], "seen_at": "2099-10-20T15:00:00"}, class_name="11Б")
    check("без даты, но с расписанием: дата от mtime",
          it is not None and it["date"] == "2099-10-22" and it["date_source"] == "расписание")

    # 8c. «через урок» в имени — берём не ближайший, а следующий урок предмета
    when = datetime(2099, 10, 20, 15, 0)
    one = due.candidates("11Б", "Физика", when)
    two = due.due_for({"name": "ДЗ через урок.pdf", "class": "11Б", "subject": "Физика",
                       "seen_at": when.isoformat()}, subject="Физика")
    check("«через урок»: второй урок предмета", len(one) >= 2 and two[0] == one[1][0]
          and "через урок" in two[2])

    # 8d. файл появился после урока — этот урок не кандидат
    during = datetime(2099, 10, 22, 11, 20)
    check("урок уже идёт: не он, а следующий", due.candidates("11Б", "Физика", during)[0][0]
          != during.date())

    # 9. код предмета -> русское название для find_lesson()
    check("taxonomy.ru_name", taxonomy.ru_name("physics") == "Физика" and taxonomy.ru_name("nope") is None)

    # 10. очередь дописывает контекст, а не теряет подпись
    reset(queue.QUEUE, queue.LABELS)
    check("queue: первый enqueue добавляет", queue.enqueue("/tmp/a.pdf", "a.pdf", path="/tmp/a.pdf") is True)
    check("queue: второй enqueue не дублирует", queue.enqueue("/tmp/a.pdf", "a.pdf", path="/tmp/a.pdf") is False)
    check("queue: подпись дописалась", queue.enqueue("/tmp/a.pdf", "a.pdf", caption="физика к пятнице",
                                                     chat_class="11Б") is True)
    row = queue._rows(queue.QUEUE)[0]
    check("queue: одна строка, контекст на месте",
          len(queue._rows(queue.QUEUE)) == 1 and row["caption"] == "физика к пятнице" and row["chat_class"] == "11Б")

    print(f"\nВерно {ok} из {ok + fails}")
    server.shutdown()
    return 0 if not fails else 1


if __name__ == "__main__":
    raise SystemExit(main())
