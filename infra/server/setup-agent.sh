#!/usr/bin/env bash
# Агент EduHub179 на тестовом сервере: swap, Ollama + модель, venv, agent/.env, служба eduhub-agent,
# пересылка из бота агенту (AGENT_URL). Запускать от root на сервере:
#   bash setup-agent.sh
# Можно запускать повторно: что уже сделано, пропускается. Секреты на экран не выводятся.
set -euo pipefail

REPO=/opt/eduhub179
AGENT=$REPO/agent
BACKEND_ENV=$REPO/backend/.env
MODEL=${MODEL:-qwen2.5:7b}

say() { printf '\n== %s\n' "$*"; }
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

[ "$(id -u)" = 0 ] || { echo "Запусти от root"; exit 1; }
[ -d "$AGENT" ] || { echo "Нет $AGENT — сначала git pull в $REPO"; exit 1; }

say "1/6 swap 4 ГБ"
if swapon --show | grep -q .; then
  echo "swap уже есть"
else
  fallocate -l 4G /swapfile && chmod 600 /swapfile && mkswap /swapfile >/dev/null && swapon /swapfile
  grep -q '^/swapfile' /etc/fstab || echo '/swapfile none swap sw 0 0' >> /etc/fstab
  echo "swap включён"
fi
free -h | sed -n 1,3p

say "2/6 Ollama и модель $MODEL (скачивание ~4.7 ГБ, несколько минут)"
command -v ollama >/dev/null || { apt-get install -y -qq zstd curl >/dev/null; curl -fsSL https://ollama.com/install.sh | sh; }
systemctl enable --now ollama >/dev/null 2>&1 || true
# Ollama слушает только localhost (по умолчанию 127.0.0.1:11434) — снаружи не видна.
for i in $(seq 1 20); do curl -fs http://127.0.0.1:11434/api/tags >/dev/null && break; sleep 1; done
ollama list | grep -q "^${MODEL}" || ollama pull "$MODEL"
ollama list

say "3/6 Python и зависимости агента"
apt-get install -y -qq python3-venv python3-pip poppler-utils >/dev/null
[ -x "$AGENT/.venv/bin/python" ] || python3 -m venv "$AGENT/.venv"
"$AGENT/.venv/bin/pip" install -q -r "$AGENT/requirements.txt"
echo "ok"

say "4/6 agent/.env (секреты берутся из backend/.env, на экран не выводятся)"
touch "$AGENT/.env"; chmod 600 "$AGENT/.env"
BOT_TOKEN=$(get_env TELEGRAM_BOT_TOKEN "$BACKEND_ENV")
[ -n "$BOT_TOKEN" ] || { echo "В backend/.env нет TELEGRAM_BOT_TOKEN"; exit 1; }
AGENT_KEY=$(get_env AGENT_KEY "$BACKEND_ENV")
if [ -z "$AGENT_KEY" ]; then
  AGENT_KEY=$(openssl rand -hex 32)
  set_env AGENT_KEY "$AGENT_KEY" "$BACKEND_ENV"
  echo "AGENT_KEY не было — сгенерирован и записан в backend/.env"
  systemctl restart eduhub-api
fi
# Один бот для всех: бот читает Telegram и пересылает агенту, агент отвечает тем же токеном.
set_env BOT_TOKEN "$BOT_TOKEN" "$AGENT/.env"
set_env AGENT_KEY "$AGENT_KEY" "$AGENT/.env"
set_env AGENT_LISTEN "127.0.0.1:8091" "$AGENT/.env"
set_env API_URL "http://127.0.0.1:8080" "$AGENT/.env"
set_env AGENT_USER_ID "00000000-0000-0000-0000-0000000000c1" "$AGENT/.env"
set_env AGENT_USER_ROLE "teacher" "$AGENT/.env"
set_env OLLAMA_URL "http://127.0.0.1:11434" "$AGENT/.env"
set_env MODEL "$MODEL" "$AGENT/.env"
set_env PHOTO_MODE "stub" "$AGENT/.env"
if [ -z "$(get_env MODERATOR_BOT_TOKEN "$AGENT/.env")" ]; then
  echo "Токен бота-модератора (ДРУГОЙ бот, не основной; ввод не показывается, Enter — пропустить):"
  while read -rs -t 0.2 -n 1000 _junk; do :; done  # выбросить служебные символы терминала
  read -rs MOD || true
  [ -n "${MOD:-}" ] && set_env MODERATOR_BOT_TOKEN "$MOD" "$AGENT/.env" && echo "записан"
fi
LOGIN_BOT=$(curl -fs "https://api.telegram.org/bot${BOT_TOKEN}/getMe" | python3 -c 'import sys,json; print("@"+json.load(sys.stdin)["result"]["username"])' 2>/dev/null || true)
[ -n "$LOGIN_BOT" ] && set_env LOGIN_BOT "$LOGIN_BOT" "$AGENT/.env" && echo "LOGIN_BOT=$LOGIN_BOT"
echo "переменные в agent/.env: $(sed 's/=.*//' "$AGENT/.env" | grep -v '^#' | tr '\n' ' ')"

say "5/6 служба eduhub-agent"
# Канал школы (--channel) по умолчанию выключен: его разбор занимает модель на минуты,
# и бот-модератор в это время молчит. Включить: CHANNEL=1 bash setup-agent.sh
FLAGS=""
[ "${CHANNEL:-0}" = 1 ] && FLAGS="--channel"
if [ -z "$(get_env MODERATOR_BOT_TOKEN "$AGENT/.env")" ]; then
  FLAGS="--no-moderator"
  echo "⚠️  Бота-модератора нет: находки из чатов будут записываться в бэкенд сразу, без проверки."
fi
cat > /etc/systemd/system/eduhub-agent.service <<EOF
[Unit]
Description=EduHub179 chat agent (homework from class chats and DMs)
After=network-online.target ollama.service eduhub-api.service
Wants=ollama.service

[Service]
WorkingDirectory=$AGENT
ExecStart=$AGENT/.venv/bin/python chat_agent.py live $FLAGS
Restart=always
RestartSec=10
Environment=PYTHONUNBUFFERED=1

[Install]
WantedBy=multi-user.target
EOF
systemctl daemon-reload
systemctl enable --now eduhub-agent >/dev/null
systemctl restart eduhub-agent

say "6/6 бот пересылает чаты и личку агенту (AGENT_URL)"
set_env AGENT_URL "http://127.0.0.1:8091/telegram/update" "$BACKEND_ENV"
systemctl restart eduhub-telegram-bot
sleep 8
for s in ollama eduhub-agent eduhub-telegram-bot eduhub-api; do
  printf '%-22s %s (перезапусков: %s)\n' "$s" "$(systemctl is-active $s)" "$(systemctl show -p NRestarts --value $s)"
done
echo
journalctl -u eduhub-agent -n 15 --no-pager -o cat | grep -viE 'token|key' || true
echo
echo "Готово. Дальше: добавь бота в чат класса и выключи ему privacy mode в @BotFather (/setprivacy → Disable)."
echo "Если задан бот-модератор: напиши ему /start — первый написавший становится модератором."
