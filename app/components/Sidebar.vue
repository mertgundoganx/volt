<script setup lang="ts">
import type { MenuItem, TabItem } from '~/utils/ui'

const store = useCollectionStore()
const { creatingFolder, startCreatingFolder } = useTreeMenu()

const creatingAtRoot = computed(() => creatingFolder.value?.parent === null)
const counts = computed(() => store.countNodes())

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
    { key: 'settings', label: 'Collection settings…', icon: 'sliders', hint: 'Headers, auth, variables', disabled: !open },
    { key: 'cookies', label: 'Cookies…', icon: 'cookie', hint: 'What this collection is holding', disabled: !open },
    { key: 'bin', label: 'Bin…', icon: 'trash', hint: 'Put back something deleted', disabled: !open },
    { key: 'run', label: 'Run…', icon: 'check', hint: 'Every request in order, with its checks', disabled: !open, divided: true },
    { key: 'oauth', label: 'Get an OAuth token…', icon: 'key', hint: 'Into the environment, as a secret', disabled: !open },
    { key: 'sync', label: 'Sync…', icon: 'history', hint: 'Pull, commit and push the folder', disabled: !open, divided: true },
    { key: 'workspaces', label: 'Workspaces…', icon: 'collection', hint: 'Group the collections you work on' },
    { key: 'monitors', label: 'Monitors…', icon: 'info', hint: 'Watch a request on a schedule', disabled: !open },
    { key: 'mock', label: 'Mock server…', icon: 'braces', hint: 'Serve the saved examples locally', disabled: !open },
    { key: 'import', label: 'Import…', icon: 'import', hint: 'From Postman or Insomnia', divided: true },
    { key: 'postman', label: 'Export for Postman', icon: 'file-down', disabled: !open },
    { key: 'docs', label: 'Write documentation…', icon: 'file', hint: 'One HTML file, no publishing', disabled: !open },
  ]
})

function onCollectionMenu(key: string) {
  if (key === 'settings') store.scopeDialog = { id: null, title: store.collection?.meta.name ?? 'Collection' }
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

const tabs = computed<TabItem[]>(() => [
  { key: 'tree', label: 'Requests', meta: counts.value.requests || null },
  { key: 'history', label: 'History', meta: store.history.length || null },
])
const tab = computed({
  get: () => store.sidebarTab,
  set: (value: string) => (store.sidebarTab = value as 'tree' | 'history'),
})
</script>

<template>
  <aside class="sidebar">
    <div class="head">
      <button
        v-if="store.workspace"
        type="button"
        class="workspace"
        title="Workspaces"
        @click="store.workspacesDialog = true"
      >
        <UiIcon name="collection" :size="12" />{{ store.workspace }}
      </button>
      <span class="silk">Collection</span>
      <span class="name">{{ store.collection?.meta.name ?? 'None open' }}</span>
      <span v-if="store.root" class="path mono" :title="store.root">{{ store.root }}</span>
    </div>

    <div class="keys">
      <button type="button" class="icon-btn" aria-label="Open collection" title="Open a collection folder" @click="store.browseAndOpen()">
        <UiIcon name="folder-open" />
      </button>
      <button type="button" class="icon-btn" aria-label="New collection" title="Create a collection in an empty folder" @click="store.browseAndInit()">
        <UiIcon name="collection" />
      </button>
      <UiMenuButton :items="collectionItems" label="Collection actions" align="start" @select="onCollectionMenu">
        <UiIcon name="more" :size="15" />
      </UiMenuButton>
      <span class="spacer" />
      <button
        type="button"
        class="icon-btn"
        aria-label="New folder"
        :disabled="!store.isOpen"
        :title="store.isOpen ? 'New folder' : 'Open a collection first'"
        @click="startCreatingFolder(null)"
      >
        <UiIcon name="folder-plus" />
      </button>
      <button
        type="button"
        class="icon-btn"
        aria-label="New request"
        :disabled="!store.isOpen"
        :title="store.isOpen ? 'New request' : 'Open a collection first'"
        @click="store.createRequest(null)"
      >
        <UiIcon name="file-plus" />
      </button>
    </div>

    <UiTabs v-if="store.isOpen" v-model="tab" :items="tabs" label="Sidebar" />

    <div v-if="store.isOpen && store.sidebarTab === 'tree'" class="filter">
      <UiIcon name="search" :size="13" />
      <input
        ref="filterInput"
        v-model="filter"
        class="field inline mono"
        aria-label="Search requests"
        placeholder="Search"
        spellcheck="false"
        @keydown.esc.prevent="filter = ''"
      >
      <button v-if="filter" type="button" class="icon-btn quiet sm" aria-label="Clear search" title="Clear (Esc)" @click="filter = ''">
        <UiIcon name="x" :size="13" />
      </button>
    </div>

    <nav v-if="store.isOpen && store.sidebarTab === 'tree' && filter.trim()" class="scroll" aria-label="Search results">
      <button v-for="hit in hits" :key="hit.id" type="button" class="hit" @click="store.select(hit.id)">
        <span class="method" :data-method="hit.method">{{ hit.method }}</span>
        <span class="hit-text">
          <span class="hit-name">{{ hit.name }}</span>
          <span class="hit-id mono">{{ hit.id }}</span>
        </span>
      </button>
      <p v-if="!hits.length" class="none silk">Nothing matches</p>
    </nav>


    <nav v-else-if="store.isOpen && store.sidebarTab === 'history'" class="scroll" aria-label="History">
      <HistoryList />
    </nav>

    <nav v-else class="scroll" aria-label="Requests">
      <NewFolderInput v-if="creatingAtRoot" :parent="null" :depth="0" />
      <TreeNode v-if="store.tree.length" :nodes="store.tree" :depth="0" />

      <div v-else-if="store.isOpen && !creatingAtRoot" class="empty">
        <span class="silk">Empty collection</span>
        <p>Nothing here yet. Requests you add are saved as YAML files in this folder.</p>
        <button type="button" class="btn btn-sm" @click="store.createRequest(null)">
          <UiIcon name="file-plus" :size="14" />New request
        </button>
      </div>

      <div v-else-if="!store.isOpen" class="empty">
        <p>Open a collection to see its requests here.</p>
      </div>
    </nav>

    <footer v-if="store.isOpen && store.sidebarTab === 'tree' && store.tree.length" class="foot">
      <span class="silk">{{ counts.requests }} {{ counts.requests === 1 ? 'request' : 'requests' }}</span>
      <span class="silk">{{ counts.folders }} {{ counts.folders === 1 ? 'folder' : 'folders' }}</span>
    </footer>

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
  border-right: 1px solid var(--line);
}

.head {
  display: grid;
  gap: 5px;
  padding: var(--s-4) var(--s-4) var(--s-3);
  min-width: 0;
}
.name {
  font-stretch: 100%;
  font-weight: 750;
  font-size: 16px;
  letter-spacing: -0.02em;
  line-height: 1.2;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.path {
  font-size: var(--t-meta);
  color: var(--silk);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  /* Keep the end of the path, which is the part that tells folders apart. */
  direction: rtl;
  text-align: left;
}

.keys { display: flex; gap: 6px; padding: 0 var(--s-4) var(--s-3); }

.sidebar :deep(.ui-tabs) { padding: 0 var(--s-4); }

.scroll { flex: 1; min-height: 0; overflow: auto; padding: var(--s-2) 0 var(--s-3); }

.empty { display: grid; gap: var(--s-2); justify-items: start; padding: var(--s-3) var(--s-4); }
.empty p { margin: 0; color: var(--silk); font-size: var(--t-small); line-height: 1.5; }

.foot {
  display: flex;
  justify-content: space-between;
  padding: 10px var(--s-4);
  border-top: 1px solid var(--line);
  flex: none;
}

.filter {
  display: flex;
  align-items: center;
  gap: var(--s-2);
  padding: 5px var(--s-3) 5px var(--s-4);
  border-bottom: 1px solid var(--line-soft);
  color: var(--silk);
}
.filter .field { flex: 1; min-width: 0; font-size: var(--t-small); }

.hit {
  display: flex;
  align-items: baseline;
  gap: var(--s-2);
  width: 100%;
  padding: 5px var(--s-4);
  text-align: left;
}
.hit:hover { background: var(--hover); }
.hit:hover .hit-name { color: var(--ink); }
.hit-text { display: grid; min-width: 0; }
.hit-name { font-size: var(--t-small); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.hit-id { color: var(--faint); font-size: var(--t-meta); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.none { padding: var(--s-5) var(--s-4); text-align: center; }

.workspace {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  justify-self: start;
  padding: 2px 7px;
  border: 1px solid var(--line);
  border-radius: var(--r-xs);
  color: var(--silk);
  font-size: var(--t-meta);
  font-stretch: 72%;
  text-transform: uppercase;
  letter-spacing: 0.06em;
}
.workspace:hover { color: var(--accent-text); border-color: var(--accent-line); background: var(--accent-tint); }
</style>
