"""Транспорт: настоящий HTTP или записанные ответы.

Смысл в том, чтобы DriveSource не знал, откуда берутся JSON-ы. В тестах вместо сети
подставляется FixtureTransport с ключами вида «changes:TOKEN», «file:FOLDER_ID», «start»,
и любой промах по ключу — ошибка, а не тихий поход в интернет.
"""
import json
import urllib.parse
import urllib.request


class UrllibTransport:
    """Настоящий Google: только чтение (GET), токен в заголовке."""

    def __init__(self, access_token=None, timeout=60):
        self.access_token = access_token
        self.timeout = timeout

    def get(self, url, params=None, headers=None):
        if params:
            url = url + "?" + urllib.parse.urlencode(params)
        hdrs = {"Authorization": f"Bearer {self.access_token}"} if self.access_token else {}
        hdrs.update(headers or {})
        req = urllib.request.Request(url, headers=hdrs)
        with urllib.request.urlopen(req, timeout=self.timeout) as r:
            return json.loads(r.read())


class FixtureTransport:
    """Записанные ответы, разложенные по ключам. Считает вызовы — тесты это проверяют."""

    def __init__(self, routes):
        self.routes = dict(routes)
        self.hits = []

    @staticmethod
    def key(url, params):
        params = params or {}
        if url.endswith("/changes/startPageToken"):
            return "start"
        if url.endswith("/changes"):
            return "changes:" + str(params.get("pageToken") or "")
        if url.endswith("/files"):
            return "files:" + str(params.get("pageToken") or "")
        if "/files/" in url:
            return "file:" + url.rsplit("/", 1)[-1]
        return url

    def get(self, url, params=None, headers=None):
        k = self.key(url, params)
        self.hits.append(k)
        if k not in self.routes:
            raise AssertionError(f"фикстура не знает запрос {k!r} (есть: {sorted(self.routes)})")
        resp = self.routes[k]
        return resp() if callable(resp) else resp
