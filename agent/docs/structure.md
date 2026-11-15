# structure.md — кто кого зовёт

Карта зависимостей агента: точки входа, кто что импортирует и в каком порядке
вызываются модули. Дополняет `docs/files.md` (там — роли внутри `files/`).

## Точки входа

| что запускают | модуль | что стартует |
|---|---|---|
| `python3 chat_agent.py live` / `chat_agent.py chats/<экспорт>` | `telegram/chat_agent.py` | живой бот: чаты, вложения, ДЗ, модерация |
| `python3 watch_channel.py loop` | `telegram/watch_channel.py` | каналы школы и «Кружочка» |
| `python3 moderator.py` | `telegram/moderator.py` | бот-модератор (кнопки) |
| `python3 backend_api.py retry` | `server/backend_api.py` | повтор отправки очереди в бэкенд |
| `python3 -m eduhub_agent.ingest scan "<папка>"` | `ingest/__main__.py` | разовый проход по папке |
| `python3 -m eduhub_agent.crawler sync ...` | `crawler/__main__.py` | синхронизация папки/Drive |
| `python3 -m eduhub_agent.llm.llm_extract` | `llm/llm_extract.py` | прогон постов канала через модель |
| `python3 tests/test_*.py` | тесты | импортируют модули напрямую, модель подменяют |

Корневые файлы (`agent/chat_agent.py`, `watch_channel.py`, `backend_api.py`,
`moderator.py`) — только обёртки `runpy`: чтобы старые команды не менялись.

## Слои

```
paths.py ────────────── общие пути: CHATS, STATE, data(), TESSDATA
   │
   ├── config.py ────── env() из .env
   │
   ├── files/ ───────── чтение, метки, маршрут файла
   │     ▲
   │     ├── ingest/ ── разовый проход по папке (драйвер)
   │     └── crawler/ ─ синхронизация изменений (драйвер)
   │
   ├── server/ ──────── timetable.py + backend_api.py
   │
   ├── llm/ ─────────── события канала: правила + модель
   │
   └── telegram/ ────── боты и чаты; единственный, кто знает про Telegram
```

Правило: `files/` не знает про Telegram, Drive, ingest и бэкенд. Драйверы
(`ingest`, `crawler`, `queue`) вызывают `files/`, а не наоборот.

## files/ — кто использует

| модуль | импортирует | его используют | роль |
|---|---|---|---|
| `read.py` | `ocr_triage` | `attachments`, `queue`, `ingest/__main__`, `crawler/sync` | файл → `Reading(text, status, engine)` |
| `ocr_triage.py` | `ocr_providers`, `paths.CHATS` | `read` | Tesseract + порог; очередь сомнительного |
| `ocr_providers.py` | `paths.TESSDATA` | `ocr_triage` | движки OCR |
| `attachments.py` | `read` | `telegram/chat_agent` | скачать вложение, прочитать, отдать текст |
| `taxonomy.py` | stdlib | `hints`, `classify` | предметы, синонимы, класс |
| `hints.py` | `taxonomy` | `labels` | подпись → чат → путь |
| `name_rules.py` | stdlib | `labels` | тип по имени |
| `labels.py` | `hints`, `name_rules`, `classify`, `ingest.grade` | `queue`, `ingest/__main__`, `crawler/sync` | подсказки → правила → модель |
| `decide.py` | — | `queue`, `ingest/__main__`, `crawler/sync` | маршрут файла |
| `queue.py` | `read`, `labels`, `decide`, `paths.STATE` | `telegram/chat_agent` (только `enqueue`) | фоновая разметка |

## classify/

| файл | импортирует | его используют | роль |
|---|---|---|---|
| `__init__.py` | `files.taxonomy`, `prompt.md`, Ollama HTTP | `files/labels` | модель + схема + строгий `parse()` |
| `prompt.md` | — | `classify.prompt_text()` | шаблон промпта (`{{SUBJECTS}}`, `{{TYPES}}`) |

`labels` зовёт `classify.classify()` только если подсказка + правило имени не дали
`high` по предмету и типу.

## ingest/

| файл | импортирует | его используют | роль |
|---|---|---|---|
| `source.py` | stdlib | `ingest/__main__` | локальный источник файлов |
| `grade.py` | stdlib | `labels`, `ingest/__main__`, `crawler/sync` | класс из пути/имени |
| `__main__.py` | `source`, `read`, `labels`, `decide`, `grade` | CLI | `manifest.jsonl`, `review.csv`, `report.md` |

## crawler/

| файл | импортирует | его используют | роль |
|---|---|---|---|
| `__main__.py` | `sync`, `sources.*`, `transport` | CLI | разбор флагов, выбор источника |
| `sync.py` | `state`, `read`, `labels`, `decide`, `grade` | `crawler/__main__` | снимок/изменения → события |
| `state.py` | stdlib | `sync` | курсор + отпечатки, атомарная запись |
| `transport.py` | stdlib | `__main__`, `DriveSource` (duck-typing) | HTTP или записанные ответы |
| `sources/drive.py` | stdlib, транспорт получает снаружи | `__main__` | `full_scan()` / `changes()` |
| `sources/local.py` | stdlib | `__main__`, `sync` | обход папки |

## telegram/

| файл | импортирует | его используют | роль |
|---|---|---|---|
| `chat_agent.py` | `files.attachments`, `files.queue`, `llm.baseline_rules`, `server.timetable`, `chat_moderation`, `dm` | точка входа | сообщения чатов → находки |
| `dm.py` | `config`, `backend_api`, `timetable`, `chat_moderation`, `questions` | `chat_agent` | ДЗ в личку боту |
| `questions.py` | stdlib + Ollama | `dm` | намерение вопроса (правила → модель) |
| `chat_moderation.py` | `backend_api`, `moderator` | `chat_agent`, `dm` | очередь находок на подтверждение |
| `moderator.py` | `config`, `backend_api` | `chat_moderation`; точка входа | кнопки «Опубликовать / Отклонить» |
| `watch_channel.py` | `fetch_channel`, `kruzhochek`, `llm_extract`, `moderator` | точка входа | опрос каналов |
| `fetch_channel.py` | `paths.data` | `watch_channel` | скачать/разобрать пост канала |
| `kruzhochek.py` | `paths.data` | `watch_channel` | анонсы «Кружочка» |
| `llm_extract.py` | `baseline_rules`, `paths.data` | `watch_channel`, CLI | события из постов: модель + правила |
| `baseline_rules.py` | `paths.data` | `llm_extract`, `chat_agent` | регулярки дат/времени/кабинетов |

## server/ и llm/

| файл | импортирует | его используют | роль |
|---|---|---|---|
| `server/timetable.py` | `paths.data` | `chat_agent`, `dm`, `backend_api` | уроки, замены, срок сдачи |
| `server/backend_api.py` | `config`, `timetable` | `chat_moderation`, `dm`, `moderator`; CLI | ДЗ → `POST/PATCH /api/homework` |
| `llm/build_dataset.py` | `telegram.chat_agent` | CLI обучения | датасет из экспортов чатов |
| `llm/train_classifier.py`, `make_labels.py`, `show_labels.py` | `paths` | CLI | обучение/разметка, в прод не входят |

## Три реальных вызова

**Файл из Telegram**
```
chat_agent (сообщение)
  → attachments.download()
  → attachments.read_many() → read.read() → ocr_triage.read_image() → ocr_providers
  → attachments.texts_from() → текст в модель чата (находки)
  → files.queue.enqueue()    # на фоновую разметку
```
Модель чата и файловая разметка — два разных пути. `queue` копит файлы, но обработчика
в проде пока нет (см. ниже).

**Разовый проход по папке**
```
python -m eduhub_agent.ingest scan DIR --out OUT
  → source.files()
  → read.read()          # текст + статус
  → labels.label()       # hints → name_rules → classify (если нужно)
  → decide.decide()      # accepted / queued / review / skip
  → OUT/manifest.jsonl + review.csv + report.md
```

**Синхронизация Drive**
```
python -m eduhub_agent.crawler sync --drive --folder-id X
  → sync.run()
  → DriveSource.full_scan()/changes() → transport (HTTP)
  → diff_snapshot()/apply_changes() → state.py
  → на каждое событие: read.read() → labels.label() → decide.decide()
  → events.jsonl + summary.md
```

## Известные провода, которые болтаются

1. **`queue.process()` никто не вызывает в проде.** `chat_agent` только `enqueue`-ит;
   воркера/таймера нет. Очередь — библиотека, а не работающий фон.
2. **`files/labels.py` → `ingest/grade.py`** — разворот слоёв: `files/` не должен
   зависеть от драйвера `ingest`. Плюс `ingest/grade.py` дублирует регулярки
   `taxonomy.grade_from`.
3. **`files/attachments.py`** — Telegram-специфика внутри `files/`. По-хорошему ей
   место в `telegram/`, а в `files/` — только `read`.
4. **`read → label → decide` повторяется в трёх драйверах** (`ingest`, `crawler`,
   `queue`). Один `files/process_file()` убрал бы копию.
5. `queue` и `ocr_triage` используют слово `queued` в разных смыслах (очередь
   разметки и очередь OCR). В `decide.py` это уже описано, но имена всё ещё путают.
