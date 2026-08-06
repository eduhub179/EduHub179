# EduHub179 — Unified Educational Platform (scaffold)

This is a scaffold repository for the MVP of the "Unified Educational Platform — School 179" project.

Contents
- Architecture: backend (Rust, Axum), mobile (Flutter), infra (docker-compose)
- The repository contains minimal files so that Copilot/agents can work with the project without external dependencies: simple Cargo.toml files, basic src/lib.rs, migrations and a dev docker-compose.

Quick start (local)
1. Start the dev stack:
   - cd infra && docker compose up -d
   - create a .env based on infra/.env.example
2. Backend (Rust):
   - cd backend
   - cargo build
   - cargo run -p backend-bin
3. Mobile (Flutter):
   - cd mobile
   - flutter pub get
   - flutter run

Repository structure (based on docs/copilot-instructions.md):
- backend/ — Rust workspace: domain, application, infrastructure, presentation, bin
- mobile/ — Flutter app (lib/, pubspec.yaml)
- infra/ — docker-compose.yml, .env.example
- backend/migrations/ — initial SQL migrations

Why this was done
- The repository now has a minimal skeleton for all key parts of the architecture. This lets AI agents (Copilot, Claude, etc.) and developers operate on the code quickly, adding real implementations and tests without loading external files.

What's next (recommendations)
- Fill in backend/crates/* with real implementations: module files, traits in domain, use-cases in application, Postgres/Redis/S3 adapters in infrastructure.
- Add CI (workflow) for building the workspace and checking migrations.
- Resolve the open questions from docs/copilot-instructions.md, in particular the messenger vs mesh-sync module.
