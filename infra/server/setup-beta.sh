#!/usr/bin/env bash
# Подготовка тестового сервера к бете. Запускать от root:
#   bash /opt/eduhub-beta/setup-beta.sh
# Делает: служебного админа для публикации недель, таймер публикации (пятница 18:00 МСК),
# проверку сервера каждые 5 минут с сообщением в Telegram, ключ для пушей (VAPID),
# и сразу публикует следующую неделю. Можно запускать повторно. Секреты на экран не выводятся.
set -euo pipefail
. "$(dirname "$0")/common.sh"
[ "$(id -u)" = 0 ] || { echo "Запусти от root"; exit 1; }
[ "$(cd "$(dirname "$0")" && pwd)" = "$TOOLS" ] || { echo "Скрипты должны лежать в $TOOLS"; exit 1; }
say() { printf '\n== %s\n' "$*"; }

say "1/6 Python для хэша паролей"
[ -x "$VENV/bin/python" ] || python3 -m venv "$VENV"
"$VENV/bin/pip" install -q argon2-cffi
echo ok

say "2/6 служебный админ svc.weeks (пароль случайный, лежит только в /root/.eduhub-weeks)"
PASS=$(openssl rand -hex 24)
HASH=$(printf '%s' "$PASS" | hash_password)
r=$(upsert_admin svc.weeks "$HASH" "Служебный" "Публикация недель")
[ "$r" = OK ] || { echo "Не получилось: $r"; exit 1; }
umask 077; printf 'LOGIN=svc.weeks\nPASSWORD=%s\n' "$PASS" > /root/.eduhub-weeks; unset PASS HASH
chmod 600 /root/.eduhub-weeks
echo ok

say "3/6 таймер публикации недели (пятница 18:00 по Москве)"
cat > /etc/systemd/system/eduhub-publish-week.service <<UNIT
[Unit]
Description=EduHub179: create and publish next week's timetable
After=eduhub-api.service

[Service]
Type=oneshot
ExecStart=/usr/bin/python3 $TOOLS/publish-week.py
UNIT
cat > /etc/systemd/system/eduhub-publish-week.timer <<UNIT
[Unit]
Description=EduHub179: publish next week every Friday

[Timer]
OnCalendar=Fri *-*-* 18:00:00 Europe/Moscow
Persistent=true

[Install]
WantedBy=timers.target
UNIT

say "4/6 проверка сервера каждые 5 минут"
cat > /etc/systemd/system/eduhub-health.service <<UNIT
[Unit]
Description=EduHub179: health check with Telegram alert

[Service]
Type=oneshot
ExecStart=/bin/bash $TOOLS/healthcheck.sh
SuccessExitStatus=1
UNIT
cat > /etc/systemd/system/eduhub-health.timer <<UNIT
[Unit]
Description=EduHub179: health check every 5 minutes

[Timer]
OnBootSec=3min
OnUnitActiveSec=5min

[Install]
WantedBy=timers.target
UNIT
systemctl daemon-reload
systemctl enable --now eduhub-publish-week.timer eduhub-health.timer >/dev/null
systemctl list-timers 'eduhub-*' --no-pager | sed -n '1,6p'

say "5/6 ключ для пушей (VAPID)"
if [ -z "$(get_env VAPID_PRIVATE_KEY "$BACKEND_ENV")" ]; then
  cp -p "$BACKEND_ENV" "$BACKEND_ENV.bak-$(date +%Y%m%d-%H%M)"
  set_env VAPID_PRIVATE_KEY "$(openssl rand 32 | base64 | tr '+/' '-_' | tr -d '=')" "$BACKEND_ENV"
  [ -n "$(get_env VAPID_SUBJECT "$BACKEND_ENV")" ] || set_env VAPID_SUBJECT "mailto:admin@179.ru" "$BACKEND_ENV"
  systemctl restart eduhub-api
  echo "ключ создан, API перезапущен"
  for i in $(seq 1 30); do curl -fs -o /dev/null http://127.0.0.1:8080/api/health && break; sleep 1; done
else
  echo "ключ уже был, не трогаю (смена ключа сломала бы подписки)"
fi
echo "публичный ключ отдаётся: HTTP $(curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:8080/api/notifications/vapid-public-key)"

say "6/6 публикую следующую неделю и проверяю сервер"
python3 "$TOOLS/publish-week.py" || echo "⚠️ неделя не опубликована, пришли вывод Claude"
bash "$TOOLS/healthcheck.sh" --test || true
bash "$TOOLS/healthcheck.sh" || true
echo
echo "Готово."
