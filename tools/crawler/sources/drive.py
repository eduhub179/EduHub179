"""Источник «Google Drive»: только изменения, без переливания диска.

Два режима, и они принципиально разные:

  * full_scan()  — первый запуск (или --full): обойти папки и записать отпечатки;
  * changes()    — дальше: спросить у Drive «что изменилось с моего токена».

Токен — это `startPageToken`/`newStartPageToken` из `changes.list`. Он сохраняется только
после обработки последней страницы: упали в середине — следующий запуск повторит пачку,
и ничего не потеряется. Дубликаты отсекает состояние (id + отпечаток), а не Google.
"""
import urllib.parse

API = "https://www.googleapis.com/drive/v3"
FOLDER_MIME = "application/vnd.google-apps.folder"
FILE_FIELDS = "id,name,mimeType,size,modifiedTime,md5Checksum,version,parents,trashed"
CHANGE_FIELDS = f"nextPageToken,newStartPageToken,changes(changeType,fileId,removed,file({FILE_FIELDS}))"


class DriveSource:
    name = "drive"
    incremental = True

    def __init__(self, transport, folder_ids=None, page_size=1000):
        self.transport = transport
        self.folder_ids = list(folder_ids or [])
        self.page_size = page_size

    # --- служебное -----------------------------------------------------------------
    def _get(self, path, **params):
        return self.transport.get(API + path, params={k: v for k, v in params.items() if v is not None})

    def start_token(self):
        """Точка отсчёта «сейчас». С неё начинается инкрементальный режим."""
        return self._get("/changes/startPageToken", supportsAllDrives="true")["startPageToken"]

    def _item(self, f, state):
        return {
            "id": f["id"],
            "name": f.get("name", ""),
            "path": self._path_of(f, state),
            "size": int(f.get("size") or 0),
            "mtime": f.get("modifiedTime"),
            # у Google-документов md5 нет — отличие даёт version
            "hash": f.get("md5Checksum") or f"v{f.get('version', '')}",
            "mime": f.get("mimeType", ""),
        }

    # --- путь из id ------------------------------------------------------------------
    def _folder_name(self, folder_id, state):
        cached = state.folder(folder_id)
        if cached is not None:
            return cached
        resp = self._get(f"/files/{folder_id}", fields="id,name,parents", supportsAllDrives="true")
        name = resp.get("name", "")
        state.put_folder(folder_id, name)
        return name

    def _path_of(self, f, state):
        """Собирает «Алгебра/9 класс/ДЗ-15.pdf», поднимаясь по parents.

        Google отдаёт только id папок. Без этого шага классификатор видит одно имя файла
        и теряет предмет с классом — папка «Алгебра» и есть главный сигнал.
        Имена и parents кешируются в состоянии: одна папка встречается десятки раз.
        """
        parts = [f.get("name", "")]
        parents = f.get("parents") or []
        depth = 0
        while parents and depth < 25:                 # 25 — защита от петли в parents
            folder_id = parents[0]
            if folder_id in self.folder_ids:          # корень в путь не пишем
                break
            info = state.folder(folder_id)
            if info is None:
                resp = self._get(f"/files/{folder_id}", fields="id,name,parents",
                                 supportsAllDrives="true")
                info = {"name": resp.get("name", ""), "parents": resp.get("parents") or []}
                state.put_folder(folder_id, info["name"], info["parents"])
            parts.insert(0, info["name"])
            parents = info.get("parents") or []
            depth += 1
        return "/".join(p for p in parts if p)

    # --- первый запуск ----------------------------------------------------------------
    def full_scan(self, state):
        """Обход дерева папок. Используется один раз, на первом запуске (бэкфилл)."""
        queue = list(self.folder_ids)
        seen_folders = set()
        while queue:
            folder_id = queue.pop(0)
            if folder_id in seen_folders:
                continue
            seen_folders.add(folder_id)
            page_token = None
            while True:
                resp = self._get(f"/files", q=f"'{folder_id}' in parents and trashed=false",
                                 fields=f"nextPageToken,files({FILE_FIELDS})",
                                 pageSize=self.page_size, pageToken=page_token,
                                 supportsAllDrives="true", includeItemsFromAllDrives="true")
                for f in resp.get("files", []):
                    if f.get("mimeType") == FOLDER_MIME:
                        queue.append(f["id"])
                    else:
                        yield self._item(f, state)
                page_token = resp.get("nextPageToken")
                if not page_token:
                    break

    # --- дальше только изменения --------------------------------------------------------
    def changes(self, cursor, state):
        """Возвращает (список изменений, новый курсор).

        change.kind: created/updated решает движок по состоянию (Google говорит просто
        «файл изменился»), удаление приходит явно — removed=true или trashed.
        """
        changes, page_token = [], cursor
        while True:
            resp = self._get("/changes", pageToken=page_token, pageSize=self.page_size,
                             includeRemoved="true", includeItemsFromAllDrives="true",
                             supportsAllDrives="true", fields=CHANGE_FIELDS)
            for ch in resp.get("changes", []):
                f = ch.get("file") or {}
                if ch.get("removed") or f.get("trashed"):
                    changes.append({"kind": "deleted", "id": ch.get("fileId"),
                                    "name": f.get("name", "")})
                    continue
                if f.get("mimeType") == FOLDER_MIME:
                    state.put_folder(f["id"], f.get("name", ""), f.get("parents"))
                    continue
                changes.append({"kind": "file", "item": self._item(f, state)})
            nxt = resp.get("nextPageToken")
            if nxt:
                page_token = nxt
                continue
            new_cursor = resp.get("newStartPageToken") or page_token
            return changes, new_cursor
