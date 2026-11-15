"""Воркер очереди: разобрать то, что бот уже принял.

Живой бот не ждёт модель: файл скачали, человеку ответили, а чтение и метки считаются
здесь — один раз или по кругу. Это тот «воркер», которого не хватало: `queue.enqueue()`
вызывается из ЛС и живых чатов, а `queue.process()` до сих пор звали только тесты.

    python3 -m eduhub_agent.files.work               # разобрать всё, что ждёт
    python3 -m eduhub_agent.files.work --loop 60     # то же раз в минуту (Ctrl+C — остановить)
    python3 -m eduhub_agent.files.work --apply       # разобрать и сразу отдать бэкенду
    python3 -m eduhub_agent.files.work --class 11Б   # класс для файлов, где его нет в пути

`--apply` вызывает `act.publish(dry_run=False)`: уходят только `accepted` + high, остальное
в `state/review.jsonl`. Без `--apply` ничего не отправляется.
"""
import argparse
import sys
import time

from eduhub_agent.files import act, queue


def run(limit=50, apply=False, class_name=None, classifier=None):
    """Разобрать очередь; с apply — отдать принятое бэкенду. -> (статистика очереди, act)."""
    stats = queue.process(limit=limit, classifier=classifier)
    print("очередь: " + ", ".join(f"{k} {v}" for k, v in stats.items() if v) or "очередь: пусто")
    if not apply:
        return stats, None
    published = act.publish(class_name=class_name, dry_run=False)
    print("в бэкенд: " + ", ".join(f"{k} {v}" for k, v in published.items() if v))
    return stats, published


def main(argv=None):
    p = argparse.ArgumentParser(description="Разобрать очередь файлов и (по желанию) отдать их бэкенду.")
    p.add_argument("--limit", type=int, default=50, help="сколько файлов за прогон")
    p.add_argument("--loop", type=int, metavar="SEC", help="повторять раз в SEC секунд")
    p.add_argument("--apply", action="store_true", help="сразу отдать accepted в бэкенд")
    p.add_argument("--class", dest="class_name", help="класс, если его нет в пути: 11Б")
    a = p.parse_args(argv)
    if not a.loop:
        run(a.limit, a.apply, a.class_name)
        return 0
    print(f"воркер: раз в {a.loop} с, Ctrl+C — остановить")
    try:
        while True:
            run(a.limit, a.apply, a.class_name)
            time.sleep(a.loop)
    except KeyboardInterrupt:
        print("\nостановлен")
    return 0


if __name__ == "__main__":
    sys.exit(main())
