#!/usr/bin/env bash
# Проверка тестового сервера раз в 5 минут (таймер eduhub-health). Если что-то сломалось или
# починилось, пишет в Telegram через бота-модератора тому, кто модератор.
#   bash /opt/eduhub-beta/healthcheck.sh          проверить (пишет только при изменениях)
#   bash /opt/eduhub-beta/healthcheck.sh --test   прислать тестовое сообщение
set -uo pipefail
. "$(dirname "$0")/common.sh"
STATE=/var/lib/eduhub-beta/health.state
mkdir -p "$(dirname "$STATE")"
AGENT_ENV=$REPO/agent/.env

send() {
  local token chat
  token=$(get_env MODERATOR_BOT_TOKEN "$AGENT_ENV")
  chat=$(get_env MODERATOR_CHAT_ID "$AGENT_ENV")
  [ -n "$chat" ] || chat=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("chat_id",""))' "$REPO/agent/moderator.json" 2>/dev/null)
  if [ -z "$token" ] || [ -z "$chat" ]; then echo "Некому писать: нет MODERATOR_BOT_TOKEN или модератора"; return 1; fi
  curl -fsS -m 20 -o /dev/null "https://api.telegram.org/bot${token}/sendMessage" \
    --data-urlencode "chat_id=$chat" --data-urlencode "text=$1" || { echo "Telegram не принял сообщение"; return 1; }
}

if [ "${1:-}" = "--test" ]; then
  send "🩺 EduHub179: проверка связи. Если сервер сломается, напишу сюда." && echo "Отправлено"
  exit
fi

problems=()
for s in eduhub-api eduhub-telegram-bot eduhub-agent nginx docker; do
  systemctl is-active --quiet "$s" || problems+=("служба $s: $(systemctl is-active "$s")")
done
for s in eduhub-publish-week eduhub-backup; do
  systemctl is-failed --quiet "$s.service" && problems+=("не удалось: $s (journalctl -u $s -n 20)")
done
curl -fsS -m 10 -o /dev/null http://127.0.0.1:8080/api/health || problems+=("API не отвечает на /api/health")
running=$(docker ps --filter name=infra- --format '{{.Names}}' | wc -l)
[ "$running" -ge 3 ] || problems+=("контейнеров infra-* запущено $running из 3 (база, Redis, файлы)")
disk=$(df --output=pcent / | tail -1 | tr -dc 0-9)
[ "$disk" -lt 90 ] || problems+=("диск заполнен на $disk%")
last=$(ls -t "$REPO"/backups/* 2>/dev/null | head -1)
if [ -z "$last" ]; then
  problems+=("нет ни одного бэкапа")
elif [ ! -s "$last" ]; then
  problems+=("последний бэкап пустой: $(basename "$last")")
elif [ $(( $(date +%s) - $(stat -c %Y "$last") )) -gt $((26*3600)) ]; then
  problems+=("бэкапа не было больше суток")
fi

now=$(date +%s)
if [ ${#problems[@]} -eq 0 ]; then
  if [ -s "$STATE" ]; then send "✅ EduHub179: всё снова работает." && : > "$STATE"; fi
  echo "ok"
  exit 0
fi
text=$(printf '• %s\n' "${problems[@]}")
old=$(sed -n '2,$p' "$STATE" 2>/dev/null)
old_time=$(head -1 "$STATE" 2>/dev/null || echo 0)
# Пишем, если список проблем изменился, и напоминаем раз в 6 часов.
if [ "$text" != "$old" ] || [ $((now - ${old_time:-0})) -gt $((6*3600)) ]; then
  send "⚠️ EduHub179, тестовый сервер:
$text" && printf '%s\n%s\n' "$now" "$text" > "$STATE"
fi
printf '%s\n' "$text"
exit 1
