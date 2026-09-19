<script setup lang="ts">
import type { MenuItem } from '~/utils/ui'

const emit = defineEmits<{ editEnvironment: []; openSettings: [] }>()
const store = useCollectionStore()

/** The last path segment names the folder; the rest is where it lives. */
function splitPath(path: string) {
  const parts = path.split(/[\\/]/).filter(Boolean)
  const name = parts.pop() ?? path
  return { name, parent: parts.join(path.includes('\\') ? '\\' : '/') }
}

/** Where to go: the collections opened lately, and the three ways to a new one. */
const switchItems = computed<MenuItem[]>(() => {
  const recent = store.recent
    .filter((path) => path !== store.root)
    .slice(0, 6)
    .map((path) => {
      const { name, parent } = splitPath(path)
      return { key: `open:${path}`, label: name, icon: 'folder' as const, hint: parent }
    })
  return [
    ...recent,
    { key: 'browse', label: 'Open a folder…', icon: 'folder-open', hint: 'One with a collection.yaml', divided: recent.length > 0 },
    { key: 'init', label: 'New collection…', icon: 'collection', hint: 'Creates the files in an empty folder' },
    { key: 'import', label: 'Import…', icon: 'import', hint: 'From Postman, Insomnia or OpenAPI' },
  ]
})

function onSwitch(key: string) {
  if (key.startsWith('open:')) store.open(key.slice(5))
  else if (key === 'browse') store.browseAndOpen()
  else if (key === 'init') store.browseAndInit()
  else if (key === 'import') store.browseAndImport()
}

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
    <button
      v-if="store.workspace"
      type="button"
      class="workspace"
      title="Workspaces"
      @click="store.workspacesDialog = true"
    >
      <UiIcon name="collection" :size="13" />{{ store.workspace }}
    </button>

    <UiMenuButton :items="switchItems" label="Switch collection" align="start" class="switcher" @select="onSwitch">
      <UiIcon name="folder" :size="14" class="folder" />
      <span class="name">{{ store.collection?.meta.name ?? 'No collection' }}</span>
    </UiMenuButton>
    <span v-if="store.root" class="path mono" :title="store.root">{{ store.root }}</span>

    <span class="spacer" />

    <button
      v-if="store.mock"
      type="button"
      class="serving chip accent"
      :title="`Serving examples on http://127.0.0.1:${store.mock.port}`"
      @click="store.mockDialog = true"
    >
      <span class="led live" />mock :{{ store.mock.port }}
    </button>

    <button
      v-if="!store.options.verifyTls"
      type="button"
      class="chip warn tls"
      title="TLS certificates are not verified. Click to change."
      @click="emit('openSettings')"
    >
      <UiIcon name="warning" :size="12" />TLS off
    </button>

    <div v-if="store.isOpen" class="env">
      <template v-if="store.environments.length">
        <UiSegmented v-if="envAsSegments" v-model="activeEnv" :options="envOptions" label="Environment" mono size="sm" />
        <UiSelect v-else v-model="activeEnv" :options="envOptions" label="Environment" mono class="env-select" />
        <button type="button" class="icon-btn quiet sm" aria-label="Edit environment" title="Edit environment variables" @click="emit('editEnvironment')">
          <UiIcon name="pencil" :size="14" />
        </button>
      </template>
      <button v-else type="button" class="btn btn-sm" @click="emit('editEnvironment')">
        <UiIcon name="globe" :size="14" />Add environment
      </button>
    </div>
  </header>
</template>

<style scoped>
.app-bar {
  display: flex;
  align-items: center;
  gap: var(--s-3);
  height: var(--h-bar);
  padding: 0 var(--s-3) 0 var(--s-2);
  background: var(--bg-0);
  border-bottom: 1px solid var(--line);
  flex: none;
  min-width: 0;
}

.switcher { flex: none; max-width: 300px; }
.switcher :deep(.name) { font-weight: 600; font-size: var(--t-body); color: var(--ink); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.switcher :deep(.folder) { color: var(--silk); }

.path {
  min-width: 0;
  margin-left: calc(var(--s-2) * -1);
  color: var(--faint);
  font-size: var(--t-meta);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  /* The end of a path is the part that tells folders apart. */
  direction: rtl;
  text-align: left;
  flex-shrink: 10;
}
@media (max-width: 1100px) { .path { display: none; } }

.workspace {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  flex: none;
  height: 24px;
  padding: 0 8px;
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
  color: var(--silk);
  font-size: var(--t-small);
  font-weight: 500;
}
.workspace:hover { color: var(--ink); border-color: var(--line-strong); }

.env { display: flex; align-items: center; gap: var(--s-1); flex: none; }
.env-select :deep(select) { height: 26px; width: 150px; font-size: var(--t-meta); }

.serving, .tls { flex: none; height: 22px; }
.serving:hover, .tls:hover { filter: brightness(0.96); }
</style>
