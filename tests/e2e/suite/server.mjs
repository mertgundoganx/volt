// A local server with one endpoint per thing the app has to handle.
import { createServer } from 'node:http'
import { createHash } from 'node:crypto'

export function startServer(port = 8787) {
  const server = createServer(async (req, res) => {
    const url = new URL(req.url, `http://127.0.0.1:${port}`)
    const chunks = []
    for await (const c of req) chunks.push(c)
    const raw = Buffer.concat(chunks)
    const json = (status, value, headers = {}) => {
      res.writeHead(status, { 'content-type': 'application/json', ...headers })
      res.end(JSON.stringify(value))
    }
    const p = url.pathname
    if (p === '/echo' || p.startsWith('/echo/')) {
      return json(200, {
        method: req.method,
        path: p,
        query: Object.fromEntries(url.searchParams),
        headers: req.headers,
        body: raw.toString('utf8'),
        bytes: raw.length,
      })
    }
    if (p.startsWith('/status/')) return json(Number(p.slice(8)), { status: Number(p.slice(8)) })
    if (p === '/slow') {
      await new Promise((r) => setTimeout(r, Number(url.searchParams.get('ms') ?? 3000)))
      return json(200, { slow: true })
    }
    if (p === '/cookie/set') return json(200, { set: true }, { 'set-cookie': 'sid=abc123; Path=/' })
    if (p === '/cookie/echo') return json(200, { cookie: req.headers.cookie ?? null })
    if (p === '/redirect') { res.writeHead(302, { location: '/echo?redirected=1' }); return res.end() }
    if (p === '/basic') {
      const ok = req.headers.authorization === `Basic ${Buffer.from('user:pass').toString('base64')}`
      return json(ok ? 200 : 401, { ok }, ok ? {} : { 'www-authenticate': 'Basic realm="t"' })
    }
    if (p === '/token') {
      const form = new URLSearchParams(raw.toString('utf8'))
      const basic = req.headers.authorization === 'Basic ' + Buffer.from('cid:csecret').toString('base64')
      const ok = form.get('grant_type') === 'client_credentials' && (basic || (form.get('client_id') === 'cid' && form.get('client_secret') === 'csecret'))
      return json(ok ? 200 : 401, ok ? { access_token: 'oauth-tok', token_type: 'Bearer', expires_in: 3600 } : { error: 'invalid_client', got: Object.fromEntries(form) })
    }
    if (p === '/login') return json(200, { data: { token: 't-123', user: { id: 7, name: 'Ada' } } })
    if (p === '/me') {
      const ok = req.headers.authorization === 'Bearer t-123'
      return json(ok ? 200 : 401, ok ? { id: 7, name: 'Ada' } : { error: 'no token' })
    }
    if (p === '/html') { res.writeHead(200, { 'content-type': 'text/html' }); return res.end('<!doctype html><h1>Hello page</h1>') }
    if (p === '/text') { res.writeHead(200, { 'content-type': 'text/plain' }); return res.end('plain words') }
    if (p === '/xml') { res.writeHead(200, { 'content-type': 'application/xml' }); return res.end('<a><b>1</b></a>') }
    if (p === '/big') {
      const items = Array.from({ length: 40000 }, (_, i) => ({ id: i, name: `item ${i}`, tags: ['a', 'b'], nested: { ok: i % 2 === 0 } }))
      return json(200, { items })
    }
    if (p === '/image') {
      const png = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==', 'base64')
      res.writeHead(200, { 'content-type': 'image/png' })
      return res.end(png)
    }
    if (p === '/sse') {
      res.writeHead(200, { 'content-type': 'text/event-stream', 'cache-control': 'no-cache' })
      let n = 0
      const t = setInterval(() => {
        n += 1
        res.write(`event: tick\ndata: {"n":${n}}\n\n`)
        if (n === 3) { clearInterval(t); res.end() }
      }, 200)
      return
    }
    if (p === '/graphql') {
      const body = JSON.parse(raw.toString('utf8') || '{}')
      if (/__schema/.test(body.query ?? '')) return json(200, { data: { __schema: { queryType: { name: 'Query' }, mutationType: null, subscriptionType: null, types: [
        { kind: 'OBJECT', name: 'Query', fields: [{ name: 'hello', args: [{ name: 'name', type: { kind: 'SCALAR', name: 'String', ofType: null }, defaultValue: null }], type: { kind: 'SCALAR', name: 'String', ofType: null }, isDeprecated: false, deprecationReason: null }], inputFields: null, interfaces: [], enumValues: null, possibleTypes: null },
        { kind: 'SCALAR', name: 'String', fields: null, inputFields: null, interfaces: null, enumValues: null, possibleTypes: null },
      ], directives: [] } } })
      return json(200, { data: { hello: `hi ${body.variables?.name ?? 'there'}` }, query: body.query })
    }
    json(404, { error: 'not found', path: p })
  })

  // A WebSocket that echoes text back, just enough of RFC 6455 for a client.
  server.on('upgrade', (req, socket) => {
    const key = req.headers['sec-websocket-key']
    const accept = createHash('sha1').update(key + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11').digest('base64')
    socket.write(`HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`)
    const send = (op, payload) => {
      const len = payload.length
      const head = len < 126 ? Buffer.from([0x80 | op, len]) : Buffer.from([0x80 | op, 126, len >> 8, len & 255])
      socket.write(Buffer.concat([head, payload]))
    }
    send(1, Buffer.from('welcome'))
    let buf = Buffer.alloc(0)
    socket.on('data', (d) => {
      buf = Buffer.concat([buf, d])
      while (buf.length >= 2) {
        const op = buf[0] & 15
        let len = buf[1] & 127
        let off = 2
        if (len === 126) { len = buf.readUInt16BE(2); off = 4 }
        const masked = buf[1] & 128
        const need = off + (masked ? 4 : 0) + len
        if (buf.length < need) return
        const mask = masked ? buf.subarray(off, off + 4) : null
        const data = Buffer.from(buf.subarray(off + (masked ? 4 : 0), need))
        if (mask) for (let i = 0; i < data.length; i++) data[i] ^= mask[i % 4]
        buf = buf.subarray(need)
        if (op === 1) send(1, Buffer.from(`echo: ${data.toString()}`))
        else if (op === 8) { send(8, Buffer.alloc(0)); socket.end() }
        else if (op === 9) send(10, data)
      }
    })
    socket.on('error', () => {})
  })
  return new Promise((r) => server.listen(port, '127.0.0.1', () => r(server)))
}
