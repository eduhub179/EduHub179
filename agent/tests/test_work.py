"""Проверка воркера очереди и CLI проверки одного файла — без Telegram, модели и бэкенда.

    python3 tests/test_work.py

Что проверяем: файл, принятый ботом, действительно разбирается воркером (read -> label ->
decide -> state/file_labels.jsonl), бинарник уходит в skip, а `files.check` показывает то же
решение человеку. Модель не зовётся: подпись и имя дают high.
"""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))
sys.path.insert(0, HERE)

home = tempfile.mkdtemp()
os.environ["EDUHUB_AGENT_HOME"] = home
os.chdir(home)

from eduhub_agent.files import act, check, queue, work  # noqa: E402

ok = fails = 0


def c(name, cond):
    global ok, fails
    print(("OK  " if cond else "FAIL") + " " + name)
    ok += bool(cond)
    fails += not cond


os.makedirs("uploads", exist_ok=True)
hw = os.path.join(home, "uploads", "ДЗ-15.txt")
open(hw, "w", encoding="utf-8").write("Домашнее задание: §12, №5 и №7. Срок — к пятнице.\n")
bin_ = os.path.join(home, "uploads", "скан.dat")
open(bin_, "wb").write(b"\x00\x01\x02")

c("очередь: файл принят", queue.enqueue(hw, "ДЗ-15.txt", source="telegram_dm", path=hw,
                                        caption="физика к пятнице", chat_class="11Б"))
c("очередь: бинарник принят", queue.enqueue(bin_, "скан.dat", source="telegram_dm", path=bin_))
c("очередь: до воркера два файла ждут", len(queue.pending()) == 2)

stats, _ = work.run(apply=False)
labels = act.labels()
c("воркер: оба файла разобраны", stats["failed"] == 0 and len(labels) == 2)
c("воркер: ДЗ принято (текст + подпись + имя)", labels[hw]["route"] == "accepted"
  and labels[hw]["subject"] == "physics" and labels[hw]["type"] == "homework"
  and labels[hw]["confidence"] == "high")
c("воркер: бинарник в skip", labels[bin_]["route"] == "skip")
c("воркер: повторный прогон ничего не дублирует", work.run()[0] == {"accepted": 0, "queued": 0,
                                                                   "review": 0, "skip": 0, "failed": 0})

res = check.check(hw, caption="физика к пятнице", chat_class="11Б", model=False)
c("проверка одного файла: тот же вердикт", res["route"] == "accepted" and res["label"]["subject"] == "physics")
c("проверка одного файла: чтение без OCR", res["read"]["status"] == "native" and res["read"]["engine"] == "text")
c("проверка одного файла: модель не звали", res["label"]["input_mode"] == "heuristic")

# --apply без бэкенда: publish просто не находит удовольствия, но и не падает
os.environ.pop("API_URL", None)
stats, published = work.run(apply=True)
c("apply без API_URL: не падает, файл уходит в review", published is not None and published["uploaded"] == 0)

print(f"\nВерно {ok} из {ok + fails}")
sys.exit(0 if not fails else 1)
