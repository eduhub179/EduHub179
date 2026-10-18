"""Настройки: переменные окружения из процесса или из .env в корне agent/.

Раньше env() жил в moderator.py, и половина модулей тянула бота-модератора только за ним.
"""
import os

from eduhub_agent.paths import ENV_FILE


def env(key, default=None):
    if os.environ.get(key):
        return os.environ[key]
    try:
        for line in open(ENV_FILE, encoding="utf-8"):
            k, _, v = line.strip().partition("=")
            if k.strip() == key:
                return v.strip().strip('"').strip("'")
    except FileNotFoundError:
        pass
    return default
