<script setup lang="ts">
const store = useCollectionStore()
const emit = defineEmits<{ close: [] }>()

const target = ref('')
const stopOnFailure = ref(false)

const folders = computed(() => [{ value: '', label: 'The whole collection' }, ...store.folders.map((folder) => ({ value: folder.id, label: folder.name }))])

async function run() {
  await store.runCollection(target.value || null, stopOnFailure.value)
}

function openStep(id: string) {
  emit('close')
  store.select(id)
}
</script>

<template>
  <UiDialog title="Run" eyebrow="This collection" :width="760" @close="emit('close')">
    <p class="note">
      <UiIcon name="info" :size="14" />
      <span>
        Every request in order, carrying captured values forward — a login's token
        reaches the request after it. Checks decide whether a step passed. The same
        run happens from a terminal with <code>volt-run</code>, so CI sees what you see.
      </span>
    </p>

    <div class="bar">
      <UiSelect v-model="target" :options="folders" label="What to run" class="what" />
      <label class="stop">
        <input v-model="stopOnFailure" type="checkbox">
        <span>Stop at the first failure</span>
      </label>
      <span class="spacer" />
      <button type="button" class="btn btn-primary btn-sm" :disabled="store.running" @click="run">
        {{ store.running ? 'Running' : 'Run' }}
      </button>
    </div>

    <template v-if="store.lastRun">
      <div class="summary">
        <span class="led" :class="store.lastRun.failed ? 'bad' : 'ok'" />
        <span class="num">{{ store.lastRun.passed }} passed</span>
        <span class="num">{{ store.lastRun.failed }} failed</span>
        <span class="silk num">{{ store.lastRun.durationMs }} ms</span>
      </div>

      <ul class="steps">
        <li v-for="step in store.lastRun.steps" :key="step.id" :class="{ bad: !step.ok }">
          <button type="button" class="step" @click="openStep(step.id)">
            <span class="led" :class="step.ok ? 'ok' : 'bad'" />
            <span class="method" :data-method="step.method">{{ step.method }}</span>
            <span class="name">{{ step.name }}</span>
            <span class="outcome silk">
              {{ step.error ?? (step.status ? `${step.status} · ${step.durationMs} ms` : 'no response') }}
            </span>
          </button>
          <ul v-if="step.checks.some((check) => !check.ok) || step.captured.length" class="detail">
            <li v-for="(check, i) in step.checks.filter((one) => !one.ok)" :key="i" class="failed">
              <span class="mono">{{ check.from }}</span>
              {{ check.note ?? `expected ${check.op} ${check.expected}, got ${check.actual ?? 'nothing'}` }}
            </li>
            <li v-if="step.captured.length" class="captured">captured {{ step.captured.join(', ') }}</li>
          </ul>
        </li>
      </ul>
    </template>
    <p v-else-if="!store.running" class="empty">Nothing run yet.</p>

    <template #footer>
      <span class="silk">Captured values stay in the run; nothing is written to an environment</span>
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

.bar { display: flex; align-items: center; gap: var(--s-3); padding-bottom: var(--s-3); border-bottom: 1px solid var(--line-soft); }
.what { flex: 1; max-width: 300px; }
.stop { display: flex; align-items: center; gap: var(--s-2); font-size: var(--t-small); cursor: pointer; }

.summary { display: flex; align-items: center; gap: var(--s-3); padding: var(--s-3) 0; font-size: var(--t-small); }

.steps { list-style: none; margin: 0; padding: 0; max-height: 44vh; overflow: auto; }
.steps > li { border-bottom: 1px solid var(--line-soft); }
.step {
  display: grid;
  grid-template-columns: 8px 52px minmax(0, 1fr) auto;
  gap: var(--s-3);
  align-items: center;
  width: 100%;
  padding: 6px 0;
  text-align: left;
  font-size: var(--t-small);
}
.step:hover { background: var(--hover); }
.name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.outcome { font-size: var(--t-meta); white-space: nowrap; }

.detail { list-style: none; margin: 0 0 6px; padding: 0 0 0 calc(8px + 52px + var(--s-3) * 2); font-size: var(--t-meta); }
.detail .failed { color: var(--bad); }
.detail .captured { color: var(--silk); }
.detail .mono { margin-right: 6px; }
.empty { margin: var(--s-6) 0; color: var(--silk); font-size: var(--t-small); text-align: center; }
</style>
