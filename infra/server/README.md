# Скрипты для беты (тестовый сервер)

Скрипты для тестового сервера. `setup-agent.sh` ставит агента и модель, остальные готовят бету и копируются в `/opt/eduhub-beta`.

| Файл | Что делает |
|---|---|
| `setup-beta.sh` | Один раз: служебный админ `svc.weeks`, таймер публикации недели (пт 18:00 МСК), проверка сервера каждые 5 мин, ключ VAPID для пушей, сразу публикует следующую неделю |
| `admin-password.sh` | Создать админа сайта или сменить ему пароль (ввод скрытый) |
| `import-students.py` | Ученики из CSV (почта, фамилия, имя, [отчество], класс). Без `--apply` только считает. Имена не печатает |
| `publish-week.py` | Создать неделю из шаблонов и опубликовать (через API админки). Существующую неделю не пересоздаёт |
| `healthcheck.sh` | Службы, `/api/health`, контейнеры, диск, свежесть бэкапа. При поломке и починке пишет модератору в бота-модератора, напоминает раз в 6 ч |
| `students-template.csv` | Пример таблицы |

Установка (скрипты лежат в `infra/server/` репозитория):
```
cd /opt/eduhub179 && git pull
mkdir -p /opt/eduhub-beta && cp infra/server/{common.sh,admin-password.sh,import-students.py,publish-week.py,healthcheck.sh,setup-beta.sh} /opt/eduhub-beta/
bash /opt/eduhub-beta/setup-beta.sh
```
Логи: `journalctl -u eduhub-publish-week -n 20`, `journalctl -u eduhub-health -n 20`.
