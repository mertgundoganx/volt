<script setup lang="ts">
import type { MenuItem } from '~/utils/ui'

/**
 * The strip across the top of the bench: the mark on the left, and on the
 * right the things that are about the whole window rather than one request —
 * which environment is live, the tools, how the panes lie, settings.
 */
const emit = defineEmits<{ editEnvironment: []; openSettings: [] }>()
const layout = defineModel<'rows' | 'cols'>('layout', { required: true })
/** False in a window too narrow to put the panes side by side. */
defineProps<{ canSplit: boolean }>()
const store = useCollectionStore()

/** The environments, the live one ticked, and the way in to edit them. */
const envItems = computed<MenuItem[]>(() => [
  ...store.environments.map((env) => ({
    key: `use:${env.name}`,
    label: env.name,
    icon: env.name === store.environment?.name ? ('check' as const) : ('globe' as const),
    hint: env.vars.length === 1 ? t('1 variable') : t('{n} variables', { n: env.vars.length }),
  })),
  { key: 'edit', label: t('Manage environments…'), icon: 'pencil', hint: t('Names, values and secrets ({key}+E)', { key: modKey }), divided: store.environments.length > 0 },
])

function onEnv(key: string) {
  if (key === 'edit') emit('editEnvironment')
  else store.activeEnvironment = key.slice(4)
}

/** Everything that works on the whole collection, in one place. */
const toolItems = computed<MenuItem[]>(() => {
  const open = store.isOpen
  return [
    { key: 'run', label: t('Run…'), icon: 'play', hint: t('Every request in order, with its tests'), disabled: !open },
    { key: 'mock', label: t('Mock server…'), icon: 'server', hint: t('Serve the saved examples locally'), disabled: !open },
    { key: 'monitors', label: t('Monitors…'), icon: 'clock', hint: t('Watch a request on a schedule'), disabled: !open },
    { key: 'oauth', label: t('Get an OAuth token…'), icon: 'key', hint: t('Into the environment, as a secret'), disabled: !open, divided: true },
    { key: 'cookies', label: t('Cookies…'), icon: 'cookie', hint: t('What this collection is holding'), disabled: !open },
    { key: 'sync', label: t('Sync…'), icon: 'branch', hint: t('Pull, commit and push the folder'), disabled: !open, divided: true },
    { key: 'docs', label: t('Write documentation…'), icon: 'book', hint: t('One HTML file, no publishing'), disabled: !open },
    { key: 'postman', label: t('Export for Postman'), icon: 'export', disabled: !open },
    { key: 'openapi', label: t('Export as OpenAPI'), icon: 'export', disabled: !open },
    { key: 'workspaces', label: t('Workspaces…'), icon: 'collection', hint: t('Group the collections you work on'), divided: true },
  ]
})

function onTool(key: string) {
  if (key === 'run') {
    store.runnerTarget = null
    store.runnerDialog = true
  }
  else if (key === 'mock') store.mockDialog = true
  else if (key === 'monitors') store.monitorsDialog = true
  else if (key === 'oauth') store.oauthDialog = true
  else if (key === 'cookies') store.cookiesDialog = true
  else if (key === 'sync') store.syncDialog = true
  else if (key === 'docs') store.exportDocs()
  else if (key === 'postman') store.exportPostman()
  else if (key === 'openapi') store.exportOpenApi()
  else if (key === 'workspaces') store.workspacesDialog = true
}
</script>

<template>
  <header class="top-bar">
    <div class="brand" title="volt">
      <span class="mark" aria-hidden="true"><UiIcon name="bolt" :size="15" /></span>
      <span class="word">volt</span>
    </div>

    <button v-if="store.workspace" type="button" class="chip workspace" :title="t('Workspaces')" @click="store.workspacesDialog = true">
      <UiIcon name="collection" :size="12" />{{ store.workspace }}
    </button>

    <span class="spacer" />

    <button
      v-if="store.mock"
      type="button"
      class="chip accent"
      :title="t('Serving examples on {url}', { url: `http://127.0.0.1:${store.mock.port}` })"
      @click="store.mockDialog = true"
    >
      <span class="led live" />{{ t('mock :{port}', { port: store.mock.port }) }}
    </button>

    <button
      v-if="!store.options.verifyTls"
      type="button"
      class="chip warn"
      :title="t('TLS certificates are not verified. Click to change.')"
      @click="emit('openSettings')"
    >
      <UiIcon name="warning" :size="12" />{{ t('TLS off') }}
    </button>

    <template v-if="store.isOpen">
      <UiMenuButton
        v-if="store.environments.length"
        :items="envItems"
        :label="t('Environment')"
        align="end"
        variant="solid"
        class="env"
        :title="t('Variables come from {name}', { name: String(store.environment?.name) })"
        @select="onEnv"
      >
        <span class="led ok" />
        <span class="env-name mono">{{ store.environment?.name }}</span>
      </UiMenuButton>
      <button v-else type="button" class="btn btn-sm" @click="emit('editEnvironment')">
        <UiIcon name="globe" :size="14" />{{ t('Add environment') }}
      </button>
    </template>

    <UiMenuButton :items="toolItems" :label="t('Tools')" align="end" variant="solid" @select="onTool">
      <UiIcon name="grid" :size="14" />{{ t('Tools') }}
    </UiMenuButton>

    <span class="sep" aria-hidden="true" />

    <button
      v-if="canSplit"
      type="button"
      class="icon-btn quiet"
      :aria-label="layout === 'rows' ? t('Put the response beside the request') : t('Put the response under the request')"
      :title="layout === 'rows' ? t('Response beside the request') : t('Response under the request')"
      @click="layout = layout === 'rows' ? 'cols' : 'rows'"
    >
      <UiIcon :name="layout === 'rows' ? 'cols' : 'rows'" :size="16" />
    </button>
    <button type="button" class="icon-btn quiet" :aria-label="t('Keyboard shortcuts')" :title="t('Every shortcut — or press ?')" @click="store.shortcutsSheet = true">
      <UiIcon name="keyboard" :size="16" />
    </button>
    <button type="button" class="icon-btn quiet" :aria-label="t('Settings')" :title="t('Settings ({key}+,)', { key: modKey })" @click="emit('openSettings')">
      <UiIcon name="sliders" :size="16" />
    </button>
  </header>
</template>

<style scoped>
/* On the bench itself, not on a panel: no fill, no rule. */
.top-bar {
  display: flex;
  align-items: center;
  gap: var(--s-2);
  height: var(--h-bar);
  padding: 0 var(--gap) 0 calc(var(--gap) + 6px);
  flex: none;
  min-width: 0;
}

.brand { display: flex; align-items: center; gap: 8px; flex: none; margin-right: var(--s-2); }
/* The mark is the highlighter itself: a lime tile with the bolt in ink. */
.mark {
  display: grid;
  place-items: center;
  width: 24px;
  height: 24px;
  border-radius: 7px;
  background: var(--accent);
  color: var(--accent-ink);
}
.mark :deep(.ui-icon) { stroke-width: 1.75; }
.word { font-weight: 700; font-size: 15px; letter-spacing: -0.03em; }

.workspace { height: 24px; }
.chip { flex: none; height: 24px; padding: 0 10px; }
button.chip:hover { filter: brightness(0.96); }

/* Controls here sit on the bench, so they are a panel's colour, not a well's. */
.top-bar :deep(.btn) { background: var(--bg-1); box-shadow: var(--shadow-panel); }
.top-bar :deep(.btn:hover:not(:disabled)) { background: var(--bg-2); }
.top-bar .icon-btn.quiet:hover { background: var(--press); }

.env { max-width: 220px; }
.env-name { font-size: var(--t-meta); font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

.sep { width: 1px; height: 18px; margin: 0 2px; background: var(--line-strong); }
</style>
