# Локальный запуск проекта EduHub179

Этот файл описывает минимальный набор действий для запуска локального бэкенда и зависимостей проекта.

## 1. Убедись, что установлены нужные инструменты

Нужно иметь:
- Docker Desktop / Docker Engine
- Rust + Cargo
- Git

Проверь:

```bash
rustc --version
cargo --version
docker --version
```

Если `cargo` не найден, скорее всего Rust не добавлен в `PATH`.

## 2. Добавь Cargo в PATH

В macOS/Zsh обычно достаточно:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

Чтобы это применялось постоянно, добавь в `~/.zshrc`:

```bash
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc
```

Проверка:

```bash
cargo --version
sqlx --version
```

Если `sqlx` не найден, установи его:

```bash
cargo install sqlx-cli --no-default-features --features postgres
```

## 3. Запусти Docker контейнеры

В каталоге проекта:

```bash
cd /Users/mz/Documents/eduhub/EduHub179/infra
docker compose up -d postgres redis minio
```

Проверить статус:

```bash
docker compose ps
```

Ожидаемые сервисы:
- `postgres`
- `redis`
- `minio`

## 4. Проверь, что Postgres запущен

Проверка контейнера:

```bash
docker ps
```

Проверка списка таблиц в БД:

```bash
docker exec -it infra-postgres-1 psql -U postgres -d eduhub179 -c '\dt'
```

Если таблиц нет, значит миграции ещё не применены.

## 5. Проверь .env и DATABASE_URL

Backend ожидает переменные окружения. Обычно они должны быть в `backend/.env`.

Пример:

```env
DATABASE_URL=postgres://postgres:postgres_password@localhost:5432/eduhub179
JWT_SECRET=your_super_secret_key_at_least_32_chars
JWT_TTL_SECONDS=3600
SERVER_HOST=127.0.0.1
SERVER_PORT=8080
REDIS_URL=redis://127.0.0.1:6379
TELEGRAM_BOT_TOKEN=test_token
ORG_EMAIL_DOMAIN=@179.ru
```

Если `.env` отсутствует, создай его и укажи корректный `DATABASE_URL`.

## 6. Применить SQL миграции

Из каталога `backend`:

```bash
cd /Users/mz/Documents/eduhub/EduHub179/backend
export DATABASE_URL=postgres://postgres:postgres_password@localhost:5432/eduhub179
sqlx migrate run
```

Если миграции успешно применились, в PostgreSQL появятся таблицы, например:
- `users`
- `subjects`
- `classes`
- `schedule_weeks`
- и др.

Проверка таблиц:

```bash
docker exec -it infra-postgres-1 psql -U postgres -d eduhub179 -c '\dt'
```

## 7. Запуск backend

Из корня проекта:

```bash
cd /Users/mz/Documents/eduhub/EduHub179/backend
cargo run -p backend-bin
```

Или прямо из корня рабочей директории:

```bash
cd /Users/mz/Documents/eduhub/EduHub179
cargo run -p backend-bin
```

### Проверить, что сервер запустился

```bash
curl http://127.0.0.1:8080/health
```

Ожидаемый ответ:

```json
{"service":"eduhub-api","status":"ok"}
```

## 7.1. Проверка работы запросов к use cases

После запуска backend можно проверить реальные HTTP-вызовы к auth и schedule.

### Проверка health

```bash
curl -s http://127.0.0.1:8080/health && echo
curl -s http://127.0.0.1:8080/api/health && echo
```

Ожидаемо:

```json
{"service":"eduhub-api","status":"ok"}
```

### Проверка login use case

```bash
curl -s -X POST http://127.0.0.1:8080/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"login":"test_login","password":"correct_password"}' && echo
```

Если пользователь найден и пароль корректный, должен вернуться JWT/успешный ответ. Если нет — вернётся ошибка логина.

### Проверка schedule use case

```bash
curl -s 'http://127.0.0.1:8080/api/schedule/week?date=2026-09-07&student_id=00000000-0000-0000-0000-000000000001' && echo
```

Здесь:
- `date` — дата недели
- `student_id` — идентификатор ученика

Если в БД есть подходящие данные, должен вернуться JSON с расписанием.

### Проверка ошибок по сценариям

```bash
curl -s -X POST http://127.0.0.1:8080/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"login":"wrong_login","password":"wrong_password"}' && echo
```

В этом случае проверяется, что приложение возвращает корректную ошибку аутентификации, а не падает или уходит в внутреннюю ошибку.

## 8. Частые проблемы и решения

### Ошибка: `relation "users" does not exist`

Это означает, что база есть, но таблицы не созданы. Нужно применить миграции:

```bash
cd /Users/mz/Documents/eduhub/EduHub179/backend
export DATABASE_URL=postgres://postgres:postgres_password@localhost:5432/eduhub179
sqlx migrate run
```

### Ошибка: `command not found: sqlx`

Добавь Cargo в PATH:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

### Ошибка: Docker контейнеры не стартуют

Проверь:

```bash
docker ps -a
docker compose logs postgres
```

### `psql` не найден локально

Используй `docker exec` вместо локального клиента:

```bash
docker exec -it infra-postgres-1 psql -U postgres -d eduhub179 -c '\dt'
```

## 9. Полный порядок запуска

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cd /Users/mz/Documents/eduhub/EduHub179/infra
docker compose up -d postgres redis minio
cd /Users/mz/Documents/eduhub/EduHub179/backend
export DATABASE_URL=postgres://postgres:postgres_password@localhost:5432/eduhub179
sqlx migrate run
cargo run -p backend-bin
```

После этого можно открывать:
- `http://127.0.0.1:8080/health`
- `http://127.0.0.1:8080/api/health`

## 10. Итог

Если сделать всё по порядку:
1. добавить Cargo в PATH,
2. запустить Docker,
3. применить миграции,
4. запустить backend,

то приложение должно корректно стартовать и подключаться к локальной PostgreSQL базе.
