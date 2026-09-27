// Drives the built volt over WebDriver (tauri-driver + msedgedriver), the way
// tests/e2e/drive.mjs does, with helpers for reading the Pinia store and the
// DOM from inside the page. Shared by the three suites beside it.
import { spawn } from 'node:child_process'
import { mkdirSync } from 'node:fs'
import { homedir, tmpdir } from 'node:os'
import { join, resolve, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

export const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..')
export const exe = process.env.VOLT_EXE ?? join(repo, 'src-tauri', 'target', 'debug', process.platform === 'win32' ? 'volt.exe' : 'volt')
const driverExe = join(homedir(), '.cargo', 'bin', process.platform === 'win32' ? 'tauri-driver.exe' : 'tauri-driver')
const edge = process.env.EDGEDRIVER ?? 'msedgedriver.exe'
/** Screenshots land here, for looking at after a run. */
export const shots = process.env.E2E_SHOTS ?? join(tmpdir(), 'volt-e2e-shots')
mkdirSync(shots, { recursive: true })
const port = 4444
const base = `http://127.0.0.1:${port}`

export let failures = 0
export const results = []
export function check(label, ok, detail = '') {
  if (!ok) failures += 1
  results.push({ label, ok, detail })
  console.log(`  ${ok ? 'PASS' : 'FAIL'}  ${label}${detail !== '' && detail !== undefined ? `  — ${String(detail).slice(0, 240)}` : ''}`)
}
export const sleep = (ms) => new Promise((r) => setTimeout(r, ms))

let proc = null
export let driverLog = ''
export async function startDriver() {
  proc = spawn(driverExe, ['--port', String(port), '--native-driver', edge], { stdio: ['ignore', 'pipe', 'pipe'] })
  proc.stdout.on('data', (d) => (driverLog += d))
  proc.stderr.on('data', (d) => (driverLog += d))
  await sleep(1500)
}
export function stopDriver() { proc?.kill() }

const wd = async (method, path, body) => {
  const res = await fetch(base + path, { method, headers: { 'content-type': 'application/json' }, body: body ? JSON.stringify(body) : undefined })
  const json = await res.json()
  if (!res.ok && json.value && json.value.error) throw new Error(`${path}: ${json.value.error}: ${json.value.message}`)
  return json.value
}

const PRELUDE = `
const $ = (s, i = 0) => document.querySelectorAll(s)[i];
const $$ = (s) => [...document.querySelectorAll(s)];
const store = [...document.querySelectorAll('*')].find((e) => e.__vue_app__)?.__vue_app__.config.globalProperties.$pinia._s.get('collection');
const text = (el) => (el?.textContent ?? '').replace(/\\s+/g, ' ').trim();
const setValue = (el, v) => { const proto = el.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype : el.tagName === 'SELECT' ? HTMLSelectElement.prototype : HTMLInputElement.prototype; Object.getOwnPropertyDescriptor(proto, 'value').set.call(el, v); el.dispatchEvent(new Event(el.tagName === 'SELECT' ? 'change' : 'input', { bubbles: true })); };
const button = (label, scope = document) => [...scope.querySelectorAll('button, [role=menuitem], [role=tab], [role=radio]')].find((b) => text(b) === label || b.getAttribute('aria-label') === label) ?? [...scope.querySelectorAll('button, [role=menuitem], [role=tab], [role=radio]')].find((b) => text(b).startsWith(label));
`

export async function session(args = []) {
  const s = await wd('POST', '/session', { capabilities: { alwaysMatch: { 'tauri:options': { application: exe, args } } } })
  const id = s.sessionId
  const api = {
    id,
    /** Runs a script body in the page with helpers in scope. */
    run: (script) => wd('POST', `/session/${id}/execute/sync`, { script: PRELUDE + script, args: [] }),
    /** Runs an async body; `return` its value. */
    runAsync: (script) => wd('POST', `/session/${id}/execute/async`, { script: `const done = arguments[arguments.length - 1]; (async () => { ${PRELUDE} ${script} })().then(done, (e) => done('ERROR: ' + (e && e.message || e)));`, args: [] }),
    async until(label, script, timeoutMs = 15000) {
      const start = Date.now()
      let last
      while (Date.now() - start < timeoutMs) {
        try { last = await api.run(script) } catch (e) { last = null; api.lastError = e.message }
        if (last) return last
        await sleep(200)
      }
      throw new Error(`timed out waiting for ${label}${api.lastError ? ' (' + api.lastError.slice(0, 160) + ')' : ''}`)
    },
    /** Types into the n-th element matching a selector, key by key, like a person. */
    async type(selector, textValue, index = 0) {
      const els = await wd('POST', `/session/${id}/elements`, { using: 'css selector', value: selector })
      const el = els[index]
      if (!el) throw new Error(`no element ${selector}[${index}]`)
      const eid = Object.values(el)[0]
      await wd('POST', `/session/${id}/element/${eid}/click`, {})
      await wd('POST', `/session/${id}/element/${eid}/value`, { text: textValue })
    },
    async keys(textValue) {
      await wd('POST', `/session/${id}/actions`, { actions: [{ type: 'key', id: 'kb', actions: [...textValue].flatMap((k) => [{ type: 'keyDown', value: k }, { type: 'keyUp', value: k }]) }] })
    },
    async chord(...keysDown) {
      const down = keysDown.map((k) => ({ type: 'keyDown', value: k }))
      const up = [...keysDown].reverse().map((k) => ({ type: 'keyUp', value: k }))
      await wd('POST', `/session/${id}/actions`, { actions: [{ type: 'key', id: 'kb', actions: [...down, ...up] }] })
    },
    async screenshot(path) {
      const b64 = await wd('GET', `/session/${id}/screenshot`)
      const { writeFileSync } = await import('node:fs')
      writeFileSync(path, Buffer.from(b64, 'base64'))
    },
    close: () => wd('DELETE', `/session/${id}`).catch(() => {}),
  }
  return api
}

export const KEY = { enter: '\uE007', esc: '\uE00C', ctrl: '\uE009', shift: '\uE008', tab: '\uE004', backspace: '\uE003' }

export async function step(label, fn) {
  console.log(`
# ${label}`)
  try { await fn() } catch (e) { check(`${label}: completed`, false, e.message + " @ " + (e.stack.split(String.fromCharCode(10)).find((l) => l.includes("run.mjs")) ?? "")) }
}
