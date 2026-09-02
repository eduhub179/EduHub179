# EduHub179 — Единая образовательная платформа

Репозиторий MVP проекта «Единая образовательная платформа — Школа 179»:
единая точка входа для домашних заданий, листков и плюсника, расписания
и коммуникации по предметам вместо 4–5 разрозненных сервисов
(подробнее — docs/copilot-instructions.md).

## Содержание

- Архитектура: backend (Rust, Axum), frontend (TypeScript), infra (docker-compose)
- В репозитории — минимальный набор файлов, достаточный для работы команды
  и AI-агентов без внешних зависимостей: Cargo.toml, базовые модули, миграции
  и dev docker-compose.

## Быстрый старт (локально)

1. Запустить dev-стек:
   - `cd infra && docker compose up -d`
   - создать `.env` на основе `infra/.env.example`
2. Backend (Rust):
   - `cd backend`
   - `cargo build`
   - `cargo run -p backend-bin`
3. Frontend (TypeScript):
   - `cd frontend`
   - `npm install`
   - `npm run dev`

## Структура репозитория (выдержана из docs/copilot-instructions.md)

- `backend/` — Rust workspace: domain, application, infrastructure, presentation, bin
- `frontend/` — интерфейс на TypeScript: API-клиент (HTTP/WebSocket),
  фичи (auth, ДЗ, плюсник, расписание, админка), общие UI-компоненты
- `infra/` — docker-compose.yml, .env.example
- `backend/migrations/` — SQL-миграции
- `docs/` — мастер-документ, схема БД, стандарт документирования

## Зачем это сделано

- В репозитории есть скелет всех ключевых частей архитектуры — разработчики
  и AI-агенты быстро входят в код, добавляя реальные реализации и тесты
  без загрузки внешних файлов.
- Интерфейс разрабатывается на TypeScript: более распространённый стек
  (проще вводить новых участников, включая десятиклассников), единая кодовая
  база для веба с перспективой мобильного клиента.

## Что далее (рекомендации)

- Зафиксировать фреймворк фронтенда (React/Next.js для веба или React Native
  для мобильного клиента) и отразить выбор в docs/copilot-instructions.md.
- Дополнять backend/crates/* реальными реализациями: use-cases в application,
  адаптеры в infrastructure, handlers/DTO в presentation.
- Добавить CI (workflow) для сборки workspace и проверки миграций.
- Решить открытые вопросы из docs/copilot-instructions.md,
  в частности модуль messenger vs mesh-sync.
