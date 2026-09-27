// Everything around the request — collection and folder settings, options,
// OAuth, cookies, the bin, copies, monitors, shortcuts, runner skips, examples.
import { mkdirSync, rmSync, writeFileSync, existsSync, readFileSync, readdirSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { startServer } from './server.mjs'
import { startDriver, stopDriver, session, check, step, sleep, KEY, driverLog, shots } from './lib.mjs'
import * as lib from './lib.mjs'

const only = process.env.SECTIONS?.split(',')
const want = (name) => !only || only.includes(name)

const collection = join(tmpdir(), 'volt-e2e-two')
const other = join(tmpdir(), 'volt-e2e-other')
for (const dir of [collection, other]) {
  rmSync(dir, { recursive: true, force: true })
  mkdirSync(join(dir, 'environments'), { recursive: true })
}
writeFileSync(join(collection, 'collection.yaml'), 'name: Two\nversion: 1\n')
writeFileSync(join(collection, 'environments', 'local.yaml'), 'name: local\nvars:\n- name: base\n  value: http://127.0.0.1:8787\n')
writeFileSync(join(other, 'collection.yaml'), 'name: Other\nversion: 1\n')
const read = (rel) => readFileSync(join(collection, rel), 'utf8')

const server = await startServer(8787)
await startDriver()
let app
try {
  app = await session([collection])
  const { run, runAsync, until, type, chord } = app
  const title = await until('the app', `return store.collection?.meta.name ?? null`, 30000)
  if (title !== 'Two') throw new Error(`wrong collection ${title}; stopping`)

  const newRequest = async (folder = null) => {
    const before = await run(`return store.activeId`)
    await runAsync(`await store.createRequest(${JSON.stringify(folder)}); return 1`)
    return until('a new request', `return store.activeId && store.activeId !== ${JSON.stringify(before)} && $('section.request') ? store.activeId : null`)
  }
  const setUrl = async (url) => {
    await run(`setValue($('input[aria-label="URL"]'), '')`)
    await type('input[aria-label="URL"]', url)
  }
  const send = async (timeout = 20000) => {
    await run(`window.__before = store.response; store.error = null; $('button.send').click()`)
    await until('a response', `return !store.sending && (store.response !== window.__before || store.error) ? 1 : null`, timeout)
    return run(`return { status: store.response?.status ?? null, body: store.response?.body ?? '', failure: store.error }`)
  }
  const openTab = (label) => run(`button(${JSON.stringify(label)}, $('section.request')).click()`)
  const echoed = (r) => { try { return JSON.parse(r.body) } catch { return {} } }

  if (want('scope')) await step('Collection and folder settings', async () => {
    const saved = await runAsync(`const meta = JSON.parse(JSON.stringify(store.collection.meta)); meta.headers = [{ name: 'X-Coll', value: 'from-collection', enabled: true }]; meta.auth = { type: 'bearer', token: 'coll-token' }; meta.vars = [{ name: 'who', value: 'collection', enabled: true }]; return await store.saveCollectionMeta(meta)`)
    check('collection settings save', saved === true, saved)
    await runAsync(`await store.createFolder(null, 'Scoped'); return 1`)
    await until('folder', `return store.tree.some((n) => n.name === 'Scoped') ? 1 : null`)
    const fsaved = await runAsync(`const f = await store.loadFolder('scoped'); f.vars = [{ name: 'who', value: 'folder', enabled: true }]; f.headers = [{ name: 'X-Folder', value: 'yes', enabled: true }]; return await store.saveFolder('scoped', f)`)
    check('folder settings save', fsaved === true, fsaved)
    await newRequest('scoped')
    await setUrl('{{base}}/echo?who={{who}}')
    const r = await send()
    const e = echoed(r)
    check('the collection header is sent', e.headers?.['x-coll'] === 'from-collection', JSON.stringify(e.headers))
    check('the folder header is sent', e.headers?.['x-folder'] === 'yes')
    check('auth is inherited from the collection', e.headers?.authorization === 'Bearer coll-token', e.headers?.authorization)
    check('a folder variable wins over the collection one', e.query?.who === 'folder', e.query?.who)
    await run(`button('Save request').click()`)

    // API key in the query string.
    await openTab('Auth')
    await run(`button('API key', $('section.request')).click()`)
    await until('fields', `return $('input[aria-label="Key name"]') ? 1 : null`)
    await run(`setValue($('input[aria-label="Key name"]'), 'api_key'); setValue($('input[aria-label="Key value"]'), 'k-1'); button('Query string', $('section.request')).click()`)
    const r2 = await send()
    check('an API key can go in the query', echoed(r2).query?.api_key === 'k-1', JSON.stringify(echoed(r2).query))
    check('and then the inherited bearer is not sent', !echoed(r2).headers?.authorization, echoed(r2).headers?.authorization)
  })

  if (want('options')) await step('Per-request options', async () => {
    await newRequest()
    await setUrl('{{base}}/slow?ms=3000')
    await runAsync(`store.request.options = { timeout_ms: 500 }; store.touch(); return 1`)
    const r = await send()
    const banner = await run(`return text($('.error-strip'))`)
    check('a per-request timeout stops a slow response', !!r.failure && /No response|timed out/i.test(banner), banner.slice(0, 160))
    await run(`store.error = null`)
    await setUrl('{{base}}/redirect')
    await runAsync(`store.request.options = { follow_redirects: false }; store.touch(); return 1`)
    const r2 = await send()
    check('redirects can be switched off per request', r2.status === 302, r2.status)
    await openTab('Options')
    const opts = await run(`return text($('section.request .panel'))`)
    check('the Options tab shows the override', /redirect/i.test(opts), opts.slice(0, 120))
  })

  if (want('oauth')) await step('OAuth 2 client credentials', async () => {
    const tok = await runAsync(`const t = await store.getOAuthToken({ tokenUrl: 'http://127.0.0.1:8787/token', authUrl: '', clientId: 'cid', clientSecret: 'csecret', scope: '', audience: '', basicAuth: true }, 'client_credentials', 'oauthToken'); return JSON.stringify(t) + ' ' + store.error`)
    check('a token is fetched', /oauth-tok/.test(tok), tok.slice(0, 160))
    const v = await run(`return store.environment.vars.find((x) => x.name === 'oauthToken')`)
    check('it lands in the environment as a secret', v?.secret === true && v?.value === 'oauth-tok', JSON.stringify(v))
    check('and not in the YAML', !read('environments/local.yaml').includes('oauth-tok'))
    const tok2 = await runAsync(`const t = await store.getOAuthToken({ tokenUrl: 'http://127.0.0.1:8787/token', authUrl: '', clientId: 'cid', clientSecret: 'csecret', scope: '', audience: '', basicAuth: false }, 'client_credentials', 'oauthToken'); return JSON.stringify(t)`)
    check('credentials can go in the body instead', /oauth-tok/.test(tok2), tok2.slice(0, 120))
    await run(`store.oauthDialog = true`)
    await until('dialog', `return $('.ui-dialog') ? 1 : null`)
    await app.screenshot(join(shots, 'oauth.png'))
    await run(`store.oauthDialog = false`)
  })

  if (want('cookies')) await step('Cookies and the bin', async () => {
    await newRequest()
    await setUrl('{{base}}/cookie/set')
    await send()
    const jar = await runAsync(`return JSON.stringify(await store.loadCookies())`)
    check('the jar holds the cookie', /sid/.test(jar), jar.slice(0, 120))
    await runAsync(`await store.clearCookies(); return 1`)
    await setUrl('{{base}}/cookie/echo')
    const r = await send()
    check('clearing the jar stops it being sent', echoed(r).cookie === null, r.body)
    await run(`store.cookiesDialog = true`)
    await until('dialog', `return $('.ui-dialog') ? 1 : null`)
    await run(`store.cookiesDialog = false`)

    await run(`button('Save request').click()`)
    const id = await run(`return store.activeId`)
    await runAsync(`store.lastDeleted = await window.__TAURI_INTERNALS__.invoke('delete_node', { root: store.root, id: ${JSON.stringify(id)} }); await store.reload(); return 1`)
    const bin = await runAsync(`return JSON.stringify(await store.loadTrash())`)
    check('a deleted request is in the bin', bin.includes(id), bin.slice(0, 160))
    const back = await runAsync(`const all = await store.loadTrash(); return await store.restoreDeleted(all.find((d) => JSON.stringify(d).includes(${JSON.stringify(id)})))`)
    check('and comes back from it', back === true && existsSync(join(collection, id)), back)
    await run(`store.trashDialog = true`)
    await until('dialog', `return $('.ui-dialog') ? 1 : null`)
    await run(`store.trashDialog = false`)
  })

  if (want('copy')) await step('Copy to another collection', async () => {
    const id = await newRequest()
    await setUrl('{{base}}/echo/copied')
    await run(`button('Save request').click()`)
    await until('saved', `return store.dirty ? null : 1`)
    const ok = await runAsync(`return await store.copyRequestTo(${JSON.stringify(id)}, ${JSON.stringify(other)}, false)`)
    const there = readdirSync(other).filter((f) => f.endsWith('.yaml') && f !== 'collection.yaml')
    check('a request can be copied to another collection', ok === true && there.length === 1, `${ok} ${there}`)
    check('and it is still here', existsSync(join(collection, id)))
    const moved = await runAsync(`return await store.copyRequestTo(${JSON.stringify(id)}, ${JSON.stringify(other)}, true)`)
    check('or moved', moved === true && !existsSync(join(collection, id)), moved)
  })

  if (want('monitor')) await step('Monitors judge by tests', async () => {
    const id = await newRequest()
    await setUrl('{{base}}/login')
    await runAsync(`store.request.checks = [{ from: '$.data.user.name', op: 'is', value: 'Bob' }]; await store.save(); return 1`)
    await runAsync(`await store.addMonitor(${JSON.stringify(id)}, 'Login watch', 3600); return 1`)
    const m = await runAsync(`const mon = store.monitors.find((x) => x.name === 'Login watch'); await store.runMonitor(mon); return JSON.stringify(store.monitorRuns[0])`)
    check('a 200 whose test fails is a failed run', /Test failed/.test(m), m)
    await runAsync(`const mon = store.monitors.find((x) => x.name === 'Login watch'); await store.removeMonitor(mon.id); return 1`)
  })

  if (want('runner')) await step('The runner skips sockets', async () => {
    await runAsync(`await store.createFolder(null, 'Mixed'); return 1`)
    await until('folder', `return store.tree.some((n) => n.name === 'Mixed') ? 1 : null`)
    await newRequest('mixed')
    await setUrl('{{base}}/echo')
    await run(`button('Save request').click()`)
    await newRequest('mixed')
    await setUrl('ws://127.0.0.1:8787/ws')
    await run(`button('Save request').click()`)
    await until('saved', `return store.dirty ? null : 1`)
    const saved = readdirSync(join(collection, 'mixed')).filter((f) => f.endsWith('.yaml') && f !== 'folder.yaml').map((f) => read(`mixed/${f}`))
    check('a ws:// URL saved the request as a WebSocket', saved.some((y) => /kind: websocket/.test(y)), saved.map((y) => y.split('\n')[1]).join(' / '))
    const res = await runAsync(`const r = await store.runCollection('mixed', false); return JSON.stringify(r)`)
    const parsed = JSON.parse(res)
    check('the socket is skipped, not failed', parsed.failed === 0 && parsed.steps.some((s) => s.skipped), res.slice(0, 300))
    await run(`store.runnerTarget = 'mixed'; store.runnerDialog = true`)
    await until('dialog', `return $('.ui-dialog') ? 1 : null`)
    await app.screenshot(join(shots, 'runner-skip.png'))
    await run(`store.runnerDialog = false`)
  })

  if (want('examples')) await step('Examples and comparison', async () => {
    const id = await newRequest()
    await setUrl('{{base}}/login')
    await run(`button('Save request').click()`)
    await send()
    const kept = await runAsync(`return await store.saveExample('Signed in')`)
    check('an example is kept', kept === true)
    const cmp = await runAsync(`return JSON.stringify(await store.compareExample(${JSON.stringify(id)}, 'Signed in'))`)
    check('the same response agrees with it', /"same":true/.test(cmp), cmp.slice(0, 200))
    await setUrl('{{base}}/me')
    await send()
    const cmp2 = await runAsync(`return JSON.stringify(await store.compareExample(${JSON.stringify(id)}, 'Signed in'))`)
    check('a different one says how it differs', /"same":false/.test(cmp2), cmp2.slice(0, 200))
  })

  if (want('keys')) await step('Keyboard', async () => {
    await newRequest()
    await setUrl('{{base}}/echo/keys')
    await run(`window.__before = store.response`)
    await chord(KEY.ctrl, KEY.enter)
    await until('sent', `return !store.sending && store.response !== window.__before ? 1 : null`, 10000)
    check('Ctrl+Enter sends', true)
    await chord(KEY.ctrl, 's')
    await until('saved', `return store.dirty ? null : 1`, 5000)
    check('Ctrl+S saves', true)
    const n = await run(`return store.tabs.length`)
    await run(`document.activeElement?.blur()`)
    await chord(KEY.ctrl, 'n')
    const n2 = await until('new', `return store.tabs.length > ${n} ? store.tabs.length : null`, 5000).catch(() => n)
    check('Ctrl+N makes a request', n2 > n, `${n} → ${n2}`)
    await chord(KEY.ctrl, 'w')
    const n3 = await until('closed', `return store.tabs.length < ${n2} ? store.tabs.length : null`, 5000).catch(() => n2)
    check('Ctrl+W closes it', n3 < n2, `${n2} → ${n3}`)
    await run(`document.activeElement?.blur()`)
    await chord(KEY.ctrl, 'p')
    const pal = await until('palette', `return $('.palette, [aria-label="Find a request"]') ? 1 : null`, 3000).catch(() => 0)
    check('Ctrl+P opens the palette', pal === 1)
    await app.keys(KEY.esc)
    await run(`document.activeElement?.blur()`)
    await app.keys('?')
    const sheet = await until('sheet', `return store.shortcutsSheet ? 1 : null`, 3000).catch(() => 0)
    check('? shows the shortcuts', sheet === 1)
    await run(`store.shortcutsSheet = false`)
  })

  if (want('undefined')) await step('An undefined variable is one click from defined', async () => {
    await newRequest()
    await setUrl('{{base}}/echo?x={{notYet}}')
    const link = await until('link', `return $$('.add-var').map(text).join(',') || null`, 5000)
    check('the undefined name is shown', link.includes('notYet'), link)
    await run(`$('.add-var').click()`)
    const opened = await until('editor', `return $('.ui-dialog') ? $$('.ui-dialog input').map((i) => i.value).join(' ') : null`, 5000)
    check('clicking it opens the environment with the name filled in', opened.includes('notYet'), opened.slice(0, 160))
    await app.keys(KEY.esc)
  })

  if (want('history')) await step('History to a new request', async () => {
    await runAsync(`await store.refreshHistory(); return 1`)
    const first = await run(`return store.history[0]?.id`)
    await runAsync(`await store.openHistory(${JSON.stringify(first)}); return 1`)
    const before = await run(`return store.tree.length`)
    await runAsync(`await store.saveHistoryAsNew(); return 1`)
    const after = await until('saved', `return !store.historyEntry && store.activeId ? store.tree.length : null`, 5000)
    check('a history entry saves as a new request', after >= before, `${before} → ${after}`)
  })

  const errors = await app.run(`return store.error`)
  check('no error left on screen at the end', !errors, errors)
} catch (e) {
  check('run completed', false, e.message)
} finally {
  if (app) { try { await app.screenshot(join(shots, 'last2.png')) } catch {} ; await app.close() }
  stopDriver()
  server.close()
}
if (lib.failures) console.log(driverLog.slice(-1500))
console.log(lib.failures ? `\n${lib.failures} FAILED` : '\nALL PASSED')
process.exit(lib.failures ? 1 : 0)
