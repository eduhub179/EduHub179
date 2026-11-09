#!/usr/bin/env bash
# Создать админа для сайта или задать новый пароль существующему админу.
#   bash /opt/eduhub-beta/admin-password.sh            (логин спросит, по умолчанию admin)
# Пароль вводится скрыто и нигде не сохраняется, в базе только хэш.
set -euo pipefail
. "$(dirname "$0")/common.sh"

echo "Админы в базе сейчас: $(psql_run -c "select count(*) from users where role='admin' and is_active")"
read -rp "Логин админа (Enter = admin): " LOGIN
LOGIN=$(printf '%s' "${LOGIN:-admin}" | tr 'A-Z' 'a-z' | tr -d ' ')
[[ "$LOGIN" =~ ^[a-z0-9._-]{1,64}$ ]] || { echo "Логин: только латиница, цифры, точка, _ и -"; exit 1; }
read -rsp "Новый пароль (не меньше 10 символов): " P1; echo
read -rsp "Ещё раз: " P2; echo
[ "$P1" = "$P2" ] || { echo "Пароли не совпали"; exit 1; }
[ ${#P1} -ge 10 ] || { echo "Слишком короткий"; exit 1; }
HASH=$(printf '%s' "$P1" | hash_password); unset P1 P2
case "$(upsert_admin "$LOGIN" "$HASH" "Администратор" "EduHub")" in
  OK) echo "Готово: вход на сайт с логином $LOGIN и этим паролем." ;;
  NOT_ADMIN) echo "Логин $LOGIN уже занят учеником или учителем. Возьми другой, например ${LOGIN}.admin"; exit 1 ;;
  *) echo "Что-то пошло не так"; exit 1 ;;
esac
