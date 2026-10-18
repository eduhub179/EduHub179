"""Фото доски или нет: маленькая модель для картинок через Ollama отвечает «да/нет».

Правило агента (решил Саша): фото без подписи, на котором нет печатного текста, считаем ДЗ, если это фото доски
с записями (в школе в основном белые маркерные доски), а иначе пропускаем.
Модель: BOARD_MODEL, по умолчанию qwen2.5vl:3b (3.2 ГБ, один раз: ollama pull qwen2.5vl:3b), ~11 с на фото.
moondream (1.7 ГБ) проверили 2026-10-16 на 25 фото: ни одной доски не нашла, не годится. С qwen2.5:7b в 8 ГБ вместе
qwen2.5vl:3b не помещается, Ollama грузит их по очереди; фото без подписи приходят редко, так что это терпимо.
Ответы запоминаются в state/board_cache.json, чтобы одно фото не проверять дважды, чтобы одно фото не проверять дважды.
"""
import base64
import hashlib
import json
import os
import urllib.request
from pathlib import Path

from eduhub_agent.paths import STATE

BOARD_MODEL = os.environ.get("BOARD_MODEL", "qwen2.5vl:3b")
URL = os.environ.get("OLLAMA_URL", "http://localhost:11434") + "/api/generate"
CACHE = STATE / "board_cache.json"
PROMPT = ("Is this a photo of a classroom board (usually a white marker whiteboard, sometimes a chalkboard) "
          "with handwritten notes, formulas, tasks or a homework assignment written on it? Answer only yes or no.")
_cache = None


def _load():
    global _cache
    if _cache is None:
        try:
            _cache = json.load(open(CACHE, encoding="utf-8"))
        except (OSError, ValueError):
            _cache = {}
    return _cache


def is_board(path):
    """True/False, или None, если модель недоступна (тогда агент решает по-старому)."""
    data = Path(path).read_bytes()
    key = BOARD_MODEL + ":" + hashlib.md5(data).hexdigest()
    cache = _load()
    if key in cache:
        return cache[key]
    body = {"model": BOARD_MODEL, "prompt": PROMPT, "images": [base64.b64encode(data).decode()],
            "stream": False, "options": {"temperature": 0, "num_predict": 5}}
    req = urllib.request.Request(URL, data=json.dumps(body).encode(), headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=180) as r:
            answer = json.loads(r.read()).get("response", "").strip().lower()
    except Exception as e:
        print(f"   модель для фото ({BOARD_MODEL}) недоступна: {e}. Поставь: ollama pull {BOARD_MODEL}")
        return None
    verdict = answer.startswith(("yes", "да"))
    cache[key] = verdict
    try:
        STATE.mkdir(exist_ok=True)
        json.dump(cache, open(CACHE, "w", encoding="utf-8"))
    except OSError:
        pass
    return verdict
