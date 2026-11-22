import { closeSync, constants, fstatSync, openSync, readSync } from "node:fs"
import { isAbsolute } from "node:path"

const maxTokenBytes = 512
const maxFileBytes = maxTokenBytes + 2 // Optional CRLF terminator.

export function serviceToken(): string {
  const token = process.env.PROXY_CONSOLE_TOKEN
  if (token) return token

  const path = process.env.PROXY_CONSOLE_TOKEN_FILE
  try {
    if (!path || !isAbsolute(path) || process.getuid === undefined) throw new Error()
    const fd = openSync(path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK)
    try {
      const stat = fstatSync(fd)
      if (!stat.isFile() || stat.uid !== process.getuid() || (stat.mode & 0o077) !== 0 || stat.size > maxFileBytes) {
        throw new Error()
      }
      const buffer = Buffer.alloc(maxFileBytes + 1)
      let bytes = 0
      while (bytes < buffer.length) {
        const read = readSync(fd, buffer, bytes, buffer.length - bytes, null)
        if (read === 0) break
        bytes += read
      }
      const value = buffer.toString("utf8", 0, bytes).replace(/\r?\n$/, "")
      if (value.length < 32 || value.length > maxTokenBytes || /[^\x21-\x7e]/.test(value)) throw new Error()
      return value
    } finally {
      closeSync(fd)
    }
  } catch {
    throw new Error("A scoped service connection is required")
  }
}
