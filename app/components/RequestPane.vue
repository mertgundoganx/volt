<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import {
  METHODS,
  type Auth,
  type Body,
  type Capture,
  type Check,
  type RequestKind,
  type RequestOptions,
} from '~/types'
import type { MenuItem, SegmentOption, TabItem } from '~/utils/ui'
import { prettyJson } from '~/utils/syntax'
import { isCurl } from '~/utils/curl'

const store = useCollectionStore()
const tab = ref<'params' | 'headers' | 'body' | 'auth' | 'captures' | 'checks' | 'options' | 'docs'>('params')

const request = computed(() => store.request!)

/** Variables the active environment defines, for marking undefined ones. */
const known = computed(() => store.environment?.vars.map((v) => v.name) ?? [])

type BodyKind = Body['type']
type AuthKind = Auth['type']

const bodyKinds: SegmentOption<Exclude<BodyKind, 'binary'>>[] = [
  { value: 'none', label: 'None' },
  { value: 'json', label: 'JSON' },
  { value: 'text', label: 'Text' },
  { value: 'xml', label: 'XML' },
  { value: 'urlencoded', label: 'URL-encoded' },
  { value: 'form', label: 'Multipart' },
  { value: 'graphql', label: 'GraphQL' },
]

const authKinds: SegmentOption<AuthKind>[] = [
  { value: 'inherit', label: 'Inherit' },
  { value: 'none', label: 'None' },
  { value: 'bearer', label: 'Bearer' },
  { value: 'basic', label: 'Basic' },
  { value: 'apikey', label: 'API key' },
  { value: 'digest', label: 'Digest' },
  { value: 'ntlm', label: 'NTLM' },
  { value: 'awssigv4', label: 'AWS' },
]

const AUTH_NAMES: Record<AuthKind, string> = {
  inherit: 'inherited auth',
  none: 'no auth',
  bearer: 'a bearer token',
  basic: 'basic auth',
  apikey: 'an API key',
  digest: 'digest auth',
  ntlm: 'NTLM',
  awssigv4: 'an AWS signature',
}

// --- Per-request send options ------------------------------------------------
// Each one is either absent, meaning "whatever Settings says", or present with
// a value of its own. The checkbox is which of the two, never a third state.
type OptionKey = keyof RequestOptions

const overrides = computed(() => Object.keys(request.value.options ?? {}).length)

function overridden(key: OptionKey) {
  return request.value.options?.[key] != null
}

function setOption(key: OptionKey, value: string | number | boolean | null) {
  const next: RequestOptions = { ...(request.value.options ?? {}) }
  if (value === null) delete next[key]
  else (next as Record<string, unknown>)[key] = value
  request.value.options = Object.keys(next).length ? next : null
  store.touch()
}

/** Ticking the box starts from what Settings would have done. */
function toggleOption(key: OptionKey, on: boolean) {
  if (!on) return setOption(key, null)
  const fallback: Record<OptionKey, string | number | boolean> = {
    timeout_ms: store.options.timeoutMs,
    follow_redirects: store.options.followRedirects,
    verify_tls: store.options.verifyTls,
    proxy: store.options.proxy ?? '',
    client_cert: store.options.clientCert ?? '',
  }
  setOption(key, fallback[key])
}

// --- Captures ----------------------------------------------------------------
// Always one blank row at the end, like the key/value editors.
const captureRows = computed(() => {
  const rows = request.value.captures ?? []
  const last = rows[rows.length - 1]
  return last && !last.name && !last.from ? rows : [...rows, { name: '', from: '', enabled: true }]
})

function updateCapture(index: number, patch: Partial<Capture>) {
  const next = [...captureRows.value]
  next[index] = { ...next[index]!, ...patch }
  const kept = next.filter((row, i) => row.name || row.from || i === next.length - 1).filter((row) => row.name || row.from)
  request.value.captures = kept.length ? kept : []
  store.touch()
}

function removeCapture(index: number) {
  request.value.captures = captureRows.value.filter((_, i) => i !== index).filter((row) => row.name || row.from)
  store.touch()
}

// --- Checks ------------------------------------------------------------------
const CHECK_OPS: SegmentOption<Check['op']>[] = [
  { value: 'is', label: 'is' },
  { value: 'isnot', label: 'is not' },
  { value: 'contains', label: 'contains' },
  { value: 'exists', label: 'exists' },
  { value: 'missing', label: 'missing' },
  { value: 'under', label: 'under' },
  { value: 'over', label: 'over' },
]
const TAKES_VALUE = (op: Check['op']) => op !== 'exists' && op !== 'missing'

const checkRows = computed(() => {
  const rows = request.value.checks ?? []
  const last = rows[rows.length - 1]
  return last && !last.from ? rows : [...rows, { from: '', op: 'is' as const, value: '', enabled: true }]
})

function updateCheck(index: number, patch: Partial<Check>) {
  const next = [...checkRows.value]
  next[index] = { ...next[index]!, ...patch }
  request.value.checks = next.filter((row) => row.from)
  store.touch()
}

function removeCheck(index: number) {
  request.value.checks = checkRows.value.filter((_, i) => i !== index).filter((row) => row.from)
  store.touch()
}

// --- What kind of request this is --------------------------------------------
const KINDS: SegmentOption<RequestKind>[] = [
  { value: 'http', label: 'HTTP' },
  { value: 'websocket', label: 'WebSocket' },
  { value: 'sse', label: 'SSE' },
  { value: 'grpc', label: 'gRPC' },
]

const requestKind = computed({
  get: () => request.value.kind ?? 'http',
  set: (kind: RequestKind) => {
    request.value.kind = kind
    // gRPC carries its call in the body; the others are plain requests.
    if (kind === 'grpc' && request.value.body.type !== 'grpc') {
      request.value.body = { type: 'grpc', proto: '', method: '', message: '' }
    } else if (kind !== 'grpc' && request.value.body.type === 'grpc') {
      request.value.body = { type: 'none' }
    }
    store.touch()
  },
})

const tabs = computed<TabItem[]>(() => [
  { key: 'params', label: 'Params', meta: request.value.params.length || null },
  { key: 'headers', label: 'Headers', meta: request.value.headers.length || null },
  { key: 'body', label: 'Body', meta: request.value.body.type === 'none' ? null : request.value.body.type },
  { key: 'auth', label: 'Auth', meta: request.value.auth.type },
  { key: 'captures', label: 'Captures', meta: (request.value.captures?.length ?? 0) || null },
  { key: 'checks', label: 'Tests', meta: (request.value.checks?.length ?? 0) || null },
  { key: 'options', label: 'Options', meta: overrides.value || null },
  { key: 'docs', label: 'Docs', dot: !!request.value.docs?.trim() },
])

/**
 * Show where the request will land, query string included, and which
 * variables have no value. Replies can arrive out of order while typing, so
 * only the latest one is kept.
 */
const resolved = ref<{ value: string; missing: string[] }>({ value: '', missing: [] })
let previewTick = 0
watchEffect(async () => {
  const url = request.value.url.trim()
  const query = request.value.params
    .filter((p) => p.enabled && p.name)
    .map((p) => `${p.name}=${p.value}`)
    .join('&')
  const full = query ? `${url}${url.includes('?') ? '&' : '?'}${query}` : url
  const vars = store.environment?.vars ?? []

  const tick = ++previewTick
  if (!url || (full === url && !url.includes('{{'))) {
    resolved.value = { value: '', missing: [] }
    return
  }
  try {
    const out = await invoke<{ value: string; missing: string[] }>('preview', { input: full, envVars: vars })
    if (tick === previewTick) resolved.value = out
  } catch {
    if (tick === previewTick) resolved.value = { value: '', missing: [] }
  }
})

function setBodyKind(kind: BodyKind) {
  if (kind === request.value.body.type) return
  const next: Record<string, Body> = {
    none: { type: 'none' },
    json: { type: 'json', content: '' },
    text: { type: 'text', content: '' },
    xml: { type: 'xml', content: '' },
    urlencoded: { type: 'urlencoded', fields: [] },
    form: { type: 'form', fields: [] },
    graphql: { type: 'graphql', query: '', variables: '' },
    grpc: { type: 'grpc', proto: '', method: '', message: '' },
  }
  // Switching between the text kinds keeps what was typed.
  const current = request.value.body
  const carried = 'content' in current ? current.content : ''
  const body = next[kind]!
  if ('content' in body) body.content = carried
  request.value.body = body
  store.touch()
}

function setAuthKind(kind: AuthKind) {
  if (kind === request.value.auth.type) return
  const next: Record<AuthKind, Auth> = {
    inherit: { type: 'inherit' },
    none: { type: 'none' },
    bearer: { type: 'bearer', token: '' },
    basic: { type: 'basic', username: '', password: '' },
    apikey: { type: 'apikey', key: '', value: '', location: 'header' },
    digest: { type: 'digest', username: '', password: '' },
    ntlm: { type: 'ntlm', username: '', password: '', domain: '' },
    awssigv4: { type: 'awssigv4', key_id: '', secret: '', region: 'us-east-1', service: '', session_token: '' },
  }
  request.value.auth = next[kind]
  store.touch()
}

const bodyKind = computed({ get: () => request.value.body.type, set: setBodyKind })
const authKind = computed({ get: () => request.value.auth.type, set: setAuthKind })

const textBody = computed({
  get: () => ('content' in request.value.body ? request.value.body.content : ''),
  set: (value: string) => {
    if ('content' in request.value.body) {
      request.value.body.content = value
      store.touch()
    }
  },
})

const fieldBody = computed({
  get: () => ('fields' in request.value.body ? request.value.body.fields : []),
  set: (value) => {
    if ('fields' in request.value.body) {
      request.value.body.fields = value
      store.touch()
    }
  },
})

// Parse-only: `prettyJson` also builds the whole formatted copy, and this runs
// on every keystroke in the body editor.
const canFormat = computed(() => {
  if (request.value.body.type !== 'json') return false
  const trimmed = textBody.value.trimStart()
  if (!trimmed.startsWith('{') && !trimmed.startsWith('[')) return false
  try {
    JSON.parse(textBody.value)
    return true
  } catch {
    return false
  }
})
function formatJson() {
  const out = prettyJson(textBody.value)
  if (out.isJson && out.text !== textBody.value) textBody.value = out.text
}

const apiKeyLocation = computed({
  get: () => (request.value.auth.type === 'apikey' ? request.value.auth.location : 'header'),
  set: (location: 'header' | 'query') => {
    if (request.value.auth.type === 'apikey') {
      request.value.auth.location = location
      store.touch()
    }
  },
})

const collectionAuth = computed(() => AUTH_NAMES[store.collection?.meta.auth.type ?? 'none'])

/// The one action the request has, whatever kind it is.
function onSend() {
  if (!store.sending && request.value.url) store.go()
}

const sendLabel = computed(() => {
  if (store.sending) return 'Sending'
  const kind = requestKind.value
  // A streaming gRPC method is a connection, and the key says so.
  if (kind === 'grpc') return store.grpcStreams ? (store.myStream ? 'Disconnect' : 'Connect') : 'Call'
  if (kind === 'websocket' || kind === 'sse') return store.myStream ? 'Disconnect' : 'Connect'
  return 'Send'
})

/** A curl command pasted into the URL fills the whole request instead. */
function onUrlPaste(event: ClipboardEvent) {
  const text = event.clipboardData?.getData('text/plain') ?? ''
  if (!isCurl(text)) return
  event.preventDefault()
  store.importCurl(text, { mode: 'replace' })
}

const curlItems: MenuItem[] = [
  { key: 'copy', label: 'Copy as cURL', icon: 'copy', hint: 'Secret values stay as {{name}}, safe to share' },
  { key: 'copy-secrets', label: 'Copy with secret values', icon: 'unlock', hint: 'Ready to run; do not paste it anywhere public' },
  { key: 'fill', label: 'Fill from cURL…', icon: 'import', hint: 'For a multi-line command', divided: true },
  { key: 'code', label: 'Generate code…', icon: 'braces', hint: 'fetch, Python, Go, C#, PHP, Ruby', divided: true },
]

function onCurlMenu(key: string) {
  if (key === 'copy') store.copyCurl(false)
  else if (key === 'copy-secrets') store.copyCurl(true)
  else if (key === 'fill') store.curlDialog = { mode: 'replace' }
  else if (key === 'code') store.codeDialog = true
}

// Written inline in the template, the braces would close the interpolation.
const secretsAsVars = (names: string[]) => names.map((n) => `{{${n}}}`).join(', ')

/** The file a history entry came from may since have been moved or deleted. */
const originalExists = computed(() => {
  const id = store.historyEntry?.requestId
  return !!id && store.findNode(id) !== null
})

const sentAt = computed(() => {
  const at = store.historyEntry?.at
  return at ? new Date(at).toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' }) : ''
})

function setName(event: Event) {
  request.value.name = (event.target as HTMLInputElement).value
  store.touch()
}

function setMethod(event: Event) {
  request.value.method = (event.target as HTMLSelectElement).value
  store.touch()
}

useShortcut('mod+enter', 'Send', () => onSend())
useShortcut('mod+d', 'Duplicate the request', () => {
  if (store.activeId && !store.historyEntry) store.duplicate(store.activeId)
})
useShortcut('mod+s', 'Save the request', () => {
  // A request reopened from history has no file yet; saving it makes one.
  if (store.historyEntry) store.saveHistoryAsNew()
  else if (store.dirty) store.save()
})
</script>

<template>
  <section class="request" aria-label="Request">
    <header class="head">
      <input
        class="title"
        :value="request.name"
        aria-label="Request name"
        spellcheck="false"
        placeholder="Untitled request"
        @input="setName"
      >
      <span class="file mono" :title="store.historyEntry ? 'Not saved to a file' : (store.activeId ?? '')">
        {{ store.historyEntry ? 'Not saved to a file' : store.activeId }}
      </span>
      <span class="spacer" />

      <UiMenuButton :items="curlItems" label="cURL" align="end" @select="onCurlMenu">
        <UiIcon name="braces" :size="14" />cURL
      </UiMenuButton>

      <template v-if="store.historyEntry">
        <button v-if="originalExists" type="button" class="btn btn-quiet btn-sm" title="Open the request file this was sent from" @click="store.select(store.historyEntry.requestId!)">
          <UiIcon name="arrow-up-right" :size="14" />Open saved request
        </button>
        <button type="button" class="btn btn-sm" :title="`Write this request into the collection (${modKey}+S)`" @click="store.saveHistoryAsNew()">
          Save as new
        </button>
      </template>
      <template v-else>
        <span v-if="store.dirty" class="state"><span class="led warn" /><span class="silk">Unsaved</span></span>
        <button type="button" class="btn btn-sm" :disabled="!store.dirty" aria-label="Save request" :title="`Save (${modKey}+S)`" @click="store.save()">
          Save
        </button>
      </template>
    </header>

    <div v-if="store.historyEntry" class="from-history">
      <UiIcon name="history" :size="14" />
      <span>
        Reopened from history · sent {{ sentAt }}
        <template v-if="store.historyEntry.environment">with <b>{{ store.historyEntry.environment }}</b></template>
      </span>
    </div>

    <div class="probe" :class="{ sending: store.sending }">
      <label v-if="requestKind === 'http'" class="method-pick" :title="`Method: ${request.method}`">
        <select class="method tag" :data-method="request.method" :value="request.method" aria-label="Method" @change="setMethod">
          <option v-for="m in METHODS" :key="m" :value="m">{{ m }}</option>
        </select>
        <UiIcon name="chevron-down" :size="11" class="chev" />
      </label>
      <label v-else class="method-pick" title="What kind of request this is">
        <select class="method tag" :value="requestKind" aria-label="Kind of request" @change="requestKind = ($event.target as HTMLSelectElement).value as RequestKind">
          <option v-for="kind in KINDS" :key="kind.value" :value="kind.value">{{ kind.label }}</option>
        </select>
        <UiIcon name="chevron-down" :size="11" class="chev" />
      </label>

      <UiVarInput
        class="url"
        variant="bare"
        :model-value="request.url"
        :known="known"
        label="URL"
        placeholder="{{baseUrl}}/users, or paste a whole curl command"
        @update:model-value="request.url = $event; store.touch()"
        @enter="onSend"
        @paste="onUrlPaste"
      />

      <button type="button" class="send" :disabled="store.sending || !request.url" :title="`${sendLabel} (${modKey}+Enter)`" @click="onSend">
        <span>{{ sendLabel }}</span>
        <UiIcon name="send" :size="14" />
      </button>
    </div>

    <div v-if="resolved.value || resolved.missing.length" class="resolved">
      <template v-if="resolved.value">
        <UiIcon name="arrow-right" :size="12" class="arrow" />
        <span class="value mono selectable" :title="resolved.value">{{ resolved.value }}</span>
      </template>
      <span v-if="resolved.missing.length" class="chip warn" :title="`No value in ${store.environment?.name ?? 'any environment'}`">
        <UiIcon name="warning" :size="12" />Undefined: {{ resolved.missing.join(', ') }}
      </span>
    </div>

    <div v-if="store.curlNotice" class="curl-notice" role="status">
      <div class="notice-head">
        <span class="led ok" />
        <span class="silk">Filled from curl</span>
        <span v-if="store.curlNotice.secrets.length" class="moved">
          <UiIcon name="lock" :size="12" />
          <span>
            {{ store.curlNotice.secrets.length === 1 ? 'Credential moved to' : 'Credentials moved to' }}
            <code>{{ secretsAsVars(store.curlNotice.secrets) }}</code>
            in <b>{{ store.curlNotice.environment }}</b>
          </span>
        </span>
        <span class="spacer" />
        <button type="button" class="icon-btn quiet sm" aria-label="Dismiss" title="Dismiss" @click="store.curlNotice = null">
          <UiIcon name="x" :size="13" />
        </button>
      </div>
      <ul v-if="store.curlNotice.notes.length" class="notice-notes">
        <li v-for="note in store.curlNotice.notes" :key="note">{{ note }}</li>
      </ul>
    </div>

    <UiTabs v-model="tab" :items="tabs" label="Request sections" />

    <div class="panel">
      <KeyValueEditor
        v-if="tab === 'params'"
        v-model="request.params"
        name-label="Parameter"
        add-label="Add parameter"
        :known="known"
        @update:model-value="store.touch()"
      />

      <KeyValueEditor
        v-else-if="tab === 'headers'"
        v-model="request.headers"
        name-label="Header"
        add-label="Add header"
        :known="known"
        @update:model-value="store.touch()"
      />

      <div v-else-if="tab === 'body'" class="section">
        <div class="toolbar">
          <UiSegmented v-model="bodyKind" :options="bodyKinds" label="Body type" size="sm" />
          <span class="spacer" />
          <button v-if="request.body.type === 'json'" type="button" class="btn btn-quiet btn-sm" :disabled="!canFormat" :title="canFormat ? 'Pretty-print the JSON' : 'Not valid JSON yet'" @click="formatJson">
            <UiIcon name="braces" :size="14" />Format
          </button>
        </div>

        <p v-if="request.body.type === 'none'" class="note">This request sends no body.</p>
        <p v-else-if="request.body.type === 'binary'" class="note">
          <span>Sends the file <code>{{ request.body.path }}</code> from the collection folder. Edit it in the YAML.</span>
        </p>
        <GrpcEditor v-else-if="request.body.type === 'grpc'" v-model="request.body" />

        <GraphQlEditor v-else-if="request.body.type === 'graphql'" v-model="request.body" />

        <KeyValueEditor
          v-else-if="'fields' in request.body"
          v-model="fieldBody"
          name-label="Field"
          add-label="Add field"
          :known="known"
          :files="request.body.type === 'form'"
        />
        <UiCodeEditor
          v-else
          v-model="textBody"
          label="Request body"
          :placeholder="request.body.type === 'json' ? '{\n  &quot;name&quot;: &quot;{{userName}}&quot;\n}' : ''"
        />
      </div>

      <div v-else-if="tab === 'auth'" class="section">
        <div class="toolbar">
          <UiSegmented v-model="authKind" :options="authKinds" label="Auth type" size="sm" />
        </div>

        <!-- Text sits in one span: as direct flex children, the gap would split it around <code>. -->
        <p v-if="request.auth.type === 'inherit'" class="note">
          <UiIcon name="lock" :size="14" />
          <span>Uses the collection's auth, set in <code>collection.yaml</code>: currently {{ collectionAuth }}.</span>
        </p>
        <p v-else-if="request.auth.type === 'none'" class="note">Sends no credentials, even if the collection has some.</p>

        <div v-else class="form">
          <template v-if="request.auth.type === 'bearer'">
            <span class="silk">Token</span>
            <UiVarInput v-model="request.auth.token" :known="known" label="Token" placeholder="{{token}}" @update:model-value="store.touch()" />
            <span class="hint">Sent as <code>Authorization: Bearer …</code></span>
          </template>

          <template v-else-if="request.auth.type === 'basic'">
            <span class="silk">Username</span>
            <UiVarInput v-model="request.auth.username" :known="known" label="Username" @update:model-value="store.touch()" />
            <span />
            <span class="silk">Password</span>
            <UiVarInput v-model="request.auth.password" :known="known" label="Password" placeholder="{{password}}" @update:model-value="store.touch()" />
            <span class="hint">Keep it in a secret variable, not here.</span>
          </template>

          <template v-else-if="request.auth.type === 'apikey'">
            <span class="silk">Name</span>
            <UiVarInput v-model="request.auth.key" :known="known" label="Key name" placeholder="X-API-Key" @update:model-value="store.touch()" />
            <span />
            <span class="silk">Value</span>
            <UiVarInput v-model="request.auth.value" :known="known" label="Key value" placeholder="{{apiKey}}" @update:model-value="store.touch()" />
            <span />
            <span class="silk">Send in</span>
            <UiSegmented
              v-model="apiKeyLocation"
              :options="[{ value: 'header', label: 'Header' }, { value: 'query', label: 'Query string' }]"
              label="Send API key in"
              size="sm"
            />
            <span />
          </template>

          <template v-else-if="request.auth.type === 'digest'">
            <span class="silk">Username</span>
            <UiVarInput v-model="request.auth.username" :known="known" label="Username" @update:model-value="store.touch()" />
            <span />
            <span class="silk">Password</span>
            <UiVarInput v-model="request.auth.password" :known="known" label="Password" placeholder="{{password}}" @update:model-value="store.touch()" />
            <span class="hint">Answered after the server challenges, so the first send returns 401 and the second carries it.</span>
          </template>

          <template v-else-if="request.auth.type === 'ntlm'">
            <span class="silk">Username</span>
            <UiVarInput v-model="request.auth.username" :known="known" label="Username" @update:model-value="store.touch()" />
            <span />
            <span class="silk">Password</span>
            <UiVarInput v-model="request.auth.password" :known="known" label="Password" placeholder="{{password}}" @update:model-value="store.touch()" />
            <span />
            <span class="silk">Domain</span>
            <UiVarInput v-model="request.auth.domain" :known="known" label="Domain" @update:model-value="store.touch()" />
            <span class="hint">Three legs down one connection, so this request is sent over HTTP/1.1.</span>
          </template>

          <template v-else-if="request.auth.type === 'awssigv4'">
            <span class="silk">Key ID</span>
            <UiVarInput v-model="request.auth.key_id" :known="known" label="Access key ID" placeholder="{{awsKeyId}}" @update:model-value="store.touch()" />
            <span />
            <span class="silk">Secret</span>
            <UiVarInput v-model="request.auth.secret" :known="known" label="Secret access key" placeholder="{{awsSecret}}" @update:model-value="store.touch()" />
            <span />
            <span class="silk">Region</span>
            <UiVarInput v-model="request.auth.region" :known="known" label="Region" placeholder="eu-west-1" @update:model-value="store.touch()" />
            <span />
            <span class="silk">Service</span>
            <UiVarInput v-model="request.auth.service" :known="known" label="Service" placeholder="execute-api" @update:model-value="store.touch()" />
            <span />
            <span class="silk">Session</span>
            <UiVarInput v-model="request.auth.session_token" :known="known" label="Session token" placeholder="optional" @update:model-value="store.touch()" />
            <span class="hint">Signed over the finished request, so a copied cURL command works for about fifteen minutes.</span>
          </template>

        </div>
      </div>
      <div v-else-if="tab === 'captures'" class="section captures">
        <p class="lede">
          <span>
            Take a value out of the response and keep it in the environment, so the next
            request can use it. Read <code>status</code>, <code>header:Name</code>,
            <code>body</code>, or a path into a JSON body like <code>$.data.token</code>.
          </span>
        </p>

        <div class="table" role="table" aria-label="Captures">
          <div class="row head" role="row">
            <span role="columnheader" class="silk">Variable</span>
            <span role="columnheader" class="silk">From the response</span>
            <span role="columnheader" class="silk center">Secret</span>
            <span role="columnheader" />
          </div>

          <div v-for="(row, i) in captureRows" :key="i" class="row" role="row" :class="{ blank: !row.name && !row.from }">
            <input
              class="field inline mono"
              :value="row.name"
              :placeholder="row.name || row.from ? '' : 'token'"
              aria-label="Variable to write"
              spellcheck="false"
              @input="updateCapture(i, { name: ($event.target as HTMLInputElement).value })"
            >
            <input
              class="field inline mono"
              :value="row.from"
              :placeholder="row.name || row.from ? '' : '$.data.token'"
              aria-label="Where to read it from"
              spellcheck="false"
              @input="updateCapture(i, { from: ($event.target as HTMLInputElement).value })"
            >
            <span class="center">
              <button
                type="button"
                class="secret-toggle"
                :class="{ on: row.secret }"
                :disabled="!row.name"
                :aria-pressed="!!row.secret"
                :aria-label="row.secret ? 'Kept as a secret' : 'Kept in the environment YAML'"
                :title="row.secret ? 'Secret — stored in the gitignored .env file' : 'Plain — stored in the environment YAML'"
                @click="updateCapture(i, { secret: !row.secret })"
              >
                <UiIcon :name="row.secret ? 'lock' : 'unlock'" :size="13" />
              </button>
            </span>
            <span class="center">
              <button
                v-if="row.name || row.from"
                type="button"
                class="icon-btn quiet sm"
                :aria-label="`Remove ${row.name || 'capture'}`"
                title="Remove"
                @click="removeCapture(i)"
              >
                <UiIcon name="x" :size="14" />
              </button>
            </span>
          </div>
        </div>
      </div>

      <div v-else-if="tab === 'checks'" class="section captures">
        <p class="lede">
          <span>
            What has to be true of the response. The same places a capture reads —
            <code>status</code>, <code>time</code>, <code>header:Name</code>,
            <code>body</code>, <code>$.data.id</code> — with something to compare.
            A failing test is what makes a run fail, here and in CI.
          </span>
        </p>

        <div class="table" role="table" aria-label="Checks">
          <div class="row check head" role="row">
            <span role="columnheader" class="silk">From the response</span>
            <span role="columnheader" class="silk">Compare</span>
            <span role="columnheader" class="silk">Value</span>
            <span role="columnheader" />
          </div>

          <div v-for="(row, i) in checkRows" :key="i" class="row check" role="row" :class="{ blank: !row.from }">
            <input
              class="field inline mono"
              :value="row.from"
              :placeholder="row.from ? '' : 'status'"
              aria-label="What to read"
              spellcheck="false"
              @input="updateCheck(i, { from: ($event.target as HTMLInputElement).value })"
            >
            <UiSelect
              :model-value="row.op"
              :options="CHECK_OPS.map((op) => ({ value: op.value, label: op.label }))"
              label="How to compare"
              class="op"
              @update:model-value="updateCheck(i, { op: $event as Check['op'] })"
            />
            <input
              v-if="TAKES_VALUE(row.op)"
              class="field inline mono"
              :value="row.value ?? ''"
              :placeholder="row.from ? '' : '200'"
              aria-label="Expected value"
              spellcheck="false"
              @input="updateCheck(i, { value: ($event.target as HTMLInputElement).value })"
            >
            <span v-else class="nothing silk">nothing to compare</span>
            <span class="center">
              <button
                v-if="row.from"
                type="button"
                class="icon-btn quiet sm"
                :aria-label="`Remove the check on ${row.from}`"
                title="Remove"
                @click="removeCheck(i)"
              >
                <UiIcon name="x" :size="14" />
              </button>
            </span>
          </div>
        </div>
      </div>

      <div v-else-if="tab === 'options'" class="section options">
        <p class="lede">
          <span>
            How this one request goes out. Unticked, the send options follow Settings,
            so a collection stays portable between machines.
          </span>
        </p>

        <div class="opt">
          <span class="silk kind-label">This request is</span>
          <div class="control">
            <UiSegmented v-model="requestKind" :options="KINDS" label="Kind of request" size="sm" />
          </div>
        </div>

        <div class="opt">
          <label class="pick">
            <input
              type="checkbox"
              :checked="overridden('timeout_ms')"
              aria-label="Override the timeout"
              @change="toggleOption('timeout_ms', ($event.target as HTMLInputElement).checked)"
            >
            <span class="silk">Timeout</span>
          </label>
          <div class="control">
            <input
              class="field mono num"
              type="number"
              min="1"
              step="500"
              aria-label="Timeout in milliseconds"
              :disabled="!overridden('timeout_ms')"
              :value="request.options?.timeout_ms ?? store.options.timeoutMs"
              @input="setOption('timeout_ms', Math.max(1, Number(($event.target as HTMLInputElement).value) || 1))"
            >
            <span class="unit silk">ms</span>
          </div>
        </div>

        <div class="opt">
          <label class="pick">
            <input
              type="checkbox"
              :checked="overridden('follow_redirects')"
              aria-label="Override redirects"
              @change="toggleOption('follow_redirects', ($event.target as HTMLInputElement).checked)"
            >
            <span class="silk">Redirects</span>
          </label>
          <div class="control">
            <label class="switch-row">
              <input
                type="checkbox"
                class="switch"
                aria-label="Follow redirects"
                :disabled="!overridden('follow_redirects')"
                :checked="request.options?.follow_redirects ?? store.options.followRedirects"
                @change="setOption('follow_redirects', ($event.target as HTMLInputElement).checked)"
              >
              <span>Follow up to 10</span>
            </label>
          </div>
        </div>

        <div class="opt">
          <label class="pick">
            <input
              type="checkbox"
              :checked="overridden('verify_tls')"
              aria-label="Override TLS verification"
              @change="toggleOption('verify_tls', ($event.target as HTMLInputElement).checked)"
            >
            <span class="silk">TLS</span>
          </label>
          <div class="control">
            <label class="switch-row">
              <input
                type="checkbox"
                class="switch"
                aria-label="Verify certificates"
                :disabled="!overridden('verify_tls')"
                :checked="request.options?.verify_tls ?? store.options.verifyTls"
                @change="setOption('verify_tls', ($event.target as HTMLInputElement).checked)"
              >
              <span>Verify certificates</span>
            </label>
            <span v-if="request.options?.verify_tls === false" class="warn-note">
              <UiIcon name="warning" :size="13" />Anyone on the network can read this request.
            </span>
          </div>
        </div>

        <div class="opt">
          <label class="pick">
            <input
              type="checkbox"
              :checked="overridden('proxy')"
              aria-label="Override the proxy"
              @change="toggleOption('proxy', ($event.target as HTMLInputElement).checked)"
            >
            <span class="silk">Proxy</span>
          </label>
          <div class="control">
            <input
              class="field mono"
              aria-label="Proxy URL"
              spellcheck="false"
              :placeholder="store.options.proxy || 'http://127.0.0.1:8080'"
              :disabled="!overridden('proxy')"
              :value="request.options?.proxy ?? store.options.proxy ?? ''"
              @input="setOption('proxy', ($event.target as HTMLInputElement).value)"
            >
            <span v-if="overridden('proxy') && !request.options?.proxy" class="unit silk">direct</span>
          </div>
        </div>

        <div class="opt">
          <label class="pick">
            <input
              type="checkbox"
              :checked="overridden('client_cert')"
              aria-label="Override the client certificate"
              @change="toggleOption('client_cert', ($event.target as HTMLInputElement).checked)"
            >
            <span class="silk">Client cert</span>
          </label>
          <div class="control">
            <input
              class="field mono"
              aria-label="Client certificate path"
              spellcheck="false"
              :placeholder="store.options.clientCert || 'certs/client.pem'"
              :disabled="!overridden('client_cert')"
              :value="request.options?.client_cert ?? store.options.clientCert ?? ''"
              @input="setOption('client_cert', ($event.target as HTMLInputElement).value)"
            >
            <span class="unit silk">PEM</span>
          </div>
        </div>
      </div>


      <div v-else class="section">
        <textarea
          class="docs"
          :value="request.docs ?? ''"
          aria-label="Notes"
          placeholder="What this request is for, what it needs first, anything surprising about the response…"
          @input="request.docs = ($event.target as HTMLTextAreaElement).value; store.touch()"
        />
      </div>
    </div>
  </section>
</template>

<style scoped>
.request { display: flex; flex-direction: column; min-height: 0; min-width: 0; }

.head {
  display: flex;
  align-items: center;
  gap: var(--s-2);
  min-height: 44px;
  padding: var(--s-2) var(--s-4) var(--s-1);
}
.title {
  flex: 0 1 auto;
  min-width: 80px;
  max-width: 50%;
  margin-left: -6px;
  padding: 2px 6px;
  border: 1px solid transparent;
  border-radius: var(--r-sm);
  background: transparent;
  font-weight: 600;
  font-size: 15px;
  letter-spacing: -0.01em;
  line-height: 1.3;
  text-overflow: ellipsis;
  transition: background var(--dur) var(--ease), border-color var(--dur) var(--ease);
}
.title:hover { background: var(--hover); }
.title:focus { background: var(--well); border-color: var(--accent); }
.file { min-width: 0; color: var(--faint); font-size: var(--t-meta); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

.state { display: flex; align-items: center; gap: 6px; flex: none; padding: 0 4px; }
.state .silk { color: var(--warn); }

.from-history {
  display: flex;
  align-items: center;
  gap: var(--s-2);
  margin: 0 var(--s-4) var(--s-2);
  padding: 6px var(--s-3);
  border: 1px dashed var(--line-strong);
  border-radius: var(--r-sm);
  color: var(--ink-2);
  font-size: var(--t-small);
}
.from-history .ui-icon { color: var(--silk); }

/*
 * The URL bar: method stamp, address, Send. One bar, because to the person
 * typing it is one thing — where, and go.
 */
.probe {
  display: flex;
  align-items: stretch;
  height: var(--h-probe);
  margin: var(--s-1) var(--s-4) 0;
  border: 1px solid var(--line);
  border-radius: var(--r-md);
  background: var(--well);
  overflow: hidden;
  flex: none;
  transition: border-color var(--dur) var(--ease), box-shadow var(--dur) var(--ease);
}
.probe:hover { border-color: var(--line-strong); }
.probe:focus-within { border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent-tint); }

.method-pick {
  position: relative;
  display: flex;
  align-items: center;
  padding: 0 4px 0 8px;
  border-right: 1px solid var(--line);
  flex: none;
}
.method-pick .method {
  appearance: none;
  height: 22px;
  padding: 0 22px 0 8px;
  border: 0;
  cursor: default;
}
.method-pick .method:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
.method-pick option { color: var(--ink); background: var(--bg-3); font-family: var(--font-mono); }
.method-pick .chev { position: absolute; right: 10px; color: var(--on-method); pointer-events: none; }

.url { flex: 1; min-width: 0; height: 100%; }
.url :deep(.mirror), .url :deep(.real) { font-size: 13px; }
.url :deep(.real), .url :deep(.mirror) { padding-left: var(--s-3); }

/* The one key that does the thing. */
.send {
  flex: none;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 0 16px 0 18px;
  background: var(--accent);
  color: var(--accent-ink);
  font-weight: 600;
  font-size: var(--t-body);
  transition: background var(--dur) var(--ease);
}
.send:hover:not(:disabled) { background: var(--accent-hi); }
.send:active:not(:disabled) { background: var(--accent-lo); }
.send:disabled { background: var(--bg-0); color: var(--faint); border-left: 1px solid var(--line); }
.send:focus-visible { outline-offset: -3px; }

/* While a request is in flight the key moves. */
.sending .send:disabled {
  background: repeating-linear-gradient(-45deg, var(--accent) 0 8px, var(--accent-lo) 8px 16px);
  background-size: 22.6px 22.6px;
  border-left: 0;
  color: var(--accent-ink);
  animation: stripes 700ms linear infinite;
}
@keyframes stripes { to { background-position: 22.6px 0; } }

.resolved {
  display: flex;
  align-items: center;
  gap: var(--s-2);
  min-height: 26px;
  padding: 4px var(--s-4) 0 calc(var(--s-4) + 8px);
  min-width: 0;
}
.resolved .arrow { color: var(--faint); }
.resolved .value {
  flex: 1;
  min-width: 0;
  color: var(--silk);
  font-size: var(--t-meta);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.resolved .chip { flex: none; }

.curl-notice {
  margin: var(--s-2) var(--s-4) 0;
  padding: 6px var(--s-2) 7px var(--s-3);
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
  background: var(--bg-2);
  flex: none;
}
.notice-head { display: flex; align-items: center; gap: var(--s-3); min-width: 0; }
.moved { display: flex; align-items: center; gap: 6px; min-width: 0; color: var(--ink-2); font-size: var(--t-small); }
.moved .ui-icon { color: var(--silk); flex: none; }
.moved span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.moved b { font-weight: 600; color: var(--ink); }
.notice-notes {
  margin: 6px 0 0;
  padding: 0 0 0 19px;
  max-height: 96px;
  overflow: auto;
  color: var(--silk);
  font-size: var(--t-small);
  line-height: 1.5;
}
.notice-notes li + li { margin-top: 2px; }

.request :deep(.ui-tabs) { margin-top: var(--s-2); }

.panel { flex: 1; min-height: 0; overflow: auto; display: flex; flex-direction: column; }
.section { flex: 1; display: flex; flex-direction: column; min-height: 0; }

.toolbar {
  display: flex;
  align-items: center;
  gap: var(--s-2);
  padding: var(--s-3) var(--s-4);
  flex: none;
}

.note {
  display: flex;
  align-items: center;
  gap: var(--s-2);
  margin: 0;
  padding: var(--s-1) var(--s-4) var(--s-4);
  color: var(--silk);
  font-size: var(--t-small);
  line-height: 1.5;
}
code { font-family: var(--font-mono); font-size: 0.95em; color: var(--ink-2); }

.section :deep(.ui-editor) { border-top: 1px solid var(--line-soft); }

.form {
  display: grid;
  grid-template-columns: 96px minmax(0, 420px) minmax(0, 1fr);
  gap: var(--s-2) var(--s-3);
  align-items: center;
  padding: var(--s-1) var(--s-4) var(--s-4);
}
.form .ui-seg { justify-self: start; }
.hint { color: var(--faint); font-size: var(--t-small); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }

.docs {
  flex: 1;
  margin: 0;
  padding: var(--s-4);
  border: 0;
  background: transparent;
  color: var(--ink);
  font-family: var(--font-ui);
  font-size: var(--t-body);
  line-height: 1.6;
  resize: none;
  max-width: 78ch;
}
.docs::placeholder { color: var(--faint); }

.options { gap: var(--s-3); padding: var(--s-4); overflow: auto; }
.lede {
  display: flex;
  margin: 0 0 var(--s-2);
  max-width: 72ch;
  color: var(--silk);
  font-size: var(--t-small);
  line-height: 1.55;
}
.opt {
  display: grid;
  grid-template-columns: 150px minmax(0, 1fr);
  gap: var(--s-3);
  align-items: center;
  padding: 7px 0;
  border-top: 1px solid var(--line-soft);
}
.opt:first-of-type { border-top: 0; }
.pick { display: flex; align-items: center; gap: var(--s-2); cursor: pointer; }
.pick .silk { color: var(--ink-2); }
.kind-label { color: var(--ink-2); }
.control { display: flex; align-items: center; gap: var(--s-2); min-width: 0; }
.control .field { max-width: 260px; }
.control .field:disabled { color: var(--faint); background: transparent; border-color: var(--line-soft); }
.switch-row { display: flex; align-items: center; gap: var(--s-2); font-size: var(--t-small); cursor: pointer; }
.switch-row input:disabled + span { color: var(--faint); }
.unit { flex: none; }
.warn-note { display: flex; align-items: center; gap: 5px; color: var(--warn); font-size: var(--t-meta); }

.captures { gap: var(--s-3); padding: var(--s-4); overflow: auto; }
.captures .table { display: grid; }
.captures .row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1.4fr) 44px 28px;
  gap: var(--s-2);
  align-items: center;
  min-height: 30px;
  border-bottom: 1px solid var(--line-soft);
}
.captures .row.head { min-height: 24px; }
.captures .row.head .silk:first-child, .captures .row.head .silk:nth-child(2) { padding-left: 9px; }
.captures .row.blank { border-bottom: 0; }
.captures .center { display: grid; place-items: center; }
.secret-toggle {
  display: grid;
  place-items: center;
  width: 28px;
  height: 22px;
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
  color: var(--silk);
  background: var(--well);
  transition: background var(--dur) var(--ease), color var(--dur) var(--ease);
}
.secret-toggle.on { background: var(--accent); border-color: var(--accent); color: var(--accent-ink); }
.secret-toggle:disabled { opacity: 0.3; }

.captures .row.check { grid-template-columns: minmax(0, 1.2fr) 130px minmax(0, 1fr) 28px; }
.captures .row.check.head .silk:nth-child(2) { padding-left: 0; }
.op :deep(select) { height: 26px; font-size: var(--t-meta); }
.nothing { font-size: var(--t-meta); padding-left: 9px; }
</style>
