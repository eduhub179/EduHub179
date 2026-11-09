# Общие функции для скриптов беты. Подключается через: . "$(dirname "$0")/common.sh"
TOOLS=/opt/eduhub-beta
REPO=/opt/eduhub179
BACKEND_ENV=$REPO/backend/.env
VENV=$TOOLS/.venv
# psql в контейнере базы; для проверки на другой машине можно задать PSQL="psql -h ... -U ... -d ..."
PSQL=${PSQL:-"docker exec -i infra-postgres-1 psql -U postgres -d eduhub179"}

psql_run() { $PSQL -v ON_ERROR_STOP=1 -qAt "$@"; }

get_env() { grep -E "^$1=" "$2" 2>/dev/null | tail -1 | cut -d= -f2- || true; }
set_env() { # set_env KEY VALUE FILE — заменить или дописать строку, не печатая значение
  local k=$1 v=$2 f=$3
  if grep -qE "^$k=" "$f" 2>/dev/null; then
    python3 - "$k" "$v" "$f" <<'PY'
import sys; k, v, f = sys.argv[1:]
lines = open(f).read().splitlines()
out = [f"{k}={v}" if l.startswith(k + "=") else l for l in lines]
open(f, "w").write("\n".join(out) + "\n")
PY
  else
    printf '%s=%s\n' "$k" "$v" >> "$f"
  fi
}

# Пароль из stdin -> Argon2id-хэш в формате, который понимает бэкенд (проверено с его Argon2PasswordHasher).
hash_password() {
  "$VENV/bin/python" -c 'import sys; from argon2 import PasswordHasher; print(PasswordHasher(time_cost=2, memory_cost=19456, parallelism=1).hash(sys.stdin.read().rstrip("\n")))'
}

# Создать или обновить админа: upsert_admin LOGIN HASH LAST FIRST. Студента/учителя в админа не превращает.
upsert_admin() {
  psql_run -v login="$1" -v hash="$2" -v last="$3" -v first="$4" <<'SQL'
\set QUIET on
SELECT EXISTS (SELECT 1 FROM users WHERE login = :'login' AND role <> 'admin') AS bad \gset
\if :bad
  \echo NOT_ADMIN
\else
  INSERT INTO users (login, password_hash, role, last_name, first_name, is_active)
  VALUES (:'login', :'hash', 'admin', :'last', :'first', TRUE)
  ON CONFLICT (login) DO UPDATE SET password_hash = EXCLUDED.password_hash, is_active = TRUE;
  \echo OK
\endif
SQL
}
