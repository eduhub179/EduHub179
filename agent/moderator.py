#!/usr/bin/env python3
"""Запуск как раньше: python3 moderator.py manual_channels. Код — в eduhub_agent/telegram/moderator.py."""
import runpy

runpy.run_module("eduhub_agent.telegram.moderator", run_name="__main__", alter_sys=True)
