<script setup lang="ts">
import type { MenuItem } from '~/utils/ui'

const store = useCollectionStore()
const { creatingFolder, startCreatingFolder } = useTreeMenu()

const creatingAtRoot = computed(() => creatingFolder.value?.parent === null)

// Searching shows a flat list of matches with their paths instead of the tree:
// a hit three folders deep is easier to read as a path than as an expansion.
const filter = ref('')
const filterInput = ref<HTMLInputElement>()
const hits = computed(() => store.searchRequests(filter.value))

useShortcut('/', 'Search the collection', () => {
  store.sidebarTab = 'tree'
  nextTick(() => filterInput.value?.focus())
})
useShortcut('mod+n', 'New request', () => (store.isOpen ? store.createRequest(null) : undefined))
useShortcut('mod+shift+n', 'New folder', () => (store.isOpen ? startCreatingFolder(null) : undefined))

// Everything that acts on the whole collection, in one menu: the row was
// growing a button per feature and none of them are used every minute.
const collectionItems = computed<MenuItem[]>(() => {
  const open = store.isOpen
  return [
    { key: 'folder', label: 'New folder', icon: 'folder-plus', disabled: !open },
    { key: 'curl', label: 'New request from cURL…', icon: 'import', hint: 'Paste a whole command', disabled: !open },
    { key: 'settings', label: 'Collection settings…', icon: 'sliders', hint: 'Headers, auth, variables', disabled: !open, divided: true },
    { key: 'cookies', label: 'Cookies…', icon: 'cookie', hint: 'What this collection is holding', disabled: !open },
    { key: 'bin', label: 'Bin…', icon: 'trash', hint: 'Put back something deleted', disabled: !open },
    { key: 'run', label: 'Run…', icon: 'check', hint: 'Every request in order, with its checks', disabled: !open, divided: true },
    { key: 'oauth', label: 'Get an OAuth token…', icon: 'key', hint: 'Into the environment, as a secret', disabled: !open },
    { key: 'sync', label: 'Sync…', icon: 'history', hint: 'Pull, commit and push the folder', disabled: !open, divided: true },
    { key: 'workspaces', label: 'Workspaces…', icon: 'collection', hint: 'Group the collections you work on' },
    { key: 'monitors', label: 'Monitors…', icon: 'info', hint: 'Watch a request on a schedule', disabled: !open },
    { key: 'mock', label: 'Mock server…', icon: 'braces', hint: 'Serve the saved examples locally', disabled: !open },
    { key: 'import', label: 'Import…', icon: 'import', hint: 'From Postman, Insomnia or OpenAPI', divided: true },
    { key: 'postman', label: 'Export for Postman', icon: 'file-down', disabled: !open },
    { key: 'docs', label: 'Write documentation…', icon: 'file', hint: 'One HTML file, no publishing', disabled: !open },
  ]
})

function onCollectionMenu(key: string) {
  if (key === 'folder') startCreatingFolder(null)
  else if (key === 'curl') store.curlDialog = { mode: 'create', parent: null }
  else if (key === 'settings') store.scopeDialog = { id: null, title: store.collection?.meta.name ?? 'Collection' }
  else if (key === 'cookies') store.cookiesDialog = true
  else if (key === 'bin') store.trashDialog = true
  else if (key === 'run') store.runnerDialog = true
  else if (key === 'oauth') store.oauthDialog = true
  else if (key === 'sync') store.syncDialog = true
  else if (key === 'workspaces') store.workspacesDialog = true
  else if (key === 'monitors') store.monitorsDialog = true
  else if (key === 'mock') store.mockDialog = true
  else if (key === 'import') store.browseAndImport()
  else if (key === 'postman') store.exportPostman()
  else if (key === 'docs') store.exportDocs()
}
</script>

<template>
  <aside class="sidebar">
    <template v-if="store.sidebarTab === 'tree'">
      <div class="sidebar-tools">
        <div class="search">
          <UiIcon name="search" :size="13" />
          <input
            ref="filterInput"
            v-model="filter"
            class="field inline"
            aria-label="Search requests"
            placeholder="Search"
            spellcheck="false"
            :disabled="!store.isOpen"
            @keydown.esc.prevent="filter = ''"
          >
          <button v-if="filter" type="button" class="icon-btn quiet sm" aria-label="Clear search" title="Clear (Esc)" @click="filter = ''">
            <UiIcon name="x" :size="13" />
          </button>
        </div>
        <button
          type="button"
          class="btn btn-sm new"
          :disabled="!store.isOpen"
          :title="store.isOpen ? `New request (${modKey}+N)` : 'Open a collection first'"
          aria-label="New request"
          @click="store.createRequest(null)"
        >
          <UiIcon name="plus" :size="14" />New
        </button>
      </div>

      <!-- The collection is the root of the tree, and its menu lives on it. -->
      <div v-if="store.isOpen" class="collection-row" :title="store.root ?? ''">
        <UiIcon name="collection" :size="14" class="glyph" />
        <span class="collection-name">{{ store.collection?.meta.name }}</span>
        <UiMenuButton :items="collectionItems" label="Collection actions" align="end" class="collection-menu" @select="onCollectionMenu">
          <UiIcon name="more" :size="15" />
        </UiMenuButton>
      </div>

      <nav v-if="store.isOpen && filter.trim()" class="scroll" aria-label="Search results">
        <button v-for="hit in hits" :key="hit.id" type="button" class="hit" @click="store.select(hit.id)">
          <span class="method" :data-method="hit.method">{{ hit.method }}</span>
          <span class="hit-text">
            <span class="hit-name">{{ hit.name }}</span>
            <span class="hit-id mono">{{ hit.id }}</span>
          </span>
        </button>
        <p v-if="!hits.length" class="none">Nothing matches</p>
      </nav>

      <nav v-else class="scroll" aria-label="Requests">
        <NewFolderInput v-if="creatingAtRoot" :parent="null" :depth="0" />
        <TreeNode v-if="store.tree.length" :nodes="store.tree" :depth="0" />

        <div v-else-if="store.isOpen && !creatingAtRoot" class="empty">
          <p class="empty-title">Empty collection</p>
          <p>Every request you add is a YAML file in this folder. Start with one.</p>
          <button type="button" class="btn btn-primary btn-sm" @click="store.createRequest(null)">
            <UiIcon name="plus" :size="14" />New request
          </button>
        </div>

        <div v-else-if="!store.isOpen" class="empty">
          <p>Open a collection to see its requests here.</p>
        </div>
      </nav>
    </template>

    <nav v-else class="scroll" aria-label="History">
      <HistoryList />
    </nav>

    <!-- TreeNode is recursive, so the menu lives here and is rendered once. -->
    <TreeMenu />
  </aside>
</template>

<style scoped>
.sidebar {
  display: flex;
  flex-direction: column;
  flex: none;
  min-height: 0;
  background: var(--bg-0);
}

.sidebar-tools {
  display: flex;
  align-items: center;
  gap: var(--s-1);
  padding: var(--s-2) var(--s-2) var(--s-2) var(--s-3);
  border-bottom: 1px solid var(--line);
}
.search {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 4px;
  height: 28px;
  padding-left: 6px;
  border: 1px solid transparent;
  border-radius: var(--r-sm);
  color: var(--silk);
  transition: border-color var(--dur) var(--ease), background var(--dur) var(--ease);
}
.search:focus-within { background: var(--well); border-color: var(--accent); }
.search .field { flex: 1; min-width: 0; height: 26px; padding-left: 4px; font-size: var(--t-small); }
.search .field:focus { background: transparent; border-color: transparent; box-shadow: none; }
.search .field:hover { background: transparent; }
.new { flex: none; }

.collection-row {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 36px;
  padding: 0 var(--s-2) 0 var(--s-4);
  flex: none;
}
.collection-row .glyph { color: var(--silk); }
.collection-name { flex: 1; min-width: 0; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.collection-menu { flex: none; }

.scroll { flex: 1; min-height: 0; overflow: auto; padding: 0 0 var(--s-3); }

.empty { display: grid; gap: var(--s-2); justify-items: start; padding: var(--s-3) var(--s-4); }
.empty p { margin: 0; color: var(--silk); font-size: var(--t-small); line-height: 1.5; }
.empty-title { color: var(--ink) !important; font-weight: 600; }

.hit {
  display: flex;
  align-items: baseline;
  gap: var(--s-2);
  width: 100%;
  padding: 5px var(--s-4);
  text-align: left;
}
.hit:hover { background: var(--hover); }
.hit-text { display: grid; min-width: 0; }
.hit-name { font-size: var(--t-small); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.hit-id { color: var(--faint); font-size: var(--t-meta); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.none { margin: 0; padding: var(--s-5) var(--s-4); color: var(--silk); font-size: var(--t-small); text-align: center; }
</style>
