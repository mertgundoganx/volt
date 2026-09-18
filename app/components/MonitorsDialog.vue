<script setup lang="ts">
const store = useCollectionStore()
const emit = defineEmits<{ close: [] }>()

const requestId = ref(store.activeId ?? '')
const every = ref(60)

const choices = computed(() => {
  const out: { id: string; name: string }[] = []
  const walk = (nodes: typeof store.tree) => {
    for (const node of nodes) {
      if (node.kind === 'folder') walk(node.children)
      else out.push({ id: node.id, name: node.name })
    }
  }
  walk(store.tree)
  return out
})

const mine = computed(() => store.monitors.filter((m) => m.root === store.root))
const runsOf = (id: string) => store.monitorRuns.filter((run) => run.monitorId === id)

async function add() {
  const chosen = choices.value.find((c) => c.id === requestId.value)
  if (!chosen) return
  await store.addMonitor(chosen.id, chosen.name, every.value)
}

function when(at: number) {
  return new Date(at).toLocaleTimeString()
}
</script>

<template>
  <UiDialog title="Monitors" eyebrow="While volt is open" :width="720" @close="emit('close')">
    <p class="note">
      <UiIcon name="info" :size="14" />
      <span>
        Send a request on a schedule and hear about it when it fails. It runs while
        volt is open and nowhere else — there is no server here to keep watching
        after you close the app, and saying otherwise would be worse than not
        offering it. Every run is recorded in History like any other send.
      </span>
    </p>

    <div class="add">
      <UiSelect
        v-model="requestId"
        :options="choices.map((c) => ({ value: c.id, label: c.name }))"
        label="Request to watch"
        class="pick"
      />
      <label class="every">
        <span class="silk">Every</span>
        <input v-model.number="every" class="field mono num" type="number" min="10" step="10" aria-label="Seconds between runs">
        <span class="silk">sec</span>
      </label>
      <span class="spacer" />
      <button type="button" class="btn btn-primary btn-sm" :disabled="!requestId" @click="add">Watch it</button>
    </div>

    <ul v-if="mine.length" class="monitors">
      <li v-for="monitor in mine" :key="monitor.id">
        <span class="led" :class="runsOf(monitor.id)[0] ? (runsOf(monitor.id)[0]!.error || (runsOf(monitor.id)[0]!.status ?? 0) >= 400 ? 'bad' : 'ok') : 'off'" />
        <span class="name">{{ monitor.name }}</span>
        <span class="silk">every {{ monitor.everySeconds }}s</span>
        <span class="last">
          <template v-if="runsOf(monitor.id)[0]">
            {{ runsOf(monitor.id)[0]!.error ?? `${runsOf(monitor.id)[0]!.status} · ${runsOf(monitor.id)[0]!.durationMs} ms` }}
            <span class="silk">at {{ when(runsOf(monitor.id)[0]!.at) }}</span>
          </template>
          <span v-else class="quiet">not run yet</span>
        </span>
        <button
          type="button"
          class="btn btn-quiet btn-sm"
          @click="store.setMonitorPaused(monitor.id, !monitor.paused)"
        >
          {{ monitor.paused ? 'Resume' : 'Pause' }}
        </button>
        <button type="button" class="icon-btn quiet sm" :aria-label="`Remove ${monitor.name}`" title="Remove" @click="store.removeMonitor(monitor.id)">
          <UiIcon name="x" :size="14" />
        </button>
      </li>
    </ul>
    <p v-else class="empty">Nothing is being watched in this collection.</p>

    <template #footer>
      <span class="silk">{{ mine.length }} in this collection</span>
      <span class="spacer" />
      <button type="button" class="btn btn-primary" @click="emit('close')">Done</button>
    </template>
  </UiDialog>
</template>

<style scoped>
.note {
  display: flex;
  gap: var(--s-2);
  margin: 0 0 var(--s-4);
  color: var(--silk);
  font-size: var(--t-small);
  line-height: 1.55;
}
.note .ui-icon { margin-top: 2px; flex: none; }

.add {
  display: flex;
  align-items: center;
  gap: var(--s-3);
  padding-bottom: var(--s-3);
  border-bottom: 1px solid var(--line-soft);
}
.pick { flex: 1; min-width: 0; max-width: 320px; }
.every { display: flex; align-items: center; gap: var(--s-2); }
.every .field { width: 80px; }

.monitors { list-style: none; margin: var(--s-3) 0 0; padding: 0; display: grid; gap: 1px; }
.monitors li {
  display: grid;
  grid-template-columns: 8px minmax(0, 1fr) auto minmax(0, 1.2fr) auto auto;
  gap: var(--s-3);
  align-items: center;
  padding: 6px 0;
  border-bottom: 1px solid var(--line-soft);
  font-size: var(--t-small);
}
.name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.last { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--ink-2); font-size: var(--t-meta); }
.quiet { color: var(--silk); }
.empty { margin: var(--s-6) 0; color: var(--silk); font-size: var(--t-small); text-align: center; }
</style>
