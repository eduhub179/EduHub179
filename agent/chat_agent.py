#!/usr/bin/env python3
"""Запуск как раньше: python3 chat_agent.py live. Код — в eduhub_agent/telegram/chat_agent.py."""
import runpy

runpy.run_module("eduhub_agent.telegram.chat_agent", run_name="__main__", alter_sys=True)
