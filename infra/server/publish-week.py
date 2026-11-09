#!/usr/bin/env python3
"""Создать (из шаблонов расписания) и опубликовать неделю через API админки.

    python3 /opt/eduhub-beta/publish-week.py              # следующая неделя
    python3 /opt/eduhub-beta/publish-week.py 2026-11-16   # конкретный понедельник

Входит служебным админом, логин и пароль которого лежат в /root/.eduhub-weeks (chmod 600,
создаёт setup-beta.sh). Если неделя уже есть, она не пересоздаётся (замены не теряются),
черновик просто публикуется. Запускается таймером eduhub-publish-week по пятницам.
"""
import datetime, json, os, sys, urllib.error, urllib.request

API = os.environ.get("API", "http://127.0.0.1:8080")
CREDS = os.environ.get("CREDS", "/root/.eduhub-weeks")


def call(method, path, body=None, cookie=None):
    req = urllib.request.Request(API + path, method=method,
                                 data=None if body is None else json.dumps(body).encode(),
                                 headers={"Content-Type": "application/json", "User-Agent": "eduhub-publish-week"})
    if cookie:
        req.add_header("Cookie", cookie)
    try:
        with urllib.request.urlopen(req, timeout=60) as r:
            return r.status, r.headers, json.loads(r.read() or b"null")
    except urllib.error.HTTPError as e:
        return e.code, e.headers, (e.read() or b"").decode(errors="replace")[:300]


def login():
    creds = dict(l.split("=", 1) for l in open(CREDS).read().split() if "=" in l)
    st, _, body = call("POST", "/api/auth/login", {"login": creds["LOGIN"], "password": creds["PASSWORD"]})
    if st != 200:
        sys.exit(f"Вход не удался: HTTP {st} {body}")
    st, headers, body = call("POST", "/api/auth/session/init",
                             {"user_id": body["user"]["id"], "auth_token": body["token"], "remember_me": False})
    if st >= 300:
        sys.exit(f"Сессия не создана: HTTP {st} {body}")
    cookies = [h.split(";", 1)[0] for h in headers.get_all("Set-Cookie") or []]
    if not cookies:
        sys.exit("Сервер не выдал cookie сессии")
    return "; ".join(cookies)


def main():
    if len(sys.argv) > 1:
        monday = datetime.date.fromisoformat(sys.argv[1])
    else:
        today = datetime.datetime.now(datetime.timezone(datetime.timedelta(hours=3))).date()
        monday = today + datetime.timedelta(days=7 - today.weekday())
    if monday.weekday() != 0:
        sys.exit(f"{monday} не понедельник")
    cookie = login()
    st, _, body = call("GET", "/api/admin/weeks", cookie=cookie)
    if st != 200:
        sys.exit(f"Список недель: HTTP {st} {body}")
    week = next((w for w in body["weeks"] if w["week_start_date"] == monday.isoformat()), None)
    if week is None:
        st, _, body = call("POST", "/api/admin/weeks",
                           {"week_start_date": monday.isoformat(), "source": "templates"}, cookie=cookie)
        if st not in (200, 201):
            sys.exit(f"Неделя {monday} не создана: HTTP {st} {body}")
        print(f"Неделя {monday} создана из шаблонов")
        week = body
    if week["status"] != "published":
        st, _, body = call("PATCH", f"/api/admin/weeks/{monday.isoformat()}", {"status": "published"}, cookie=cookie)
        if st != 200:
            sys.exit(f"Неделя {monday} не опубликована: HTTP {st} {body}")
        print(f"Неделя {monday} опубликована")
    else:
        print(f"Неделя {monday} уже опубликована")


if __name__ == "__main__":
    main()
