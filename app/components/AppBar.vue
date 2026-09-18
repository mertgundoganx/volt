<script setup lang="ts">
defineProps<{ brandWidth: number }>()
const emit = defineEmits<{ editEnvironment: []; openSettings: [] }>()
const store = useCollectionStore()

/** Where the open request sits: collection, folders, request. */
const trail = computed(() => {
  if (!store.collection) return []
  const parts: { label: string; current?: boolean }[] = [{ label: store.collection.meta.name }]
  if (store.historyEntry) {
    parts.push({ label: 'History' })
  } else if (store.activeId) {
    for (const folder of store.folderTrail(store.activeId)) parts.push({ label: folder })
  }
  if (store.request) parts.push({ label: store.request.name || 'Untitled', current: true })
  return parts
})

const envOptions = computed(() => store.environments.map((e) => ({ value: e.name, label: e.name })))
/** A segmented control reads better, but only while it stays short. */
const envAsSegments = computed(
  () => envOptions.value.length <= 4 && envOptions.value.reduce((n, o) => n + o.label.length, 0) <= 28,
)
const activeEnv = computed({
  get: () => store.environment?.name ?? '',
  set: (name: string) => (store.activeEnvironment = name),
})
</script>

<template>
  <header class="app-bar">
    <div class="brand" :style="{ width: `${brandWidth}px` }">
      <span class="wordmark">volt</span>
    </div>

    <nav class="trail" aria-label="Location">
      <template v-for="(part, i) in trail" :key="i">
        <UiIcon v-if="i > 0" name="chevron-right" :size="12" class="sep" />
        <span :class="{ current: part.current }">{{ part.label }}</span>
      </template>
    </nav>

    <span class="spacer" />

    <button
      v-if="store.mock"
      type="button"
      class="serving"
      :title="`Serving examples on http://127.0.0.1:${store.mock.port}`"
      @click="store.mockDialog = true"
    >
      <span class="led ok" />MOCK :{{ store.mock.port }}
    </button>

    <div v-if="store.isOpen" class="env">
      <span class="silk">Env</span>
      <template v-if="store.environments.length">
        <UiSegmented v-if="envAsSegments" v-model="activeEnv" :options="envOptions" label="Environment" mono size="sm" />
        <UiSelect v-else v-model="activeEnv" :options="envOptions" label="Environment" mono class="env-select" />
        <button type="button" class="icon-btn quiet" aria-label="Edit environment" title="Edit environment variables" @click="emit('editEnvironment')">
          <UiIcon name="pencil" :size="14" />
        </button>
      </template>
      <button v-else type="button" class="btn btn-sm" @click="emit('editEnvironment')">
        <UiIcon name="plus" :size="14" />Add environment
      </button>
    </div>

    <button
      type="button"
      class="tls"
      :class="{ off: !store.options.verifyTls }"
      :title="store.options.verifyTls ? 'TLS certificates are verified' : 'TLS certificates are NOT verified. Click to change.'"
      @click="emit('openSettings')"
    >
      <span class="led" :class="store.options.verifyTls ? 'ok' : 'warn'" />
      <span class="silk">{{ store.options.verifyTls ? 'TLS verify' : 'TLS off' }}</span>
    </button>

    <button type="button" class="icon-btn" aria-label="Settings" title="Settings" @click="emit('openSettings')">
      <UiIcon name="sliders" />
    </button>
  </header>
</template>

<style scoped>
.app-bar {
  display: flex;
  align-items: center;
  gap: var(--s-3);
  height: var(--h-bar);
  padding-right: var(--s-3);
  /* A faint wash of current behind the name. */
  background:
    radial-gradient(90% 260% at 4% 0%, color-mix(in srgb, var(--accent) 13%, transparent), transparent 62%),
    var(--bg-0);
  border-bottom: 1px solid var(--line);
  flex: none;
}

.brand { flex: none; padding-left: var(--s-5); }
.wordmark {
  font-stretch: 125%;
  font-weight: 800;
  font-size: 17px;
  letter-spacing: -0.03em;
  line-height: 1;
  /* The current runs through the name itself. */
  background: linear-gradient(180deg, var(--ink), color-mix(in srgb, var(--accent) 55%, var(--ink)));
  -webkit-background-clip: text;
  background-clip: text;
  color: transparent;
}

.trail {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  padding-left: var(--s-2);
  color: var(--silk);
  font-size: var(--t-small);
  white-space: nowrap;
  overflow: hidden;
}
/* When space runs out, the folders give way before the request's own name. */
.trail span { overflow: hidden; text-overflow: ellipsis; flex-shrink: 4; min-width: 1.5em; }
.trail .current { color: var(--ink); font-weight: 650; flex-shrink: 1; min-width: 6em; }
.sep { color: var(--faint); }

.env { display: flex; align-items: center; gap: var(--s-2); flex: none; }
.env-select :deep(select) { height: 28px; width: 156px; font-size: var(--t-meta); }

.tls {
  display: flex;
  align-items: center;
  gap: 7px;
  height: 28px;
  padding: 0 var(--s-3);
  border-radius: var(--r-full);
  flex: none;
  transition: background var(--dur) var(--ease);
}
.tls:hover { background: var(--hover); }
.tls.off .silk { color: var(--warn); }

.serving {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  flex: none;
  height: 24px;
  padding: 0 var(--s-2);
  border: 1px solid var(--accent-line);
  border-radius: var(--r-full);
  background: var(--accent-tint);
  color: var(--accent-text);
  font-size: var(--t-meta);
  font-family: var(--font-mono);
  transition: background var(--dur) var(--ease);
}
.serving:hover { background: color-mix(in srgb, var(--accent) 26%, transparent); }
</style>
