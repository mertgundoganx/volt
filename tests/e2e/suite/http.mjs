// The request itself, over the real binary and a local server (server.mjs):
// sending, bodies, auth, captures, tests, cookies, cancel, the response views,
// the tree, environments, curl, history, runner, import/export, sockets,
// GraphQL, the mock server, themes and tabs. SECTIONS=http,tree limits it.
import { mkdirSync, rmSync, writeFileSync, existsSync, readFileSync, readdirSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { startServer } from './server.mjs'
import { startDriver, stopDriver, session, check, step, sleep, KEY, driverLog, shots } from './lib.mjs'
import * as lib from './lib.mjs'

const only = process.env.SECTIONS?.split(',')
const want = (name) => !only || only.includes(name)

const collection = join(tmpdir(), 'volt-e2e-full')
rmSync(collection, { recursive: true, force: true })
mkdirSync(join(collection, 'environments'), { recursive: true })
writeFileSync(join(collection, 'collection.yaml'), 'name: E2E\nversion: 1\n')
writeFileSync(join(collection, 'environments', 'local.yaml'), 'name: local\nvars:\n- name: base\n  value: http://127.0.0.1:8787\n')
const read = (rel) => readFileSync(join(collection, rel), 'utf8')

const server = await startServer(8787)
await startDriver()
let app
try {
  app = await session([collection])
  const { run, runAsync, until, type } = app
  const title = await until('the app', `return store.collection?.meta.name ?? null`, 30000)
  check('the throwaway collection is open', title === 'E2E', title)
  if (title !== 'E2E') throw new Error('wrong collection; stopping')

  /** Makes a fresh request from the sidebar and waits for its editor. */
  const newRequest = async () => {
    const before = await run(`return store.activeId`)
    await run(`$('.sidebar-tools .new').click()`)
    return until('a new request', `return store.activeId && store.activeId !== ${JSON.stringify(before)} && $('section.request') ? store.activeId : null`)
  }
  const setUrl = async (url) => {
    await run(`setValue($('input[aria-label="URL"]'), '')`)
    await type('input[aria-label="URL"]', url)
  }
  const send = async (timeout = 20000) => {
    await run(`window.__before = store.response; store.error = null; $('button.send').click()`)
    await until('a response', `return !store.sending && (store.response !== window.__before || store.error) ? 1 : null`, timeout)
    return run(`return { status: store.response?.status ?? null, body: store.response?.body ?? '', error: store.error, shown: text($('.response .status')) }`)
  }
  const openTab = (label) => run(`button(${JSON.stringify(label)}, $('section.request')).click()`)
  const echoed = (r) => { try { return JSON.parse(r.body) } catch { return {} } }

  if (want('http')) await step('HTTP basics', async () => {
    const id = await newRequest()
    check('New makes a request file', existsSync(join(collection, id)), id)
    await setUrl('{{base}}/echo?a=1&b=two')
    const resolved = await run(`return text($('.resolved .value'))`)
    check('the URL shows what it resolves to', resolved === 'http://127.0.0.1:8787/echo?a=1&b=two', resolved)
    const params = await run(`return store.request.params.map((p) => p.name + '=' + p.value).join('&')`)
    check('the query fills Params', params === 'a=1&b=two', params)
    let r = await send()
    check('GET goes out and comes back 200', r.status === 200, r.shown)
    check('the query arrived', echoed(r).query?.b === 'two', JSON.stringify(echoed(r).query))
    const view = await run(`return text($('.response .viewer'))`)
    check('the body is on screen', view.includes('"method"'), view.slice(0, 60))

    // Params table edits flow back into the URL.
    await openTab('Params')
    await run(`setValue($$('input[aria-label="Parameter"]')[0], 'alpha')`)
    const url = await run(`return $('input[aria-label="URL"]').value`)
    check('renaming a param rewrites the URL', url.includes('alpha=1'), url)

    // POST with a JSON body and a header.
    await run(`setValue($('select[aria-label="Method"]'), 'POST')`)
    await openTab('Body')
    await run(`button('JSON', $('section.request')).click()`)
    await until('the body editor', `return $('textarea[aria-label="Request body"]') ? 1 : null`)
    await run(`setValue($('textarea[aria-label="Request body"]'), '{"name":"{{$uuid}}","n":1}')`)
    await openTab('Headers')
    await run(`setValue($$('input[aria-label="Header"]').at(-1), 'X-Test')`)
    await until('a value input', `return $$('.kv input[aria-label="Value"]').length ? 1 : null`)
    await run(`setValue($$('.kv input[aria-label="Value"]')[0], 'yes')`)
    r = await send()
    const e = echoed(r)
    check('POST sends the JSON body', e.method === 'POST' && /"n":1/.test(e.body), e.body)
    check('with content-type application/json', /application\/json/.test(e.headers?.['content-type'] ?? ''), e.headers?.['content-type'])
    check('$uuid resolved to a uuid', /"name":"[0-9a-f-]{36}"/.test(e.body), e.body)
    check('the custom header went out', e.headers?.['x-test'] === 'yes', JSON.stringify(e.headers?.['x-test']))

    // Save writes it all.
    await run(`button('Save request').click()`)
    await until('saved', `return store.dirty ? null : 1`)
    const yaml = read(id)
    check('Save writes method, url, header and body into YAML', /method: POST/.test(yaml) && yaml.includes('{{base}}/echo') && yaml.includes('X-Test') && yaml.includes('$uuid'), yaml.replace(/\n/g, ' | ').slice(0, 300))

    // Form bodies.
    await openTab('Body')
    await run(`button('Multipart', $('section.request')).click()`)
    await until('the field editor', `return $$('input[aria-label="Field"]').length ? 1 : null`)
    await run(`setValue($$('input[aria-label="Field"]').at(-1), 'city')`)
    await until('value', `return $$('.kv input[aria-label="Value"]').length ? 1 : null`)
    await run(`setValue($$('.kv input[aria-label="Value"]')[0], 'İzmir & co')`)
    r = await send()
    const kinds = await run(`return $$('[aria-label="Body type"] [role=radio]').map(text)`)
    const e2 = echoed(r)
    check('a form body is sent in its encoding', /city/.test(e2.body), `${e2.headers?.['content-type']} ${e2.body?.slice(0, 80)} — kinds ${kinds.join('/')}`)

    await run(`button('URL-encoded', $('section.request')).click()`)
    r = await send()
    const e3 = echoed(r)
    check('a URL-encoded body is encoded', /x-www-form-urlencoded/.test(e3.headers?.['content-type'] ?? '') && /city=/.test(e3.body), `${e3.headers?.['content-type']} ${e3.body}`)

    // Status colours, error words.
    await setUrl('{{base}}/status/404')
    r = await send()
    check('404 is shown as 404', r.status === 404 && /404/.test(r.shown), r.shown)
    await setUrl('http://127.0.0.1:1/nothing')
    r = await send()
    const banner = await run(`return text($('.error-strip'))`)
    check('a refused connection is explained in words', /nothing is listening/.test(banner), banner.slice(0, 200))
    await run(`store.error = null`)
  })

  if (want('cancel')) await step('Cancel', async () => {
    await newRequest()
    await setUrl('{{base}}/slow?ms=8000')
    await run(`$('button.send').click()`)
    await until('sending', `return store.sending ? 1 : null`)
    const label = await run(`return text($('button.send'))`)
    check('Send turns into Cancel', /Cancel/i.test(label), label)
    const t0 = Date.now()
    await run(`$('button.send').click()`)
    await until('stopped', `return store.sending ? null : 1`, 5000)
    check('Cancel stops it at once', Date.now() - t0 < 3000, `${Date.now() - t0} ms`)
    const err = await run(`return store.error`)
    check('a cancel is not an error', !err, err)
  })

  if (want('auth')) await step('Auth, captures, tests, cookies, redirects', async () => {
    await newRequest()
    await setUrl('{{base}}/basic')
    await openTab('Auth')
    await run(`button('Basic', $('section.request')).click()`)
    await until('user field', `return $('input[aria-label="Username"]') ? 1 : null`)
    await run(`setValue($('input[aria-label="Username"]'), 'user'); setValue($('input[aria-label="Password"]'), 'pass')`)
    let r = await send()
    check('basic auth is accepted', r.status === 200, r.shown)

    // Capture a token from one response and use it in the next request.
    await newRequest()
    await setUrl('{{base}}/login')
    await openTab('Captures')
    await run(`setValue($('input[aria-label="Variable to write"]'), 'token'); setValue($('input[aria-label="Where to read it from"]'), '$.data.token')`)
    await openTab('Tests')
    await run(`setValue($('input[aria-label="What to read"]'), 'status')`)
    await until('value input', `return $('.table[aria-label="Checks"] input[aria-label="Expected value"]') ? 1 : null`)
    await run(`setValue($('input[aria-label="Expected value"]'), '200')`)
    r = await send()
    const checks = await run(`return store.checkResults.map((c) => c.ok)`)
    check('a passing test shows as passing', checks.length === 1 && checks[0] === true, JSON.stringify(checks))
    const envToken = await run(`return store.environment.vars.find((v) => v.name === 'token')?.value ?? null`)
    check('the capture wrote token into the environment', envToken === 't-123', envToken)
    await run(`button('Save request').click()`)

    await newRequest()
    await setUrl('{{base}}/me')
    await openTab('Auth')
    await run(`button('Bearer', $('section.request')).click()`)
    await until('token field', `return $('input[aria-label="Token"]') ? 1 : null`)
    await run(`setValue($('input[aria-label="Token"]'), '{{token}}')`)
    r = await send()
    check('the captured token authorises the next request', r.status === 200, r.shown)

    // A failing test.
    await openTab('Tests')
    await run(`setValue($('input[aria-label="What to read"]'), '$.name')`)
    await until('value input', `return $('input[aria-label="Expected value"]') ? 1 : null`)
    await run(`setValue($('input[aria-label="Expected value"]'), 'Bob')`)
    r = await send()
    const failed = await run(`return text($('.checks'))`)
    check('a failing test says what it got', /Ada/.test(failed), failed)

    // Cookies are kept per collection and sent back.
    await newRequest()
    await setUrl('{{base}}/cookie/set')
    await send()
    await setUrl('{{base}}/cookie/echo')
    r = await send()
    check('a cookie set by one response goes out with the next', /sid=abc123/.test(r.body), r.body)

    await setUrl('{{base}}/redirect')
    r = await send()
    const red = await run(`return text($('.redirect'))`)
    check('a redirect is followed and said', r.status === 200 && /redirected/.test(red + r.body), red)
  })

  if (want('views')) await step('Response views', async () => {
    await newRequest()
    await setUrl('{{base}}/login')
    await send()
    const views = await run(`return $$('.response [role=radio]').map(text)`)
    check('JSON offers Pretty, Tree and Raw', ['Pretty', 'Tree', 'Raw'].every((v) => views.includes(v)), views.join(','))
    await run(`button('Tree', $('.response')).click()`)
    const tree = await until('tree', `return text($('.response .viewer'))`)
    check('the tree shows the keys', /token/.test(tree), tree.slice(0, 80))
    await run(`button('Raw', $('.response')).click()`)
    await send()
    const diffBtn = await run(`return !!$('button[aria-label="Compare with the previous send"]')`)
    check('a second send can be compared with the first', diffBtn)

    await setUrl('{{base}}/html')
    await send()
    const hv = await run(`return $$('.response [role=radio]').map(text)`)
    check('HTML offers Preview', hv.includes('Preview'), hv.join(','))
    await setUrl('{{base}}/image')
    await send()
    const bin = await run(`return text($('.response .viewer'))`)
    check('an image is drawn rather than dumped as text', /image\/png/.test(bin) && !/Binary response/.test(bin), bin.slice(0, 80))

    // A large body renders without freezing.
    await setUrl('{{base}}/big')
    const t0 = Date.now()
    const r = await send(30000)
    await until('render', `return $('.response .viewer') ? 1 : null`)
    const dt = Date.now() - t0
    check('a 3 MB JSON response renders', r.status === 200, `${dt} ms, ${r.body.length} chars`)
    check('in reasonable time', dt < 8000, `${dt} ms`)
    await run(`button('Headers', $('.response')).click()`)
    const hdrs = await run(`return text($('.response .headers'))`)
    check('response headers are listed', /content-type/i.test(hdrs), hdrs.slice(0, 60))
    await app.screenshot(join(shots, 'views.png'))
  })

  if (want('tree')) await step('Folders and the tree', async () => {
    await run(`store.createFolder(null, 'Users API')`)
    await until('folder', `return store.tree.some((n) => n.kind === 'folder' && n.name === 'Users API') ? 1 : null`)
    check('a folder is made on disk', existsSync(join(collection, 'users-api')))
    await run(`$$('.tree .row.folder').find((r) => text(r).includes('Users API')).querySelector('button[aria-label="New request here"]').click()`)
    const inFolder = await until('request in folder', `return store.activeId?.startsWith('users-api/') ? store.activeId : null`)
    check('New request here makes it inside the folder', !!inFolder, inFolder)
    await run(`const t = $('input.title[aria-label="Request name"]'); setValue(t, 'List users')`)
    await run(`button('Save request').click()`)
    await until('saved', `return store.dirty ? null : 1`)
    const tree = await until('tree', `const t = $$('.tree .row').map(text); return t.some((x) => x.includes('List users')) ? t : null`, 5000).catch(() => run(`return $$('.tree .row').map(text)`))
    check('the renamed request shows in the tree', tree.some((t) => t.includes('List users')), tree.join(' | '))

    // Duplicate, rename, delete and undo.
    await run(`store.duplicate(store.activeId)`)
    await until('copy', `return store.tree.find((n) => n.name === 'Users API')?.children.length === 2 ? 1 : null`)
    const kids = await run(`return store.tree.find((n) => n.name === 'Users API').children.map((c) => c.name)`)
    check('duplicate makes a second request beside it', kids.length === 2, kids.join(', '))
    const copyId = await run(`return store.tree.find((n) => n.name === 'Users API').children.find((c) => c.name !== 'List users').id`)
    await run(`store.rename(${JSON.stringify(copyId)}, 'Get user')`)
    await until('renamed', `return store.tree.find((n) => n.name === 'Users API').children.some((c) => c.name === 'Get user') ? 1 : null`)
    check('rename works', true)
    const getId = await run(`return store.tree.find((n) => n.name === 'Users API').children.find((c) => c.name === 'Get user').id`)
    // The confirmation is a native window WebDriver cannot click; what follows it is the same.
    await runAsync(`store.lastDeleted = await window.__TAURI_INTERNALS__.invoke('delete_node', { root: store.root, id: ${JSON.stringify(getId)} }); await store.reload(); return 1`)
    await until('gone', `return store.tree.find((n) => n.name === 'Users API').children.length === 1 ? 1 : null`)
    check('delete removes it from disk', !existsSync(join(collection, getId)), getId)
    await run(`store.undoDelete()`)
    await until('back', `return store.tree.find((n) => n.name === 'Users API').children.length === 2 ? 1 : null`)
    check('undo puts it back', existsSync(join(collection, getId)))

    // Move to root.
    await run(`store.moveNode(${JSON.stringify(getId)}, null)`)
    await until('moved', `return store.tree.some((n) => n.name === 'Get user') ? 1 : null`)
    check('move to the root', !existsSync(join(collection, getId)))

    // Search by name and by contents.
    await run(`button('Collection').click()`)
    await type('input[aria-label="Search requests"]', 'List')
    const hits = await until('hits', `return $$('.hit').length ? $$('.hit').map(text) : null`)
    check('search finds by name', hits.some((h) => h.includes('List users')), hits.join(' | '))
    await run(`setValue($('input[aria-label="Search requests"]'), 'X-Test')`)
    const inside = await until('contents', `return $$('.hit').length ? $$('.hit').map(text) : null`, 5000).catch(() => [])
    check('search finds by what is inside', inside.length > 0, inside.join(' | '))
    await run(`setValue($('input[aria-label="Search requests"]'), '')`)
  })

  if (want('env')) await step('Environments and secrets', async () => {
    await run(`button('Environments').click()`)
    await until('editor', `return $('.ui-dialog') ? 1 : null`)
    await app.screenshot(join(shots, 'env.png'))
    const dialog = await run(`return $$('.ui-dialog input').map((i) => i.value).join(' ')`)
    check('the environment editor opens', /base/.test(dialog), dialog.slice(0, 120))
    // Add a secret through the store API the editor uses.
    const ok = await runAsync(`const env = JSON.parse(JSON.stringify(store.environment)); env.vars.push({ name: 'apiKey', value: 's3cr3t', secret: true }); return await store.saveEnvironment(env, env.name)`)
    check('saving an environment with a secret works', ok === true, ok)
    const yaml = read('environments/local.yaml')
    check('the secret value is not in the YAML', !yaml.includes('s3cr3t'), yaml.replace(/\n/g, ' | '))
    const envFile = existsSync(join(collection, '.env.local')) ? read('.env.local') : ''
    check('it is in .env.local', envFile.includes('s3cr3t'), envFile.replace(/\n/g, ' | '))
    const gi = existsSync(join(collection, '.gitignore')) ? read('.gitignore') : ''
    check('.env is gitignored', /\.env/.test(gi), gi.replace(/\n/g, ' | '))
    await run(`const d = $('.ui-dialog'); (button('Close', d) ?? button('Done', d))?.click()`)
    await sleep(300)
    // A second environment and switching.
    await runAsync(`return await store.saveEnvironment({ name: 'prod', vars: [{ name: 'base', value: 'http://127.0.0.1:8787/echo/prod', secret: false }] }, null)`)
    await until('two envs', `return store.environments.length === 2 ? 1 : null`)
    await run(`button('prod').click()`)
    await newRequest()
    await setUrl('{{base}}')
    const r = await send()
    check('switching environment changes where it goes', echoed(r).path === '/echo/prod', echoed(r).path)
    await run(`button('local').click()`)
  })

  if (want('curl')) await step('cURL in and out, code', async () => {
    await newRequest()
    await run(`setValue($('input[aria-label="URL"]'), '')`)
    const cmd = `curl -X PUT '{{base}}/echo/curl' -H 'Authorization: Bearer abc.def' -H 'X-One: 1' --data '{"k":"v"}'`
    await run(`const i = $('input[aria-label="URL"]'); const dt = new DataTransfer(); dt.setData('text/plain', ${JSON.stringify(cmd)}); i.focus(); i.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true, cancelable: true }))`)
    await until('filled', `return store.request.method === 'PUT' ? 1 : null`, 5000)
    const req = await run(`return { m: store.request.method, url: store.request.url, h: store.request.headers.map((h) => h.name), auth: store.request.auth }`)
    check('pasting curl into the URL fills the request', req.m === 'PUT' && req.url.includes('/echo/curl'), JSON.stringify(req))
    const r = await send()
    check('and it sends', echoed(r).method === 'PUT' && echoed(r).body === '{"k":"v"}', echoed(r).body)
    const gen = await runAsync(`return JSON.stringify(await store.generateCode('python', false))`)
    check('code generation gives Python', /requests/.test(gen), gen.slice(0, 160))
    const langs = await runAsync(`return JSON.stringify(await window.__TAURI_INTERNALS__.invoke('code_languages'))`)
    check('several languages are offered', JSON.parse(langs).length >= 4, langs.slice(0, 200))
  })

  if (want('history')) await step('History', async () => {
    await run(`button('History').click()`)
    await until('list', `return store.history.length ? 1 : null`)
    const n = await run(`return store.history.length`)
    check('every send is in history', n >= 5, n)
    await run(`$$('.history button, .history-list button, nav[aria-label="History"] button').find((b) => /echo|login|me/.test(text(b)))?.click()`)
    const from = await until('reopen', `return store.historyEntry ? 1 : null`, 5000).catch(() => 0)
    check('a history entry reopens', from === 1)
    await run(`button('Collection').click()`)
  })

  if (want('runner')) await step('Runner, examples', async () => {
    const run1 = await runAsync(`const r = await store.runCollection(null, false); return JSON.stringify({ n: r?.results?.length ?? r?.steps?.length, keys: Object.keys(r ?? {}), passed: r?.passed, failed: r?.failed })`)
    check('the runner runs the collection', !/ERROR|null/.test(run1), run1)
    await run(`store.runnerDialog = true`)
    await until('runner', `return $('.ui-dialog') ? 1 : null`)
    await app.screenshot(join(shots, 'runner.png'))
    await run(`store.runnerDialog = false`)
  })

  if (want('io')) await step('Import and export', async () => {
    const postman = {
      info: { name: 'From Postman', schema: 'https://schema.getpostman.com/json/collection/v2.1.0/collection.json' },
      item: [
        { name: 'Folder A', item: [{ name: 'Get one', request: { method: 'GET', url: { raw: '{{host}}/one', host: ['{{host}}'], path: ['one'] }, header: [{ key: 'Accept', value: 'application/json' }] } }] },
        { name: 'Create', request: { method: 'POST', url: '{{host}}/things', body: { mode: 'raw', raw: '{"a":1}', options: { raw: { language: 'json' } } } } },
      ],
      variable: [{ key: 'host', value: 'http://127.0.0.1:8787' }],
    }
    const src = join(tmpdir(), 'volt-e2e-postman.json')
    writeFileSync(src, JSON.stringify(postman))
    const into = join(tmpdir(), 'volt-e2e-imported')
    rmSync(into, { recursive: true, force: true })
    mkdirSync(into, { recursive: true })
    const out = await runAsync(`return JSON.stringify(await window.__TAURI_INTERNALS__.invoke('import_collection', { source: ${JSON.stringify(src)}, into: ${JSON.stringify(into)} }))`)
    check('a Postman collection imports', !/ERROR/.test(out) && existsSync(join(into, 'from-postman', 'collection.yaml')), out.slice(0, 200))
    const files = existsSync(into) ? readdirSync(into, { recursive: true }).map(String) : []
    check('with its folder and requests', files.some((f) => /folder-a./.test(f)) && files.some((f) => /create\.yaml$/.test(f)), files.join(', '))
    rmSync(into, { recursive: true, force: true })

    const ex = await runAsync(`return JSON.stringify(await window.__TAURI_INTERNALS__.invoke('export_postman', { root: store.root }))`)
    check('export for Postman makes a file', !/ERROR/.test(ex), ex.slice(0, 200))
    const docs = await runAsync(`return JSON.stringify(await window.__TAURI_INTERNALS__.invoke('export_docs', { root: store.root }))`)
    check('documentation is written', !/ERROR/.test(docs), docs.slice(0, 200))
  })

  if (want('stream')) await step('WebSocket and SSE', async () => {
    await newRequest()
    await setUrl('ws://127.0.0.1:8787/ws')
    const kind = await run(`return text($('button.send'))`)
    check('a ws:// URL makes Send say Connect', /Connect/i.test(kind), kind)
    await run(`$('button.send').click()`)
    await until('open', `return store.myStream ? 1 : null`, 8000)
    await until('welcome', `return store.streamEvents.some((e) => JSON.stringify(e).includes('welcome')) ? 1 : null`, 5000)
    await run(`const i = $('textarea[aria-label="Message to send"], input[aria-label="Message to send"]'); setValue(i, 'ping')`)
    await runAsync(`await store.sendStreamText('ping'); return 1`)
    const got = await until('echo', `return store.streamEvents.some((e) => JSON.stringify(e).includes('echo: ping')) ? 1 : null`, 5000).catch(() => 0)
    check('a message goes out and its echo comes back', got === 1)
    await runAsync(`await store.closeStream(); return 1`)
    await until('closed', `return store.myStream ? null : 1`, 5000)
    check('disconnect closes it', true)

    await newRequest()
    await setUrl('{{base}}/sse')
    await run(`store.requestKind = 'sse'`)
    const opened = await runAsync(`await store.openStream('sse'); return 1`)
    const events = await until('events', `return store.streamEvents.filter((e) => e.kind === 'message').length >= 3 ? store.streamEvents.length : null`, 8000).catch(() => 0)
    check('SSE events arrive', events > 0, `${opened} ${events}`)
  })

  if (want('graphql')) await step('GraphQL', async () => {
    await newRequest()
    await setUrl('{{base}}/graphql')
    await run(`store.request.method = 'POST'; store.request.body = { type: 'graphql', query: 'query($name: String) { hello(name: $name) }', variables: '{"name":"volt"}' }; store.touch()`)
    const r = await send()
    check('a GraphQL query with variables goes out', /hi volt/.test(r.body), r.body.slice(0, 100))
    const schema = await runAsync(`const s = await store.introspect(); return JSON.stringify(s)?.slice(0, 200) ?? 'null'`)
    check('introspection reads the schema', /hello|Query/.test(schema), schema)
  })

  if (want('mock')) await step('Mock server', async () => {
    // Keep an example for the login request, then serve it.
    const loginId = await run(`return store.tabs.find((t) => t.request?.url?.includes('/login'))?.id ?? null`)
    if (loginId) {
      await runAsync(`await store.select(${JSON.stringify(loginId)}); return 1`)
      await send()
      const kept = await runAsync(`return await store.saveExample('ok')`)
      check('an example is kept', kept === true, kept)
    }
    const m = await runAsync(`return JSON.stringify(await store.startMock(8799))`)
    check('the mock server starts', !/null|ERROR/.test(m), m)
    const res = await fetch('http://127.0.0.1:8799/login').then(async (x) => `${x.status} ${await x.text()}`).catch((e) => e.message)
    check('and answers with the example', /t-123/.test(res), res.slice(0, 120))
    await runAsync(`await store.stopMock(); return 1`)
  })

  if (want('settings')) await step('Settings and themes', async () => {
    await run(`button('Settings').click()`)
    await until('settings', `return $('.ui-dialog') ? 1 : null`)
    const before = await run(`return store.theme`)
    for (const theme of ['light', 'dark', 'linen', 'mist']) {
      await runAsync(`await store.saveTheme('${theme}'); return 1`)
      await sleep(150)
      const bg = await run(`return getComputedStyle(document.body).backgroundColor + ' ' + (document.documentElement.dataset.theme ?? '')`)
      check(`theme ${theme} applies`, bg.includes(theme), bg)
      await app.screenshot(join(shots, `theme-${theme}.png`))
    }
    await runAsync(`await store.saveTheme('${before}'); return 1`)
    await run(`const d = $('.ui-dialog'); button('Close', d)?.click()`)
  })

  if (want('tabs')) await step('Tabs survive a restart', async () => {
    const before = await run(`return store.tabs.filter((t) => !t.historyEntry).map((t) => t.id)`)
    await runAsync(`await store.persistTabs(); return 1`)
    await app.close()
    app = await session([collection])
    const again = await app.until('reopen', `return store.collection && !store.restoringTabs && store.tabs.length ? store.tabs.map((t) => t.id) : null`, 30000)
    check('the open tabs come back', again.length === before.length, `${before.length} → ${again.length}`)
  })

  const errors = await app.run(`return store.error`)
  check('no error left on screen at the end', !errors, errors)
} catch (e) {
  check('run completed', false, e.message)
} finally {
  if (app) { try { await app.screenshot(join(shots, 'last.png')) } catch {} ; await app.close() }
  stopDriver()
  server.close()
}
if (lib.failures) console.log(driverLog.slice(-1500))
console.log(lib.failures ? `\n${lib.failures} FAILED` : '\nALL PASSED')
process.exit(lib.failures ? 1 : 0)
