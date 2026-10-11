<script setup lang="ts">
const store = useCollectionStore()
const emit = defineEmits<{ close: [] }>()

// Opened from a folder's menu, the runner starts on that folder.
const target = ref(store.runnerTarget ?? '')
const stopOnFailure = ref(false)
/** A CSV or JSON file: one pass per row, its columns as variables. */
const data = ref('')

async function chooseData() {
  const { open } = await import('@tauri-apps/plugin-dialog')
  const picked = await open({
    title: t('Choose a data file: CSV with a header line, or a JSON array of objects'),
    multiple: false,
    directory: false,
    filters: [{ name: t('CSV or JSON'), extensions: ['csv', 'json'] }],
  })
  if (typeof picked === 'string') data.value = picked
}
const dataName = computed(() => data.value.split(/[\\/]/).pop() ?? '')
const rows = computed(() => new Set(store.lastRun?.steps.map((step) => step.iteration).filter(Boolean)).size)

const folders = computed(() => [{ value: '', label: t('The whole collection') }, ...store.folders.map((folder) => ({ value: folder.id, label: folder.name }))])

async function run() {
  await store.runCollection(target.value || null, stopOnFailure.value, data.value || null)
}

function openStep(id: string) {
  emit('close')
  store.select(id)
}
</script>

<template>
  <UiDialog :title="t('Run')" :eyebrow="t('This collection')" :width="760" @close="emit('close')">
    <p class="note">
      <UiIcon name="info" :size="14" />
      <span>
        {{ t("Every request in order, carrying captured values forward — a login's token reaches the request after it.") }}
        {{ t('Tests decide whether a step passed.') }}
        {{ t('The same run happens from a terminal with') }} <code>volt-run</code>, {{ t('so CI sees what you see.') }}
      </span>
    </p>

    <div class="bar">
      <UiSelect v-model="target" :options="folders" :label="t('What to run')" class="what" />
      <span class="data">
        <button type="button" class="btn btn-sm" :title="data || t('Run once per row of a CSV or JSON file')" @click="chooseData">
          <UiIcon name="file" :size="14" />{{ data ? dataName : t('Data file…') }}
        </button>
        <button v-if="data" type="button" class="icon-btn quiet sm" :aria-label="t('Run without the data file')" :title="t('Run without it')" @click="data = ''">
          <UiIcon name="x" :size="13" />
        </button>
      </span>
      <label class="stop">
        <input v-model="stopOnFailure" type="checkbox">
        <span>{{ t('Stop at the first failure') }}</span>
      </label>
      <span class="spacer" />
      <button type="button" class="btn btn-primary btn-sm" :disabled="store.running" @click="run">
        {{ store.running ? t('Running') : t('Run') }}
      </button>
    </div>

    <template v-if="store.lastRun">
      <div class="summary">
        <span class="led" :class="store.lastRun.failed ? 'bad' : 'ok'" />
        <span class="num">{{ t('{n} passed', { n: store.lastRun.passed }) }}</span>
        <span class="num">{{ t('{n} failed', { n: store.lastRun.failed }) }}</span>
        <span v-if="rows" class="silk num">{{ rows === 1 ? t('1 row') : t('{n} rows', { n: rows }) }}</span>
        <span class="silk num">{{ store.lastRun.durationMs }} ms</span>
      </div>

      <ul class="steps">
        <li v-for="(step, n) in store.lastRun.steps" :key="n" :class="{ bad: !step.ok }">
          <button type="button" class="step" @click="openStep(step.id)">
            <span class="led" :class="step.skipped ? 'off' : step.ok ? 'ok' : 'bad'" />
            <span class="method" :data-method="step.method">{{ step.method }}</span>
            <span class="name"><span v-if="step.iteration" class="row-no num" :title="t('Row {n} of the data file', { n: step.iteration })">#{{ step.iteration }}</span>{{ step.name }}</span>
            <span class="outcome silk">
              {{ step.skipped ?? step.error ?? (step.status ? `${step.status} · ${step.durationMs} ms` : t('no response')) }}
            </span>
          </button>
          <ul v-if="step.checks.some((check) => !check.ok) || step.captured.length" class="detail">
            <li v-for="(check, i) in step.checks.filter((one) => !one.ok)" :key="i" class="failed">
              <span class="mono">{{ check.from }}</span>
              {{ check.note ?? t('expected {op} {expected}, got {actual}', { op: check.op, expected: check.expected, actual: check.actual ?? t('nothing') }) }}
            </li>
            <li v-if="step.captured.length" class="captured">{{ t('captured {names}', { names: step.captured.join(', ') }) }}</li>
          </ul>
        </li>
      </ul>
    </template>
    <p v-else-if="!store.running" class="empty">{{ t('Nothing run yet.') }}</p>

    <template #footer>
      <span class="silk">{{ t('Captured values stay in the run; nothing is written to an environment') }}</span>
      <span class="spacer" />
      <button type="button" class="btn btn-primary" @click="emit('close')">{{ t('Done') }}</button>
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
.data { display: flex; align-items: center; gap: 2px; }
.data .btn { max-width: 200px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.row-no { margin-right: 8px; color: var(--faint); font-size: var(--t-meta); }
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
