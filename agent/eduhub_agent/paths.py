"""Где что лежит: одна точка правды вместо Path(__file__).parent в каждом файле.

Пакет можно запускать откуда угодно — рабочие файлы (chats/, tessdata, .env, timetable.json)
всегда лежат в корне agent/, а не рядом с модулем.
"""
import os
from pathlib import Path

# EDUHUB_AGENT_HOME переопределяет корень: тесты и прогоны пишут во временную папку,
# а не в рабочие api_queue.json и chats/.
ROOT = Path(os.environ.get("EDUHUB_AGENT_HOME") or Path(__file__).resolve().parent.parent)
CHATS = ROOT / "chats"                          # личные данные и кеши, в git не попадают
TESSDATA = ROOT / "tessdata"                    # русский язык для Tesseract
ENV_FILE = ROOT / ".env"


def data(name):
    """Файл в корне agent/: timetable.json, events.json, api_queue.json, posts.json..."""
    return ROOT / name
