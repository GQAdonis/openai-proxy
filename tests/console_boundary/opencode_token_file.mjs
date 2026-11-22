import assert from "node:assert/strict"
import { chmodSync, mkdtempSync, renameSync, rmSync, symlinkSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { authorization } from "../../plugin/src/config.ts"

const root = mkdtempSync(join(tmpdir(), "opencode-token-file-"))
const file = join(root, "service-token")
const replacement = join(root, "next-token")
const link = join(root, "linked-token")
const first = "a".repeat(32)
const second = "b".repeat(512)
const denied = /^Error: A scoped service connection is required$/

try {
  delete process.env.PROXY_CONSOLE_TOKEN
  process.env.PROXY_CONSOLE_TOKEN_FILE = file
  writeFileSync(file, first + "\n", { mode: 0o600 })
  assert.equal(authorization(), `Bearer ${first}`)
  writeFileSync(replacement, second + "\r\n", { mode: 0o600 })
  renameSync(replacement, file)
  assert.equal(authorization(), `Bearer ${second}`)

  symlinkSync(file, link)
  process.env.PROXY_CONSOLE_TOKEN_FILE = link
  assert.throws(authorization, denied)
  process.env.PROXY_CONSOLE_TOKEN_FILE = "relative-token"
  assert.throws(authorization, denied)
  process.env.PROXY_CONSOLE_TOKEN_FILE = root
  assert.throws(authorization, denied)
  process.env.PROXY_CONSOLE_TOKEN_FILE = file
  chmodSync(file, 0o640)
  assert.throws(authorization, denied)
  chmodSync(file, 0o600)
  for (const invalid of ["a".repeat(31), "a".repeat(513), first + "\n" + first, "é".repeat(32)]) {
    writeFileSync(file, invalid)
    assert.throws(authorization, denied)
  }

  process.env.PROXY_CONSOLE_TOKEN_FILE = link
  process.env.PROXY_CONSOLE_TOKEN = first
  assert.equal(authorization(), `Bearer ${first}`)
} finally {
  rmSync(root, { recursive: true, force: true })
}
