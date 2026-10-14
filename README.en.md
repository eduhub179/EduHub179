# EduHub179 — Unified Educational Platform (scaffold)

This is a scaffold repository for the MVP of the "Unified Educational Platform — School 179" project.

Contents
- Architecture: backend (Rust, Axum), frontend (Vite/React), mobile (Flutter), infra (docker-compose)
- The repository contains minimal files so that Copilot/agents can work with the project without external dependencies: simple Cargo.toml files, basic src/lib.rs, migrations and a dev docker-compose.

Quick start (local)
1. Start the dev stack:
   - cd infra && docker compose up -d
   - create a .env based on infra/.env.example
2. Backend (Rust):
   - cd backend
   - cargo build
   - cargo run -p backend-bin
3. Frontend (Vite/React):
   - cd frontend
   - npm install
   - npm run dev
4. Mobile (Flutter):
   - cd mobile
   - flutter pub get
   - flutter run

Running tests
- Backend unit + integration tests need a running PostgreSQL. The integration tests use
  `sqlx::test`, which creates and drops a throwaway database per test, so the test role needs
  CREATEDB rights.
  1. Start Postgres (e.g. `cd infra && docker compose up -d`).
  2. From `backend/`, point the tests at your database and run them:
     ```bash
     export DATABASE_URL="postgres://<user>:<password>@localhost:5432/<db>"
     cargo test --lib
     ```
  Use whatever host/port your Postgres is actually listening on (the default is `localhost:5432`).
  The URL must be for a role that can CREATE DATABASE / DROP DATABASE.

Repository structure
- backend/ — Rust workspace:
  - crates/domain — entities, value objects, repository traits, errors
  - crates/application — use cases (skeleton)
  - crates/infrastructure — Postgres implementations (sqlx)
  - crates/presentation — HTTP handlers, DTOs (skeleton)
  - crates/logic — schedule generation, week-copy service
  - crates/bin — entry points (api-server, tg-bot, sync-worker)
- frontend/ — Vite + React (not yet wired to API)
- mobile/ — Flutter app (lib/, pubspec.yaml)
- infra/ — docker-compose.yml, .env.example
- docs/ — DATABASE_ARCHITECTURE.md, SCHEDULE.md, OVERRIDES.md, CONTRIBUTING.md, copilot-instructions.md
- backend/migrations/ — SQL migrations (0001–0006)

What's implemented
- Domain: 13 aggregates (users, classes, subjects, groups, cabinets, lessons, schedule templates/weeks/instances, homework+files, events+attendees, plusnik) — 325 tests green
- Infrastructure: all 13 repository implementations + 6 migrations
- Application / Presentation: skeleton crates (use cases pending)
- Frontend: Vite/React in `frontend/` (not yet wired to API)

What we're building
Two parallel tracks:
1. **Frontend migration** — wire Vite to API routes, Mini App
2. **Aggregator agent** — Telegram class chats + Drive/Sheets → DB → proactive digest

Links to docs
- `docs/DATABASE_ARCHITECTURE.md` — full schema, indexes, FKs, triggers
- `docs/SCHEDULE.md` — schedule generation logic
- `docs/OVERRIDES.md` — instance-level overrides design