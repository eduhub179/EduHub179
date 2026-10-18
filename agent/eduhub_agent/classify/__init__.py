"""Классификатор материалов — тот самый, из трека P2, без переписывания.

Здесь лежит код из `eduhub-eval/scripts/run_identify.py` (модель + JSON-схема) и
`eduhub-eval/prompts/material_identify.md` (промпт), перенесённый как есть: те же
константы, тот же разбор ответа, тот же промпт. Меняются только пути и то, что
вызывается он не по строке файла, а по папке и имени.

Почему модель, а не правила: правила в треке писались ad hoc и в репозиторий не попали,
их код не существует. Модель — существует, измерена (subject 84%, type 86%, joint 73% на
287 размеченных файлах, 10 с на файл) и настраивается промптом, а не правкой Python.

Модель: любая в Ollama, по умолчанию qwen2.5:7b (как у агента чатов).
"""
import json
import os
import time
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
DEFAULT_PROMPT = HERE / "prompt.md"

SUBJECTS = ["algebra", "geometry", "calculus", "digital_tech", "programming"]
TYPES = ["worksheet", "homework", "test", "independent_work", "notes",
         "textbook", "presentation", "plusnik_sheet", "program", "other"]
SCHEMA = {
    "type": "object",
    "properties": {
        "subject": {"type": "string", "enum": SUBJECTS},
        "type": {"type": "string", "enum": TYPES},
        "lesson": {"type": ["integer", "null"]},
        "date": {"type": ["string", "null"]},
    },
    "required": ["subject", "type"],
}

URL = os.environ.get("OLLAMA_URL", "http://localhost:11434")
MODEL = os.environ.get("CLASSIFY_MODEL", os.environ.get("MODEL", "qwen2.5:7b"))
_probe = None


def health(base=None):
    """Список моделей в Ollama или None, если её нет. Кешируется на процесс."""
    global _probe
    base = base or URL
    if _probe is None:
        try:
            with urllib.request.urlopen(base + "/api/tags", timeout=5) as r:
                _probe = [m["name"] for m in json.load(r).get("models", [])]
        except Exception:
            _probe = []
    return _probe


def available():
    """Точное имя модели, без совпадения по префиксу: «qwen2.5:3b» не заменяет «qwen2.5:7b».

    Иначе Ollama отвечает 404 уже на запросе, и агент молча получает «unknown» вместо
    внятной причины. Пусть причина называется сразу.
    """
    models = health() or []
    return MODEL in models, MODEL


def ask(base, model, row, prompt, num_ctx=4096, num_predict=80, timeout=300, schema=None):
    """Один запрос к Ollama. Код из run_identify.py, без изменений в семантике."""
    payload = {
        "model": model, "stream": False, "think": False,
        "format": schema or SCHEMA,
        "options": {"temperature": 0, "seed": 179, "num_ctx": num_ctx,
                    "num_predict": num_predict, "top_p": 1.0},
        "messages": [{"role": "user", "content": prompt + "\n\n## Document\n"
                      + json.dumps(row, ensure_ascii=False)}],
    }
    req = urllib.request.Request(base + "/api/chat", data=json.dumps(payload).encode(),
                                 headers={"Content-Type": "application/json"})
    t0 = time.time()
    with urllib.request.urlopen(req, timeout=timeout) as r:
        obj = json.load(r)
    ms = int((time.time() - t0) * 1000)
    return obj.get("message", {}).get("content", ""), ms, obj


def parse(text):
    """Строгий разбор: весь ответ — один JSON с допустимыми значениями, иначе None."""
    try:
        o = json.loads(text.strip())
    except Exception:
        return None
    if not isinstance(o, dict):
        return None
    if o.get("subject") not in SUBJECTS or o.get("type") not in TYPES:
        return None
    lesson = o.get("lesson")
    if not isinstance(lesson, int) or isinstance(lesson, bool):
        lesson = None
    date = o.get("date")
    if not isinstance(date, str):
        date = None
    return {"subject": o["subject"], "type": o["type"], "lesson": lesson, "date": date}


def classify(path, name="", text="", timeout=300, retries=1):
    """Предмет и тип материала. Возвращает те же поля, что и раньше, плюс confidence.

    Нет Ollama или модель не ответила — subject="unknown", type="other", confidence="none":
    файл уходит человеку в review.csv, а не получает выдуманный предмет.
    """
    ok, model = available()
    if not ok:
        return {"subject": "unknown", "type": "other", "grade": None, "lesson": None, "date": None,
                "subject_conf": "none", "type_conf": "none", "confidence": "none",
                "engine": f"ollama:{model}:unavailable"}

    row = {"name": name or Path(path).name,
           "folder": str(Path(path).parent) if path and "/" in str(path) else "",
           "text": (text or "")[:6000]}
    prompt = DEFAULT_PROMPT.read_text(encoding="utf-8")
    for _ in range(retries + 1):
        try:
            txt, ms, _ = ask(URL, model, row, prompt, timeout=timeout)
        except Exception:
            continue
        got = parse(txt)
        if got:
            return {**got, "grade": None,
                    "subject_conf": "high", "type_conf": "high", "confidence": "high",
                    "engine": f"ollama:{model}", "ms": ms}
    return {"subject": "unknown", "type": "other", "grade": None, "lesson": None, "date": None,
            "subject_conf": "none", "type_conf": "none", "confidence": "none",
            "engine": f"ollama:{model}:invalid"}
