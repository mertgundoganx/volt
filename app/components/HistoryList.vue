<script setup lang="ts">
import type { HistorySummary } from '~/types'
import { statusTone } from '~/utils/ui'

const store = useCollectionStore()

// Re-render relative times without re-fetching anything.
const now = ref(Date.now())
let timer: ReturnType<typeof setInterval> | undefined
onMounted(() => {
  store.refreshHistory()
  timer = setInterval(() => (now.value = Date.now()), 30_000)
})
onUnmounted(() => clearInterval(timer))

function ago(at: number): string {
  const seconds = Math.max(0, Math.round((now.value - at) / 1000))
  if (seconds < 45) return 'now'
  const minutes = Math.round(seconds / 60)
  if (minutes < 60) return `${minutes}m`
  const hours = Math.round(minutes / 60)
  if (hours < 24) return `${hours}h`
  return new Date(at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
}

/**
 * Narrow the list by what was sent: words match the name, method and URL, and
 * `4xx`, `404` or `error` match how it came back.
 */
const filter = ref('')
const shown = computed(() => {
  const words = filter.value.toLowerCase().split(/\s+/).filter(Boolean)
  if (!words.length) return store.history
  return store.history.filter((entry) => {
    const text = `${entry.method} ${entry.name} ${entry.url}`.toLowerCase()
    return words.every((word) => {
      if (word === 'error' || word === 'err') return !!entry.error
      const band = /^([1-5])xx$/.exec(word)
      if (band) return entry.status !== null && Math.floor(entry.status / 100) === Number(band[1])
      if (/^\d{3}$/.test(word)) return entry.status === Number(word) || text.includes(word)
      return text.includes(word)
    })
  })
})

/** Group by calendar day so a long list is scannable. */
const groups = computed(() => {
  const out: { label: string; entries: HistorySummary[] }[] = []
  const today = new Date(now.value).toDateString()
  const yesterday = new Date(now.value - 86_400_000).toDateString()

  for (const entry of shown.value) {
    const day = new Date(entry.at).toDateString()
    const label =
      day === today
        ? 'Today'
        : day === yesterday
          ? 'Yesterday'
          : new Date(entry.at).toLocaleDateString([], { weekday: 'short', day: 'numeric', month: 'short' })
    const last = out[out.length - 1]
    if (last?.label === label) last.entries.push(entry)
    else out.push({ label, entries: [entry] })
  }
  return out
})
</script>

<template>
  <div class="history">
    <div v-if="store.history.length" class="filter">
      <UiIcon name="search" :size="13" />
      <input
        v-model="filter"
        class="field inline"
        aria-label="Filter history"
        placeholder="Filter — a URL, a name, 404, 5xx, error"
        spellcheck="false"
        @keydown.esc.prevent="filter = ''"
      >
    </div>
    <div class="head">
      <span class="silk">{{ filter.trim() ? `${shown.length} of ${store.history.length}` : `${store.history.length} sent` }}</span>
      <span class="spacer" />
      <button type="button" class="btn btn-quiet btn-sm" :disabled="!store.history.length" @click="store.clearHistory()">
        Clear
      </button>
    </div>

    <div v-if="!store.history.length" class="empty">
      <UiIcon name="history" :size="18" />
      <p>Requests you send land here with the response they got. History stays on this machine and never goes into the collection.</p>
    </div>

    <p v-if="store.history.length && !shown.length" class="none">Nothing sent matches</p>

    <section v-for="group in groups" :key="group.label" class="group">
      <div class="day silk">{{ group.label }}</div>
      <button
        v-for="entry in group.entries"
        :key="entry.id"
        type="button"
        class="entry"
        :class="{ active: store.historyEntry?.id === entry.id }"
        :title="`${entry.method} ${entry.url}`"
        @click="store.openHistory(entry.id)"
      >
        <span class="method" :data-method="entry.method">{{ entry.method === 'DELETE' ? 'DEL' : entry.method }}</span>
        <span class="text">
          <span class="name">{{ entry.name }}</span>
          <span class="url mono">{{ entry.url }}</span>
        </span>
        <span class="meta">
          <span class="status mono num" :class="statusTone(entry.status, !!entry.error)">{{ entry.error ? 'ERR' : entry.status }}</span>
          <span class="when mono">{{ ago(entry.at) }}</span>
        </span>
      </button>
    </section>
  </div>
</template>

<style scoped>
.filter {
  display: flex;
  align-items: center;
  gap: 4px;
  height: 28px;
  margin: var(--s-2) var(--s-2) var(--s-1);
  padding-left: 6px;
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
  color: var(--silk);
}
.filter:focus-within { border-color: var(--accent); background: var(--well); }
.filter .field { flex: 1; min-width: 0; height: 26px; font-size: var(--t-small); }
.filter .field:focus, .filter .field:hover { background: transparent; border-color: transparent; box-shadow: none; }
.none { margin: 0; padding: var(--s-5) var(--s-4); color: var(--silk); font-size: var(--t-small); text-align: center; }

.head { display: flex; align-items: center; padding: 0 var(--s-2) var(--s-1) var(--s-4); }

.empty { display: grid; gap: var(--s-2); justify-items: start; padding: var(--s-3) var(--s-4); color: var(--silk); }
.empty p { margin: 0; font-size: var(--t-small); line-height: 1.5; }

.group + .group { margin-top: var(--s-2); }
.day { padding: var(--s-3) var(--s-4) 6px; }

.entry {
  display: flex;
  align-items: center;
  gap: 8px;
  width: calc(100% - 2 * var(--s-2));
  height: 42px;
  margin: 0 var(--s-2);
  padding: 0 8px 0 12px;
  border-radius: var(--r-sm);
  text-align: left;
  color: var(--ink-2);
}
.entry:hover { background: var(--hover); color: var(--ink); }
.entry.active { background: var(--accent-tint); color: var(--ink); box-shadow: inset 2px 0 0 var(--accent); }

.entry .method { width: 32px; flex: none; }

.text { flex: 1; min-width: 0; display: grid; gap: 3px; }
.name { font-weight: 550; color: var(--ink); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.url { font-size: 10.5px; color: var(--silk); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

.meta { display: grid; gap: 4px; justify-items: end; flex: none; }
.status { font-size: 11.5px; font-weight: 650; color: var(--silk); }
.status.ok { color: var(--ok); }
.status.warn { color: var(--warn); }
.status.bad { color: var(--bad); }
.when { font-size: 10.5px; color: var(--faint); }
</style>
