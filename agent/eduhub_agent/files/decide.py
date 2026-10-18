"""Одно решение для всех источников: что делать с прочитанным файлом.

Раньше пороги были размазаны по трём местам (ingest, crawler, chat_agent), и один и тот же
файл получал разные ответы. Здесь одно место и один словарь маршрутов:

  accepted — текст прочитан и метки получены: можно класть в базу
  queued   — текст есть, но OCR не уверен (рукописное): ждёт движок получше
  review   — человеку: нет содержимого (Drive по ссылке) или метки не получены
  skip     — бинарник, пустой файл, служебная папка
"""

TEXT_OK = ("native", "ocr")


def decide(status, labels=None):
    """(маршрут, причина). labels — результат classify(); может быть None."""
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
