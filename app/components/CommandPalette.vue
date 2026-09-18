<script setup lang="ts">
const store = useCollectionStore()
const emit = defineEmits<{ close: [] }>()

const query = ref('')
const active = ref(0)
const input = ref<HTMLInputElement>()
const list = ref<HTMLElement>()

// With nothing typed, the most useful list is what you last worked on.
const results = computed(() =>
  query.value.trim()
    ? store.searchRequests(query.value)
    : store.history
        .slice(0, 12)
        .filter((entry) => entry.requestId)
        .map((entry) => ({ id: entry.requestId!, name: entry.name, method: entry.method }))
        .filter((row, i, all) => all.findIndex((other) => other.id === row.id) === i),
)

watch(query, () => (active.value = 0))

function step(by: number) {
  const count = results.value.length
  if (count) active.value = (active.value + by + count) % count
}

async function open(id: string) {
  emit('close')
  await store.select(id)
}

function onKey(event: KeyboardEvent) {
  if (event.key === 'ArrowDown') {
    event.preventDefault()
    step(1)
  } else if (event.key === 'ArrowUp') {
    event.preventDefault()
    step(-1)
  } else if (event.key === 'Enter') {
    event.preventDefault()
    const chosen = results.value[active.value]
    if (chosen) open(chosen.id)
  }
}

watch(active, async () => {
  await nextTick()
  list.value?.querySelector('.hit.on')?.scrollIntoView({ block: 'nearest' })
})

onMounted(() => input.value?.focus())
</script>

<template>
  <UiDialog title="Open a request" eyebrow="Go to" :width="560" @close="emit('close')">
    <template #title>
      <input
        ref="input"
        v-model="query"
        class="query mono"
        placeholder="Type part of a name or a path"
        aria-label="Find a request"
        spellcheck="false"
        @keydown="onKey"
      >
    </template>

    <div v-if="results.length" ref="list" class="hits" role="listbox" aria-label="Requests">
      <button
        v-for="(row, i) in results"
        :key="row.id"
        type="button"
        class="hit"
        :class="{ on: i === active }"
        role="option"
        :aria-selected="i === active"
        @mousemove="active = i"
        @click="open(row.id)"
      >
        <span class="method" :data-method="row.method">{{ row.method }}</span>
        <span class="name">{{ row.name }}</span>
        <span class="id mono">{{ row.id }}</span>
      </button>
    </div>

    <p v-else class="none">
      {{ query.trim() ? 'No request matches that.' : 'Nothing sent yet. Type to search the collection.' }}
    </p>

    <template #footer>
      <span class="silk">
        <span class="kbd">↑</span><span class="kbd">↓</span> to move · <span class="kbd">↵</span> to open · <span class="kbd">Esc</span> to close
      </span>
    </template>
  </UiDialog>
</template>

<style scoped>
.query {
  width: 100%;
  margin-left: -7px;
  padding: 2px 6px;
  border: 1px solid transparent;
  border-radius: var(--r-sm);
  background: transparent;
  font-size: 15px;
}
.query:focus { background: var(--well); border-color: var(--accent); }

.hits { display: grid; gap: 1px; max-height: 46vh; overflow: auto; margin: 0 calc(var(--s-3) * -1); }
.hit {
  display: grid;
  grid-template-columns: 52px minmax(0, auto) minmax(0, 1fr);
  gap: var(--s-3);
  align-items: baseline;
  padding: 6px var(--s-3);
  border-radius: var(--r-sm);
  text-align: left;
}
.hit.on { background: var(--accent-tint); box-shadow: inset 2px 0 0 var(--accent); }
.hit .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.hit .id { color: var(--faint); font-size: var(--t-meta); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.none { margin: var(--s-5) 0; color: var(--silk); font-size: var(--t-small); text-align: center; }
</style>
