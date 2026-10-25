import { randomUUID } from 'node:crypto'
import { mkdir, readdir, readFile, writeFile } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { defineConfig, type Connect, type Plugin } from 'vite'
import react from '@vitejs/plugin-react'

const mathTopics = new Set(['algebra', 'geometry', 'other'])
const maxUploadSize = 25 * 1024 * 1024
const mathUploadsDirectory = fileURLToPath(new URL('./src/local_data/math_uploads', import.meta.url))

const mathFilesApi = (): Plugin => {
  const handler: Connect.NextHandleFunction = (request, response, next) => {
      const url = new URL(request.url ?? '/', 'http://localhost')
      const segments = url.pathname.split('/').filter(Boolean)

      if (segments[0] !== 'api' || segments[1] !== 'math' || segments[2] !== 'files') {
        next()
        return
      }

      const sendJson = (status: number, data: unknown) => {
        response.statusCode = status
        response.setHeader('Content-Type', 'application/json; charset=utf-8')
        response.end(JSON.stringify(data))
      }

      void (async () => {
        const topicId = segments.length > 3 ? decodeURIComponent(segments[3]) : url.searchParams.get('topic')
        if (!topicId || !mathTopics.has(topicId)) {
          sendJson(400, { error: 'Неизвестная тема.' })
          return
        }

        const topicDirectory = path.join(mathUploadsDirectory, topicId)
        await mkdir(topicDirectory, { recursive: true })

        if (request.method === 'GET' && segments.length === 3) {
          const entries = await readdir(topicDirectory, { withFileTypes: true })
          const files = entries
            .filter((entry) => entry.isFile() && entry.name.includes('--'))
            .map((entry) => {
              const separator = entry.name.indexOf('--')
              return {
                id: entry.name.slice(0, separator),
                name: entry.name.slice(separator + 2),
                url: `/api/math/files/${encodeURIComponent(topicId)}/${encodeURIComponent(entry.name)}`,
              }
            })
            .sort((left, right) => left.name.localeCompare(right.name))
          sendJson(200, files)
          return
        }

        if (request.method === 'GET' && segments.length === 5) {
          const storedName = decodeURIComponent(segments[4])
          if (!/^[\da-f-]{36}--[\p{L}\p{N}._ -]+$/iu.test(storedName)) {
            sendJson(400, { error: 'Некорректное имя файла.' })
            return
          }
          const bytes = await readFile(path.join(topicDirectory, storedName))
          const originalName = storedName.slice(storedName.indexOf('--') + 2)
          response.statusCode = 200
          response.setHeader('Content-Type', 'application/octet-stream')
          response.setHeader(
            'Content-Disposition',
            `attachment; filename*=UTF-8''${encodeURIComponent(originalName)}`,
          )
          response.end(bytes)
          return
        }

        if (request.method === 'POST' && segments.length === 4) {
          const requestedName = url.searchParams.get('filename')?.trim()
          const safeName = requestedName
            ?.replace(/[\\/]/g, '_')
            .replace(/[^\p{L}\p{N}._ -]/gu, '_')
            .trim()
          if (!safeName) {
            sendJson(400, { error: 'Укажите корректное имя файла.' })
            return
          }

          const chunks: Buffer[] = []
          let size = 0
          for await (const chunk of request) {
            const buffer = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk)
            size += buffer.length
            if (size > maxUploadSize) {
              sendJson(413, { error: 'Размер файла не должен превышать 25 МБ.' })
              return
            }
            chunks.push(buffer)
          }
          if (size === 0) {
            sendJson(400, { error: 'Нельзя загрузить пустой файл.' })
            return
          }

          const storedName = `${randomUUID()}--${safeName}`
          await writeFile(path.join(topicDirectory, storedName), Buffer.concat(chunks), { flag: 'wx' })
          sendJson(201, { id: storedName.slice(0, 36), name: safeName })
          return
        }

        sendJson(405, { error: 'Метод не поддерживается.' })
      })().catch((error: unknown) => {
        console.error('Ошибка API файлов по математике:', error)
        if (!response.headersSent) {
          sendJson(500, { error: 'Не удалось обработать файл.' })
        } else {
          response.end()
        }
      })
    }

  return {
    name: 'math-files-api',
    configureServer(server) {
      server.middlewares.use(handler)
    },
    configurePreviewServer(server) {
      server.middlewares.use(handler)
    },
  }
}

export default defineConfig({
  plugins: [react(), mathFilesApi()],
  server: {
    host: '0.0.0.0',
    // The site calls the API on its own origin (/api/...): Vite forwards it to the backend.
    // Works from a phone in the same Wi-Fi (http://<computer-ip>:5173) without opening the
    // backend port, and needs no CORS. /api/math/* is still served by the plugin above.
    proxy: {
      '/api': { target: 'http://127.0.0.1:8080', changeOrigin: false },
      '/health': { target: 'http://127.0.0.1:8080', changeOrigin: false },
    },
  },
})