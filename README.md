# EduHub179 — Единая образовательная платформа (шаблон)

Это репозиторий-скаффолд для MVP проекта "Единая образовательная платформа — Школа 179".

Содержание
- Архитектура: backend (Rust, Axum), mobile (Flutter), infra (docker-compose)
- В репозитории добавлены минимальные файлы для того, чтобы Copilot/агенты могли работать с проектом без внешних зависимостей: простые Cargo.toml, базовые src/lib.rs, миграции и dev docker-compose.

Быстрый старт (локально)
1. Запустить dev-стек:
   - cd infra && docker compose up -d
   - создать .env на основе infra/.env.example
2. Backend (Rust):
   - cd backend
   - cargo build
   - cargo run -p backend-bin
3. Mobile (Flutter):
   - cd mobile
   - flutter pub get
   - flutter run

Структура репозитория (выдержана из .github/copilot-instructions.md):
- backend/ — Rust workspace: domain, application, infrastructure, presentation, bin
- mobile/ — Flutter-приложение (lib/, pubspec.yaml)
- infra/ — docker-compose.yml, .env.example
- backend/migrations/ — начальные миграции SQL

Зачем это сделано
- В репозитории теперь есть минимальный скелет для всех ключевых частей архитектуры. Это позволяет AI-агентам (Copilot, Claude и др.) и разработчикам быстро оперировать по коду, добавлять реальные реализации и тесты без загрузки внешних файлов.

Что далее (рекомендации)
- Дополнить backend/crates/* реальными реализациями: модульные файлы, трейты в domain, use-cases в application, адаптеры Postgres/Redis/S3 в infrastructure.
- Добавить CI (workflow) для сборки workspace и проверки миграций.
- Решить открытые вопросы из .github/copilot-instructions.md, в частности модуль messenger vs mesh-sync.
