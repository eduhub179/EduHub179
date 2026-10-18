#!/usr/bin/env python3
"""Запуск как раньше: python3 watch_channel.py loop. Код — в eduhub_agent/telegram/watch_channel.py."""
import runpy

runpy.run_module("eduhub_agent.telegram.watch_channel", run_name="__main__", alter_sys=True)
