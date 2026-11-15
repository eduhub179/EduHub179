"""Одно решение для файла: что делать с прочитанным содержимым.

Это решатель **только для файлов**. На входе — статус чтения (`files/read.py`) и метки
файла (`files/labels.py`): предмет, тип, уверенность. Текста сообщений здесь нет и быть
не может: ДЗ, контрольные и переносы из переписки разбирает `telegram/chat_agent.py`
своим путём. Раньше пороги были размазаны по двум местам (ingest и crawler), и один и
тот же файл получал разные ответы. Теперь одно место и один словарь маршрутов:

  accepted — текст прочитан, предмет и тип известны: можно класть в базу
  queued   — текст есть, но OCR не уверен (рукописное): ждёт движок получше.
             Это очередь OCR (`ocr_triage`), а не очередь сообщений и не модерация.
  review   — человеку: нет содержимого (Drive по ссылке) или метки не получены
  skip     — бинарник, пустой файл
"""

TEXT_OK = ("native", "ocr")


def decide(status, labels=None):
    """(маршрут, причина). status — статус read(); labels — результат label()."""
    labels = labels or {}
    if status == "binary":
        return "skip", "формат без текста"
    if status == "empty":
        return "skip", "пустой файл"
    if status == "not_downloaded":
        return "review", "нет содержимого: Drive в режиме ссылок"
    if status == "scanned":
        return "queued", "текст есть, но OCR не уверен"
    if status in TEXT_OK:
        conf = labels.get("confidence")
        if conf == "high":
            return "accepted", "текст и метки"
        if conf == "medium":
            return "review", "текст есть, метки неполные"
        return "review", "текст есть, меток нет"
    return "review", f"неизвестный статус {status}"
