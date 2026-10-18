#!/usr/bin/env python3
"""Запуск как раньше: python3 backend_api.py retry. Код — в eduhub_agent/server/backend_api.py."""
import runpy

runpy.run_module("eduhub_agent.server.backend_api", run_name="__main__", alter_sys=True)
