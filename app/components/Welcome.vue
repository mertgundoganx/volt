<script setup lang="ts">
import { statusTone } from '~/utils/ui'

const store = useCollectionStore()
const { startCreatingFolder } = useTreeMenu()

const counts = computed(() => store.countNodes())

/** The last path segment names the folder; the rest is where it lives. */
function splitPath(path: string) {
  const parts = path.split(/[\\/]/).filter(Boolean)
  const name = parts.pop() ?? path
  return { name, parent: parts.join(path.includes('\\') ? '\\' : '/') }
}

const recentCollections = computed(() => store.recent.map((path) => ({ path, ...splitPath(path) })))
const recentSends = computed(() => store.history.slice(0, 8))

/** One line that says how much is here. */
const summary = computed(() => {
  const parts: string[] = []
  const { requests, folders } = counts.value
  parts.push(requests === 1 ? '1 request' : `${requests} requests`)
  if (folders) parts.push(folders === 1 ? '1 folder' : `${folders} folders`)
  const envs = store.environments.length
  parts.push(envs === 1 ? '1 environment' : `${envs} environments`)
  return parts.join(' · ')
})

const tips = [
  { keys: [modKey, '↵'], what: 'Send' },
  { keys: [modKey, 'S'], what: 'Save the request to its file' },
  { keys: [modKey, 'N'], what: 'New request' },
  { keys: [modKey, 'P'], what: 'Jump to a request by name' },
  { keys: [modKey, 'V'], what: 'A curl command pasted into the URL fills the request' },
]

function ago(at: number) {
  const minutes = Math.max(0, Math.round((Date.now() - at) / 60_000))
  if (minutes < 1) return 'just now'
  if (minutes < 60) return `${minutes} min ago`
  const hours = Math.round(minutes / 60)
  if (hours < 24) return `${hours} h ago`
  return new Date(at).toLocaleDateString()
}
</script>

<template>
  <section class="welcome">
    <!-- No collection yet -->
    <div v-if="!store.isOpen" class="inner">
      <h1>Open a collection</h1>
      <p class="lede">
        A collection is a folder of YAML files — one per request — usually kept in the
        repository of the API it describes.
      </p>

      <div class="actions">
        <button type="button" class="btn btn-primary" @click="store.browseAndOpen()">
          <UiIcon name="folder-open" :size="15" />Open a folder
        </button>
        <button type="button" class="btn" @click="store.browseAndInit()">
          <UiIcon name="collection" :size="15" />New collection
        </button>
        <button type="button" class="btn btn-quiet" @click="store.browseAndImport()">
          <UiIcon name="import" :size="15" />Import
        </button>
      </div>

      <div v-if="recentCollections.length" class="block">
        <h2 class="silk">Recent</h2>
        <ul class="list">
          <li v-for="item in recentCollections" :key="item.path">
            <button type="button" class="line" :title="item.path" @click="store.open(item.path)">
              <UiIcon name="folder" :size="14" class="glyph" />
              <span class="main">{{ item.name }}</span>
              <span class="side mono">{{ item.parent }}</span>
            </button>
          </li>
        </ul>
      </div>
    </div>

    <!-- Collection open, nothing selected -->
    <div v-else class="inner">
      <h1>{{ store.collection?.meta.name }}</h1>
      <p class="path mono">{{ store.root }}</p>
      <p class="lede">{{ summary }}</p>
      <p v-if="store.root && store.root === store.defaultRoot" class="lede personal">
        This is your own collection, kept in the folder above. To keep requests next to the
        code they exercise, open a folder inside that repository instead — the files are the same.
      </p>

      <div class="actions">
        <button type="button" class="btn btn-primary" @click="store.createRequest(null)">
          <UiIcon name="plus" :size="15" />New request
        </button>
        <button type="button" class="btn" @click="store.curlDialog = { mode: 'create', parent: null }">
          <UiIcon name="import" :size="15" />From a cURL command
        </button>
        <button type="button" class="btn btn-quiet" @click="startCreatingFolder(null)">
          <UiIcon name="folder-plus" :size="15" />New folder
        </button>
      </div>

      <div v-if="recentSends.length" class="block">
        <h2 class="silk">Recently sent</h2>
        <ul class="list">
          <li v-for="entry in recentSends" :key="entry.id">
            <button type="button" class="line" :title="`${entry.method} ${entry.url}`" @click="store.openHistory(entry.id)">
              <span class="method" :data-method="entry.method">{{ entry.method === 'DELETE' ? 'DEL' : entry.method }}</span>
              <span class="main">{{ entry.name }}</span>
              <span class="side mono">{{ entry.url }}</span>
              <span class="status mono num" :class="statusTone(entry.status, !!entry.error)">{{ entry.error ? 'ERR' : entry.status }}</span>
              <span class="when">{{ ago(entry.at) }}</span>
            </button>
          </li>
        </ul>
      </div>

      <div class="block tips">
        <h2 class="silk">Keys</h2>
        <dl>
          <template v-for="tip in tips" :key="tip.what">
            <dt><span v-for="key in tip.keys" :key="key" class="kbd">{{ key }}</span></dt>
            <dd>{{ tip.what }}</dd>
          </template>
        </dl>
      </div>
    </div>
  </section>
</template>

<style scoped>
/* Anchored top-left like the rest of the tool, not centred like a landing page. */
.welcome {
  display: flex;
  align-items: flex-start;
  min-height: 0;
  overflow: auto;
  padding: var(--s-10) var(--s-8) var(--s-8) var(--s-10);
}

.inner { width: min(680px, 100%); display: flex; flex-direction: column; align-items: flex-start; }

h1 {
  margin: 0;
  font-weight: 600;
  font-size: var(--t-display);
  line-height: 1.2;
  letter-spacing: -0.015em;
}
.path { margin: 4px 0 0; color: var(--faint); font-size: var(--t-meta); word-break: break-all; }
.lede { margin: var(--s-3) 0 0; max-width: 56ch; color: var(--ink-2); font-size: var(--t-body); line-height: 1.55; }

.personal { max-width: 62ch; color: var(--silk); }

.actions { display: flex; flex-wrap: wrap; gap: var(--s-2); margin-top: var(--s-5); }

.block { width: 100%; margin-top: var(--s-8); display: grid; gap: var(--s-2); }
.block h2 { margin: 0; }

.list { list-style: none; margin: 0; padding: 0; border-top: 1px solid var(--line); }
.list li { border-bottom: 1px solid var(--line-soft); }
.line {
  display: flex;
  align-items: center;
  gap: var(--s-3);
  width: 100%;
  height: 34px;
  padding: 0 var(--s-2);
  text-align: left;
  color: var(--ink-2);
  transition: background var(--dur) var(--ease);
}
.line:hover { background: var(--hover); color: var(--ink); }
.line .glyph { color: var(--silk); }
.line .method { width: 36px; flex: none; }
.line .main { flex: none; max-width: 40%; font-weight: 500; color: var(--ink); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.line .side { flex: 1; min-width: 0; color: var(--faint); font-size: var(--t-meta); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.line .status { flex: none; font-size: var(--t-meta); font-weight: 600; color: var(--silk); }
.line .status.ok { color: var(--ok); }
.line .status.warn { color: var(--warn); }
.line .status.bad { color: var(--bad); }
.line .when { flex: none; color: var(--faint); font-size: var(--t-meta); }

.tips dl { display: grid; grid-template-columns: max-content 1fr; gap: 6px var(--s-4); margin: 0; align-items: center; }
.tips dt { display: flex; gap: 3px; }
.tips dd { margin: 0; color: var(--ink-2); font-size: var(--t-small); }
</style>
