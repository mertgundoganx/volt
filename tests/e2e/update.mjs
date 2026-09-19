// The updater with nothing stubbed. A debug build that calls itself an older
// version is pointed at a local latest.json that names the Windows installer
// of the newest published release and its signature (taken from GitHub), and
// is driven to Settings → Install and restart. If the signature verifies and
// the installer runs, the per-user install in %LOCALAPPDATA%\Volt changes to
// that version.
//
// Needs, beside tests/e2e/drive.mjs's driver setup:
//   volt installed from an older release's NSIS -setup.exe (silently: /S)
//   a build with the version and endpoint overridden —
//     override.json: {"version":"0.0.1","plugins":{"updater":{"endpoints":
//       ["http://127.0.0.1:8765/latest.json"],"dangerousInsecureTransportProtocol":true}}}
//     pnpm tauri build --debug --no-bundle --config override.json
// Then: EDGEDRIVER=path/to/msedgedriver.exe node tests/e2e/update.mjs
//
// Environment: VOLT_EXE, EDGEDRIVER, UPDATE_OWNER (GitHub account, default
// mertgundoganx). Keep the WebDriver session open until the version on disk
// changes: closing it kills the app mid-download.
import { spawn, execSync } from 'node:child_process'
import { createServer } from 'node:http'
import { homedir } from 'node:os'
import { join, resolve, dirname } from 'node:path'
import { statSync, existsSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..')
const exe = process.env.VOLT_EXE ?? join(repo, 'src-tauri', 'target', 'debug', 'volt.exe')
const installed = join(process.env.LOCALAPPDATA ?? '', 'Volt', 'volt.exe')
const driver = join(homedir(), '.cargo', 'bin', 'tauri-driver.exe')
const edge = process.env.EDGEDRIVER ?? 'msedgedriver.exe'
const owner = process.env.UPDATE_OWNER ?? 'mertgundoganx'
const port = 4444

let failures = 0
const check = (label, ok, detail = '') => { if (!ok) failures += 1; console.log(`  ${ok ? 'PASS' : 'FAIL'}  ${label}${detail ? `  — ${detail}` : ''}`) }
const installedVersion = () => {
  try {
    const out = execSync('reg query "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\volt" /v DisplayVersion', { encoding: 'utf8' })
    return out.match(/DisplayVersion\s+REG_SZ\s+(\S+)/)?.[1] ?? null
  } catch { return null }
}
if (!existsSync(exe) || !existsSync(installed)) {
  console.error(`needs ${exe} and an installed volt at ${installed}`)
  process.exit(2)
}
const before = installedVersion()
const sizeBefore = statSync(installed).size
console.log(`  installed before: ${before} (${sizeBefore} bytes)`)

// The newest published release, as the real endpoint describes it.
const published = await (await fetch(`https://github.com/${owner}/volt/releases/latest/download/latest.json`)).json()
const windows = published.platforms['windows-x86_64']
check('the published latest.json names a Windows installer', !!windows?.url && !!windows?.signature, windows?.url)
check('and it is the NSIS installer, which updates the per-user install in place', /setup\.exe$/.test(windows?.url ?? ''), windows?.url)
const target = published.version
console.log(`  published: ${target}`)

const server = createServer((_, res) => {
  res.setHeader('content-type', 'application/json')
  res.end(JSON.stringify(published))
}).listen(8765, '127.0.0.1')

const proc = spawn(driver, ['--port', String(port), '--native-driver', edge], { stdio: ['ignore', 'pipe', 'pipe'] })
let driverLog = ''
proc.stdout.on('data', (d) => (driverLog += d))
proc.stderr.on('data', (d) => (driverLog += d))
await new Promise((r) => setTimeout(r, 3000))

const base = `http://127.0.0.1:${port}`
const wd = async (method, path, body) => {
  const res = await fetch(base + path, { method, headers: { 'content-type': 'application/json' }, body: body ? JSON.stringify(body) : undefined })
  const json = await res.json()
  if (json.value && json.value.error) throw new Error(`${path}: ${json.value.error}: ${json.value.message}`)
  return json.value
}
let id = null
try {
  const session = await wd('POST', '/session', { capabilities: { alwaysMatch: { 'tauri:options': { application: exe } } } })
  id = session.sessionId
  const run = (script) => wd('POST', `/session/${id}/execute/sync`, { script, args: [] })
  const until = async (label, script, timeoutMs = 30000) => {
    const start = Date.now()
    while (Date.now() - start < timeoutMs) {
      let out = null
      try { out = await run(script) } catch (e) { if (/invalid session|not connected/.test(String(e.message))) throw e }
      if (out) return out
      await new Promise((r) => setTimeout(r, 400))
    }
    throw new Error(`timed out waiting for ${label}`)
  }
  await until('the app', `return document.querySelector('.app-bar') ? 1 : null`)
  const toast = await until('the update toast', `const t = document.querySelector('.toast'); return t && /available/.test(t.textContent) ? t.textContent.replace(/\\s+/g, ' ').trim() : null`, 40000)
  check(`the build finds ${target} at the endpoint`, toast.includes(`${target} is available`), toast)
  await run(`document.querySelector('button[aria-label="Settings"]').click()`)
  const ready = await until('the install offer', `const d = document.querySelector('.ui-dialog'); return d && /is ready/.test(d.textContent) ? 1 : null`)
  check('Settings offers to install it', ready === 1)
  await run(`[...document.querySelectorAll('.ui-dialog button')].find((b) => /Install and restart/.test(b.textContent)).click()`)
  const outcome = await until('the outcome', `const e = document.querySelector('.error-strip'); if (e) return 'error: ' + e.textContent.trim(); const b = [...document.querySelectorAll('.ui-dialog button')].find((b) => /Installing|Install and restart/.test(b.textContent)); return b && /Installing/.test(b.textContent) ? 'installing' : null`, 20000).catch(() => 'no word')
  check('no error while downloading and verifying', !outcome.startsWith('error'), outcome.slice(0, 200))
  // The app downloads, verifies, starts the installer and exits by itself.
  for (let i = 0; i < 80 && installedVersion() !== target; i++) await new Promise((r) => setTimeout(r, 3000))
} catch (e) {
  check('the drive completed', false, String(e.message).slice(0, 300))
} finally {
  if (id) await wd('DELETE', `/session/${id}`).catch(() => {})
  proc.kill()
  server.close()
}
const after = installedVersion()
check(`the per-user install went from ${before} to ${target}`, after === target, String(after))
const sizeAfter = existsSync(installed) ? statSync(installed).size : 0
check('the installed binary was replaced', sizeAfter > 0 && sizeAfter !== sizeBefore, `${sizeBefore} → ${sizeAfter}`)
if (failures) console.log(driverLog.slice(-2500))
console.log(failures ? `${failures} FAILED` : 'ALL PASSED')
process.exitCode = failures ? 1 : 0
