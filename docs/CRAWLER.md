# Краулер: сбор материалов в платформу

Дата: 18.10.2026. Код: `tools/crawler/` (синхронизация), `tools/drive_ingest/` (разовый разбор папки).

## Что это

Не «скачать папку и разложить», а **цикл**: посмотреть источник, понять, что появилось,
изменилось или исчезло с прошлого раза, и отдать это платформе событиями. Уведомления
ученикам («по алгебре выложили новое ДЗ») — это те же события, а не отдельная система.

```
источник → scan()/changes() → нормализация + классификация → events → платформа
                                    ↑
                              state.json (курсор + отпечатки)
```

Два свойства, без которых это не работает:

1. **Состояние между запусками.** Второй прогон по той же папке обязан дать ноль событий.
   Проверено: 47 файлов → 47 `created`, повторно → 0.
2. **Стабильный идентификатор.** Для папки — относительный путь, для Drive — `fileId`.
   Путь у Drive меняется при переименовании и перекладывании, `fileId` — нет.

## Источники

| источник | как получаем изменения | что нужно | без чего нельзя |
|---|---|---|---|
| папка на диске (Takeout, архив) | обход + sha256 | ничего | — |
| Google Drive | `changes.getStartPageToken` → `changes.list(pageToken)` | service account, папка расшарена на его email | доступ к диску класса |
| Google Classroom | `courses.courseWork.list` + `updateTime` (дельты нет) | OAuth учителя или domain-wide delegation | админ школы |
| Gmail | `users.history.list` от `historyId` | OAuth пользователя или DWD | админ школы |
| Telegram-чаты | уже сделано в `agent/` | токен бота | — |

Детали, которые ломают планы, если их не знать заранее:

- **Drive.** `changes.list` с `includeItemsFromAllDrives=true&supportsAllDrives=true`, иначе
  файлы общих дисков не видны. У бинарных файлов есть `md5Checksum` — бесплатный детектор
  изменений; у Google-документов его нет, там смотреть `version`. Push-уведомления
  (`changes.watch`) требуют публичный HTTPS и живут ограниченное время — без VPS это
  невозможно, поэтому сейчас опрос по расписанию.
- **Classroom.** Дельта-эндпоинта нет: тянем `courseWork`/`courseWorkMaterials`/`announcements`
  и сравниваем `updateTime`. Материалы Classroom часто ссылаются на файлы Drive
  (`driveFile.id`) — их добираем через Drive API, иначе получим название без содержимого.
- **Gmail.** Личный Gmail service account не читает вообще — только OAuth пользователя.
  В Workspace можно DWD, но это решение админа. Takeout (mbox) годится только для
  первичной заливки: в нём нет стабильных id, синхронизировать по нему нельзя.

## Классификация

Используется тот же классификатор, что в архивном треке P2 (`tools/drive_ingest/classify.py`).
Категории вынесены в `tools/drive_ingest/categories.json`: новый предмет или тип добавляется
строкой в файле, без правки кода.
subject ∈ {algebra, geometry, calculus, digital_tech, misc}, type ∈ {worksheet, homework,
test, independent_work, notes, textbook, program, other}.

На размеченных 287 файлах: **subject 100%, type 85.7%, joint 85.7%**. Это выше планки
scoreboard (правила 76.8%, gemma3:4b 72.9%) и ниже гибрида (88.6%, но там модель зовётся
на 11% файлов). Число завышено: gold частично размечен теми же правилами, поэтому это
in-sample оценка, а не честный тест на новых данных.

## Что можно сделать без доступов

- **папка на диске** — работает полностью сегодня;
- **адаптеры Drive/Classroom/Gmail** — писать против записанных ответов API (фикстуры в
  JSON), с injectable-клиентом: тесты идут офлайн, а с появлением ключей меняется только
  транспорт;
- **события и уведомления** — маппинг «событие → кому и что сообщить» пишется сейчас,
  отправка включается вместе с ботом;
- **импорт в бэкенд** — события уже имеют всё нужное (id, класс, предмет, тип, размер,
  хеш, **ссылку на оригинал в Drive**). Файлы пока не копируются и не скачиваются:
  платформа хранит ссылку. Скачивание и заливка в своё хранилище (`docs/FILE-STORAGE.md`) —
  отдельное решение, когда понадобится независимость от Drive.

## Границы

Краулер **не** публикует ничего сам: он пишет `events.jsonl`. Кто их применяет — импорт
или модератор — решается на стороне платформы. Так же устроен агент чатов: находки ждут
подтверждения, а не сразу уезжают в базу.

---

## Шаг 1 — Drive-адаптер (сделано 18.10.2026)

Готово: `tools/crawler` синхронизирует Google Drive по `changes.list`, проверено на
записанных ответах API, без сети (`tools/crawler/test_drive_source.py`, 18 проверок).
День, когда появится service account, — это день, когда задаётся токен, а не день, когда
начинается разработка.

Показать без ключей:

```
python3 -m tools.crawler sync --fixture tools/crawler/fixtures/drive_first.json \
    --folder-id PROOT --state state/fake.json --out out/fake    # 2 события
# повторный запуск с тем же state -> 0 событий
```

### Что появляется в коде

```
tools/crawler/
  sources/__init__.py     # реестр источников
  sources/local.py        # LocalFolderSource (перенос текущей логики из sync.py)
  sources/drive.py        # DriveSource
  transport.py            # Transport: urllib-реализация + FixtureTransport для тестов
  state.py                # SyncState (уже есть, вынести из sync.py)
  sync.py                 # движок: run(source, state, out) — без знания про Drive
  test_drive_source.py    # офлайн-тесты на фикстурах
  fixtures/drive_*.json   # записанные ответы API
```

### Контракт источника

```python
class Source(Protocol):
    name: str
    def full_scan(self) -> Iterator[Item]: ...
    def changes(self, cursor: str | None) -> tuple[list[Change], str]: ...
```

`Item`: `id, parent_id, name, mime, size, mtime, md5, version, deleted`.
`id` — `fileId` у Drive и относительный путь у папки; путь в события не попадает как
идентификатор никогда.

### Drive: конкретные вызовы

1. `GET /drive/v3/changes/startPageToken?supportsAllDrives=true` — при первом запуске.
2. `GET /drive/v3/changes?pageToken=...&pageSize=1000&includeRemoved=true&includeItemsFromAllDrives=true&supportsAllDrives=true&fields=nextPageToken,newStartPageToken,changes(changeType,fileId,removed,file(id,name,mimeType,size,modifiedTime,md5Checksum,version,parents,trashed))`
3. Идти по `nextPageToken`, пока не придёт `newStartPageToken`; **курсор сохраняется только
   после обработки всех страниц** — упали на середине, следующий запуск повторит пачку.
4. `removed=true` или `trashed=true` → событие `deleted`.
5. Папки (`application/vnd.google-apps.folder`) в события не идут, но нужны для пути:
   по `parents` поднимаемся вверх, имена кешируем в состоянии (`folders: {id: name}`),
   иначе классификатор не увидит «Алгебра/дз/9 класс».
6. Google-документы не имеют `md5Checksum` — отпечаток по `version`.

### Фикстуры (пишутся руками по документации API)

| файл | что внутри |
|---|---|
| `drive_first.json` | `startPageToken`, затем страница из 3 файлов (pdf, docx, папка) |
| `drive_second.json` | 1 новый, 1 изменённый (`md5` другой), 1 `removed=true` |
| `drive_paged.json` | две страницы: `nextPageToken` → `newStartPageToken` |
| `drive_tree.json` | `files.get` для двух папок, чтобы собрать путь |

### Тесты (офлайн, `transport.hits` проверяется)

1. первый запуск: 3 события `created`, курсор взят из `startPageToken`;
2. повторный прогон на тех же данных: **0 событий**;
3. `md5` изменился → `updated`, `version` изменился → `updated`;
4. `removed=true` → `deleted`, состояние очищено;
5. две страницы → один курсор, `changes` вызван ровно 2 раза;
6. падение на второй странице → курсор **не** сдвинулся;
7. путь из `parents` → `Алгебра/дз/9 класс/ДЗ-001.pdf`, предмет `algebra`, класс 9;
8. событие из Drive по схеме **идентично** событию из папки (одни и те же ключи) —
   импорт не должен ветвиться по источнику.

### Результат

`python3 tools/crawler/test_drive_source.py` → 18/18, сетевых вызовов ноль. Событие из
Drive по набору полей совпадает с событием из папки — проверено тестом, а не на слово.
Осталось: живой smoke-тест (два запроса) и импорт событий в бэкенд.

### Что нужно от Макса

Один service account и один `folder_id` любого классного диска — только для живого
smoke-теста (два запроса: `startPageToken` и одна страница `changes`). Без них шаг
закрывается на фикстурах.

### Чего в этом шаге нет

Classroom, Gmail, уведомления, импорт в бэкенд. Они идут после, каждый — отдельный PR.

### Оценка

~3 часа: движок 40 мин, `drive.py` 60 мин, фикстуры 30 мин, тесты 45 мин, документация 15 мин.
