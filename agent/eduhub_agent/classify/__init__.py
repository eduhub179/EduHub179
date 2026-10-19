"""Классификатор материалов — модель из трека P2, но с полным списком школы.

Код запроса и строгий разбор ответа перенесены из `eduhub-eval/scripts/run_identify.py`;
промпт — из `eduhub-eval/prompts/material_identify.md`. Отличие одно: списки предметов и
типов больше не зашиты в промпт по-отдельности.

В P2 измеряли только математику, поэтому в схеме было пять предметов. В живом агенте
подсказка может дать физику или историю, и модель обязана уметь вернуть тот же предмет,
иначе конфликт «подсказка vs модель» возникает на ровном месте. Теперь:

  * предметы — `files/taxonomy.SUBJECTS` (полный список школы + русские синонимы);
  * типы — `TYPE_HINTS` ниже, один список и для схемы, и для промпта;
  * `unknown` — разрешённый ответ предмета: если ни текст, ни имя не говорят ничего,
    модель обязана сказать `unknown`, а не выбрать ближайший похожий предмет.

Промпт (`prompt.md`) — шаблон: списки подставляются из этих констант, чтобы не было
второго места, где список надо обновлять.

Модель: любая в Ollama, по умолчанию qwen2.5:7b (как у агента чатов).
"""
import json
import os
import time
import urllib.request
from pathlib import Path

from eduhub_agent.files import taxonomy

HERE = Path(__file__).resolve().parent
DEFAULT_PROMPT = HERE / "prompt.md"

# Полный список школы из одного места. `unknown` — не предмет, а честный ответ модели;
# `misc` — не учебный материал (классный час, объявления).
SCHOOL_SUBJECTS = list(taxonomy.SUBJECTS)
SUBJECTS = SCHOOL_SUBJECTS + ["unknown"]

TYPE_HINTS = {
    "worksheet": "лист упражнений: «Листок 13», «Задачки», задачи для решения",
    "homework": "сдаваемое задание: «ДЗ», «домашняя работа», «дом_п»",
    "test": "контрольная работа, зачёт, тест, проверочная",
    "independent_work": "самостоятельная работа (СР), отработка",
    "lab": "лабораторная работа, лабораторка, практикум",
    "notes": "конспект, лекция, теория, справочный текст без заданий",
    "textbook": "учебник, задачник, пособие, сборник задач",
    "presentation": "презентация, слайды",
    "plusnik_sheet": "«Плюсник»: таблица с оценками и номерами листков, а не задания",
    "essay": "сочинение, эссе, изложение, доклад, реферат",
    "project": "проект, проектная работа",
    "answers": "готовые ответы, решения, ключи к заданиям",
    "program": "исходный код (.py, .ipynb) или файл данных к программе",
    "other": "ничего из перечисленного",
}
TYPES = list(TYPE_HINTS)
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


def _subject_lines():
    """Строки промпта из taxonomy: код — русские синонимы. Одно место правки."""
    lines = []
    for code in SCHOOL_SUBJECTS:
        if code == "misc":
            desc = "не учебный материал: классный час, объявления, разное"
        else:
            desc = ", ".join(taxonomy.SYNONYMS.get(code, [])) or code
        lines.append(f"- `{code}` — {desc}")
    lines.append("- `unknown` — ни текст, ни имя не позволяют определить предмет; не угадывать")
    return "\n".join(lines)


def _type_lines():
    return "\n".join(f"- `{code}` — {hint}" for code, hint in TYPE_HINTS.items())


_prompt_cache = None


def prompt_text():
    """Промпт с полными списками. Кешируется: файл читается один раз на процесс."""
    global _prompt_cache
    if _prompt_cache is None:
        template = DEFAULT_PROMPT.read_text(encoding="utf-8")
        _prompt_cache = (template.replace("{{SUBJECTS}}", _subject_lines())
                                 .replace("{{TYPES}}", _type_lines()))
    return _prompt_cache


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
    prompt = prompt_text()
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
