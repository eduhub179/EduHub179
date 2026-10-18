"""Провайдеры распознавания: один интерфейс, разные движки.

Сейчас работает только Tesseract — офлайн, бесплатно, хорошо читает печатный текст.
Облачный OCR (Яндекс Vision или подобный) и локальная модель для картинок объявлены,
но намеренно не подключены: `available()` возвращает False, ключей и сети нет.

Зачем так: всё, что движки должны были бы прочитать, попадает в очередь
`chats/ocr_queue.jsonl`. Подключить новый движок потом — это один класс, а не переделка
чтения вложений: `ocr_triage.read_image()` спрашивает провайдеров по очереди.

    class Provider:
        name: str
        kind: "local" | "remote"
        def available(self) -> tuple[bool, str]   # (работает, почему нет)
        def read(self, path) -> Reading | None
"""
import os
import re
import shutil
import subprocess
import time
from dataclasses import dataclass
from pathlib import Path

from eduhub_agent.paths import TESSDATA as PATHS_TESSDATA

TESSDATA = PATHS_TESSDATA


@dataclass
class Reading:
    """Что движок увидел на картинке. Уверенность считается по словам, которые Tesseract разобрал."""
    text: str
    words: int
    numbers: int
    mean_conf: float
    psm: int
    ms: int
    engine: str
    axis_like: bool = False   # цифры идут одной монотонной строкой — это подписи оси графика, не таблица


class Provider:
    name = "base"
    kind = "local"

    def available(self):
        return False, "не реализован"

    def read(self, path):
        raise NotImplementedError


class LocalTesseract(Provider):
    """Tesseract 5, два режима разметки.

    psm 3 — обычный текст в несколько колонок, psm 6 — один ровный блок. На фотографиях
    листков psm 6 иногда спасает страницу, которую psm 3 не разобрал вовсе, поэтому второй
    проход делается только если первый не прошёл проверку (это дешевле, чем всегда).
    """

    name = "local_tesseract"
    kind = "local"

    def __init__(self, lang="rus+eng", timeout=120):
        self.lang = lang
        self.timeout = timeout

    def available(self):
        exe = shutil.which("tesseract")
        if not exe:
            return False, "нет Tesseract (brew install tesseract)"
        env = self._env()
        out = subprocess.run([exe, "--list-langs"], capture_output=True, env=env)
        langs = out.stdout.decode("utf-8", "ignore").split()
        if "rus" not in langs:
            return False, "нет русского языка (brew install tesseract-lang или tessdata рядом)"
        return True, "ok"

    def _env(self):
        env = dict(os.environ)
        if (TESSDATA / "rus.traineddata").exists():
            env["TESSDATA_PREFIX"] = str(TESSDATA)
        return env

    def read(self, path, psm=3):
        exe = shutil.which("tesseract")
        if not exe:
            return None
        t0 = time.time()
        r = subprocess.run([exe, str(path), "stdout", "-l", self.lang, "--psm", str(psm), "tsv"],
                           capture_output=True, timeout=self.timeout, env=self._env())
        if r.returncode:
            return None
        text, words, numbers, conf, axis = _parse_tsv(r.stdout.decode("utf-8", "replace"))
        return Reading(text=text, words=words, numbers=numbers, mean_conf=conf, psm=psm,
                       ms=int((time.time() - t0) * 1000), engine="tesseract-5/" + self.lang,
                       axis_like=axis)


class CloudOCR(Provider):
    """Место для облачного OCR (Яндекс Vision читает и рукописный текст, и формулы).

    Не подключён намеренно: нет ключа, и наружу из школы ничего не уходит.
    Когда появится — `read()` отдаёт такой же Reading, остальной код не меняется.
    """

    name = "cloud_ocr"
    kind = "remote"

    def __init__(self, endpoint=None, api_key=None):
        self.endpoint = endpoint or os.environ.get("OCR_URL")
        self.api_key = api_key or os.environ.get("OCR_KEY")

    def available(self):
        if not (self.endpoint and self.api_key):
            return False, "нет OCR_URL/OCR_KEY"
        return False, "выключен до решения по данным учеников"


class LocalVLM(Provider):
    """Место для локальной модели, которая читает рукописное (qwen2.5vl:3b уже стоит для досок).

    Включится, если понадобится текст с доски, а не только ответ «доска/не доска».
    """

    name = "local_vlm"
    kind = "local"

    def __init__(self, model=None):
        self.model = model or os.environ.get("VLM_MODEL", "qwen2.5vl:3b")

    def available(self):
        return False, "пока только определяет доску (board.py), текст не пишет"


REGISTRY = {p.name: p for p in (LocalTesseract(), CloudOCR(), LocalVLM())}


def get(name):
    if name not in REGISTRY:
        raise KeyError(f"нет провайдера '{name}' (есть: {', '.join(REGISTRY)})")
    return REGISTRY[name]


NUM = re.compile(r"^[-−]?\d+(?:[.,]\d+)?$")


def _is_axis(tokens):
    """Подписи оси графика: пять и больше чисел подряд, по возрастанию или убыванию."""
    nums = []
    for t in tokens:
        if NUM.match(t):
            try:
                nums.append(float(t.replace(",", ".").replace("−", "-")))
            except ValueError:
                pass
    if len(nums) < 5:
        return False
    return nums == sorted(nums) or nums == sorted(nums, reverse=True)


def _parse_tsv(tsv):
    """Текст, число слов, число чисел, средняя уверенность и признак оси графика."""
    lines, confs = {}, []
    words = numbers = 0
    axis = False
    for raw in tsv.splitlines()[1:]:
        c = raw.split("\t")
        if len(c) < 12 or c[0] != "5":
            continue
        w = c[11].strip()
        if not w:
            continue
        try:
            conf = float(c[10])
        except ValueError:
            continue
        confs.append(max(conf, 0.0))
        if len(w) >= 3 and w.isalpha():
            words += 1
        if any(ch.isdigit() for ch in w):
            numbers += 1
        lines.setdefault((c[2], c[3], c[4]), []).append(w)
    for key in sorted(lines):
        if _is_axis(lines[key]):
            axis = True
    text = "\n".join(" ".join(v) for _, v in sorted(lines.items()))
    mean = sum(confs) / len(confs) if confs else 0.0
    return text, words, numbers, round(mean, 1), axis
