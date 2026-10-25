"""Провайдеры распознавания: один интерфейс, разные движки.

Сейчас работает только Tesseract — офлайн, бесплатно, хорошо читает печатный текст.
Облачный OCR (Яндекс Vision или подобный) и локальная модель для картинок объявлены,
но намеренно не подключены: `available()` возвращает False, ключей и сети нет.

Зачем так: всё, что движки должны были бы прочитать, попадает в очередь
`state/ocr_queue.jsonl`. Подключить новый движок потом — это один класс, а не переделка
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
        return False, "VLM выключен: текст с рукописного пока не пишем"


REGISTRY = {p.name: p for p in (LocalTesseract(), CloudOCR(), LocalVLM())}


def get(name):
    if name not in REGISTRY:
        raise KeyError(f"нет провайдера '{name}' (есть: {', '.join(REGISTRY)})")
    return REGISTRY[name]


NUM = re.compile(r"^[-−]?\d+(?:[.,]\d+)?$")


def _axis_from_numbers(nums, page_w):
    """Подписи оси графика, а не строка таблицы.

    nums — [(x, y, высота слова)] центры чисел. Ось узнаём по геометрии, а не по тексту:
    пять и больше чисел на одной высоте, расставленных по x с ровными промежутками
    (тики), и под ними нет колонок — у таблицы числа выстроены в столбцы, у оси под
    каждым тиком пусто. Без этой проверки ряд «1 2 3 4 5 6» в таблице выглядел как ось.
    """
    if len(nums) < 5:
        return False
    # строки: соседние числа с почти одинаковой высотой — одна строка
    ordered = sorted(nums, key=lambda n: n[1])
    rows, cur = [], []
    for n in ordered:
        if cur and abs(n[1] - cur[-1][1]) > max(n[2], cur[-1][2]) * 0.5:
            rows.append(cur)
            cur = []
        cur.append(n)
    if cur:
        rows.append(cur)

    tol = max(page_w * 0.02, 1.0)
    for row in rows:
        if len(row) < 5:
            continue
        xs = sorted(x for x, _, _ in row)
        gaps = [b - a for a, b in zip(xs, xs[1:])]
        if min(gaps) <= 0 or max(gaps) > 3 * min(gaps):
            continue                      # тики стоят ровно, у текста промежутки разные
        columns = 0
        for other in rows:
            if other is row:
                continue
            hits = sum(1 for ox, _, _ in other if any(abs(ox - tx) <= tol for tx in xs))
            if hits >= 2:
                columns += 1
        if columns == 0:
            return True
    return False


def _parse_tsv(tsv):
    """Текст, число слов, число чисел, средняя уверенность и признак оси графика."""
    lines, confs = {}, []
    words = numbers = 0
    nums, page_w = [], 1
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
        if NUM.match(w):
            try:
                left, top, width, height = int(c[6]), int(c[7]), int(c[8]), int(c[9])
                nums.append((left + width / 2, top + height / 2, height))
                page_w = max(page_w, left + width)
            except ValueError:
                pass
        lines.setdefault((c[2], c[3], c[4]), []).append(w)
    axis = _axis_from_numbers(nums, page_w)
    text = "\n".join(" ".join(v) for _, v in sorted(lines.items()))
    mean = sum(confs) / len(confs) if confs else 0.0
    return text, words, numbers, round(mean, 1), axis
