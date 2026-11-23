# EduHub179 — Unified Educational Platform

A unified educational platform for School 179: one entry point for homework, worksheets, the plusnik, and communication about these subjects. The backend is a Rust/Axum service on a PostgreSQL schema; the web client is a Vite + React + TypeScript SPA (PWA with Web Push); a Flutter app lives alongside as a scaffold.

## Contents

- Architecture: backend (Rust, Axum), frontend (Vite + React + TypeScript SPA, cookie session, Web Push), mobile (Flutter, scaffold), infra (docker-compose)
- The repository contains the working code and minimal scaffolding so that Copilot/agents can work with the project without external dependencies: Cargo workspace, SQL migrations, a dev docker-compose, and a fully wired frontend.

## Quick start (local)

1. Start the dev stack:
   - `cd infra && docker compose up -d`
   - create a `.env` based on `infra/.env.example`
2. Backend (Rust):
   - `cd backend`
   - `cargo build`
   - `cargo run -p backend-bin`
3. Frontend (Vite + React + TypeScript):
   - `cd frontend`
   - `npm install`
   - `npm run dev`
   - Vite proxies `/api` and `/health` to the backend at `http://127.0.0.1:8080` (see `frontend/vite.config.ts`), so no CORS setup is needed. Open `http://localhost:5173` (or `http://<your-ip>:5173` from a phone in the same Wi-Fi).
   - The dev server also exposes a small local file API for math uploads under `/api/math/files/*` (a Vite plugin, not the backend).
4. Mobile (Flutter):
   - `cd mobile`
   - `flutter pub get`
   - `flutter run`

## Running tests

- Backend unit + integration tests need a running PostgreSQL. The integration tests use `sqlx::test`, which creates and drops a throwaway database per test, so the test role needs CREATEDB rights.
  1. Start Postgres (e.g. `cd infra && docker compose up -d`).
  2. From `backend/`, point the tests at your database and run them:
     ```bash
     export DATABASE_URL="postgres://<user>:<password>@localhost:5432/<db>"
     cargo test --lib
     ```
  Use whatever host/port your Postgres is actually listening on (the default is `localhost:5432`). The URL must be for a role that can CREATE DATABASE / DROP DATABASE.
- Frontend tests (if/when added):
  - from `frontend/`: `npm test`
  - currently: none in the repo; recommended first targets are the pure helpers (`mondayOf`, `addDays`, `withoutReplacedLessons`, `toScheduleItem`, `parseUser`) and the optimistic plus toggle in `PlusnikPage`.

## Repository structure

- `backend/` — Rust workspace:
  - `crates/domain` — entities, value objects, repository traits, errors
  - `crates/application` — use cases (schedule building implemented; the rest is a skeleton)
  - `crates/infrastructure` — Postgres implementations (sqlx)
  - `crates/presentation` — HTTP handlers, DTOs (skeleton)
  - `crates/logic` — schedule generation, week-copy service
  - `crates/bin` — entry points (api-server, tg-bot, sync-worker)
- `frontend/` — Vite + React + TypeScript SPA:
  - `src/pages/` — Login, Register (first password by emailed code), Main, Schedule (week/day), TeacherDay ("Мой день"), Math, Plusnik (matrix), CourseMaterials, Admin (5 tabs), Profile
  - `src/auth/` — AuthProvider + authService: cookie session (HttpOnly), profile kept in localStorage, SESSION_EXPIRED_EVENT handling, push subscription sync on login/startup
  - `src/api/` — typed API client (authApi, classApi, scheduleApi, plusnik, materials, errors, access)
  - `src/hooks/` — useSchedule (week-aware, teacher/class/student sources)
  - `src/components/` — Header, Schedule (WeekNav, DayLessonList, LessonModal, ScheduleTable), Math (Guest/Authenticated/Admin views), Admin tabs, FileDropDialog, NotificationsSettings
  - `public/` — PWA manifest, service worker (Web Push), icons, a dev-only schedule debug page
  - `src/types/ScheduleTypes.ts` — ScheduleItem, Homework, DaySchedule
  - `vite.config.ts` — dev proxy to the backend + local math-files API plugin
- `mobile/` — Flutter app (lib/, pubspec.yaml), currently a scaffold
- `infra/` — docker-compose.yml, .env.example, server scripts for the beta host
- `docs/` — DATABASE_ARCHITECTURE.md, SCHEDULE.md, OVERRIDES.md, CONTRIBUTING.md, copilot-instructions.md, LOCAL_RUN.en.md
- `backend/migrations/` — SQL migrations (0001–0006)

## What's implemented

- Domain: 13 aggregates (users, classes, subjects, groups, cabinets, lessons, schedule templates/weeks/instances, homework+files, events+attendees, plusnik) — 325 tests green
- Infrastructure: all 13 repository implementations + 6 migrations
- Application: schedule building operations (`generate_from_templates`, `copy_week`, `create_draft`, `publish`) are implemented and covered by tests; other use cases are pending
- Presentation: skeleton crates
- Frontend: Vite + React + TypeScript SPA, wired to the API:
  - Auth: email + password login, first-login flow (emailed code → set password), HttpOnly cookie session with a 401 → auto-logout handler; "remember me" (30 days vs 12 hours)
  - Schedule: week and day views, class and teacher schedules, lesson cards with homework, cancelled/substituted/extra badges, admin one-off changes
  - Teacher "My day": lessons of a chosen day, "homework set / not set" summary
  - Plusnik: students × tasks matrix, one-tap grant/revoke, optimistic updates, column highlight, read-only for students, edit for teachers/admins (published sheets only)
  - Course materials: topics sidebar, file upload (100 MB), external links, drag-and-drop dialog, per-topic caching
  - Math landing page with role-aware views (guest / student / teacher / admin)
  - Admin panel: substitutions & cancellations, timetable, users, announcements, schedule weeks
  - Profile + push notification settings
  - PWA: installable manifest, service worker with Web Push (VAPID), click-to-focus tab, renotify by tag, requireInteraction for homework
- Mobile: Flutter scaffold (single screen)

## Links to docs

- `docs/DATABASE_ARCHITECTURE.md` — full schema, indexes, FKs, triggers
- `docs/SCHEDULE.md` — schedule generation logic
- `docs/OVERRIDES.md` — instance-level overrides design
- `docs/LOCAL_RUN.en.md` — local run guide (Docker, migrations, backend, sample API calls)
- `docs/API.md` — endpoints used by the SPA (to be added; today the contract lives in `frontend/src/api/*`)
