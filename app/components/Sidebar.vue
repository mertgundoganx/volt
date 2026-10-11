<script setup lang="ts">
import type { MenuItem, SegmentOption } from '~/utils/ui'
import { isCurl } from '~/utils/curl'
import type { SearchHit } from '~/types'

const store = useCollectionStore()
const { creatingFolder, startCreatingFolder } = useTreeMenu()

const creatingAtRoot = computed(() => creatingFolder.value?.parent === null)

const TABS: SegmentOption<'tree' | 'history'>[] = [
  { value: 'tree', label: t('Collections') },
  { value: 'history', label: t('History') },
]

// Searching shows a flat list of matches with their paths instead of the tree:
// a hit three folders deep is easier to read as a path than as an expansion.
const filter = ref('')
const filterInput = ref<HTMLInputElement>()
const hits = computed(() => store.searchRequests(filter.value))

// What is *inside* requests — URLs, headers, bodies, notes — comes from Rust,
// a moment after the names, so typing stays quick.
const inside = ref<SearchHit[]>([])
let insideTimer: ReturnType<typeof setTimeout> | undefined
watch(filter, (query) => {
  clearTimeout(insideTimer)
  inside.value = []
  if (!query.trim() || !store.root) return
  insideTimer = setTimeout(async () => {
    const found = await store.searchContents(query)
    if (filter.value === query) inside.value = found ?? []
  }, 250)
})
/** Contents hits that the name search did not already list. */
const insideOnly = computed(() => inside.value.filter((hit) => !hits.value.some((named) => named.id === hit.id)))

/** A curl command dropped on the empty part of the tree lands at the root. */
function onRootDrop(event: DragEvent) {
  const text = event.dataTransfer?.getData('text/plain') ?? ''
  if (isCurl(text)) void store.importCurl(text, { mode: 'create', parent: null })
}

useShortcut('/', t('Search the collection'), () => {
  store.sidebarTab = 'tree'
  nextTick(() => filterInput.value?.focus())
})
useShortcut('mod+n', t('New request'), () => (store.isOpen ? store.createRequest(null) : undefined))
useShortcut('mod+shift+n', t('New folder'), () => (store.isOpen ? startCreatingFolder(null) : undefined))

/** Everything that can be made or brought in, beside the key that makes a request. */
const createItems = computed<MenuItem[]>(() => {
  const open = store.isOpen
  return [
    { key: 'folder', label: t('New folder'), icon: 'folder-plus', disabled: !open },
    { key: 'curl', label: t('Request from cURL…'), icon: 'braces', hint: t('Paste a whole command'), disabled: !open },
    { key: 'init', label: t('New collection…'), icon: 'collection', hint: t('Creates the files in an empty folder'), divided: true },
    { key: 'browse', label: t('Open a folder…'), icon: 'folder-open', hint: t('One with a collection.yaml') },
    { key: 'import', label: t('Import…'), icon: 'import', hint: 'Postman, Insomnia, Bruno, OpenAPI, HAR' },
  ]
})

function onCreate(key: string) {
  if (key === 'folder') startCreatingFolder(null)
  else if (key === 'curl') store.curlDialog = { mode: 'create', parent: null }
  else if (key === 'init') store.browseAndInit()
  else if (key === 'browse') store.browseAndOpen()
  else if (key === 'import') store.browseAndImport()
}

/** What belongs to the open collection itself. */
const collectionItems = computed<MenuItem[]>(() => [
  { key: 'request', label: t('New request'), icon: 'plus' },
  { key: 'folder', label: t('New folder'), icon: 'folder-plus' },
  { key: 'settings', label: t('Collection settings…'), icon: 'sliders', hint: t('Headers, auth, variables'), divided: true },
  { key: 'run', label: t('Run…'), icon: 'play', hint: t('Every request in order') },
  { key: 'bin', label: t('Bin…'), icon: 'trash', hint: t('Put back something deleted'), divided: true },
])

function onCollectionMenu(key: string) {
  if (key === 'request') store.createRequest(null)
  else if (key === 'folder') startCreatingFolder(null)
  else if (key === 'settings') store.scopeDialog = { id: null, title: store.collection?.meta.name ?? t('Collection') }
  else if (key === 'bin') store.trashDialog = true
  else if (key === 'run') {
    store.runnerTarget = null
    store.runnerDialog = true
  }
}

/**
 * The other collections this machine knows, under the open one, so moving
 * between them is a click on a name rather than a trip through a menu. Only
 * one is open at a time; its tabs come back when you return to it.
 */
const others = computed(() =>
  store.knownCollections
    .filter((path) => path !== store.root)
    .map((path) => {
      const parts = path.split(/[\\/]/).filter(Boolean)
      return { path, name: parts.pop() ?? path }
    }),
)
</script>

<template>
  <aside class="sidebar panel">
    <div class="side-head">
      <UiSegmented v-model="store.sidebarTab" :options="TABS" :label="t('Sidebar')" />
    </div>

    <template v-if="store.sidebarTab === 'tree'">
      <div class="sidebar-tools">
        <div class="search">
          <UiIcon name="search" :size="13" />
          <input
            ref="filterInput"
            v-model="filter"
            class="field inline"
            :aria-label="t('Search requests')"
            :placeholder="t('Search')"
            spellcheck="false"
            :disabled="!store.isOpen"
            @keydown.esc.prevent="filter = ''"
          >
          <button v-if="filter" type="button" class="icon-btn quiet sm" :aria-label="t('Clear search')" :title="t('Clear (Esc)')" @click="filter = ''">
            <UiIcon name="x" :size="13" />
          </button>
        </div>
        <!-- One key, two halves: the plus makes a request, the arrow lists the rest. -->
        <div class="create">
          <button
            type="button"
            class="new"
            :disabled="!store.isOpen"
            :title="store.isOpen ? t('New request ({key}+N)', { key: modKey }) : t('Open a collection first')"
            :aria-label="t('New request')"
            @click="store.createRequest(null)"
          >
            <UiIcon name="plus" :size="14" />{{ t('New') }}
          </button>
          <UiMenuButton :items="createItems" :label="t('Create or import')" align="end" class="more" :title="t('New folder, new collection, import…')" @select="onCreate">
            <span class="visually-hidden">{{ t('More') }}</span>
          </UiMenuButton>
        </div>
      </div>

      <nav v-if="store.isOpen && filter.trim()" class="scroll" :aria-label="t('Search results')">
        <button v-for="hit in hits" :key="hit.id" type="button" class="hit" @click="store.select(hit.id)">
          <span class="method" :data-method="hit.method">{{ hit.method }}</span>
          <span class="hit-text">
            <span class="hit-name">{{ hit.name }}</span>
            <span class="hit-id mono">{{ hit.id }}</span>
          </span>
        </button>
        <template v-if="insideOnly.length">
          <p class="silk group-label">{{ t('In contents') }}</p>
          <button v-for="hit in insideOnly" :key="hit.id" type="button" class="hit" @click="store.select(hit.id)">
            <span class="method" :data-method="hit.method">{{ hit.method }}</span>
            <span class="hit-text">
              <span class="hit-name">{{ hit.name }}</span>
              <span class="hit-id mono">{{ hit.foundIn }} · {{ hit.id }}</span>
            </span>
          </button>
        </template>
        <p v-if="!hits.length && !insideOnly.length" class="none">{{ t('Nothing matches') }}</p>
      </nav>

      <nav v-else class="scroll" :aria-label="t('Requests')" @dragover.prevent @drop.prevent="onRootDrop">
        <!-- The collection is the root of the tree, and its menu lives on it. -->
        <div v-if="store.isOpen" class="collection-row" :title="store.root ?? ''">
          <UiIcon name="collection" :size="14" class="glyph" />
          <span class="collection-name">{{ store.collection?.meta.name }}</span>
          <UiMenuButton :items="collectionItems" :label="t('Collection actions')" align="end" class="collection-menu" @select="onCollectionMenu">
            <UiIcon name="more" :size="15" />
          </UiMenuButton>
        </div>

        <NewFolderInput v-if="creatingAtRoot" :parent="null" :depth="0" />
        <TreeNode v-if="store.tree.length" :nodes="store.tree" :depth="0" />

        <div v-else-if="store.isOpen && !creatingAtRoot" class="empty">
          <p class="empty-title">{{ t('Nothing here yet') }}</p>
          <p>{{ t('Every request you add is a YAML file in this folder.') }}</p>
          <button type="button" class="btn btn-primary btn-sm" @click="store.createRequest(null)">
            <UiIcon name="plus" :size="14" />{{ t('New request') }}
          </button>
        </div>

        <div v-else-if="!store.isOpen" class="empty">
          <p>{{ t('Open a collection to see its requests here.') }}</p>
        </div>

        <template v-if="others.length">
          <p class="silk group-label others-label">{{ t('Other collections') }}</p>
          <div v-for="other in others" :key="other.path" class="other" :title="other.path">
            <button type="button" class="other-open" @click="store.open(other.path)">
              <UiIcon name="folder" :size="14" class="glyph" />
              <span class="other-name">{{ other.name }}</span>
            </button>
            <button type="button" class="icon-btn quiet sm forget" :aria-label="t('Remove {name} from this list', { name: other.name })" :title="t('Remove from this list — the folder is left alone')" @click="store.forgetCollection(other.path)">
              <UiIcon name="x" :size="12" />
            </button>
          </div>
        </template>
      </nav>
    </template>

    <nav v-else class="scroll" :aria-label="t('History')">
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
}

.side-head { display: flex; padding: var(--s-2) var(--s-2) 0; }
.side-head :deep(.ui-seg) { flex: 1; }
.side-head :deep(.opt) { flex: 1; justify-content: center; }

.sidebar-tools {
  display: flex;
  align-items: center;
  gap: var(--s-2);
  padding: var(--s-2);
}
.search {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 4px;
  height: 30px;
  padding: 0 4px 0 9px;
  border: 1px solid transparent;
  border-radius: var(--r-sm);
  background: var(--well);
  color: var(--silk);
  transition: border-color var(--dur) var(--ease);
}
.search:focus-within { border-color: var(--focus); }
.search .field { flex: 1; min-width: 0; height: 26px; padding-left: 4px; font-size: var(--t-small); }
.search .field:focus, .search .field:hover { background: transparent; border-color: transparent; box-shadow: none; }

/* The split key: highlighter on both halves, a hairline of ink between. */
.create { display: flex; flex: none; border-radius: var(--r-sm); overflow: hidden; }
.new {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 30px;
  padding: 0 10px 0 9px;
  background: var(--accent);
  color: var(--accent-ink);
  font-weight: 600;
  font-size: var(--t-small);
  transition: background var(--dur) var(--ease);
}
.new:hover:not(:disabled) { background: var(--accent-hi); }
.new:disabled { background: var(--well); color: var(--faint); }
.create :deep(.more) {
  height: 30px;
  width: 24px;
  padding: 0;
  border: 0;
  border-left: 1px solid color-mix(in srgb, var(--accent-ink) 22%, transparent);
  border-radius: 0;
  background: var(--accent);
  color: var(--accent-ink);
}
.create :deep(.more:hover) { background: var(--accent-hi); }
.create :deep(.more .chev) { margin: 0; color: inherit; }

.collection-row {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 34px;
  padding: 0 var(--s-2) 0 14px;
}
.glyph { color: var(--silk); }
.collection-name { flex: 1; min-width: 0; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.collection-menu { flex: none; }

.scroll { flex: 1; min-height: 0; overflow: auto; padding: 0 0 var(--s-3); }

.empty { display: grid; gap: var(--s-2); justify-items: start; padding: var(--s-2) 14px var(--s-3); }
.empty p { margin: 0; color: var(--silk); font-size: var(--t-small); line-height: 1.5; }
.empty-title { color: var(--ink) !important; font-weight: 600; }

.hit {
  display: flex;
  align-items: baseline;
  gap: var(--s-2);
  width: calc(100% - 12px);
  margin: 0 6px;
  padding: 5px 8px;
  border-radius: var(--r-sm);
  text-align: left;
}
.hit:hover { background: var(--hover); }
.hit-text { display: grid; min-width: 0; }
.hit-name { font-size: var(--t-small); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.hit-id { color: var(--faint); font-size: var(--t-meta); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.none { margin: 0; padding: var(--s-5) var(--s-4); color: var(--silk); font-size: var(--t-small); text-align: center; }
.group-label { margin: var(--s-3) 0 4px; padding: 0 14px; }

.others-label { margin-top: var(--s-5); }
.other { display: flex; align-items: center; margin: 0 6px; border-radius: var(--r-sm); }
.other:hover { background: var(--hover); }
.other-open {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 8px;
  height: var(--h-row);
  padding: 0 8px;
  text-align: left;
  color: var(--ink-2);
}
.other-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--t-small); }
.forget { visibility: hidden; margin-right: 3px; }
.other:hover .forget, .forget:focus-visible { visibility: visible; }
</style>
