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
const known = computed(() => store.variableNames)

type BodyKind = Body['type']
type AuthKind = Auth['type']

const bodyKinds: SegmentOption<BodyKind>[] = [
  { value: 'none', label: t('None') },
  { value: 'json', label: 'JSON' },
  { value: 'text', label: t('Text') },
  { value: 'xml', label: 'XML' },
  { value: 'urlencoded', label: 'URL-encoded' },
  { value: 'form', label: 'Multipart' },
  { value: 'graphql', label: 'GraphQL' },
  { value: 'binary', label: t('File') },
]

const authKinds: SegmentOption<AuthKind>[] = [
  { value: 'inherit', label: t('Inherit from the collection') },
  { value: 'none', label: t('No auth') },
  { value: 'bearer', label: t('Bearer token') },
  { value: 'basic', label: 'Basic' },
  { value: 'apikey', label: t('API key') },
  { value: 'digest', label: 'Digest' },
  { value: 'ntlm', label: 'NTLM' },
  { value: 'awssigv4', label: t('AWS Signature') },
  { value: 'oauth1', label: 'OAuth 1.0' },
]

const AUTH_NAMES: Record<AuthKind, string> = {
  inherit: t('inherited auth'),
  none: t('no auth'),
  bearer: t('a bearer token'),
  basic: t('basic auth'),
  apikey: t('an API key'),
  digest: t('digest auth'),
  ntlm: 'NTLM',
  awssigv4: t('an AWS signature'),
  oauth1: t('an OAuth 1.0 signature'),
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
    ca_cert: store.options.caCert ?? '',
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
  { value: 'is', label: t('is') },
  { value: 'isnot', label: t('is not') },
  { value: 'contains', label: t('contains') },
  { value: 'matches', label: t('matches') },
  { value: 'exists', label: t('exists') },
  { value: 'missing', label: t('missing') },
  { value: 'under', label: t('under') },
  { value: 'over', label: t('over') },
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

/**
 * Only the sections this kind of request has. A socket has no body to write
 * before connecting and no response to test, so those tabs are not offered
 * rather than being there and doing nothing.
 */
const tabs = computed<TabItem[]>(() => {
  const kind = requestKind.value
  const all: (TabItem & { for?: RequestKind[] })[] = [
    { key: 'params', label: t('Params'), meta: request.value.params.length || null, for: ['http', 'websocket', 'sse'] },
    { key: 'headers', label: t('Headers'), meta: request.value.headers.length || null },
    { key: 'body', label: kind === 'grpc' ? t('Call') : t('Body'), meta: request.value.body.type === 'none' || kind === 'grpc' ? null : request.value.body.type, for: ['http', 'grpc'] },
    { key: 'auth', label: t('Auth'), meta: request.value.auth.type === 'inherit' ? null : request.value.auth.type },
    { key: 'checks', label: t('Tests'), meta: (request.value.checks?.length ?? 0) || null, for: ['http'] },
    { key: 'captures', label: t('Captures'), meta: (request.value.captures?.length ?? 0) || null, for: ['http'] },
    { key: 'options', label: t('Settings'), meta: overrides.value || null },
    { key: 'docs', label: t('Notes'), dot: !!request.value.docs?.trim() },
  ]
  return all.filter((one) => !one.for || one.for.includes(kind))
})
// The tab that was open may not exist for this kind of request.
watch(tabs, (now) => {
  if (!now.some((one) => one.key === tab.value)) tab.value = (now.find((one) => one.key === 'body') ?? now[0]!).key as typeof tab.value
}, { immediate: true })

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
    // The preview has no body to hand a helper; only a send does.
    if (tick === previewTick) resolved.value = { value: out.value, missing: out.missing.filter((name) => name !== '$body') }
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
    binary: { type: 'binary', path: '' },
    grpc: { type: 'grpc', proto: '', method: '', message: '' },
  }
  // Switching between the text kinds keeps what was typed.
  const current = request.value.body
  const carried = 'content' in current ? current.content : ''
  const body = next[kind]!
  if ('content' in body) body.content = carried
  // And between the two field kinds, keeps the fields. A file cannot go in a
  // URL-encoded body, so those rows stay behind.
  if ('fields' in body && 'fields' in current) {
    body.fields = current.fields
      .filter((field) => !('file' in field && field.file) || body.type === 'form')
      .map((field) => ({ ...field }))
  }
  request.value.body = body
  store.touch()
}

function setBinaryPath(path: string) {
  if (request.value.body.type !== 'binary') return
  request.value.body.path = path
  store.touch()
}

async function chooseBinary() {
  const path = await store.pickFile(t('Choose the file to send as the body'))
  if (path) setBinaryPath(path)
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
    oauth1: { type: 'oauth1', consumer_key: '', consumer_secret: '', token: '', token_secret: '', signature_method: 'HMAC-SHA1' },
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

/// The one action the request has, whatever kind it is — or, while one is
/// in flight, the way to stop it.
function onSend() {
  if (store.sending) {
    void store.cancelSend()
    return
  }
  if (request.value.url) store.go()
}

/**
 * The URL as people expect to see it: the address with its query on the end.
 * The query lives in the params rows; typing one into the URL puts it there,
 * and the file keeps a clean `url` with the params beside it.
 */
const composedUrl = computed(() => {
  // The pane can outlive its request by a tick when the last tab closes.
  const current = store.request
  if (!current) return ''
  const url = current.url
  const enabled = (current.params ?? []).filter((p) => p.enabled && (p.name || p.value))
  // What was typed stays as typed while it still says the same thing: `?a`
  // written back as `?a=` put the caret after an `=` the person had not
  // typed, and the `=` they typed next became `a==`.
  if (typed.value !== null) {
    const said = splitUrl(typed.value)
    if (said.url === url && said.params.length === enabled.length && said.params.every((p, i) => p.name === enabled[i]!.name && p.value === enabled[i]!.value)) {
      return typed.value
    }
  }
  const query = enabled.map((p) => `${p.name}=${p.value}`).join('&')
  return query ? `${url}${url.includes('?') ? '&' : '?'}${query}` : url
})

const urlField = ref<{ focus: () => void }>()
// Each time the store asks, once: the pane may be new (the first request of a
// session) or already there, and a later remount must not take the caret again.
function answerFocus() {
  if (store.focusUrl === store.focusUrlAnswered) return
  store.focusUrlAnswered = store.focusUrl
  nextTick(() => urlField.value?.focus())
}
watch(() => store.focusUrl, answerFocus)
onMounted(answerFocus)

/** The last text typed into the URL, to show back while it still matches. */
const typed = ref<string | null>(null)
watch(() => store.activeId, () => (typed.value = null))

function splitUrl(text: string) {
  const at = text.indexOf('?')
  if (at === -1) return { url: text, params: [] as { name: string; value: string }[] }
  const params = text
    .slice(at + 1)
    .split('&')
    .filter(Boolean)
    .map((pair) => {
      const eq = pair.indexOf('=')
      return eq === -1 ? { name: pair, value: '' } : { name: pair.slice(0, eq), value: pair.slice(eq + 1) }
    })
  return { url: text.slice(0, at), params }
}

function setUrl(text: string) {
  typed.value = text
  const disabled = request.value.params.filter((p) => !p.enabled)
  const { url, params } = splitUrl(text)
  request.value.url = url
  // A ws:// address is a socket whatever the request was: that is what the
  // person typing it means, and HTTP cannot send it anyway.
  if (/^wss?:\/\//i.test(text.trim()) && requestKind.value === 'http') requestKind.value = 'websocket'
  if (text.includes('?')) request.value.params = [...params.map((p) => ({ ...p, enabled: true })), ...disabled]
  else if (request.value.params.length !== disabled.length) request.value.params = disabled
  store.touch()
}

const sendLabel = computed(() => {
  if (store.sending) return t('Cancel')
  const kind = requestKind.value
  // A streaming gRPC method is a connection, and the key says so.
  if (kind === 'grpc') return store.grpcStreams ? (store.myStream ? t('Disconnect') : t('Connect')) : t('Call|verb')
  if (kind === 'websocket' || kind === 'sse') return store.myStream ? t('Disconnect') : t('Connect')
  if (store.isSubscription) return store.myStream ? t('Unsubscribe') : t('Subscribe')
  return t('Send')
})

/** A curl command pasted into the URL fills the whole request instead. */
function onUrlPaste(event: ClipboardEvent) {
  const text = event.clipboardData?.getData('text/plain') ?? ''
  if (!isCurl(text)) return
  event.preventDefault()
  store.importCurl(text, { mode: 'replace' })
}

const curlItems: MenuItem[] = [
  { key: 'copy', label: t('Copy as cURL'), icon: 'copy', hint: t('Secret values stay as {{name}}, safe to share') },
  { key: 'copy-secrets', label: t('Copy with secret values'), icon: 'unlock', hint: t('Ready to run; do not paste it anywhere public') },
  { key: 'fill', label: t('Fill from cURL…'), icon: 'import', hint: t('For a multi-line command'), divided: true },
  { key: 'code', label: t('Generate code…'), icon: 'braces', hint: 'fetch, Python, Go, C#, PHP, Ruby', divided: true },
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

/**
 * One picker for "what is this": an HTTP method, or one of the other kinds.
 * To the person choosing, GET and WebSocket are answers to the same question.
 */
const picked = computed(() => (requestKind.value === 'http' ? request.value.method : `kind:${requestKind.value}`))
/** A native select is as wide as its widest option; this one is as wide as what it shows. */
const pickedWidth = computed(() => {
  const shown = requestKind.value === 'http' ? request.value.method : (KINDS.find((kind) => kind.value === requestKind.value)?.label ?? '')
  return `calc(${shown.length}ch + 40px)`
})
function setMethod(event: Event) {
  const value = (event.target as HTMLSelectElement).value
  if (value.startsWith('kind:')) {
    requestKind.value = value.slice(5) as RequestKind
    return
  }
  if (requestKind.value !== 'http') requestKind.value = 'http'
  request.value.method = value
  store.touch()
}

useShortcut('mod+enter', t('Send'), () => onSend())
useShortcut('mod+d', t('Duplicate the request'), () => {
  if (store.activeId && !store.historyEntry) store.duplicate(store.activeId)
})
useShortcut('mod+s', t('Save the request'), () => {
  // A request reopened from history has no file yet; saving it makes one.
  if (store.historyEntry) store.saveHistoryAsNew()
  else if (store.dirty) store.save()
})
</script>

<template>
  <section class="request" :aria-label="t('Request')">
    <header class="head">
      <input
        class="title"
        :value="request.name"
        :aria-label="t('Request name')"
        spellcheck="false"
        :placeholder="t('Untitled request')"
        @input="setName"
      >
      <span class="file mono" :title="store.historyEntry ? t('Not saved to a file') : (store.activeId ?? '')">
        {{ store.historyEntry ? t('Not saved to a file') : store.activeId }}
      </span>
      <span class="spacer" />

      <UiMenuButton :items="curlItems" :label="t('Code')" align="end" @select="onCurlMenu">
        <UiIcon name="braces" :size="14" />{{ t('Code') }}
      </UiMenuButton>

      <template v-if="store.historyEntry">
        <button v-if="originalExists" type="button" class="btn btn-quiet btn-sm" :title="t('Open the request file this was sent from')" @click="store.select(store.historyEntry.requestId!)">
          <UiIcon name="arrow-up-right" :size="14" />{{ t('Open saved request') }}
        </button>
        <button type="button" class="btn btn-sm" :title="t('Write this request into the collection ({mod}+S)', { mod: modKey })" @click="store.saveHistoryAsNew()">
          {{ t('Save as new') }}
        </button>
      </template>
      <template v-else>
        <span v-if="store.dirty" class="state"><span class="led warn" /><span class="silk">{{ t('Unsaved') }}</span></span>
        <button type="button" class="btn btn-sm" :disabled="!store.dirty" :aria-label="t('Save request')" :title="t('Save ({mod}+S)', { mod: modKey })" @click="store.save()">
          {{ t('Save') }}
        </button>
      </template>
    </header>

    <div v-if="store.historyEntry" class="from-history">
      <UiIcon name="history" :size="14" />
      <span>
        {{ t('Reopened from history · sent {when}', { when: sentAt }) }}
        <template v-if="store.historyEntry.environment">{{ t('with') }} <b>{{ store.historyEntry.environment }}</b></template>
      </span>
    </div>

    <div class="probe" :class="{ sending: store.sending }">
      <label class="method-pick" :title="t('The method — or another kind of request: WebSocket, SSE, gRPC')">
        <select class="method tag" :data-method="requestKind === 'http' ? request.method : 'OTHER'" :style="{ width: pickedWidth }" :value="picked" :aria-label="t('Method')" @change="setMethod">
          <option v-for="m in METHODS" :key="m" :value="m">{{ m }}</option>
          <optgroup :label="t('Not HTTP')">
            <option v-for="kind in KINDS.filter((one) => one.value !== 'http')" :key="kind.value" :value="`kind:${kind.value}`">{{ kind.label }}</option>
          </optgroup>
        </select>
        <UiIcon name="chevron-down" :size="11" class="chev" />
      </label>

      <UiVarInput
        ref="urlField"
        class="url"
        variant="bare"
        :model-value="composedUrl"
        :known="known"
        label="URL"
        :placeholder="t('{{baseUrl}}/users, or paste a whole curl command')"
        @update:model-value="setUrl($event)"
        @enter="onSend"
        @paste="onUrlPaste"
      />

      <button type="button" class="send" :class="{ cancel: store.sending }" :disabled="!store.sending && !request.url" :title="store.sending ? t('Stop this send') : `${sendLabel} (${modKey}+Enter)`" @click="onSend">
        <UiIcon :name="store.sending ? 'x' : 'bolt'" :size="14" />
        <span>{{ sendLabel }}</span>
      </button>
    </div>

    <div v-if="resolved.value || resolved.missing.length" class="resolved">
      <template v-if="resolved.value">
        <UiIcon name="arrow-right" :size="12" class="arrow" />
        <span class="value mono selectable" :title="resolved.value">{{ resolved.value }}</span>
      </template>
      <span v-if="resolved.missing.length" class="chip warn undefined" :title="store.environment ? t('No value in {environment} — click a name to add it', { environment: store.environment.name }) : t('No value in any environment — click a name to add it')">
        <UiIcon name="warning" :size="12" />{{ t('Undefined:') }}
        <button
          v-for="name in resolved.missing"
          :key="name"
          type="button"
          class="add-var"
          :title="store.environment ? t('Add {name} to {environment}', { name, environment: store.environment.name }) : t('Add {name} to an environment', { name })"
          @click="store.openEnvironmentWith = name"
        >{{ name }}</button>
      </span>
    </div>

    <div v-if="store.curlNotice" class="curl-notice" role="status">
      <div class="notice-head">
        <span class="led ok" />
        <span class="silk">{{ t('Filled from curl') }}</span>
        <span v-if="store.curlNotice.secrets.length" class="moved">
          <UiIcon name="lock" :size="12" />
          <span>
            {{ store.curlNotice.secrets.length === 1 ? t('Credential moved to') : t('Credentials moved to') }}
            <code>{{ secretsAsVars(store.curlNotice.secrets) }}</code>
            {{ t('in') }} <b>{{ store.curlNotice.environment }}</b>
          </span>
        </span>
        <span class="spacer" />
        <button type="button" class="icon-btn quiet sm" :aria-label="t('Dismiss')" :title="t('Dismiss')" @click="store.curlNotice = null">
          <UiIcon name="x" :size="13" />
        </button>
      </div>
      <ul v-if="store.curlNotice.notes.length" class="notice-notes">
        <li v-for="note in store.curlNotice.notes" :key="note">{{ note }}</li>
      </ul>
    </div>

    <UiTabs v-model="tab" :items="tabs" :label="t('Request sections')" />

    <div class="panel">
      <KeyValueEditor
        v-if="tab === 'params'"
        v-model="request.params"
        :name-label="t('Parameter')"
        :add-label="t('Add parameter')"
        :known="known"
        @update:model-value="store.touch()"
      />

      <KeyValueEditor
        v-else-if="tab === 'headers'"
        v-model="request.headers"
        :name-label="t('Header')"
        :add-label="t('Add header')"
        :known="known"
        @update:model-value="store.touch()"
      />

      <div v-else-if="tab === 'body'" class="section">
        <div class="toolbar">
          <UiSegmented v-model="bodyKind" :options="bodyKinds" :label="t('Body type')" size="sm" />
          <span class="spacer" />
          <button v-if="request.body.type === 'json'" type="button" class="btn btn-quiet btn-sm" :disabled="!canFormat" :title="canFormat ? t('Pretty-print the JSON') : t('Not valid JSON yet')" @click="formatJson">
            <UiIcon name="braces" :size="14" />{{ t('Format') }}
          </button>
        </div>

        <p v-if="request.body.type === 'none'" class="note">{{ t('This request sends no body.') }}</p>
        <div v-else-if="request.body.type === 'binary'" class="file-body">
          <UiVarInput
            :model-value="request.body.path"
            :known="known"
            :label="t('File to send')"
            placeholder="uploads/avatar.png"
            @update:model-value="setBinaryPath($event)"
          />
          <button type="button" class="btn btn-sm" @click="chooseBinary">
            <UiIcon name="folder" :size="14" />{{ t('Choose…') }}
          </button>
          <p class="hint">
            {{ t("The file's bytes are the body, sent as they are. A path inside the collection folder is kept relative, so it works on a teammate's machine too.") }}
          </p>
        </div>
        <GrpcEditor v-else-if="request.body.type === 'grpc'" v-model="request.body" />

        <GraphQlEditor v-else-if="request.body.type === 'graphql'" v-model="request.body" />

        <KeyValueEditor
          v-else-if="'fields' in request.body"
          v-model="fieldBody"
          :name-label="t('Field')"
          :add-label="t('Add field')"
          :known="known"
          :files="request.body.type === 'form'"
        />
        <UiCodeEditor
          v-else
          v-model="textBody"
          :label="t('Request body')"
          :language="request.body.type"
          :known="known"
          :placeholder="request.body.type === 'json' ? '{\n  &quot;name&quot;: &quot;{{userName}}&quot;\n}' : ''"
        />
      </div>

      <div v-else-if="tab === 'auth'" class="section">
        <div class="toolbar">
          <!-- A list, not a row of keys: there are too many kinds to lay side by side. -->
          <UiSelect :model-value="authKind" :options="authKinds" :label="t('Auth type')" class="auth-pick" @update:model-value="authKind = $event as AuthKind" />
        </div>

        <!-- Text sits in one span: as direct flex children, the gap would split it around <code>. -->
        <p v-if="request.auth.type === 'inherit'" class="note">
          <UiIcon name="lock" :size="14" />
          <span>{{ t("Uses the collection's auth, set in") }} <code>collection.yaml</code>{{ t(': currently {what}.', { what: collectionAuth }) }}</span>
        </p>
        <p v-else-if="request.auth.type === 'none'" class="note">{{ t('Sends no credentials, even if the collection has some.') }}</p>

        <div v-else class="form">
          <template v-if="request.auth.type === 'bearer'">
            <span class="silk">{{ t('Token') }}</span>
            <UiVarInput v-model="request.auth.token" :known="known" :label="t('Token')" placeholder="{{token}}" @update:model-value="store.touch()" />
            <span class="hint">{{ t('Sent as') }} <code>Authorization: Bearer …</code></span>
          </template>

          <template v-else-if="request.auth.type === 'basic'">
            <span class="silk">{{ t('Username') }}</span>
            <UiVarInput v-model="request.auth.username" :known="known" :label="t('Username')" @update:model-value="store.touch()" />
            <span />
            <span class="silk">{{ t('Password') }}</span>
            <UiVarInput v-model="request.auth.password" :known="known" :label="t('Password')" placeholder="{{password}}" @update:model-value="store.touch()" />
            <span class="hint">{{ t('Keep it in a secret variable, not here.') }}</span>
          </template>

          <template v-else-if="request.auth.type === 'apikey'">
            <span class="silk">{{ t('Name') }}</span>
            <UiVarInput v-model="request.auth.key" :known="known" :label="t('Key name')" placeholder="X-API-Key" @update:model-value="store.touch()" />
            <span />
            <span class="silk">{{ t('Value') }}</span>
            <UiVarInput v-model="request.auth.value" :known="known" :label="t('Key value')" placeholder="{{apiKey}}" @update:model-value="store.touch()" />
            <span />
            <span class="silk">{{ t('Send in') }}</span>
            <UiSegmented
              v-model="apiKeyLocation"
              :options="[{ value: 'header', label: t('Header') }, { value: 'query', label: t('Query string') }]"
              :label="t('Send API key in')"
              size="sm"
            />
            <span />
          </template>

          <template v-else-if="request.auth.type === 'digest'">
            <span class="silk">{{ t('Username') }}</span>
            <UiVarInput v-model="request.auth.username" :known="known" :label="t('Username')" @update:model-value="store.touch()" />
            <span />
            <span class="silk">{{ t('Password') }}</span>
            <UiVarInput v-model="request.auth.password" :known="known" :label="t('Password')" placeholder="{{password}}" @update:model-value="store.touch()" />
            <span class="hint">{{ t('Answered after the server challenges, so the first send returns 401 and the second carries it.') }}</span>
          </template>

          <template v-else-if="request.auth.type === 'ntlm'">
            <span class="silk">{{ t('Username') }}</span>
            <UiVarInput v-model="request.auth.username" :known="known" :label="t('Username')" @update:model-value="store.touch()" />
            <span />
            <span class="silk">{{ t('Password') }}</span>
            <UiVarInput v-model="request.auth.password" :known="known" :label="t('Password')" placeholder="{{password}}" @update:model-value="store.touch()" />
            <span />
            <span class="silk">{{ t('Domain') }}</span>
            <UiVarInput v-model="request.auth.domain" :known="known" :label="t('Domain')" @update:model-value="store.touch()" />
            <span class="hint">{{ t('Three legs down one connection, so this request is sent over HTTP/1.1.') }}</span>
          </template>

          <template v-else-if="request.auth.type === 'awssigv4'">
            <span class="silk">{{ t('Key ID') }}</span>
            <UiVarInput v-model="request.auth.key_id" :known="known" :label="t('Access key ID')" placeholder="{{awsKeyId}}" @update:model-value="store.touch()" />
            <span />
            <span class="silk">{{ t('Secret') }}</span>
            <UiVarInput v-model="request.auth.secret" :known="known" :label="t('Secret access key')" placeholder="{{awsSecret}}" @update:model-value="store.touch()" />
            <span />
            <span class="silk">{{ t('Region') }}</span>
            <UiVarInput v-model="request.auth.region" :known="known" :label="t('Region')" placeholder="eu-west-1" @update:model-value="store.touch()" />
            <span />
            <span class="silk">{{ t('Service') }}</span>
            <UiVarInput v-model="request.auth.service" :known="known" :label="t('Service')" placeholder="execute-api" @update:model-value="store.touch()" />
            <span />
            <span class="silk">{{ t('Session') }}</span>
            <UiVarInput v-model="request.auth.session_token" :known="known" :label="t('Session token')" :placeholder="t('optional')" @update:model-value="store.touch()" />
            <span class="hint">{{ t('Signed over the finished request, so a copied cURL command works for about fifteen minutes.') }}</span>
          </template>

          <template v-else-if="request.auth.type === 'oauth1'">
            <span class="silk">{{ t('Consumer key') }}</span>
            <UiVarInput v-model="request.auth.consumer_key" :known="known" :label="t('Consumer key')" placeholder="{{consumerKey}}" @update:model-value="store.touch()" />
            <span />
            <span class="silk">{{ t('Consumer secret') }}</span>
            <UiVarInput v-model="request.auth.consumer_secret" :known="known" :label="t('Consumer secret')" placeholder="{{consumerSecret}}" @update:model-value="store.touch()" />
            <span />
            <span class="silk">{{ t('Token') }}</span>
            <UiVarInput v-model="request.auth.token" :known="known" :label="t('Access token')" :placeholder="t('optional')" @update:model-value="store.touch()" />
            <span />
            <span class="silk">{{ t('Token secret') }}</span>
            <UiVarInput v-model="request.auth.token_secret" :known="known" :label="t('Token secret')" :placeholder="t('optional')" @update:model-value="store.touch()" />
            <span />
            <span class="silk">{{ t('Signature') }}</span>
            <UiSegmented
              v-model="request.auth.signature_method"
              :options="[{ value: 'HMAC-SHA1', label: 'HMAC-SHA1' }, { value: 'HMAC-SHA256', label: 'HMAC-SHA256' }]"
              :label="t('Signature method')"
              size="sm"
              mono
              @update:model-value="store.touch()"
            />
            <span class="hint">{{ t('Signed over the finished request and sent as an') }} <code>Authorization: OAuth …</code> {{ t('header.') }}</span>
          </template>

        </div>
      </div>
      <div v-else-if="tab === 'captures'" class="section captures">
        <p class="lede">
          <span>
            {{ t('Keep a value from the response in the environment, so the next request can use it. Read') }}
            <code>status</code>, <code>header:Name</code>, <code>body</code>, {{ t('or a path like') }}
            <code>$.data.token</code>.
          </span>
        </p>

        <div class="table" role="table" :aria-label="t('Captures')">
          <div class="row head" role="row">
            <span role="columnheader" class="silk">{{ t('Variable') }}</span>
            <span role="columnheader" class="silk">{{ t('From the response') }}</span>
            <span role="columnheader" class="silk center">{{ t('Secret') }}</span>
            <span role="columnheader" />
          </div>

          <div v-for="(row, i) in captureRows" :key="i" class="row" role="row" :class="{ blank: !row.name && !row.from }">
            <input
              class="field inline mono"
              :value="row.name"
              :placeholder="row.name || row.from ? '' : 'token'"
              :aria-label="t('Variable to write')"
              spellcheck="false"
              @input="updateCapture(i, { name: ($event.target as HTMLInputElement).value })"
            >
            <input
              class="field inline mono"
              :value="row.from"
              :placeholder="row.name || row.from ? '' : '$.data.token'"
              :aria-label="t('Where to read it from')"
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
                :aria-label="row.secret ? t('Kept as a secret') : t('Kept in the environment YAML')"
                :title="row.secret ? t('Secret — stored in the gitignored .env file') : t('Plain — stored in the environment YAML')"
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
                :aria-label="row.name ? t('Remove {name}', { name: row.name }) : t('Remove capture')"
                :title="t('Remove')"
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
            {{ t('What has to be true of the response. Read') }} <code>status</code>, <code>time</code>,
            <code>header:Name</code>, <code>body</code> {{ t('or a path like') }} <code>$.data.id</code>,
            {{ t('and say what it should be.') }}
          </span>
        </p>

        <div class="table" role="table" :aria-label="t('Checks')">
          <div class="row check head" role="row">
            <span role="columnheader" class="silk">{{ t('From the response') }}</span>
            <span role="columnheader" class="silk">{{ t('Compare') }}</span>
            <span role="columnheader" class="silk">{{ t('Value') }}</span>
            <span role="columnheader" />
          </div>

          <div v-for="(row, i) in checkRows" :key="i" class="row check" role="row" :class="{ blank: !row.from }">
            <input
              class="field inline mono"
              :value="row.from"
              :placeholder="row.from ? '' : 'status'"
              :aria-label="t('What to read')"
              spellcheck="false"
              @input="updateCheck(i, { from: ($event.target as HTMLInputElement).value })"
            >
            <UiSelect
              :model-value="row.op"
              :options="CHECK_OPS.map((op) => ({ value: op.value, label: op.label }))"
              :label="t('How to compare')"
              class="op"
              @update:model-value="updateCheck(i, { op: $event as Check['op'] })"
            />
            <input
              v-if="TAKES_VALUE(row.op)"
              class="field inline mono"
              :value="row.value ?? ''"
              :placeholder="row.from ? '' : '200'"
              :aria-label="t('Expected value')"
              spellcheck="false"
              @input="updateCheck(i, { value: ($event.target as HTMLInputElement).value })"
            >
            <span v-else class="nothing silk">{{ t('nothing to compare') }}</span>
            <span class="center">
              <button
                v-if="row.from"
                type="button"
                class="icon-btn quiet sm"
                :aria-label="t('Remove the check on {from}', { from: row.from })"
                :title="t('Remove')"
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
            {{ t('How this one request goes out. Unticked, the send options follow Settings, so a collection stays portable between machines.') }}
          </span>
        </p>

        <div class="opt">
          <label class="pick">
            <input
              type="checkbox"
              :checked="overridden('timeout_ms')"
              :aria-label="t('Override the timeout')"
              @change="toggleOption('timeout_ms', ($event.target as HTMLInputElement).checked)"
            >
            <span class="silk">{{ t('Timeout') }}</span>
          </label>
          <div class="control">
            <input
              class="field mono num"
              type="number"
              min="1"
              step="500"
              :aria-label="t('Timeout in milliseconds')"
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
              :aria-label="t('Override redirects')"
              @change="toggleOption('follow_redirects', ($event.target as HTMLInputElement).checked)"
            >
            <span class="silk">{{ t('Redirects') }}</span>
          </label>
          <div class="control">
            <label class="switch-row">
              <input
                type="checkbox"
                class="switch"
                :aria-label="t('Follow redirects')"
                :disabled="!overridden('follow_redirects')"
                :checked="request.options?.follow_redirects ?? store.options.followRedirects"
                @change="setOption('follow_redirects', ($event.target as HTMLInputElement).checked)"
              >
              <span>{{ t('Follow up to 10') }}</span>
            </label>
          </div>
        </div>

        <div class="opt">
          <label class="pick">
            <input
              type="checkbox"
              :checked="overridden('verify_tls')"
              :aria-label="t('Override TLS verification')"
              @change="toggleOption('verify_tls', ($event.target as HTMLInputElement).checked)"
            >
            <span class="silk">TLS</span>
          </label>
          <div class="control">
            <label class="switch-row">
              <input
                type="checkbox"
                class="switch"
                :aria-label="t('Verify certificates')"
                :disabled="!overridden('verify_tls')"
                :checked="request.options?.verify_tls ?? store.options.verifyTls"
                @change="setOption('verify_tls', ($event.target as HTMLInputElement).checked)"
              >
              <span>{{ t('Verify certificates') }}</span>
            </label>
            <span v-if="request.options?.verify_tls === false" class="warn-note">
              <UiIcon name="warning" :size="13" />{{ t('Anyone on the network can read this request.') }}
            </span>
          </div>
        </div>

        <div class="opt">
          <label class="pick">
            <input
              type="checkbox"
              :checked="overridden('proxy')"
              :aria-label="t('Override the proxy')"
              @change="toggleOption('proxy', ($event.target as HTMLInputElement).checked)"
            >
            <span class="silk">{{ t('Proxy') }}</span>
          </label>
          <div class="control">
            <input
              class="field mono"
              :aria-label="t('Proxy URL')"
              spellcheck="false"
              :placeholder="store.options.proxy || 'http://127.0.0.1:8080'"
              :disabled="!overridden('proxy')"
              :value="request.options?.proxy ?? store.options.proxy ?? ''"
              @input="setOption('proxy', ($event.target as HTMLInputElement).value)"
            >
            <span v-if="overridden('proxy') && !request.options?.proxy" class="unit silk">{{ t('direct') }}</span>
          </div>
        </div>

        <div class="opt">
          <label class="pick">
            <input
              type="checkbox"
              :checked="overridden('client_cert')"
              :aria-label="t('Override the client certificate')"
              @change="toggleOption('client_cert', ($event.target as HTMLInputElement).checked)"
            >
            <span class="silk">{{ t('Client cert') }}</span>
          </label>
          <div class="control">
            <input
              class="field mono"
              :aria-label="t('Client certificate path')"
              spellcheck="false"
              :placeholder="store.options.clientCert || 'certs/client.pem'"
              :disabled="!overridden('client_cert')"
              :value="request.options?.client_cert ?? store.options.clientCert ?? ''"
              @input="setOption('client_cert', ($event.target as HTMLInputElement).value)"
            >
            <span class="unit silk">PEM</span>
          </div>
        </div>

        <div class="opt">
          <label class="pick">
            <input
              type="checkbox"
              :checked="overridden('ca_cert')"
              :aria-label="t('Override the trusted authorities')"
              @change="toggleOption('ca_cert', ($event.target as HTMLInputElement).checked)"
            >
            <span class="silk">{{ t('Trusted CA') }}</span>
          </label>
          <div class="control">
            <input
              class="field mono"
              :aria-label="t('Trusted authorities path')"
              spellcheck="false"
              :placeholder="store.options.caCert || 'certs/ca.pem'"
              :disabled="!overridden('ca_cert')"
              :value="request.options?.ca_cert ?? store.options.caCert ?? ''"
              @input="setOption('ca_cert', ($event.target as HTMLInputElement).value)"
            >
            <span class="unit silk">PEM</span>
          </div>
        </div>
      </div>


      <div v-else class="section">
        <textarea
          class="docs"
          :value="request.docs ?? ''"
          :aria-label="t('Notes')"
          :placeholder="t('What this request is for, what it needs first, anything surprising about the response…')"
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
  min-height: 46px;
  padding: var(--s-3) var(--s-3) var(--s-1) var(--s-4);
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
  font-weight: 650;
  font-size: 16px;
  letter-spacing: -0.02em;
  line-height: 1.3;
  text-overflow: ellipsis;
  transition: background var(--dur) var(--ease), border-color var(--dur) var(--ease);
}
.title:hover { background: var(--hover); }
.title:focus { background: var(--well); border-color: var(--focus); }
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
 * The URL bar: what, where, and go — one long pill, because to the person
 * typing it is one thing. The method is a mark at its head and the Send key
 * is the highlighter at its tail.
 */
.probe {
  display: flex;
  align-items: center;
  height: var(--h-probe);
  margin: var(--s-1) var(--s-3) 0;
  padding: 0 5px;
  border: 1.5px solid transparent;
  border-radius: var(--r-full);
  background: var(--well);
  flex: none;
  transition: border-color var(--dur) var(--ease), background var(--dur) var(--ease);
}
.probe:focus-within { border-color: var(--focus); background: var(--bg-2); }

.method-pick {
  position: relative;
  display: flex;
  align-items: center;
  flex: none;
}
.method-pick .method {
  appearance: none;
  padding: 0 24px 0 13px;
  border: 0;
  text-align: left;
  /* On the well a wash of the method's colour is too faint; the surface is not. */
  background: var(--bg-1);
  box-shadow: var(--shadow-panel);
  cursor: default;
}
.probe:focus-within .method-pick .method { background: var(--well); box-shadow: none; }
.method-pick .method:focus-visible { outline: 2px solid var(--focus); outline-offset: 2px; }
.method-pick option, .method-pick optgroup { color: var(--ink); background: var(--bg-3); font-family: var(--font-mono); }
.method-pick .chev { position: absolute; right: 10px; color: var(--silk); pointer-events: none; }

.url { flex: 1; min-width: 0; height: 100%; }
.url :deep(.mirror), .url :deep(.real) { font-size: 13px; }
.url :deep(.real), .url :deep(.mirror) { padding-left: var(--s-3); }

/* The one key that does the thing: the bolt, on the highlighter. */
.send {
  flex: none;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 32px;
  padding: 0 16px 0 12px;
  border-radius: var(--r-full);
  background: var(--accent);
  color: var(--accent-ink);
  font-weight: 650;
  font-size: var(--t-body);
  transition: background var(--dur) var(--ease), transform var(--dur) var(--ease);
}
.send :deep(.ui-icon) { stroke-width: 1.75; }
.send:hover:not(:disabled) { background: var(--accent-hi); }
.send:active:not(:disabled) { background: var(--accent-lo); transform: scale(0.97); }
.send:disabled { background: color-mix(in srgb, var(--ink) 7%, var(--well)); color: var(--faint); }
/* While a request is in flight the key is the way to stop it. */
.send.cancel { background: var(--ink); color: var(--bg-1); }
.send.cancel:hover { background: var(--ink-2); }
.send:focus-visible { outline-offset: 2px; border-radius: var(--r-full); }
.resolved .undefined { gap: 4px; }
.add-var {
  padding: 0 4px;
  border-radius: 3px;
  color: inherit;
  font: inherit;
  text-decoration: underline dotted;
  text-underline-offset: 2px;
}
.add-var:hover { background: color-mix(in srgb, var(--warn) 18%, transparent); }

.resolved {
  display: flex;
  align-items: center;
  gap: var(--s-2);
  min-height: 24px;
  padding: 4px var(--s-4) 0 calc(var(--s-4) + 10px);
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

.request :deep(.ui-tabs) { margin-top: var(--s-1); }

.panel { flex: 1; min-height: 0; overflow: auto; display: flex; flex-direction: column; }
.section { flex: 1; display: flex; flex-direction: column; min-height: 0; }

.toolbar {
  display: flex;
  align-items: center;
  gap: var(--s-2);
  padding: var(--s-2) var(--s-3) var(--s-3);
  flex: none;
  min-width: 0;
  overflow-x: auto;
  scrollbar-width: none;
}
.toolbar::-webkit-scrollbar { display: none; }
.auth-pick :deep(select) { width: 240px; }

.file-body {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: var(--s-2);
  align-items: center;
  padding: var(--s-1) var(--s-4) var(--s-4);
}
.file-body .hint { grid-column: 1 / -1; margin: 0; color: var(--silk); font-size: var(--t-small); line-height: 1.5; }

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

.section :deep(.ui-editor) { margin: 0 var(--s-3) var(--s-3); border-radius: var(--r-md); overflow: hidden; }

.form {
  display: grid;
  /* The label column grows with its longest label: another language needs more. */
  grid-template-columns: minmax(96px, max-content) minmax(0, 420px) minmax(0, 1fr);
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
  grid-template-columns: minmax(150px, max-content) minmax(0, 1fr);
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
  border: 1px solid transparent;
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
