// What 0.5 changed and added: the bench itself — top strip, collections list,
// the one picker, panes side by side — and the body editor, the path filter,
// the second language, OAuth 1.0 and the new OAuth grants, GraphQL
// subscriptions, HAR and Bruno import, OpenAPI export. SECTIONS=a,b limits it.
import { mkdirSync, rmSync, writeFileSync, existsSync, readFileSync, readdirSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { startServer } from './server.mjs'
import { startDriver, stopDriver, session, check, step, sleep, KEY, driverLog, shots, english } from './lib.mjs'
import * as lib from './lib.mjs'

const only = process.env.SECTIONS?.split(',')
const want = (name) => !only || only.includes(name)

const collection = join(tmpdir(), 'volt-e2e-bench')
const second = join(tmpdir(), 'volt-e2e-bench-second')
for (const [dir, name] of [[collection, 'Bench'], [second, 'Second']]) {
  rmSync(dir, { recursive: true, force: true })
  mkdirSync(join(dir, 'environments'), { recursive: true })
  writeFileSync(join(dir, 'collection.yaml'), `name: ${name}\nversion: 1\n`)
  writeFileSync(join(dir, 'environments', 'local.yaml'), 'name: local\nvars:\n- name: base\n  value: http://127.0.0.1:8787\n')
}
const read = (rel) => readFileSync(join(collection, rel), 'utf8')

const server = await startServer(8787)
await startDriver()
let app
let restoreLanguage = async () => {}
try {
  app = await session([collection])
  const { run, runAsync, until, type } = app
  const title = await until('the app', `return store.collection?.meta.name ?? null`, 30000)
  if (title !== 'Bench') throw new Error(`wrong collection ${title}; stopping`)
  restoreLanguage = await english(app)
  await app.resize(1400, 900)

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

  if (want('shell')) await step('The bench', async () => {
    const panels = await run(`return $$('.panel').length`)
    check('the sidebar, the request area and nothing else are panels to start with', panels >= 2, panels)
    await run(`button('Tools').click()`)
    const tools = await until('the tools menu', `const m = $$('[role=menu] [role=menuitem]').map(text); return m.length ? m : null`)
    check('Tools gathers what works on the whole collection', ['Run…', 'Mock server…', 'Monitors…', 'Cookies…', 'Sync…'].every((name) => tools.some((item) => item.startsWith(name))), tools.join(' | ').slice(0, 200))
    check('with both exports', tools.some((item) => item.startsWith('Export for Postman')) && tools.some((item) => item.startsWith('Export as OpenAPI')))
    await app.keys(KEY.esc)

    // New puts the caret in the address, where the next keystroke belongs.
    await run(`$('.sidebar-tools .new').click()`)
    const focused = await until('focus', `return document.activeElement?.getAttribute('aria-label') === 'URL' ? 1 : null`, 4000).catch(() => 0)
    check('a new request starts with the caret in the URL', focused === 1, await run(`return document.activeElement?.getAttribute('aria-label')`))
    await setUrl('{{base}}/echo')
    await run(`button('Save request').click()`)
    await until('saved', `return store.dirty ? null : 1`)

    // The other collections are a click away, and the tabs wait for the way back.
    await runAsync(`await store.open(${JSON.stringify(second)}); return 1`)
    await until('second', `return store.collection?.meta.name === 'Second' ? 1 : null`)
    const others = await until('others', `const o = $$('.other .other-name').map(text); return o.length ? o : null`)
    check('the collection just left is listed under the open one', others.some((name) => /volt-e2e-bench$/i.test(name)), others.join(', '))
    await run(`$$('.other .other-open').find((b) => /volt-e2e-bench$/i.test(text(b))).click()`)
    await until('back', `return store.collection?.meta.name === 'Bench' && !store.restoringTabs ? 1 : null`)
    const tabs = await until('tabs', `return store.tabs.length ? store.tabs.length : null`, 5000).catch(() => 0)
    check('coming back puts its tabs back', tabs >= 1, tabs)
    await runAsync(`await store.forgetCollection(store.recent.find((p) => /bench-second$/i.test(p))); return 1`)
    const left = await run(`return $$('.other .other-name').map(text)`)
    check('a collection can be taken off the list', !left.some((name) => /bench-second$/i.test(name)), left.join(', '))
    check('without touching its folder', existsSync(join(second, 'collection.yaml')))
  })

  if (want('picker')) await step('One picker for the method and the kind', async () => {
    await newRequest()
    await run(`setValue($('select[aria-label="Method"]'), 'kind:websocket')`)
    const now = await run(`return { kind: store.request.kind, label: text($('button.send')), tabs: $$('section.request [role=tab]').map((t) => text(t).replace(/\\s.*$/, '')) }`)
    check('choosing WebSocket makes the request one', now.kind === 'websocket' && now.label === 'Connect', JSON.stringify(now))
    check('and a socket has no Body, Tests or Captures', !now.tabs.includes('Body') && !now.tabs.includes('Tests') && !now.tabs.includes('Captures'), now.tabs.join(','))
    await run(`setValue($('select[aria-label="Method"]'), 'POST')`)
    // A gRPC request in a pane that is new has its own first tab, not Params.
    await run(`setValue($('select[aria-label="Method"]'), 'kind:grpc')`)
    await runAsync(`await store.saveLanguage('tr'); await store.saveLanguage('en'); return 1`)
    const grpc = await until('grpc tab', `const on = $('section.request [role=tab][aria-selected=true]'); return on ? text(on) : null`, 5000).catch(() => '')
    check('a gRPC request opens on a tab it has', /^Call/.test(grpc), grpc)
    await run(`setValue($('select[aria-label="Method"]'), 'POST')`)
    const back = await run(`return { kind: store.request.kind ?? 'http', method: store.request.method, tabs: $$('section.request [role=tab]').map((t) => text(t).replace(/\\s.*$/, '')) }`)
    check('choosing a method makes it HTTP again', back.kind === 'http' && back.method === 'POST' && back.tabs.includes('Body'), JSON.stringify(back))
  })

  if (want('layout')) await step('Side by side', async () => {
    const rows = await run(`return $('.work')?.className`)
    await run(`$('button[aria-label="Put the response beside the request"]').click()`)
    const cols = await until('cols', `return /cols/.test($('.work').className) ? $('.work').className : null`, 3000).catch(() => '')
    const boxes = await run(`const p = $$('.work > .panel').map((e) => e.getBoundingClientRect()); return p.length === 2 ? Math.abs(p[0].top - p[1].top) < 2 && p[1].left > p[0].right : false`)
    check('the response moves beside the request', /cols/.test(cols) && boxes === true, `${rows} → ${cols}`)
    await app.resize(960, 700)
    const narrow = await until('rows', `return /rows/.test($('.work').className) ? 1 : null`, 3000).catch(() => 0)
    check('a narrow window stacks them whatever was chosen', narrow === 1)
    await app.resize(1400, 900)
    await until('cols again', `return /cols/.test($('.work').className) ? 1 : null`, 3000).catch(() => 0)
    await run(`$('button[aria-label="Put the response under the request"]').click()`)
    await app.screenshot(join(shots, 'bench.png'))
  })

  if (want('editor')) await step('The body editor', async () => {
    await newRequest()
    await setUrl('{{base}}/echo')
    await run(`setValue($('select[aria-label="Method"]'), 'POST')`)
    await openTab('Body')
    await run(`button('JSON', $('section.request')).click()`)
    await until('editor', `return $('textarea[aria-label="Request body"]') ? 1 : null`)
    await run(`setValue($('textarea[aria-label="Request body"]'), '{\\n  "name": "{{base}}",\\n  "n": 1,\\n  "ok": true,\\n  "who": "{{nobody}}"\\n}')`)
    const kinds = await until('colours', `const k = [...new Set($$('.ui-editor .mirror span').map((s) => s.className))]; return k.length > 2 ? k : null`)
    check('JSON is coloured as it is typed', ['key', 'str', 'num', 'lit'].every((kind) => kinds.includes(kind)), kinds.join(','))
    check('a defined variable is marked and an undefined one is flagged', kinds.includes('var') && kinds.includes('missing'), kinds.join(','))
    const same = await run(`const m = $('.ui-editor .mirror'); const a = $('textarea[aria-label="Request body"]'); return m.textContent.trimEnd() === a.value.trimEnd()`)
    check('the coloured layer says exactly what the textarea holds', same === true)
    // Typed like a person: an opening brace brings its partner, Enter indents.
    await run(`setValue($('textarea[aria-label="Request body"]'), '')`)
    await type('textarea[aria-label="Request body"]', '{' + KEY.enter + '"a": 1')
    const typed = await run(`return $('textarea[aria-label="Request body"]').value`)
    check('brackets close and Enter indents', typed === '{\n  "a": 1\n}', JSON.stringify(typed))
    const r = await send()
    check('and what was typed is what is sent', echoed(r).body === '{\n  "a": 1\n}', echoed(r).body)
    // Enter and the closing bracket went in as edits, so undo still walks back through them.
    await run(`$('textarea[aria-label="Request body"]').focus()`)
    await app.chord(KEY.ctrl, 'z')
    await app.chord(KEY.ctrl, 'z')
    const undone = await run(`return $('textarea[aria-label="Request body"]').value`)
    check('undo still works after Enter and a closed bracket', undone !== typed && undone.length < typed.length, JSON.stringify(undone))
  })

  if (want('filter')) await step('A path into the response', async () => {
    await newRequest()
    await setUrl('{{base}}/login')
    await send()
    await run(`button('Filter by a path').click()`)
    await until('filter', `return $('input[aria-label="Path to show"]') ? 1 : null`)
    await type('input[aria-label="Path to show"]', '$.data.user.name')
    const one = await until('filtered', `const v = text($('.response .viewer')); return /Ada/.test(v) && !/token/.test(v) ? text($('.find .count')) : null`, 4000).catch(() => '')
    check('the viewer shows only what the path matches', one === '1 value', one)
    await run(`setValue($('input[aria-label="Path to show"]'), '$..id')`)
    const many = await until('deep', `return /value/.test(text($('.find .count'))) ? text($('.response .viewer')) : null`, 4000).catch(() => '')
    check('..name finds a key at any depth', /7/.test(many), many.slice(0, 60))
    await run(`setValue($('input[aria-label="Path to show"]'), '$.nope')`)
    const none = await until('none', `return text($('.find .count')) === 'Nothing there' ? 1 : null`, 4000).catch(() => 0)
    check('a path that matches nothing says so', none === 1)
    await run(`setValue($('input[aria-label="Path to show"]'), '$.constructor')`)
    const inherited = await until('own keys', `return text($('.find .count')) === 'Nothing there' ? 1 : null`, 4000).catch(() => 0)
    check('a path only matches keys the body really has', inherited === 1, await run(`return text($('.find .count'))`))
    await run(`button('Close filter').click()`)
    const whole = await run(`return /token/.test(text($('.response .viewer')))`)
    check('closing the filter shows the whole body again', whole === true)
  })

  if (want('language')) await step('Turkish', async () => {
    await runAsync(`await store.saveLanguage('tr'); return 1`)
    const tr = await until('turkish', `const s = text($('button.send')); return s && s !== 'Send' ? { send: s, tabs: $$('section.request [role=tab]').map((t) => text(t)).join(' '), side: $$('.side-head [role=radio]').map(text).join(' ') } : null`, 5000).catch(() => null)
    check('the Send key speaks Turkish', tr?.send === 'Gönder', JSON.stringify(tr))
    check('and so do the sections and the sidebar', /Başlıklar/.test(tr?.tabs ?? '') && /Geçmiş/.test(tr?.side ?? ''), JSON.stringify(tr))
    await app.screenshot(join(shots, 'turkish.png'))
    const state = await run(`return { open: !!store.request, url: store.request?.url }`)
    check('the open request survives the change of language', state.open === true, JSON.stringify(state))
    // Every language answers, and none of them falls back to English for the key that matters most.
    const said = {}
    for (const code of ['de', 'es', 'it', 'nl', 'pt', 'ru']) {
      await runAsync(`await store.saveLanguage('${code}'); return 1`)
      said[code] = await until(code, `const s = text($('button.send')); return s && s !== 'Send' && s !== 'Gönder' ? s + ' / ' + $$('.side-head [role=radio]').map(text).join(' ') : null`, 5000).catch(() => 'Send')
    }
    check('German, Spanish, Italian, Dutch, Portuguese and Russian each have their own words', Object.values(said).every((s) => s !== 'Send') && new Set(Object.values(said)).size >= 5, JSON.stringify(said))
    await app.screenshot(join(shots, 'russian.png'))
    await runAsync(`await store.saveLanguage('en'); return 1`)
    await until('english', `return text($('button.send')) === 'Send' ? 1 : null`, 5000)

    // Through Settings, the way a person changes it: the dialog must close.
    await run(`button('Settings').click()`)
    await until('settings', `return $('.ui-dialog select[aria-label="Language"]') ? 1 : null`)
    await run(`setValue($('.ui-dialog select[aria-label="Language"]'), 'de')`)
    await run(`button('Save settings', $('.ui-dialog')).click()`)
    const closed = await until('closed', `return !$('.ui-dialog') && text($('button.send')) === 'Senden' ? 1 : null`, 5000).catch(() => 0)
    check('choosing a language in Settings redraws the window and closes the dialog', closed === 1, await run(`return (!!$('.ui-dialog')) + ' ' + text($('button.send'))`))
    await runAsync(`await store.saveLanguage('en'); return 1`)
    await until('english', `return text($('button.send')) === 'Send' ? 1 : null`, 5000)
  })

  if (want('oauth1')) await step('OAuth 1.0', async () => {
    await newRequest()
    await setUrl('{{base}}/echo?b=2&a=1')
    await openTab('Auth')
    await run(`setValue($('select[aria-label="Auth type"]'), 'oauth1')`)
    await until('fields', `return $('input[aria-label="Consumer key"]') ? 1 : null`)
    await run(`setValue($('input[aria-label="Consumer key"]'), 'ck'); setValue($('input[aria-label="Consumer secret"]'), 'cs')`)
    const r = await send()
    const header = echoed(r).headers?.authorization ?? ''
    check('the request carries an OAuth signature', /^OAuth /.test(header) && /oauth_consumer_key="ck"/.test(header) && /oauth_signature="[^"]+"/.test(header) && /oauth_signature_method="HMAC-SHA1"/.test(header), header.slice(0, 200))
    check('and the secret is not in it', !/cs"/.test(header.replace(/oauth_consumer_key="ck"/, '')))
    await run(`button('Save request').click()`)
    await until('saved', `return store.dirty ? null : 1`)
    const yaml = read(await run(`return store.activeId`))
    check('it is saved as oauth1', /type: oauth1/.test(yaml), yaml.replace(/\n/g, ' | ').slice(0, 300))
  })

  if (want('grants')) await step('OAuth 2.0: password and device', async () => {
    const config = `{ tokenUrl: 'http://127.0.0.1:8787/token', authUrl: '', deviceUrl: 'http://127.0.0.1:8787/device', clientId: 'cid', clientSecret: 'csecret', scope: '', audience: '', basicAuth: false, username: 'ada', password: 'pw' }`
    const tok = await runAsync(`const t = await store.getOAuthToken(${config}, 'password', 'pwToken'); return JSON.stringify(t) + ' ' + store.error`)
    check('the password grant fetches a token', /password-tok/.test(tok), tok.slice(0, 160))
    const started = await runAsync(`return JSON.stringify(await store.startOAuthDevice(${config}))`)
    check('the device grant starts with a code to type', /WDJB-MJHT/.test(started) && /activate/.test(started), started.slice(0, 200))
    const code = JSON.parse(started)?.deviceCode
    const first = await runAsync(`return JSON.stringify(await store.pollOAuthDevice(${config}, ${JSON.stringify(code)}, 'deviceToken'))`)
    check('polling before approval says pending', /"pending"/.test(first), first)
    const then = await runAsync(`return JSON.stringify(await store.pollOAuthDevice(${config}, ${JSON.stringify(code)}, 'deviceToken'))`)
    check('and after approval hands over the token', /"done"/.test(then) && /device-tok/.test(then), then.slice(0, 160))
    const kept = await run(`return store.environment.vars.filter((v) => /Token$/.test(v.name)).map((v) => v.name + ':' + v.secret).join(',')`)
    check('both tokens are kept as secrets', /pwToken:true/.test(kept) && /deviceToken:true/.test(kept), kept)

    // The dialog remembers the form for the collection, without the secrets.
    await run(`store.oauthDialog = true`)
    await until('dialog', `return $('.ui-dialog select[aria-label="Grant"]') ? 1 : null`)
    await run(`setValue($('.ui-dialog select[aria-label="Grant"]'), 'device')`)
    await run(`setValue($('input[aria-label="Token URL"]'), 'http://127.0.0.1:8787/token'); setValue($('input[aria-label="Device authorization URL"]'), 'http://127.0.0.1:8787/device'); setValue($('input[aria-label="Client ID"]'), 'cid'); setValue($('input[aria-label="Client secret"]'), 'shh'); setValue($('input[aria-label="Variable name"]'), 'uiToken')`)
    await run(`button('Get the token', $('.ui-dialog')).click()`)
    const shown = await until('the code', `return text($('.ui-dialog .device .code')) || null`, 5000).catch(() => '')
    check('the dialog shows the code to type', shown === 'WDJB-MJHT', shown)
    const got = await until('the token', `return /Got a Bearer token/.test(text($('.ui-dialog'))) ? 1 : null`, 10000).catch(() => 0)
    check('and takes the token once it is approved', got === 1)
    await app.screenshot(join(shots, 'oauth-device.png'))
    await run(`store.oauthDialog = false`)
    const saved = await runAsync(`return JSON.stringify(await store.loadOAuthSetup())`)
    check('the form is remembered, the secret is not', /"grant":"device"/.test(saved) && /uiToken/.test(saved) && !/shh/.test(saved), saved.slice(0, 240))
  })

  if (want('subscription')) await step('A GraphQL subscription', async () => {
    await newRequest()
    await setUrl('{{base}}/graphql')
    await run(`store.request.method = 'POST'; store.request.body = { type: 'graphql', query: 'subscription { tick }', variables: '' }; store.touch()`)
    const label = await until('label', `return text($('button.send')) === 'Subscribe' ? 1 : null`, 3000).catch(() => 0)
    check('a subscription makes the key say Subscribe', label === 1, await run(`return text($('button.send'))`))
    await run(`$('button.send').click()`)
    const ticks = await until('ticks', `const m = store.streamEvents.filter((e) => e.kind === 'message').map((e) => e.data); return m.length >= 2 ? m : null`, 8000).catch(() => [])
    check('each result arrives as a message', ticks.length >= 2 && /"tick": ?1/.test(ticks[0]) && /"tick": ?2/.test(ticks[1]), JSON.stringify(ticks).slice(0, 200))
    const pane = await run(`return !!$('section.stream')`)
    check('in the pane a socket uses', pane === true)
    await runAsync(`if (store.stream) await store.closeStream(); return 1`)
    await run(`store.request.body = { type: 'graphql', query: 'query { hello }', variables: '' }; store.touch()`)
    const again = await until('send', `return text($('button.send')) === 'Send' ? 1 : null`, 3000).catch(() => 0)
    check('a query is still a plain Send', again === 1)
  })

  if (want('io')) await step('HAR and Bruno in, OpenAPI out', async () => {
    const har = join(tmpdir(), 'volt-e2e.har')
    writeFileSync(har, JSON.stringify({ log: { version: '1.2', entries: [
      { request: { method: 'GET', url: 'https://api.example.com/users?page=2', headers: [{ name: 'Accept', value: 'application/json' }, { name: ':authority', value: 'api.example.com' }], queryString: [{ name: 'page', value: '2' }] }, response: { status: 200, content: { mimeType: 'application/json' } } },
      { request: { method: 'POST', url: 'https://api.example.com/users', headers: [{ name: 'Content-Type', value: 'application/json' }], queryString: [], postData: { mimeType: 'application/json', text: '{"name":"Ada"}' } }, response: { status: 201, content: { mimeType: 'application/json' } } },
      { request: { method: 'GET', url: 'https://api.example.com/logo.png', headers: [], queryString: [] }, response: { status: 200, content: { mimeType: 'image/png' } } },
    ] } }))
    const into = join(tmpdir(), 'volt-e2e-into')
    rmSync(into, { recursive: true, force: true })
    mkdirSync(into, { recursive: true })
    const out = await runAsync(`return JSON.stringify(await window.__TAURI_INTERNALS__.invoke('import_collection', { source: ${JSON.stringify(har)}, into: ${JSON.stringify(into)} }))`)
    const harFiles = existsSync(into) ? readdirSync(into, { recursive: true }).map(String).filter((f) => f.endsWith('.yaml')) : []
    const requests = harFiles.filter((f) => !/collection\.yaml|folder\.yaml|environments/.test(f))
    check('a HAR file imports its requests and leaves the image out', !/^"?ERROR/.test(out) && requests.length === 2, `${out.slice(0, 120)} — ${harFiles.join(', ')}`)

    const bruno = join(tmpdir(), 'volt-e2e-bruno')
    rmSync(bruno, { recursive: true, force: true })
    mkdirSync(join(bruno, 'users'), { recursive: true })
    mkdirSync(join(bruno, 'environments'), { recursive: true })
    writeFileSync(join(bruno, 'bruno.json'), JSON.stringify({ version: '1', name: 'From Bruno', type: 'collection' }))
    writeFileSync(join(bruno, 'users', 'create.bru'), 'meta {\n  name: Create user\n  type: http\n  seq: 1\n}\n\npost {\n  url: {{host}}/users\n  body: json\n  auth: bearer\n}\n\nheaders {\n  X-One: 1\n  ~X-Off: 2\n}\n\nauth:bearer {\n  token: abc.def.ghi\n}\n\nbody:json {\n  {\n    "name": "Ada"\n  }\n}\n\nassert {\n  res.status: eq 201\n}\n')
    writeFileSync(join(bruno, 'environments', 'local.bru'), 'vars {\n  host: http://127.0.0.1:8787\n}\nvars:secret [\n  apiKey\n]\n')
    rmSync(into, { recursive: true, force: true })
    mkdirSync(into, { recursive: true })
    const b = await runAsync(`return JSON.stringify(await window.__TAURI_INTERNALS__.invoke('import_collection', { source: ${JSON.stringify(bruno)}, into: ${JSON.stringify(into)} }))`)
    const made = existsSync(into) ? readdirSync(into, { recursive: true }).map(String) : []
    const createFile = made.find((f) => /create.*\.yaml$/.test(f))
    const created = createFile ? readFileSync(join(into, createFile), 'utf8') : ''
    check('a Bruno collection folder imports', !/^"?ERROR/.test(b) && !!createFile, `${b.slice(0, 160)} — ${made.join(', ')}`)
    check('with method, URL, header, body and a test', /method: POST/.test(created) && created.includes('{{host}}/users') && /X-One/.test(created) && /Ada/.test(created) && /201/.test(created), created.replace(/\n/g, ' | ').slice(0, 400))
    check('and the literal token moved out of the file', !created.includes('abc.def.ghi'), created.replace(/\n/g, ' | ').slice(0, 200))
    rmSync(into, { recursive: true, force: true })

    const api = await runAsync(`return JSON.stringify(await window.__TAURI_INTERNALS__.invoke('export_openapi', { root: store.root }))`)
    const doc = JSON.parse(api)?.yaml ?? ''
    check('the collection exports as an OpenAPI document', /openapi:\s*["']?3\.0/.test(doc) && /paths:/.test(doc) && /\/echo/.test(doc), doc.slice(0, 200).replace(/\n/g, ' | '))
  })

  if (want('trust')) await step('Certificates in Settings', async () => {
    await run(`button('Settings').click()`)
    await until('settings', `return $('.ui-dialog #ca-cert') ? 1 : null`)
    check('Settings has a field for trusted authorities', true)
    await run(`setValue($('#client-cert'), 'C:/certs/me.p12')`)
    const asks = await until('passphrase', `return $('#client-cert-password') ? 1 : null`, 3000).catch(() => 0)
    check('a .p12 certificate asks for its passphrase', asks === 1)
    const langs = await run(`return [...$('.ui-dialog select[aria-label="Language"]').options].map((o) => o.textContent)`)
    check('and the language can be chosen from all eight', ['Deutsch', 'English', 'Español', 'Italiano', 'Nederlands', 'Português', 'Русский', 'Türkçe'].every((name) => langs.includes(name)), langs.join(','))
    await app.keys(KEY.esc)
  })

  const errors = await app.run(`return store.error`)
  check('no error left on screen at the end', !errors, errors)
} catch (e) {
  check('run completed', false, e.message)
} finally {
  if (app) { await restoreLanguage(); try { await app.screenshot(join(shots, 'last4.png')) } catch {} ; await app.close() }
  stopDriver()
  server.close()
}
if (lib.failures) console.log(driverLog.slice(-1500))
console.log(lib.failures ? `\n${lib.failures} FAILED` : '\nALL PASSED')
process.exit(lib.failures ? 1 : 0)
