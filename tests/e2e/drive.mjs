// The real thing, end to end: the built volt.exe driven over WebDriver
// through tauri-driver. It opens a throwaway collection, makes a request from
// the sidebar, sends it across the real network, reads the status back off
// the screen and checks the YAML that Save wrote. Nothing is stubbed.
//
// Needs, once:
//   cargo install tauri-driver --locked
//   an msedgedriver matching the installed WebView2, at EDGEDRIVER
//   pnpm tauri build --debug --no-bundle       (the binary with the SPA embedded)
// Then: node tests/e2e/drive.mjs
//
// Environment: VOLT_EXE (default src-tauri/target/debug/volt.exe), EDGEDRIVER
// (default msedgedriver.exe on PATH), E2E_URL (default a public JSON endpoint).
import { spawn } from 'node:child_process'
import { mkdirSync, rmSync, writeFileSync, existsSync, readFileSync } from 'node:fs'
import { homedir, tmpdir } from 'node:os'
import { join, resolve, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..')
const exe = process.env.VOLT_EXE ?? join(repo, 'src-tauri', 'target', 'debug', 'volt.exe')
const driver = join(homedir(), '.cargo', 'bin', process.platform === 'win32' ? 'tauri-driver.exe' : 'tauri-driver')
const edge = process.env.EDGEDRIVER ?? 'msedgedriver.exe'
const target = process.env.E2E_URL ?? 'https://jsonplaceholder.typicode.com/todos/1'
const port = 4444

if (!existsSync(exe)) {
  console.error(`no binary at ${exe} — run: pnpm tauri build --debug --no-bundle`)
  process.exit(2)
}

// A collection of its own, so nobody's is touched. It is handed to volt as an
// argument, which `startup_collection` opens instead of the last one.
const collection = join(tmpdir(), `volt-e2e-${process.pid}`)
rmSync(collection, { recursive: true, force: true })
mkdirSync(join(collection, 'environments'), { recursive: true })
writeFileSync(join(collection, 'collection.yaml'), 'name: E2E\nversion: 1\n')
writeFileSync(join(collection, 'environments', 'local.yaml'), 'name: local\nvars: []\n')

let failures = 0
const check = (label, ok, detail = '') => {
  if (!ok) failures += 1
  console.log(`  ${ok ? 'PASS' : 'FAIL'}  ${label}${detail ? `  — ${detail}` : ''}`)
}

const proc = spawn(driver, ['--port', String(port), '--native-driver', edge], { stdio: ['ignore', 'pipe', 'pipe'] })
let driverLog = ''
proc.stdout.on('data', (d) => (driverLog += d))
proc.stderr.on('data', (d) => (driverLog += d))
await new Promise((r) => setTimeout(r, 1500))

const base = `http://127.0.0.1:${port}`
const wd = async (method, path, body) => {
  const res = await fetch(base + path, { method, headers: { 'content-type': 'application/json' }, body: body ? JSON.stringify(body) : undefined })
  const json = await res.json()
  if (json.value && json.value.error) throw new Error(`${path}: ${json.value.error}: ${json.value.message}`)
  return json.value
}

let id = null
try {
  const session = await wd('POST', '/session', {
    capabilities: { alwaysMatch: { 'tauri:options': { application: exe, args: [collection] } } },
  })
  id = session.sessionId
  const run = (script) => wd('POST', `/session/${id}/execute/sync`, { script, args: [] })
  const until = async (label, script, timeoutMs = 20000) => {
    const start = Date.now()
    while (Date.now() - start < timeoutMs) {
      const out = await run(script)
      if (out) return out
      await new Promise((r) => setTimeout(r, 250))
    }
    throw new Error(`timed out waiting for ${label}`)
  }

  const title = await until('the app', `const t = document.querySelector('.app-bar .name')?.textContent?.trim(); return t && t !== 'No collection' ? t : null`)
  check('the collection named on the command line is open', title === 'E2E', title)
  if (title !== 'E2E') throw new Error(`not the throwaway collection; stopping before touching ${title}`)

  await run(`document.querySelector('.sidebar-tools .new').click()`)
  await until('the editor', `return document.querySelector('section.request') ? 1 : null`)
  check('New makes a request file on disk', existsSync(join(collection, 'new-request.yaml')))

  // The URL is typed through the element, the way a person does it.
  const url = await wd('POST', `/session/${id}/element`, { using: 'css selector', value: 'input[aria-label="URL"]' })
  const urlId = Object.values(url)[0]
  await wd('POST', `/session/${id}/element/${urlId}/click`, {})
  await wd('POST', `/session/${id}/element/${urlId}/value`, { text: target })
  await run(`document.querySelector('button.send').click()`)

  const status = await until('a response', `const s = document.querySelector('.response .status'); return s && /\\d{3}/.test(s.textContent) ? s.textContent.trim() : null`, 30000)
  check('the request went out and came back over the real network', /^200/.test(status), status)
  const body = await run(`return document.querySelector('.viewer')?.textContent || ''`)
  check('and a body came with it', body.trim().length > 0, body.slice(0, 80).replace(/\s+/g, ' '))

  await run(`document.querySelector('button[aria-label="Save request"]').click()`)
  await new Promise((r) => setTimeout(r, 600))
  const yaml = readFileSync(join(collection, 'new-request.yaml'), 'utf8')
  check('Save wrote the URL into the YAML', yaml.includes(target), yaml.split('\n').slice(0, 4).join(' | '))
} catch (e) {
  check('the drive completed', false, String(e.message).slice(0, 300))
} finally {
  if (id) await wd('DELETE', `/session/${id}`).catch(() => {})
  proc.kill()
  rmSync(collection, { recursive: true, force: true })
}
if (failures) console.log(driverLog.slice(-4000))
console.log(failures ? `${failures} FAILED` : 'ALL PASSED')
process.exitCode = failures ? 1 : 0
