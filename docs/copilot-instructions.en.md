# Unified Educational Platform — School 179

The project's master document. Updated as development progresses; serves as shared context for the team and for AI agents (see the "Development Process" section).

---

## 1. Problem

Currently, studying requires juggling 4–5 different places, with no unified notifications:

- **Algebra** — homework as photos in Google Drive: no notifications, inconvenient to search.
- **Spetsmat (advanced math)** — a worksheet system. Worksheets (PDFs with problems) are handed out at school and duplicated on Google Drive. Grading is by the number of solved problems (the "plusnik"): ~6 teachers put "+" in a shared Google spreadsheet.
  Example spreadsheet: https://docs.google.com/spreadsheets/d/1q_hsGYG0pd2Cz6OBw2ITQF17bhkvZ4jfpu5BNOhkQ6U/edit
- **A number of subjects** — homework given orally or on the board → students retell it in messengers.
- **Informatics** — a separate website https://server.179.ru — works well, **don't touch it**.
- **EZHD** — the official system, but grades/integration are **not part of the MVP**, planned later.

## 2. Solution

A unified educational platform: a mobile app (with web in mind), a single entry point for homework, worksheets, plusnik, and communication about these subjects. The app is the single source of truth for this data. Google Drive/Sheets is used only once, for the initial migration of old data.

## 3. MVP functionality

**Student**
- List of subjects + schedule
- Homework per subject: text, files (PDF/photos), deadlines
- Push notifications about new homework and changes
- Plusnik (spetsmat): dashboard — how many problems solved, average score, list of worksheets
- Proposal of oral homework by the duty student → teacher confirmation
- Mini-messenger per subject (question/answer with the teacher) — *status in MVP not finalized, see §8*

**Teacher**
- Creating homework: files, text, class/group, deadline
- Plusnik: class → list of students → tap on a surname → "+", in one tap
- Bulk awarding (select all), quick student search, class statistics
- Moderation of proposed oral homework (approve / reject with a comment)
- Replies to students in the messenger, homework history with editing

**Administrator**
- Importing students/teachers from the school system
- Managing subjects and classes
- One-time migration of old spetsmat data from Google Drive/Sheets

## 4. Technical requirements

- ~1000 concurrent users
- Fail-safe: no unexpected runtime exceptions
- Strong typing, OOP/composition, persistent data structures
- Output correctness
- Interest in low-level optimization (SIMD/AVX) — where justified, not a goal in itself in the MVP
- File storage: S3-compatible (self-hosted MinIO or Yandex Object Storage)

## 5. Stack

| Layer | Technology | Why |
|---|---|---|
| Backend | Rust + Axum | memory safety, `Result<T,E>` instead of exceptions, zero-cost abstractions |
| DB | PostgreSQL + PgBouncer | connection pooling for 1000+ users |
| Cache | Redis | sessions, frequent queries |
| Files | MinIO / Yandex Object Storage | S3-compatible |
| Mobile app | Flutter (Dart) | one codebase for iOS/Android/Web |
| Realtime | WebSockets | messenger, push |

## 6. Architecture

### 6.1 Clean Architecture (Hexagonal), 4 layers, depend only on traits

```
Presentation   — HTTP/WebSocket handlers, DTO
Application    — Use Cases / Services (creating homework, pluses, moderation)
Domain         — Entities (Homework, PlusnikRecord, Message), Value Objects, repository traits
Infrastructure — PostgreSQL, Redis, S3, authentication
```

### 6.2 Fail-safe via `Result<T, E>`
No `try/catch`. Every function returns a `Result`. The compiler won't let you compile code with an unhandled error.

### 6.3 The app as the single source of truth
- Homework → PostgreSQL + files in S3
- Plusnik → PostgreSQL (`plusnik_records`, `tasks`, `sheets`)
- Messenger → PostgreSQL (`messages`)
- Google Drive/Sheets — only for the initial migration

### 6.4 Teacher interface (the key success factor of the product)
For teachers to actually leave Google Sheets, the interface must be an order of magnitude more convenient: class → list of students → tap on a surname → "+" → done; bulk awarding; quick search; class statistics.

### 6.5 Roles and permissions
- **Student** — own homework, own plusnik, chat with the teacher
- **Teacher** — own subjects/classes, creating homework, plusnik, replies to students
- **Admin** — users, subjects, classes

### 6.6 Oral homework moderation
The duty student enters homework → sends it for moderation → the teacher approves (it becomes visible to everyone) or rejects it with a comment → the class gets a push upon approval.

### 6.7 Authentication
Login via school email (magic link / 6-digit code). JWT for sessions.

## 7. Scalability and roadmap

**Confirmed, post-MVP:**
- New subjects — without changing business logic
- Horizontal backend scaling (multiple Axum workers behind a load balancer)
- Full EZHD integration (grades, a new trait), integration with server.179.ru via API

**Conditional direction, not MVP:**
- Blockchain / school internal currency — only if the team separately decides to build an internal currency system **and** sponsors/partners are found for it. Currently this is a direction hypothesis, not a commitment and not part of the roadmap in the narrow sense. We don't design architecture for it in advance (YAGNI); if/when the decision is made — it's built as a separate module on top of the already-built plusnik (points → tokens).

---

## 8. Open MVP questions

**Blockchain — decided.** Not part of the MVP. A separate conditional development direction, see §7. Removed from immediate planning.

**MESH sync vs messenger — open.** The real MVP will likely get **one** of the two — read-only sync with MESH or a per-subject mini-messenger — not both; possibly neither in the end. Not decided which.

- The messenger is technically simpler to implement than reverse-engineering MESH (which already has the highest schedule uncertainty of the whole project).
- But "simpler" ≠ "more needed": the messenger will most likely be useful to users if built — the question isn't complexity per se, but the trade-off between schedule risk (MESH) and practical value for the user (messenger).
- If the messenger is chosen in the end — it's worth explicitly thinking through how it differs from Sferum enough that people would actually use it, rather than duplicating a familiar channel (otherwise it's extra scope without benefit).
- If MESH is chosen — allocate separate time for reconnaissance (traffic capture, searching for existing unofficial clients) before committing to a timeline for that part.

Until the question is resolved — don't design the module for either option in advance (see §9.2).

---

## 9. Development process

### 9.1 Repository and structure for scaffolding

A private GitHub repository, created right away. Below is the technical specification for the initial structure, with exact file names, so that scaffolding (e.g., GitHub Copilot) doesn't require manual renaming later. Items marked *(§8)* — don't create until the MESH/messenger open question is resolved.

```
school179-platform/
├── backend/
│   ├── Cargo.toml                            # workspace manifest, members = crates/*
│   ├── crates/
│   │   ├── domain/
│   │   │   ├── Cargo.toml
│   │   │   └── src/
│   │   │       ├── lib.rs
│   │   │       ├── entities/
│   │   │       │   ├── mod.rs
│   │   │       │   ├── user.rs               # User, Role
│   │   │       │   ├── subject.rs            # Subject, Class
│   │   │       │   ├── homework.rs           # Homework, OralHomeworkProposal
│   │   │       │   ├── plusnik_record.rs     # PlusnikRecord, Sheet, Task
│   │   │       │   └── message.rs            # Message (§8)
│   │   │       ├── value_objects/
│   │   │       │   ├── mod.rs
│   │   │       │   ├── deadline.rs
│   │   │       │   └── role.rs
│   │   │       ├── repositories/             # traits, implementation — in infrastructure
│   │   │       │   ├── mod.rs
│   │   │       │   ├── user_repository.rs
│   │   │       │   ├── homework_repository.rs
│   │   │       │   ├── plusnik_repository.rs
│   │   │       │   └── message_repository.rs # (§8)
│   │   │       └── errors.rs                 # DomainError
│   │   ├── application/
│   │   │   ├── Cargo.toml
│   │   │   └── src/
│   │   │       ├── lib.rs
│   │   │       ├── use_cases/
│   │   │       │   ├── mod.rs
│   │   │       │   ├── auth/
│   │   │       │   │   ├── mod.rs
│   │   │       │   │   ├── request_login_code.rs
│   │   │       │   │   └── verify_login_code.rs
│   │   │       │   ├── homework/
│   │   │       │   │   ├── mod.rs
│   │   │       │   │   ├── create_homework.rs
│   │   │       │   │   ├── list_homework.rs
│   │   │       │   │   ├── propose_oral_homework.rs
│   │   │       │   │   └── moderate_oral_homework.rs
│   │   │       │   ├── plusnik/
│   │   │       │   │   ├── mod.rs
│   │   │       │   │   ├── grant_plus.rs
│   │   │       │   │   ├── bulk_grant_plus.rs
│   │   │       │   │   ├── get_student_progress.rs
│   │   │       │   │   └── get_class_statistics.rs
│   │   │       │   ├── messenger/            # (§8)
│   │   │       │   │   ├── mod.rs
│   │   │       │   │   ├── send_message.rs
│   │   │       │   │   └── list_messages.rs
│   │   │       │   └── notifications/
│   │   │       │       ├── mod.rs
│   │   │       │       └── notify_new_homework.rs
│   │   │       └── ports/                    # external dependencies of use-cases besides repositories
│   │   │           ├── mod.rs
│   │   │           ├── notification_sender.rs
│   │   │           └── file_storage.rs
│   │   ├── infrastructure/
│   │   │   ├── Cargo.toml
│   │   │   └── src/
│   │   │       ├── lib.rs
│   │   │       ├── config.rs                 # .env loading
│   │   │       ├── postgres/
│   │   │       │   ├── mod.rs
│   │   │       │   ├── connection.rs         # pool + PgBouncer
│   │   │       │   ├── user_repository_pg.rs
│   │   │       │   ├── homework_repository_pg.rs
│   │   │       │   ├── plusnik_repository_pg.rs
│   │   │       │   └── message_repository_pg.rs  # (§8)
│   │   │       ├── redis/
│   │   │       │   ├── mod.rs
│   │   │       │   └── session_store.rs
│   │   │       ├── storage/
│   │   │       │   ├── mod.rs
│   │   │       │   └── s3_file_storage.rs    # MinIO / Yandex Object Storage
│   │   │       ├── auth/
│   │   │       │   ├── mod.rs
│   │   │       │   ├── jwt.rs
│   │   │       │   └── magic_link.rs
│   │   │       └── notifications/
│   │   │           ├── mod.rs
│   │   │           └── push_sender.rs
│   │   └── presentation/
│   │       ├── Cargo.toml
│   │       └── src/
│   │           ├── lib.rs
│   │           ├── router.rs
│   │           ├── handlers/
│   │           │   ├── mod.rs
│   │           │   ├── auth_handler.rs
│   │           │   ├── homework_handler.rs
│   │           │   ├── plusnik_handler.rs
│   │           │   ├── messenger_handler.rs  # (§8)
│   │           │   └── admin_handler.rs
│   │           ├── dto/
│   │           │   ├── mod.rs
│   │           │   ├── user_dto.rs
│   │           │   ├── homework_dto.rs
│   │           │   ├── plusnik_dto.rs
│   │           │   └── message_dto.rs        # (§8)
│   │           ├── websocket/
│   │           │   ├── mod.rs
│   │           │   └── connection_handler.rs
│   │           └── middleware/
│   │               ├── mod.rs
│   │               ├── auth_middleware.rs
│   │               └── error_handler.rs
│   ├── bin/                                  # composition root
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── main.rs                       # manual DI, wiring all crates, starting Axum
│   ├── migrations/
│   │   ├── 0001_create_users.sql
│   │   ├── 0002_create_subjects_and_classes.sql
│   │   ├── 0003_create_homework.sql
│   │   ├── 0004_create_plusnik.sql
│   │   └── 0005_create_messages.sql          # (§8)
│   └── tests/
│       ├── homework_flow_test.rs
│       ├── plusnik_flow_test.rs
│       └── auth_flow_test.rs
├── mobile/
│   ├── lib/
│   │   ├── main.dart
│   │   ├── core/
│   │   │   ├── network/
│   │   │   │   ├── api_client.dart
│   │   │   │   └── websocket_client.dart
│   │   │   ├── theme/
│   │   │   │   └── app_theme.dart
│   │   │   └── widgets/                      # grows as common UI components appear
│   │   └── features/
│   │       ├── auth/
│   │       │   ├── auth_screen.dart
│   │       │   ├── auth_repository.dart
│   │       │   └── auth_controller.dart
│   │       ├── homework/
│   │       │   ├── homework_list_screen.dart
│   │       │   ├── homework_detail_screen.dart
│   │       │   ├── homework_repository.dart
│   │       │   └── homework_controller.dart
│   │       ├── plusnik/
│   │       │   ├── plusnik_dashboard_screen.dart   # for the student
│   │       │   ├── plusnik_teacher_screen.dart      # for the teacher: class → tap → "+"
│   │       │   ├── plusnik_repository.dart
│   │       │   └── plusnik_controller.dart
│   │       ├── messenger/                    # (§8)
│   │       │   ├── messenger_screen.dart
│   │       │   ├── messenger_repository.dart
│   │       │   └── messenger_controller.dart
│   │       └── admin/
│   │           ├── user_management_screen.dart
│   │           └── subject_class_management_screen.dart
│   └── pubspec.yaml
├── infra/
│   ├── docker-compose.yml                    # postgres, redis, minio — local dev stack
│   └── .env.example
├── docs/
│   └── decisions.md
├── .claude/
│   └── agents/                               # backend-core.md, infra.md, mobile.md, migration.md
├── CLAUDE.md
└── README.md
```

`_controller.dart` — a neutral name, not tied to a specific state-management library (Riverpod/Bloc/Provider). The library choice is not fixed in this document — when first setting up the project, one should be explicitly chosen and added here.

Rust crate dependency rules (this is Clean Architecture in practice, not just a folder layout — follow strictly):
- `domain` imports nothing from `application`, `infrastructure`, `presentation`.
- `application` imports only `domain`.
- `infrastructure` imports `domain` (to implement its traits), does not import `application`/`presentation`.
- `presentation` imports `application` (and through it `domain` types), does not touch `infrastructure` directly.
- `bin` — the only crate that imports all four: wires dependencies manually (DI) and starts the server.

Initial setup (scaffolding steps):
1. `cargo new --lib` for `domain`, `application`, `infrastructure`, `presentation`; `cargo new --bin` for `bin`. Connect all five in the workspace `Cargo.toml`.
2. Create the files from the tree above as empty modules (`mod.rs` with submodule declarations, the rest of the files with stub structs/traits without implementations). Files marked *(§8)* — don't create until the open question is resolved.
3. `flutter create mobile`, then manually create the `lib/core/` and `lib/features/` structure per the tree above (don't create the `messenger/` folder until §8).
4. `docker-compose.yml`: Postgres 16, Redis 7, MinIO, with health checks and default ports.
5. `.env.example`: connection variables for each service from docker-compose + a JWT secret placeholder.
6. Migrations `0001`–`0004` — create right away for the corresponding entities; `0005_create_messages.sql` — don't create until §8.

### 9.2 Modules / subagents (`.claude/agents/`)
The old split (`mesh-sync`, `blockchain`, `frontend`) doesn't match this scope. For the current MVP, the logical split is:
- `backend-core` — domain + application layers (Rust)
- `infra` — Postgres/Redis/MinIO/auth (Rust)
- `mobile` — Flutter app
- `migration` — one-time import script from Google Drive/Sheets

`mesh-sync` should only be created when §8 is resolved in favor of MESH. `blockchain` — only when/if the conditional direction from §7 is implemented. While both questions are open — don't create crates/agents for them in advance; that would be structuring around a decision not yet made.

### 9.3 Models — initial assignment (verify empirically, not final)
- **Opus** — architectural decisions, security review (auth, access rights)
- **Sonnet** — main backend/mobile code
- **Haiku** — routine, layout, test fixtures

Given that the whole stack is not about AI but about a strict system in Rust, the value of top models here is mainly in correctness review (fail-safe, typing), not creative tasks — worth checking whether a cheaper model would suffice for routine Rust code almost everywhere.

### 9.4 Tasks
GitHub Issues + Projects is enough for 4 people; a separate task-management agent is not needed.

---

## 10. Context: why the project exists

Portfolio framing: technically complex components (strict Rust architecture, fail-safe guarantees, with the possible return of the blockchain part) are valuable as a demonstration of engineering skills, even if they carry higher schedule risk than simpler but less impressive alternatives. Real usefulness for the school (teachers actually leaving Google Sheets) is the second, but no less important success criterion: if teachers don't switch, plusnik as a feature is meaningless regardless of code quality.

---

## 11. Competitive landscape: Google Classroom vs Canvas vs Moodle vs our project

This section should have appeared before the architecture, not after — recording it retroactively.

### 11.1 Google Classroom
Covers most of the MVP "out of the box": homework (text/files/deadlines), push notifications, a points-based grading system (potentially covering plusnik via bulk awarding of points), private comments on assignments (partially covering the messenger). Doesn't have: oral homework moderation, self-hosting, the ability to embed custom logic. An institutional (Workspace for Education) subscription is risky for a Russian school because of the practice of blocking Google Workspace for sanctioned organizations; meanwhile, regular personal accounts can technically use Classroom right now — i.e., part of the MVP (especially plusnik) should be explicitly checked as "isn't this already solved today by free Classroom on personal accounts" before spending sprints on it.

### 11.2 Canvas (Instructure)
A commercial LMS, launched in 2011 as a more modern alternative to Blackboard/Moodle — technically more modern than Moodle (Rails/Ember vs legacy PHP), pleasant UX, LTI integrations, built-in teacher-student messenger, real-time collaborative editing.

An important point specifically by your criterion: **"open source" is misleading here**. The source code is formally available under AGPLv3, but Instructure doesn't support self-hosting in practice — in reality, the school rents their cloud rather than deploying it themselves. That is, by the main criterion (self-hosting for security and control), Canvas fails just like Google Classroom, just disguised as "open".

Additional downsides for your case: paid ($5–25 per student per year; only the heavily stripped-down "Canvas Free for Teacher" is free, for one teacher, not a school); an American company — the same payment problem from Russia as with other US services; in April–May 2026 Canvas had a real data breach (ShinyHunters attack — names, emails, student ID numbers, user correspondence stolen) — this isn't a hypothetical risk of trusting a foreign cloud, but a precedent that actually happened with exactly this scenario; in July 2024 Instructure was acquired by the private equity fund KKR for $4.8 billion — the product now develops in the fund's interests, not the educational mission, which may affect price/priorities to the detriment of schools.

Conclusion on Canvas: closer to Google Classroom than to Moodle, precisely by your key criterion — modern and pleasant, but closed third-party hosting, paid, with a recent real security incident. It doesn't provide the one thing that would make it worth considering (real self-hosting), so the assessment below doesn't change — Canvas occupies Classroom's niche, just paid and American-style.

### 11.3 Moodle
Open-source, self-hosted, enormous functionality (~490,000 lines, 2000+ plugins, community since 2001, 1,000,000+ installations), in Russia used mostly in universities. Real weaknesses: the interface/installation is geared toward administrators, not the fast mobile gesture of a teacher ("class → tap on a surname → +" — that's not what Moodle was designed for); generality (must support every grading system in the world) — excessive complexity for one specific school system; installation is in English, changing the language is awkward; custom business logic (oral homework moderation) would have to be written as a plugin on a legacy PHP API rather than in Rust — a direct contradiction of the project's technological goal (a portfolio in Rust/Clean Architecture).

**An important caution about scale**: ~490,000 lines and 2000+ Moodle plugins are not a benchmark "to grow toward", even with a plan to make the project open-source. Open source doesn't create contributors by itself — Moodle got a community because it first became the de facto standard, and only then did an ecosystem grow around it. Opening the source of a niche school project without users doesn't reproduce that path automatically. The benchmark is to solve a specific school's problem well; growth in reach (if it happens) is a consequence, not a separate goal at the start.

### 11.4 What to take from Moodle's features

*Worth building into the architecture right now (not coding in the MVP, but not closing the door either):*
- Grade/plus change history — who and when awarded, cheap and resolves disputes
- Reports by student/class/group — a direct continuation of the plusnik dashboard
- Explicit workflow states for homework (draft → submitted → checked) — the same pattern already designed for oral homework moderation, just broader

*Consider later, not in the MVP:*
- Rubrics for partial (not just binary) grading
- A forum instead of a private 1:1 messenger — the question is visible to everyone at once, not duplicated N times; possibly covers the original problem better than the current §8 option
- LTI as an open protocol for future integration with server.179.ru instead of a custom API
- Badges instead of blockchain — a cheap replacement if the real goal is gamification/recognition, not a tradable currency with sponsors (see §7)

*Explicitly don't carry over:* support for all world grading systems, LDAP/SAML/CAS/external SSO, SCORM/xAPI, guest access, content multilingualism — none of this solves this particular school's problem.

### 11.5 Positioning

Don't try to compete with Moodle on feature coverage (20+ years of development can't be caught up) and don't try to surpass Classroom/Canvas as a universal LMS. Win on three specific points that no competitor has all at once: (1) mobile-first UX for exactly the two target school workflows, not a general web interface; (2) custom business logic (oral homework moderation) that can't be embedded in Classroom/Canvas, nor cheaply in Moodle; (3) full control over the data schema and stack — important both for the portfolio goal, practically for future AI features (see §12), and for the real security of school data (see the Canvas incident above — your own infrastructure doesn't guarantee no leaks, but at least it doesn't depend on someone else's decision about protection priorities).

---

## 12. AI features (post-MVP, not part of the MVP)

### 12.1 Generating quizzes for reviewing covered material
Idea: generate review quizzes from a student's already-submitted homework/worksheets based on an LLM. Realistic only after MVP — depends on structured homework/worksheet data already accumulated in the system.

Technical principle (the same one already applied to geometric drawing generation and other projects): an LLM can't be trusted to both invent a question and be the sole judge of the answer's correctness — especially in math, where that's a direct path to hallucinations. A separate deterministic verification layer (symbolic math/calculator, not the model itself) is needed to validate the generated correct answer before showing it to the student.

Mandatory manual teacher approval of a generated quiz before students see it — this doesn't need to be designed from scratch: the same moderation UI pattern already designed for oral homework is reused (§3, §6.6) — proposal → teacher approval/rejection → becomes visible.

Explicitly not in the MVP: no need to build architecture for this in advance (YAGNI), except for one thing — the homework/worksheet storage structure must be clean enough (see §11.4, full control over the data schema) so this feature can be added later without rewriting the domain layer.
