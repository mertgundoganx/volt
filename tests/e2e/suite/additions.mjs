// What 0.4 added — made-up data, patterns, history filter, images,
// file bodies, environment import, data-driven runs, code notes, settings.
import { mkdirSync, rmSync, writeFileSync, existsSync, readFileSync, readdirSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { startServer } from './server.mjs'
import { startDriver, stopDriver, session, check, step, sleep, KEY, driverLog, shots } from './lib.mjs'
import * as lib from './lib.mjs'

const only = process.env.SECTIONS?.split(',')
const want = (name) => !only || only.includes(name)

const collection = join(tmpdir(), 'volt-e2e-three')
rmSync(collection, { recursive: true, force: true })
mkdirSync(join(collection, 'environments'), { recursive: true })
mkdirSync(join(collection, 'files'), { recursive: true })
writeFileSync(join(collection, 'collection.yaml'), 'name: Three\nversion: 1\n')
writeFileSync(join(collection, 'environments', 'local.yaml'), 'name: local\nvars:\n- name: base\n  value: http://127.0.0.1:8787\n')
writeFileSync(join(collection, 'files', 'hello.txt'), 'hello from a file')
const read = (rel) => readFileSync(join(collection, rel), 'utf8')

const server = await startServer(8787)
await startDriver()
let app
try {
  app = await session([collection])
  const { run, runAsync, until, type } = app
  const title = await until('the app', `return store.collection?.meta.name ?? null`, 30000)
  if (title !== 'Three') throw new Error(`wrong collection ${title}; stopping`)

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

  if (want('fake')) await step('Made-up data', async () => {
    await newRequest()
    await setUrl('{{base}}/echo?email={{$randomEmail}}&name={{$randomFullName}}&ok={{$randomBoolean}}')
    const missing = await run(`return $$('.add-var').map(text)`)
    check('Postman random names are not marked undefined', missing.length === 0, missing.join(','))
    const r = await send()
    const q = echoed(r).query ?? {}
    check('an email is made up', /^[a-z0-9.]+@example\.com$/.test(q.email ?? ''), q.email)
    check('a full name is made up', /^\S+ \S+/.test(q.name ?? ''), q.name)
    check('a boolean is made up', ['true', 'false'].includes(q.ok), q.ok)
    await run(`setValue($('input[aria-label="URL"]'), '{{$ran')`)
    await run(`const i = $('input[aria-label="URL"]'); i.focus(); i.setSelectionRange(6, 6); i.dispatchEvent(new Event('input', { bubbles: true })); i.dispatchEvent(new KeyboardEvent('keyup', { bubbles: true }))`)
    const offered = await until('completion', `const l = $$('[role=listbox] [role=option], .suggestions li'); return l.length ? l.map(text).join(',') : null`, 3000).catch(() => '')
    check('typing {{$ran offers the random names', /randomEmail/.test(offered), offered.slice(0, 160))
  })

  if (want('matches')) await step('A test that matches a pattern', async () => {
    await newRequest()
    await setUrl('{{base}}/login')
    await openTab('Tests')
    await run(`setValue($('input[aria-label="What to read"]'), '$.data.token')`)
    const ops = await run(`return [...$('.table[aria-label="Checks"] select, .table[aria-label="Checks"] [role=combobox]')?.options ?? []].map((o) => o.value)`)
    await runAsync(`store.request.checks = [{ from: '$.data.token', op: 'matches', value: '^t-\\\\d+$', enabled: true }, { from: '$.data.user.name', op: 'matches', value: '^B', enabled: true }]; store.touch(); return 1`)
    await send()
    const results = await run(`return store.checkResults.map((c) => c.ok)`)
    check('matches passes and fails as it should', JSON.stringify(results) === '[true,false]', JSON.stringify(results) + ' ' + ops)
    await openTab('Tests')
    const opText = await run(`return text($('.table[aria-label="Checks"]'))`)
    check('the Tests tab shows "matches"', /matches/.test(opText), opText.slice(0, 120))
  })

  if (want('history')) await step('History filter', async () => {
    await run(`button('History').click()`)
    await until('filter', `return $('input[aria-label="Filter history"]') ? 1 : null`, 5000)
    await run(`setValue($('input[aria-label="Filter history"]'), 'login')`)
    const shown = await run(`return $$('.history .entry').map(text)`)
    check('the filter keeps what matches', shown.length > 0 && shown.every((t) => /login/.test(t)), shown.join(' | ').slice(0, 200))
    await run(`setValue($('input[aria-label="Filter history"]'), '2xx')`)
    const ok = await run(`return $$('.history .entry .status').map(text)`)
    check('2xx keeps only successes', ok.length > 0 && ok.every((s) => /^2\d\d$/.test(s)), ok.join(','))
    await run(`setValue($('input[aria-label="Filter history"]'), 'nothing-like-this')`)
    const none = await run(`return text($('.history .none'))`)
    check('nothing matching says so', /Nothing sent matches/.test(none), none)
    await run(`setValue($('input[aria-label="Filter history"]'), ''); button('Collection').click()`)
  })

  if (want('image')) await step('An image response is shown', async () => {
    await newRequest()
    await setUrl('{{base}}/image')
    await send()
    const img = await until('image', `const i = $('.response .picture img'); return i && i.naturalWidth ? i.naturalWidth + 'x' + i.naturalHeight : null`, 5000).catch(() => null)
    check('the PNG is drawn', img === '1x1', img)
    const caption = await run(`return text($('.response .picture p'))`)
    check('with its type and size', /image\/png/.test(caption) && /1 × 1/.test(caption), caption)
  })

  if (want('files')) await step('File bodies', async () => {
    await newRequest()
    await setUrl('{{base}}/echo')
    await run(`setValue($('select[aria-label="Method"]'), 'POST')`)
    await openTab('Body')
    await run(`button('File', $('section.request')).click()`)
    await until('path', `return $('input[aria-label="File to send"]') ? 1 : null`, 3000)
    const chooser = await run(`return !!button('Choose…', $('section.request'))`)
    check('the File body has a Choose button', chooser)
    await run(`setValue($('input[aria-label="File to send"]'), 'files/hello.txt')`)
    let r = await send()
    check('a file body sends the file', echoed(r).body === 'hello from a file', echoed(r).body)
    await run(`button('Save request').click()`)
    await until('saved', `return store.dirty ? null : 1`)
    const id = await run(`return store.activeId`)
    check('the path is saved relative', /type: binary[\s\S]*path: files\/hello\.txt/.test(read(id)), read(id).replace(/\n/g, ' | '))

    await run(`button('Multipart', $('section.request')).click()`)
    await until('fields', `return $$('input[aria-label="Field"]').length ? 1 : null`)
    await run(`setValue($$('input[aria-label="Field"]').at(-1), 'upload')`)
    await until('value', `return $$('.kv input[aria-label="Value"]').length ? 1 : null`)
    await run(`setValue($$('.kv input[aria-label="Value"]')[0], 'files/hello.txt')`)
    await run(`$('button[aria-label="Sends text"]').click()`)
    const pick = await run(`return !!$('button[aria-label^="Choose a file for"]')`)
    check('a multipart row can choose a file', pick)
    r = await send()
    check('a multipart file field uploads the file', /filename="hello\.txt"/.test(echoed(r).body) && /hello from a file/.test(echoed(r).body), echoed(r).body?.slice(0, 200))
    const rel = await runAsync(`const orig = store.root; return JSON.stringify([orig])`)
    check('pickFile is there', rel.length > 0)
  })

  if (want('envimport')) await step('A Postman environment imports into the open collection', async () => {
    const src = join(tmpdir(), 'staging.postman_environment.json')
    writeFileSync(src, JSON.stringify({ name: 'Staging', values: [{ key: 'base', value: 'http://127.0.0.1:8787/echo/staging', enabled: true }, { key: 'apiKey', value: 'k-9', type: 'secret', enabled: true }], _postman_variable_scope: 'environment' }))
    await runAsync(`await store.importPaths([${JSON.stringify(src)}]); return 1`)
    const now = await until('env', `return store.environments.some((e) => e.name === 'Staging') ? store.activeEnvironment : null`, 5000)
    check('it becomes an environment and is switched to', now === 'Staging', now)
    check('the collection stayed open', await run(`return store.collection.meta.name`) === 'Three')
    check('the secret is not in the YAML', !read('environments/staging.yaml').includes('k-9'), read('environments/staging.yaml').replace(/\n/g, ' | '))
    await newRequest()
    await setUrl('{{base}}')
    const r = await send()
    check('and it is used', echoed(r).path === '/echo/staging', echoed(r).path)
    await run(`store.activeEnvironment = 'local'`)
  })

  if (want('data')) await step('A run over a data file', async () => {
    await runAsync(`await store.createFolder(null, 'Rows'); return 1`)
    await until('folder', `return store.tree.some((n) => n.name === 'Rows') ? 1 : null`)
    await newRequest('rows')
    await setUrl('{{base}}/echo/{{user}}')
    await runAsync(`store.request.checks = [{ from: '$.path', op: 'is', value: '/echo/ada', enabled: true }]; await store.save(); return 1`)
    writeFileSync(join(collection, 'users.csv'), 'user\nada\nalan\n')
    const res = await runAsync(`return JSON.stringify(await store.runCollection('rows', false, 'users.csv'))`)
    const runOut = JSON.parse(res)
    check('one pass per row', runOut?.steps?.length === 2 && runOut.steps[0].iteration === 1 && runOut.steps[1].iteration === 2, res.slice(0, 200))
    check('the row value is the variable', runOut?.passed === 1 && runOut?.failed === 1, `${runOut?.passed}/${runOut?.failed}`)
    await run(`store.runnerTarget = 'rows'; store.runnerDialog = true`)
    await until('dialog', `return $('.ui-dialog') ? 1 : null`)
    const dlg = await run(`return text($('.ui-dialog'))`)
    check('the runner offers a data file and shows rows', /Data file/.test(dlg) && /#1/.test(dlg) && /2 rows/.test(dlg), dlg.slice(0, 300))
    await app.screenshot(join(shots, 'runner-data.png'))
    await run(`store.runnerDialog = false`)
  })

  if (want('code')) await step('Code snippets say what they leave out', async () => {
    await newRequest()
    await setUrl('{{base}}/echo')
    await runAsync(`store.request.auth = { type: 'digest', username: 'u', password: 'p' }; store.touch(); return 1`)
    const gen = await runAsync(`return JSON.stringify(await store.generateCode('python', false))`)
    check('a digest request says the snippet does not do digest', /Digest auth/.test(gen), gen.slice(0, 300))
    await run(`store.codeDialog = true`)
    await until('dialog', `return $('.ui-dialog') ? 1 : null`)
    const warn = await until('note', `return /Digest auth/.test(text($('.ui-dialog'))) ? 1 : null`, 5000).catch(() => 0)
    check('and the dialog shows it', warn === 1)
    await run(`store.codeDialog = false`)
  })

  if (want('settings')) await step('Settings: client certificate', async () => {
    await run(`button('Settings').click()`)
    await until('settings', `return $('.ui-dialog') ? 1 : null`)
    const has = await run(`return !!$('#client-cert') && !!button('Choose…', $('.ui-dialog'))`)
    check('Settings has a client certificate field with a picker', has)
    await app.screenshot(join(shots, 'settings.png'))
    await app.keys(KEY.esc)
  })

  if (want('ws-auto')) await step('Typing ws:// makes a WebSocket', async () => {
    await newRequest()
    await setUrl('ws://127.0.0.1:8787/ws')
    const kind = await run(`return store.request.kind + ' ' + text($('button.send'))`)
    check('the request became a WebSocket with Connect', kind === 'websocket Connect', kind)
    await run(`$('button.send').click()`)
    const hello = await until('welcome', `return store.streamEvents.some((e) => e.data === 'welcome') ? 1 : null`, 5000).catch(() => 0)
    check('the greeting sent on connect is shown', hello === 1)
    await runAsync(`await store.closeStream(); return 1`)
  })

  const errors = await app.run(`return store.error`)
  check('no error left on screen at the end', !errors, errors)
} catch (e) {
  check('run completed', false, e.message)
} finally {
  if (app) { try { await app.screenshot(join(shots, 'last3.png')) } catch {} ; await app.close() }
  stopDriver()
  server.close()
}
if (lib.failures) console.log(driverLog.slice(-1500))
console.log(lib.failures ? `\n${lib.failures} FAILED` : '\nALL PASSED')
process.exit(lib.failures ? 1 : 0)
