<script setup lang="ts">
import type { TabItem } from '~/utils/ui'
import { statusTone } from '~/utils/ui'
import { invoke } from '@tauri-apps/api/core'
import { save as saveDialog } from '@tauri-apps/plugin-dialog'
import { countMatches, prettyJson } from '~/utils/syntax'
import type { Comparison, Example } from '~/types'

const store = useCollectionStore()
const tab = ref<'body' | 'headers' | 'timeline' | 'examples'>('body')
type View = 'pretty' | 'tree' | 'raw' | 'preview' | 'diff'
const view = ref<View>('pretty')
const wrap = ref(false)
const copied = ref(false)

const res = computed(() => store.response)
const tone = computed(() => statusTone(res.value?.status))
const size = computed(() => humanSize(res.value?.sizeBytes ?? 0))

const pretty = computed(() => {
  const r = res.value
  if (!r || r.bodyIsBase64) return { text: '', isJson: false }
  return prettyJson(r.body)
})
const contentType = computed(
  () => res.value?.headers.find((h) => h.name.toLowerCase() === 'content-type')?.value.split(';')[0]?.trim() ?? '',
)

const shown = computed(() => (view.value === 'pretty' && pretty.value.isJson ? pretty.value.text : (res.value?.body ?? '')))
const language = computed(() => (view.value === 'pretty' && pretty.value.isJson ? 'json' : 'text'))

const isHtml = computed(() => contentType.value === 'text/html')

/** The ways a body can be looked at: what it is decides which are offered. */
const viewOptions = computed<{ value: View; label: string }[]>(() => {
  const out: { value: View; label: string }[] = [{ value: 'pretty', label: 'Pretty' }]
  if (pretty.value.isJson) out.push({ value: 'tree', label: 'Tree' })
  if (isHtml.value) out.push({ value: 'preview', label: 'Preview' })
  out.push({ value: 'raw', label: 'Raw' })
  return out
})
const segmentView = computed({
  get: () => (view.value === 'diff' ? 'pretty' : view.value),
  set: (next: View) => (view.value = next),
})

// A new body may not support the view the last one was in.
watch([res, () => store.previousResponse], () => {
  if (view.value === 'tree' && !pretty.value.isJson) view.value = 'pretty'
  if (view.value === 'preview' && !isHtml.value) view.value = 'pretty'
  if (view.value === 'diff' && !store.previousResponse) view.value = 'pretty'
})

/** This send's body as the diff sees it: pretty when it is JSON. */
const currentText = computed(() => (pretty.value.isJson ? pretty.value.text : (res.value?.body ?? '')))

/** The previous send's body, pretty-printed the same way, for the diff. */
const previousText = computed(() => {
  const r = store.previousResponse
  if (!r || r.bodyIsBase64) return ''
  return prettyJson(r.body).text
})

function onCapture(path: string, name: string) {
  store.addCapture(path, name)
}

async function copyPath(path: string) {
  try {
    await navigator.clipboard.writeText(path)
    store.notify('Path copied', path)
  } catch {
    store.error = 'Could not copy to the clipboard.'
  }
}

const redirected = computed(() => {
  const r = res.value
  return !!r && !!r.finalUrl && r.finalUrl !== r.sentUrl
})


const tabs = computed<TabItem[]>(() => [
  { key: 'body', label: 'Body', meta: res.value?.bodyIsBase64 ? 'binary' : pretty.value.isJson ? 'json' : contentType.value.split('/')[1] || null },
  { key: 'headers', label: 'Headers', meta: res.value?.headers.length ?? 0 },
  { key: 'timeline', label: 'Timeline' },
  { key: 'examples', label: 'Examples', meta: examples.value.length || null },
])

// --- Find in the body --------------------------------------------------------
const finding = ref(false)
useShortcut('mod+f', 'Find in the response', () => (store.response ? openFind() : undefined))
const query = ref('')
const current = ref(0)
const findInput = ref<HTMLInputElement>()

const matches = computed(() => countMatches(shown.value, query.value))
watch([query, shown], () => (current.value = 0))

async function openFind() {
  finding.value = true
  await nextTick()
  findInput.value?.focus()
  findInput.value?.select()
}

function closeFind() {
  finding.value = false
  query.value = ''
}

function step(by: number) {
  if (!matches.value) return
  current.value = (current.value + by + matches.value) % matches.value
}

// --- Saving ------------------------------------------------------------------
function suggestedName() {
  const type = contentType.value
  const extension =
    pretty.value.isJson || type.includes('json')
      ? 'json'
      : (type.split('/')[1]?.replace(/[^a-z0-9]+/gi, '') || (res.value?.bodyIsBase64 ? 'bin' : 'txt'))
  return `response.${extension}`
}

async function saveBody() {
  const body = res.value
  if (!body) return
  await store.attempt(async () => {
    const path = await saveDialog({ defaultPath: suggestedName(), title: 'Save response body' })
    if (!path) return
    // The raw body, not the pretty-printed view: what the server actually sent.
    await invoke('save_body', { path, body: body.body, base64Encoded: body.bodyIsBase64 })
    store.notify('Response saved', path)
  })
}

// --- Timeline ----------------------------------------------------------------
// Built from what the plan actually sent plus the response head. Not the wire
// bytes: reqwest will not hand those over, and inventing them would be worse
// than not showing them.
const timeline = computed(() => {
  const r = res.value
  if (!r) return ''
  const lines: string[] = []
  const version = r.version || 'HTTP/1.1'

  lines.push(`${r.sentMethod || '—'} ${r.sentUrl} ${version}`)
  for (const header of r.sentHeaders ?? []) lines.push(`${header.name}: ${header.value}`)
  if (r.sentBodyBytes) lines.push('', `[request body, ${r.sentBodyBytes} bytes]`)

  for (const hop of r.redirects ?? []) lines.push('', `→ redirected to ${hop}`)

  lines.push('', `${version} ${r.status} ${r.statusText}`)
  for (const header of r.headers) lines.push(`${header.name}: ${header.value}`)
  lines.push('', `[response body, ${r.sizeBytes} bytes]`)
  lines.push(
    '',
    `waiting ${r.timeToFirstByteMs} ms · transferring ${Math.max(0, r.durationMs - r.timeToFirstByteMs)} ms · total ${r.durationMs} ms`,
  )
  return lines.join('\n')
})

// --- Saved examples ----------------------------------------------------------
const examples = ref<Example[]>([])
const naming = ref(false)
const exampleName = ref('')

/**
 * A name for the example that says which one this is: the status, then the
 * first thing in the body that reads like a label — a name, a title, an id.
 */
function suggestedExampleName() {
  const r = res.value
  if (!r) return ''
  let hint = ''
  if (pretty.value.isJson) {
    try {
      let value: unknown = JSON.parse(r.body)
      if (Array.isArray(value)) value = value[0]
      if (value && typeof value === 'object') {
        const record = value as Record<string, unknown>
        const key = ['name', 'title', 'email', 'id', 'error', 'message'].find((k) => ['string', 'number'].includes(typeof record[k]))
        if (key) hint = ` · ${key} ${String(record[key]).slice(0, 24)}`
      }
    } catch {
      // Not the JSON it looked like; the status alone is a fine name.
    }
  }
  return `${r.status} ${r.statusText}${hint}`
}
const shownExample = ref<Example | null>(null)

/**
 * Comparing the reading on screen against a kept example: which is being
 * compared, and what came back.
 */
const comparing = ref<string | null>(null)
const comparison = ref<Comparison | null>(null)

async function compareWith(name: string) {
  if (!store.activeId) return
  comparing.value = name
  comparison.value = null
  comparison.value = await store.compareExample(store.activeId, name)
  if (!comparison.value) comparing.value = null
}

watch(
  () => store.activeId,
  async (id) => {
    shownExample.value = null
    comparing.value = null
    comparison.value = null
    examples.value = id ? await store.loadExamples(id) : []
  },
  { immediate: true },
)

async function keepExample() {
  const name = exampleName.value.trim()
  if (!name) return
  if (await store.saveExample(name)) {
    naming.value = false
    exampleName.value = ''
    examples.value = store.activeId ? await store.loadExamples(store.activeId) : []
  }
}

async function dropExample(name: string) {
  if (!store.activeId) return
  examples.value = await store.deleteExample(store.activeId, name)
  if (shownExample.value?.name === name) shownExample.value = null
  if (comparing.value === name) {
    comparing.value = null
    comparison.value = null
  }
}

let copiedTimer: ReturnType<typeof setTimeout> | undefined
async function copyBody() {
  try {
    await navigator.clipboard.writeText(shown.value)
    copied.value = true
    clearTimeout(copiedTimer)
    copiedTimer = setTimeout(() => (copied.value = false), 1400)
  } catch {
    store.error = 'Could not copy to the clipboard.'
  }
}
</script>

<template>
  <section class="response" aria-label="Response" aria-live="polite">
    <template v-if="res && !store.sending">
      <UiTabs v-model="tab" :items="tabs" label="Response sections">
        <template #end>
          <div class="reading">
            <span class="status" :class="tone">
              <span class="led" :class="tone" />
              <span class="num">{{ res.status }}</span><span class="text">{{ res.statusText }}</span>
            </span>
            <span class="measure mono num" :title="`First byte after ${res.timeToFirstByteMs} ms, ${Math.max(0, res.durationMs - res.timeToFirstByteMs)} ms transferring`">{{ res.durationMs }} ms</span>
            <span class="measure mono num">{{ size.value }} {{ size.unit }}</span>
            <span v-if="res.missingVars.length" class="chip warn" :title="`Sent without a value: ${res.missingVars.join(', ')}`">
              <UiIcon name="warning" :size="12" />{{ res.missingVars.length }} undefined
            </span>
          </div>
          <template v-if="tab === 'body' && res.body">
            <span class="tools-sep" />
            <template v-if="!res.bodyIsBase64">
              <UiSegmented
                v-if="viewOptions.length > 2"
                v-model="segmentView"
                :options="viewOptions"
                label="Body view"
                size="sm"
              />
              <button
                v-if="store.previousResponse"
                type="button"
                class="icon-btn quiet sm"
                :class="{ on: view === 'diff' }"
                :aria-pressed="view === 'diff'"
                aria-label="Compare with the previous send"
                title="Compare with the previous send"
                @click="view = view === 'diff' ? 'pretty' : 'diff'"
              >
                <UiIcon name="diff" :size="14" />
              </button>
              <button type="button" class="icon-btn quiet sm" :class="{ on: finding }" :aria-pressed="finding" aria-label="Find in body" :title="`Find in body (${modKey}+F)`" @click="finding ? closeFind() : openFind()">
                <UiIcon name="search" :size="14" />
              </button>
              <button type="button" class="icon-btn quiet sm" :class="{ on: wrap }" :aria-pressed="wrap" aria-label="Wrap long lines" title="Wrap long lines" @click="wrap = !wrap">
                <UiIcon name="wrap" :size="14" />
              </button>
              <button type="button" class="icon-btn quiet sm" :aria-label="copied ? 'Copied' : 'Copy body'" :title="copied ? 'Copied' : 'Copy body'" @click="copyBody">
                <UiIcon :name="copied ? 'check' : 'copy'" :size="14" />
              </button>
            </template>
            <button type="button" class="icon-btn quiet sm" aria-label="Save body to a file" title="Save body to a file" @click="saveBody">
              <UiIcon name="file-down" :size="14" />
            </button>
          </template>
        </template>
      </UiTabs>

      <div v-if="finding && tab === 'body'" class="find">
        <UiIcon name="search" :size="13" />
        <input
          ref="findInput"
          v-model="query"
          class="field inline mono"
          aria-label="Find in body"
          placeholder="Find"
          spellcheck="false"
          @keydown.enter.prevent="step($event.shiftKey ? -1 : 1)"
          @keydown.esc.prevent="closeFind()"
        >
        <span class="count num">{{ query ? (matches ? `${current + 1} of ${matches}` : 'No matches') : '' }}</span>
        <button type="button" class="icon-btn quiet sm" aria-label="Previous match" title="Previous (Shift+Enter)" :disabled="!matches" @click="step(-1)">
          <UiIcon name="chevron-up" :size="14" />
        </button>
        <button type="button" class="icon-btn quiet sm" aria-label="Next match" title="Next (Enter)" :disabled="!matches" @click="step(1)">
          <UiIcon name="chevron-down" :size="14" />
        </button>
        <button type="button" class="icon-btn quiet sm" aria-label="Close find" title="Close (Esc)" @click="closeFind()">
          <UiIcon name="x" :size="14" />
        </button>
      </div>

      <div v-if="redirected" class="redirect">
        <UiIcon name="arrow-right" :size="13" />
        <span class="silk">Redirected to</span>
        <span class="mono selectable">{{ res.finalUrl }}</span>
      </div>

      <div v-if="store.checkResults.length" class="checks" :class="{ failed: store.checkResults.some((check) => !check.ok) }">
        <span class="silk">Tests</span>
        <span v-for="(check, i) in store.checkResults" :key="i" class="check" :class="{ bad: !check.ok }">
          <span class="led" :class="check.ok ? 'ok' : 'bad'" />
          <span class="mono">{{ check.from }}</span>
          <span v-if="!check.ok" class="why">{{ check.note ?? `${check.op} ${check.expected}, got ${check.actual ?? 'nothing'}` }}</span>
        </span>
      </div>

      <div v-if="store.captureNotes.length" class="capture-notes">
        <UiIcon name="warning" :size="13" />
        <span>{{ store.captureNotes.join(' · ') }}</span>
      </div>

      <div class="viewer">
        <template v-if="tab === 'body'">
          <div v-if="res.bodyIsBase64" class="blank">
            <UiIcon name="file" :size="18" />
            <p><b>Binary response</b>, {{ size.value }} {{ size.unit }}<template v-if="contentType"> of <code>{{ contentType }}</code></template>. It is not shown as text.</p>
          </div>
          <div v-else-if="!res.body" class="blank">
            <p>The response has no body.</p>
          </div>
          <UiJsonTree v-else-if="view === 'tree'" :text="res.body" @capture="onCapture" @copy="copyPath" />
          <iframe v-else-if="view === 'preview'" class="preview" sandbox="" :srcdoc="res.body" title="The response, rendered" />
          <UiDiffView v-else-if="view === 'diff'" :before="previousText" :after="currentText" />
          <UiCodeView v-else :text="shown" :language="language" :wrap="wrap" :find="finding ? query : ''" :current="current" />
        </template>

        <table v-else-if="tab === 'headers'" class="headers mono selectable">
          <tbody>
            <tr v-for="(header, i) in res.headers" :key="i">
              <th scope="row">{{ header.name }}</th>
              <td>{{ header.value }}</td>
            </tr>
          </tbody>
        </table>

        <UiCodeView v-else-if="tab === 'timeline'" :text="timeline" language="text" wrap />

        <div v-else class="examples">
          <div v-if="shownExample" class="shown">
            <div class="shown-head">
              <button type="button" class="btn btn-quiet btn-sm" @click="shownExample = null">
                <UiIcon name="chevron-up" :size="14" />Back to the list
              </button>
              <span class="silk">{{ shownExample.name }}</span>
              <span class="num">{{ shownExample.status }} {{ shownExample.statusText }}</span>
            </div>
            <UiCodeView :text="shownExample.body" :language="shownExample.body.trimStart().startsWith('{') || shownExample.body.trimStart().startsWith('[') ? 'json' : 'text'" wrap />
          </div>

          <template v-else>
            <div v-if="naming" class="naming">
              <input
                v-model="exampleName"
                class="field"
                aria-label="Name for this example"
                placeholder="What this response shows — “404, unknown user”"
                @keydown.enter.prevent="keepExample()"
                @keydown.esc.prevent="naming = false"
              >
              <button type="button" class="btn btn-primary btn-sm" :disabled="!exampleName.trim()" @click="keepExample()">Keep</button>
              <button type="button" class="btn btn-quiet btn-sm" @click="naming = false">Cancel</button>
            </div>
            <button v-else-if="store.activeId" type="button" class="btn btn-sm keep" @click="naming = true; exampleName = suggestedExampleName()">
              <UiIcon name="plus" :size="14" />Keep this response as an example
            </button>

            <div v-if="comparison" class="compare">
              <div class="compare-head">
                <span class="silk">Against</span>
                <span class="against">{{ comparison.example }}</span>
                <button type="button" class="icon-btn quiet sm" aria-label="Close the comparison" title="Close" @click="comparing = null; comparison = null">
                  <UiIcon name="x" :size="14" />
                </button>
              </div>
              <p v-if="comparison.same" class="agrees">
                <span class="led ok" /><span>Same shape and same status as the example.</span>
              </p>
              <template v-else>
                <p v-if="comparison.status" class="drift">
                  <span class="led bad" /><span>Status {{ comparison.status[1] }} now, {{ comparison.status[0] }} when this was kept.</span>
                </p>
                <p v-if="comparison.contentType" class="drift">
                  <span class="led warn" /><span>Type {{ comparison.contentType[1] || 'not said' }} now, {{ comparison.contentType[0] || 'not said' }} then.</span>
                </p>
                <p v-for="path in comparison.removed" :key="`gone-${path}`" class="drift">
                  <span class="led bad" /><span><code class="mono">{{ path }}</code> is gone.</span>
                </p>
                <p v-for="path in comparison.added" :key="`new-${path}`" class="drift">
                  <span class="led ok" /><span><code class="mono">{{ path }}</code> is new.</span>
                </p>
                <p v-for="field in comparison.retyped" :key="`type-${field.path}`" class="drift">
                  <span class="led warn" /><span><code class="mono">{{ field.path }}</code> holds a {{ field.now }} now, not a {{ field.was }}.</span>
                </p>
              </template>
              <p v-if="comparison.note" class="hint">{{ comparison.note }}</p>
            </div>

            <ul v-if="examples.length" class="example-list">
              <li v-for="example in examples" :key="example.name">
                <button type="button" class="example" @click="shownExample = example">
                  <span class="led" :class="statusTone(example.status)" />
                  <span class="num">{{ example.status }}</span>
                  <span class="example-name">{{ example.name }}</span>
                  <span class="silk">{{ new Date(example.at).toLocaleDateString() }}</span>
                </button>
                <button
                  v-if="res"
                  type="button"
                  class="icon-btn quiet sm"
                  :class="{ on: comparing === example.name }"
                  :aria-label="`Compare this response with ${example.name}`"
                  title="Compare with this reading"
                  @click="compareWith(example.name)"
                >
                  <UiIcon name="diff" :size="14" />
                </button>
                <button type="button" class="icon-btn quiet sm" :aria-label="`Remove ${example.name}`" title="Remove" @click="dropExample(example.name)">
                  <UiIcon name="x" :size="14" />
                </button>
              </li>
            </ul>
            <p v-else class="blank-note">
              Nothing kept yet. An example is written beside the request, so it is reviewed and shared with it.
            </p>
          </template>
        </div>
      </div>
    </template>

    <template v-else>
      <div class="idle-bar" :class="{ sending: store.sending }">
        <template v-if="store.sending">
          <span class="led live" />
          <span class="silk">Sending</span>
          <span class="progress" aria-hidden="true"><i /></span>
        </template>
        <template v-else>
          <span class="led off" />
          <span class="silk">No response yet</span>
          <span class="idle-hint">Send the request <span class="kbd">{{ modKey }} ↵</span></span>
        </template>
      </div>
      <div class="viewer blank">
        <div class="nothing">
          <span class="glyph mono" aria-hidden="true">{ }</span>
          <p v-if="store.sending">Waiting for the response…</p>
          <p v-else>The body and headers of the response show up here.</p>
        </div>
      </div>
    </template>
  </section>
</template>

<style scoped>
.response {
  display: flex;
  flex-direction: column;
  min-height: 0;
  min-width: 0;
  background: var(--well);
  --code-bg: var(--well);
  container-type: inline-size;
}

/* The reading sits at the right of the tab row: what came back, how fast, how big. */
.reading { display: flex; align-items: center; gap: var(--s-3); flex: none; min-width: 0; }
/* The status is a stamp, like the method on the request: what came back, sealed. */
.status {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 20px;
  padding: 0 7px;
  border-radius: var(--r-xs);
  background: var(--m-other);
  color: var(--on-method);
  font: 600 11px / 1 var(--font-mono);
  white-space: nowrap;
}
.status .led { display: none; }
.status .text { margin-left: 4px; }
.status.ok { background: var(--ok); }
.status.warn { background: var(--warn); }
.status.bad { background: var(--bad); }
.measure { color: var(--silk); font-size: var(--t-meta); white-space: nowrap; }
.tools-sep { width: 1px; height: 16px; background: var(--line); margin: 0 2px; }

.idle-bar {
  display: flex;
  align-items: center;
  gap: var(--s-2);
  height: 36px;
  padding: 0 var(--s-4);
  border-bottom: 1px solid var(--line);
  flex: none;
}
.idle-hint { display: flex; align-items: center; gap: 6px; margin-left: var(--s-2); color: var(--faint); font-size: var(--t-small); }
.progress {
  position: relative;
  width: 120px;
  height: 3px;
  margin-left: var(--s-2);
  border-radius: 2px;
  background: var(--line);
  overflow: hidden;
}
.progress i {
  position: absolute;
  inset: 0;
  width: 40%;
  border-radius: 2px;
  background: var(--accent);
  animation: sweep 1s var(--ease) infinite alternate;
}
@keyframes sweep { from { transform: translateX(-100%); } to { transform: translateX(250%); } }

.redirect {
  display: flex;
  align-items: center;
  gap: var(--s-2);
  padding: 6px var(--s-5);
  border-bottom: 1px solid var(--line-soft);
  color: var(--silk);
  font-size: var(--t-meta);
  flex: none;
  min-width: 0;
}
.redirect .mono { color: var(--ink-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.viewer { flex: 1; min-height: 0; overflow: auto; }
/* Sandboxed with nothing allowed: no scripts, no forms, no navigation. */
.preview { display: block; width: 100%; height: 100%; border: 0; background: #fff; }

.blank { display: grid; place-items: center; padding: var(--s-8) var(--s-5); color: var(--silk); font-size: var(--t-small); }
.nothing { display: grid; gap: var(--s-3); justify-items: center; text-align: center; }
/* A quiet mark rather than an empty rectangle: the body goes here. */
.glyph {
  font-size: 32px;
  font-weight: 500;
  letter-spacing: -0.04em;
  line-height: 1;
  color: var(--line-strong);
}
.blank p { margin: 0; }
.blank b { color: var(--ink-2); font-weight: 600; }
.blank code { font-family: var(--font-mono); }

.headers { width: 100%; border-collapse: collapse; font-size: var(--t-small); }
.headers tr { border-bottom: 1px solid var(--line-soft); }
.headers th {
  width: 1%;
  padding: 7px var(--s-5) 7px var(--s-4);
  text-align: left;
  font-weight: 500;
  color: var(--silk);
  white-space: nowrap;
  vertical-align: top;
}
.headers td { padding: 7px var(--s-4) 7px 0; color: var(--ink); word-break: break-all; }

/*
 * Narrow panes: the timing bar goes first, then the gaps tighten. Kept last:
 * a container query adds no specificity, so an earlier block loses to the
 * base rules below it.
 */
@container (max-width: 760px) {
  .measure { display: none; }
  .response :deep(.ui-tabs) { gap: var(--s-3); }
  .response :deep(.ui-tab .meta) { display: none; }
}
@container (max-width: 680px) {
  .status .text { display: none; }
  .response :deep(.ui-seg) { display: none; }
}

.find {
  display: flex;
  align-items: center;
  gap: var(--s-2);
  padding: 5px var(--s-4);
  border-bottom: 1px solid var(--line-soft);
  background: var(--bg-1);
}
.find .field { flex: 1; max-width: 280px; }
.find .count { flex: none; min-width: 9ch; color: var(--silk); font-size: var(--t-meta); }

.examples { padding: var(--s-4); overflow: auto; display: grid; gap: var(--s-3); align-content: start; }
.keep { justify-self: start; }
.naming { display: flex; gap: var(--s-2); align-items: center; }
.naming .field { flex: 1; max-width: 420px; }

.example-list { list-style: none; margin: 0; padding: 0; display: grid; gap: 1px; }
.example-list li { display: flex; align-items: center; gap: var(--s-2); }
.example {
  flex: 1;
  display: grid;
  grid-template-columns: 8px 34px minmax(0, 1fr) auto;
  gap: var(--s-3);
  align-items: center;
  padding: 6px var(--s-2);
  border-radius: var(--r-sm);
  text-align: left;
}
.example:hover { background: var(--hover); }
.example-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--t-small); }
.compare {
  display: grid;
  gap: var(--s-2);
  padding: var(--s-3);
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
  background: var(--well);
}
.compare-head { display: flex; align-items: center; gap: var(--s-2); }
.compare-head .silk { flex: none; }
.against { flex: 1; font-size: var(--t-small); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.compare p { margin: 0; display: flex; align-items: center; gap: var(--s-2); font-size: var(--t-small); }
.compare code { font-size: var(--t-meta); color: var(--ink); }
.compare .hint { color: var(--silk); }
.blank-note { margin: 0; color: var(--silk); font-size: var(--t-small); max-width: 60ch; }

.shown { display: grid; gap: var(--s-2); min-height: 0; }
.shown-head { display: flex; align-items: center; gap: var(--s-3); }

.capture-notes {
  display: flex;
  align-items: center;
  gap: var(--s-2);
  padding: 6px var(--s-4);
  border-bottom: 1px solid var(--line-soft);
  color: var(--warn);
  font-size: var(--t-meta);
}

.checks {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--s-2) var(--s-3);
  padding: 6px var(--s-4);
  border-bottom: 1px solid var(--line-soft);
  font-size: var(--t-meta);
}
.check { display: inline-flex; align-items: center; gap: 5px; }
.check.bad { color: var(--bad); }
.why { color: var(--silk); }
.checks.failed .why { color: inherit; }
</style>
