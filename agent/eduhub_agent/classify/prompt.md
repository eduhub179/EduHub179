# Material identification prompt (task M2 / P2 bake-off)

You label teaching materials from a Russian school. For the single document below,
output **one JSON object, nothing else**. No comments, no explanation, no markdown.

## Output schema
```json
{"subject": "<subject>", "type": "<type>", "lesson": <int|null>, "date": "<dd.mm.yyyy>|null"}
```

## subject — pick exactly one
- `algebra` — алгебра
- `geometry` — геометрия, планиметрия, стереометрия
- `calculus` — математический анализ, матан, производные/интегралы/пределы, линейная алгебра
- `digital_tech` — цифровые технологии, цифровая электроника, логические схемы, logisim
- `programming` — Python, программирование, алгоритмы, игры/задачи на код

## type — pick exactly one
- `worksheet` — a sheet of exercises: «Листок 13», «Задачки», «A. Напишите программу», задачи для решения
- `homework` — a turn-in assignment: «ДЗ», «домашняя работа», «дом_п», «зачётное ДЗ»
- `test` — контрольная работа, зачёт, тест
- `independent_work` — самостоятельная работа (СР)
- `notes` — конспект, теория, аксиомы, справочный текст без заданий
- `textbook` — учебник, задачник, сборник
- `presentation` — презентация, слайды
- `plusnik_sheet` — «Плюсник»: таблица с оценками/номерами листков
- `program` — исходный код (.py, .ipynb) или файл данных к программе
- `other` — ничего из перечисленного

## Rules
1. Judge by **content** first. Use the name/folder only as supporting evidence.
2. A page that *describes tasks* is `worksheet`; a file that *is code* is `program`.
3. `lesson` = the sheet/lesson number if the document states one («Листок 7», «урок 12»), else null.
   Never invent a number.
4. `date` only if the document states a real date, else null. Score lists are not dates.
5. Never output a subject or type outside the lists above.
6. If the text is empty or meaningless, decide from the name. If that is meaningless too, pick the most plausible subject and use type `other`.

## Input
One JSON object with some of: `name`, `folder`, `text`.
