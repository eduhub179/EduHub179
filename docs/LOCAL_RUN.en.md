# Local run guide for EduHub179

This file describes the minimum required steps to start the local backend and its dependencies.

## 1. Make sure the required tools are installed

You need:
- Docker Desktop / Docker Engine
- Rust + Cargo
- Git

Check:

```bash
rustc --version
cargo --version
docker --version
```

If `cargo` is missing, Rust is probably not in `PATH`.

## 2. Add Cargo to PATH

On macOS / zsh, usually this is enough:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

To keep it permanent, add it to `~/.zshrc`:

```bash
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc
```

Verify:

```bash
cargo --version
sqlx --version
```

If `sqlx` is not found, install it:

```bash
cargo install sqlx-cli --no-default-features --features postgres
```

## 3. Start Docker containers

From the project directory:

```bash
cd /Users/mz/Documents/eduhub/EduHub179/infra
docker compose up -d postgres redis minio
```

Check status:

```bash
docker compose ps
```

Expected services:
- `postgres`
- `redis`
- `minio`

## 4. Verify that Postgres is running

Check running containers:

```bash
docker ps
```

Check tables in the database:

```bash
docker exec -it infra-postgres-1 psql -U postgres -d eduhub179 -c '\dt'
```

If there are no tables, migrations have not been applied yet.

## 5. Check `.env` and `DATABASE_URL`

The backend expects environment variables. Usually they sit in `backend/.env`.

Example:

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

If `.env` is missing, create it and set the correct `DATABASE_URL`.

## 6. Apply SQL migrations

From the `backend` directory:

```bash
cd /Users/mz/Documents/eduhub/EduHub179/backend
export DATABASE_URL=postgres://postgres:postgres_password@localhost:5432/eduhub179
sqlx migrate run
```

If migrations are applied successfully, PostgreSQL will contain tables such as:
- `users`
- `subjects`
- `classes`
- `schedule_weeks`
- and others

Check tables again:

```bash
docker exec -it infra-postgres-1 psql -U postgres -d eduhub179 -c '\dt'
```

## 7. Start the backend

From the project root:

```bash
cd /Users/mz/Documents/eduhub/EduHub179/backend
cargo run -p backend-bin
```

Or from the workspace root:

```bash
cd /Users/mz/Documents/eduhub/EduHub179
cargo run -p backend-bin
```

### Verify the server started

```bash
curl http://127.0.0.1:8080/health
```

Expected response:

```json
{"service":"eduhub-api","status":"ok"}
```

## 7.1. Verify request execution against use cases

After the backend is running, you can check the real HTTP requests to the auth and schedule use cases.

### Health check

```bash
curl -s http://127.0.0.1:8080/health && echo
curl -s http://127.0.0.1:8080/api/health && echo
```

Expected output:

```json
{"service":"eduhub-api","status":"ok"}
```

### Login use case check

```bash
curl -s -X POST http://127.0.0.1:8080/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"login":"test_login","password":"correct_password"}' && echo
```

If the user exists and the password is correct, the server should return a successful auth result or JWT. If not, it should return a login error.

### Schedule use case check

```bash
curl -s 'http://127.0.0.1:8080/api/schedule/week?date=2026-09-07&student_id=00000000-0000-0000-0000-000000000001' && echo
```

Here:
- `date` — week date
- `student_id` — student identifier

If the database has matching data, the API should return schedule JSON.

### Negative login scenario

```bash
curl -s -X POST http://127.0.0.1:8080/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"login":"wrong_login","password":"wrong_password"}' && echo
```

This checks that the app returns a normal auth error instead of crashing or throwing a backend error.

## 8. Common issues and fixes

### Error: `relation "users" does not exist`

This means the database exists but the schema has not been created. Apply the migrations:

```bash
cd /Users/mz/Documents/eduhub/EduHub179/backend
export DATABASE_URL=postgres://postgres:postgres_password@localhost:5432/eduhub179
sqlx migrate run
```

### Error: `command not found: sqlx`

Add Cargo to PATH:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

### Docker containers do not start

Check:

```bash
docker ps -a
docker compose logs postgres
```

### `psql` is not installed locally

Use Docker instead:

```bash
docker exec -it infra-postgres-1 psql -U postgres -d eduhub179 -c '\dt'
```

## 9. Full startup order

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cd /Users/mz/Documents/eduhub/EduHub179/infra
docker compose up -d postgres redis minio
cd /Users/mz/Documents/eduhub/EduHub179/backend
export DATABASE_URL=postgres://postgres:postgres_password@localhost:5432/eduhub179
sqlx migrate run
cargo run -p backend-bin
```

After that, you can open:
- `http://127.0.0.1:8080/health`
- `http://127.0.0.1:8080/api/health`

## 10. Summary

If you follow the steps in order:
1. add Cargo to PATH,
2. start Docker,
3. apply migrations,
4. start the backend,

the application should connect to local PostgreSQL and work correctly.
