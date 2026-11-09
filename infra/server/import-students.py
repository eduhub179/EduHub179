#!/usr/bin/env python3
"""Импорт учеников из таблицы (CSV) в базу EduHub179.

    python3 /opt/eduhub-beta/import-students.py students.csv           # только посчитать, ничего не менять
    python3 /opt/eduhub-beta/import-students.py students.csv --apply   # записать

Столбцы (первая строка — заголовки, порядок любой, разделитель , или ;):
    почта (или логин), фамилия, имя, [отчество], класс (11Б, 11 Б, 2027б)
Без заголовков считается порядок: почта, фамилия, имя, класс.

Новые ученики создаются без пароля: входят в бота и на сайт по коду с почты.
Существующему ученику обновляются ФИО и класс (пароль и привязка к Telegram не трогаются).
Учителей и админов скрипт не меняет. На экран выводятся только количества и названия классов,
имена никуда не печатаются.
"""
import csv, datetime, os, re, shlex, subprocess, sys

PSQL = os.environ.get("PSQL", "docker exec -i infra-postgres-1 psql -U postgres -d eduhub179")
DOMAIN = "@179.ru"
LATIN = str.maketrans({"b": "б", "v": "в", "i": "и", "a": "а", "g": "г", "d": "д", "e": "е"})
HEAD = {
    "login": ("почта", "email", "e-mail", "mail", "логин", "login"),
    "last": ("фамилия", "last"),
    "first": ("имя", "first", "name"),
    "middle": ("отчество", "middle"),
    "class": ("класс", "class"),
}


def school_end_year(today):
    return today.year + 1 if today.month >= 9 else today.year


def parse_class(raw, today):
    """'11Б' / '11 б' / '11-B' / '2027б' -> (год выпуска, буква) или None."""
    s = raw.strip().lower().replace(" ", "").replace("-", "").translate(LATIN)
    m = re.fullmatch(r"(\d{1,4})([а-яё])", s)
    if not m:
        return None
    num, letter = int(m.group(1)), m.group(2)
    if 1 <= num <= 11:
        return school_end_year(today) + 11 - num, letter
    if 2000 <= num <= 2100:
        return num, letter
    return None


def parse_login(raw):
    s = raw.strip().lower()
    if "@" in s:
        if not s.endswith(DOMAIN):
            return None
        s = s[: -len(DOMAIN)]
    return s if re.fullmatch(r"[a-z0-9._-]{1,64}", s) else None


def columns(header):
    cols = {}
    for i, h in enumerate(header):
        h = h.strip().lower()
        for key, names in HEAD.items():
            if key not in cols and any(h.startswith(n) for n in names):
                cols[key] = i
                break
    return cols


def q(s):
    return "NULL" if s is None else "'" + s.replace("'", "''") + "'"


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    apply = "--apply" in sys.argv
    if len(args) != 1:
        print(__doc__)
        sys.exit(1)
    text = open(args[0], encoding="utf-8-sig").read()
    dialect = csv.Sniffer().sniff(text[:4096], delimiters=",;\t")
    rows = [r for r in csv.reader(text.splitlines(), dialect) if any(c.strip() for c in r)]
    cols = columns(rows[0])
    first_row = 1
    if {"login", "last", "first", "class"} <= cols.keys():
        rows, first_row = rows[1:], 2
    else:
        cols = {"login": 0, "last": 1, "first": 2, "class": 3}
        print("Заголовков не нашёл, беру порядок: почта, фамилия, имя, класс")

    today = datetime.date.today()
    bad, students = [], {}
    for n, r in enumerate(rows, start=first_row):
        get = lambda k: r[cols[k]].strip() if k in cols and cols[k] < len(r) else ""
        login, cls = parse_login(get("login")), parse_class(get("class"), today)
        last, first, middle = get("last"), get("first"), get("middle") or None
        if not (login and cls and last and first):
            bad.append(n)
            continue
        students[login] = (last[:100], first[:100], middle[:100] if middle else None, cls)
    dup = len(rows) - len(bad) - len(students)

    values = ",\n".join(
        f"({q(l)},{q(a)},{q(b)},{q(c)},{y},{q(k)})" for l, (a, b, c, (y, k)) in students.items()
    )
    sql = f"""
\\set ON_ERROR_STOP on
BEGIN;
CREATE TEMP TABLE imp (login text, last_name text, first_name text, middle_name text, gy int, letter text) ON COMMIT DROP;
{"INSERT INTO imp VALUES " + values + ";" if values else ""}
CREATE TEMP TABLE m ON COMMIT DROP AS
  SELECT i.*, c.class_id, u.user_id, u.role::text AS role
  FROM imp i
  LEFT JOIN classes c ON c.graduation_year = i.gy AND c.letter::text = i.letter
  LEFT JOIN users u ON u.login = i.login;
SELECT 'new', count(*) FROM m WHERE class_id IS NOT NULL AND user_id IS NULL
UNION ALL SELECT 'update', count(*) FROM m WHERE class_id IS NOT NULL AND role = 'student'
UNION ALL SELECT 'not_student', count(*) FROM m WHERE role IN ('teacher', 'admin')
UNION ALL SELECT 'no_class', count(*) FROM m WHERE class_id IS NULL AND coalesce(role, 'student') = 'student';
SELECT 'missing_class', gy || letter, count(*) FROM m WHERE class_id IS NULL GROUP BY 2 ORDER BY 2;
"""
    if apply:
        sql += """
INSERT INTO users (login, role, last_name, first_name, middle_name, class_id, is_active)
  SELECT login, 'student', last_name, first_name, middle_name, class_id, TRUE
  FROM m WHERE class_id IS NOT NULL AND user_id IS NULL;
UPDATE users u SET last_name = m.last_name, first_name = m.first_name, middle_name = m.middle_name,
                   class_id = m.class_id, is_active = TRUE
  FROM m WHERE m.user_id = u.user_id AND m.role = 'student' AND m.class_id IS NOT NULL;
COMMIT;
"""
    else:
        sql += "ROLLBACK;\n"
    out = subprocess.run(shlex.split(PSQL) + ["-qAt", "-F", "|"], input=sql, text=True,
                         capture_output=True)
    if out.returncode != 0:
        print("Ошибка базы:", out.stderr.strip()[-500:])
        sys.exit(1)

    end = school_end_year(today)
    res = {}
    for line in out.stdout.splitlines():
        p = line.split("|")
        if p[0] == "missing_class":
            gy, letter = int(p[1][:4]), p[1][4:]
            res.setdefault("missing", []).append(f"{end + 11 - gy}{letter.upper()} ({p[2]})")
        elif len(p) == 2:
            res[p[0]] = int(p[1])
    print(f"Строк в файле: {len(rows)}")
    print(f"  новых учеников: {res.get('new', 0)}")
    print(f"  уже были, обновить ФИО и класс: {res.get('update', 0)}")
    if res.get("not_student"):
        print(f"  логин занят учителем или админом, пропущено: {res['not_student']}")
    if res.get("no_class"):
        print(f"  класса нет в базе, пропущено: {res['no_class']}  ->  {', '.join(res.get('missing', []))}")
    if dup:
        print(f"  повторы одного логина (взята последняя строка): {dup}")
    if bad:
        print(f"  не разобрал строки (нет почты @179.ru, ФИО или класса): {len(bad)}, номера: {bad[:30]}")
    print("Записано в базу." if apply else "Это проверка, база не менялась. Чтобы записать, добавь --apply")


if __name__ == "__main__":
    main()
