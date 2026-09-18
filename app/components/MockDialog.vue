<script setup lang="ts">
const store = useCollectionStore()
const emit = defineEmits<{ close: [] }>()

// In the template the inner braces would close the interpolation.
const variableExample = '{{' + 'variable' + '}}'

const port = ref(3939)
const starting = ref(false)

onMounted(() => store.refreshMock())

async function start() {
  starting.value = true
  await store.startMock(port.value)
  starting.value = false
}

const base = computed(() => (store.mock ? `http://127.0.0.1:${store.mock.port}` : ''))
const copied = ref(false)

async function copyBase() {
  try {
    await navigator.clipboard.writeText(base.value)
    copied.value = true
    setTimeout(() => (copied.value = false), 1400)
  } catch {
    store.error = 'Could not copy to the clipboard.'
  }
}
</script>

<template>
  <UiDialog title="Mock server" eyebrow="This machine" :width="720" @close="emit('close')">
    <p class="note">
      <UiIcon name="info" :size="14" />
      <span>
        Answers with the responses you have kept as examples, on this machine only.
        It runs while volt runs; there is no hosted URL, because there is no volt
        service to host one.
      </span>
    </p>

    <div class="bar">
      <template v-if="store.mock">
        <span class="led ok" />
        <code class="base">{{ base }}</code>
        <button type="button" class="icon-btn quiet sm" :aria-label="copied ? 'Copied' : 'Copy the address'" :title="copied ? 'Copied' : 'Copy'" @click="copyBase">
          <UiIcon :name="copied ? 'check' : 'copy'" :size="14" />
        </button>
        <span class="spacer" />
        <button type="button" class="btn btn-sm" @click="store.stopMock()">Stop</button>
      </template>
      <template v-else>
        <label class="port">
          <span class="silk">Port</span>
          <input v-model.number="port" class="field mono num" type="number" min="0" max="65535" aria-label="Port">
        </label>
        <span class="hint">0 lets the system pick one</span>
        <span class="spacer" />
        <button type="button" class="btn btn-primary btn-sm" :disabled="starting" @click="start">
          {{ starting ? 'Starting' : 'Start serving' }}
        </button>
      </template>
    </div>

    <table v-if="store.mock?.routes.length" class="routes">
      <thead>
        <tr>
          <th scope="col" class="silk">Method</th>
          <th scope="col" class="silk">Path</th>
          <th scope="col" class="silk">Answers with</th>
          <th scope="col" class="silk">Status</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="(route, i) in store.mock.routes" :key="i">
          <td><span class="method" :data-method="route.method">{{ route.method }}</span></td>
          <td class="mono">
            {{ route.path }}
            <span v-for="condition in route.conditions" :key="condition" class="chip when">{{ condition }}</span>
          </td>
          <td>{{ route.example }}<span class="from">{{ route.name }}</span></td>
          <td class="num">{{ route.status }}</td>
        </tr>
      </tbody>
    </table>

    <p v-else-if="!store.mock" class="empty">
      Keep a response as an example first — that is what the mock serves.
    </p>

    <p v-if="store.mock" class="hint foot">
      A path segment that came from a <code>{{ variableExample }}</code> matches anything.
      A chip is a condition the call has to meet as well — a query parameter or a
      JSON field the request gives a value of its own.
      Add <code>?example=Name</code> to ask for a particular one.
    </p>

    <template #footer>
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

.bar {
  display: flex;
  align-items: center;
  gap: var(--s-3);
  padding: var(--s-2) 0 var(--s-3);
  border-bottom: 1px solid var(--line-soft);
}
.port { display: flex; align-items: center; gap: var(--s-2); }
.port .field { width: 90px; }
.base { font-size: var(--t-code); }
.hint { color: var(--faint); font-size: var(--t-meta); margin: 0; }
.foot { margin-top: var(--s-3); }

.routes { width: 100%; border-collapse: collapse; font-size: var(--t-small); margin-top: var(--s-3); }
.routes th { text-align: left; padding-bottom: 4px; border-bottom: 1px solid var(--line-soft); }
.routes td { padding: 5px 0; border-bottom: 1px solid var(--line-soft); }
.when { margin-left: var(--s-2); font-size: var(--t-meta); }
.from { display: block; color: var(--silk); font-size: var(--t-meta); }
.empty { margin: var(--s-6) 0; color: var(--silk); font-size: var(--t-small); text-align: center; }
</style>
