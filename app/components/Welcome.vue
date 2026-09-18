<script setup lang="ts">
const store = useCollectionStore()
const { startCreatingFolder } = useTreeMenu()

const counts = computed(() => store.countNodes())

/** The last path segment names the folder; the rest is where it lives. */
function splitPath(path: string) {
  const parts = path.split(/[\\/]/).filter(Boolean)
  const name = parts.pop() ?? path
  return { name, parent: parts.join(path.includes('\\') ? '\\' : '/') }
}

const recent = computed(() => store.recent.map((path) => ({ path, ...splitPath(path) })))

const shortcuts = [
  { keys: [modKey, '↵'], what: 'Send the request' },
  { keys: [modKey, 'S'], what: 'Save it to its file' },
  { keys: [modKey, 'V'], what: 'A curl command into the URL fills the request' },
  { keys: ['Double-click'], what: 'Rename in the sidebar' },
  { keys: ['Right-click'], what: 'Move, delete, add inside' },
  { keys: ['Drag'], what: 'Reorder or move into a folder' },
]
</script>

<template>
  <section class="welcome">
    <!-- No collection yet -->
    <div v-if="!store.isOpen" class="inner">
      <span class="silk">No collection open</span>
      <h1>Point volt at a folder</h1>
      <p class="lede">
        A collection is a directory of YAML files that lives in your repository. Open one,
        start a new one, or bring your requests over from another client.
      </p>

      <div class="starts">
        <button type="button" class="start" @click="store.browseAndOpen()">
          <UiIcon name="folder-open" :size="18" />
          <span class="what">Open a collection</span>
          <span class="how">A folder with a <code>collection.yaml</code></span>
        </button>
        <button type="button" class="start" @click="store.browseAndInit()">
          <UiIcon name="collection" :size="18" />
          <span class="what">New collection</span>
          <span class="how">Creates the files in an empty folder</span>
        </button>
        <button type="button" class="start" @click="store.browseAndImport()">
          <UiIcon name="import" :size="18" />
          <span class="what">Import</span>
          <span class="how">From a Postman or Insomnia export</span>
        </button>
      </div>

      <div v-if="recent.length" class="recent">
        <span class="silk">Recent</span>
        <ul>
          <li v-for="item in recent" :key="item.path">
            <button type="button" class="recent-item" :title="item.path" @click="store.open(item.path)">
              <UiIcon name="folder" :size="14" />
              <span class="name">{{ item.name }}</span>
              <span class="parent mono">{{ item.parent }}</span>
            </button>
          </li>
        </ul>
      </div>
    </div>

    <!-- Collection open, nothing selected -->
    <div v-else class="inner">
      <span class="silk">Collection</span>
      <h1>{{ store.collection?.meta.name }}</h1>
      <p class="path mono">{{ store.root }}</p>

      <div class="readout">
        <UiMeasure label="Requests" :value="counts.requests" />
        <UiMeasure label="Folders" :value="counts.folders" />
        <UiMeasure label="Environments" :value="store.environments.length" />
        <UiMeasure label="Sent" :value="store.history.length" />
      </div>

      <div class="actions">
        <button type="button" class="btn" @click="store.createRequest(null)">
          <UiIcon name="file-plus" :size="15" />New request
        </button>
        <button type="button" class="btn btn-quiet" @click="store.curlDialog = { mode: 'create', parent: null }">
          <UiIcon name="import" :size="15" />From cURL
        </button>
        <button type="button" class="btn btn-quiet" @click="startCreatingFolder(null)">
          <UiIcon name="folder-plus" :size="15" />New folder
        </button>
      </div>

      <div class="shortcuts">
        <span class="silk">At hand</span>
        <dl>
          <template v-for="shortcut in shortcuts" :key="shortcut.what">
            <dt><span v-for="key in shortcut.keys" :key="key" class="kbd">{{ key }}</span></dt>
            <dd>{{ shortcut.what }}</dd>
          </template>
        </dl>
      </div>
    </div>
  </section>
</template>

<style scoped>
/* Anchored left like a tool's start page, not centred like a landing page. */
.welcome {
  display: flex;
  align-items: flex-start;
  min-height: 0;
  overflow: auto;
  padding: clamp(32px, 8vh, 80px) var(--s-8) var(--s-8) clamp(32px, 6vw, 88px);
}

.inner { width: min(620px, 100%); display: flex; flex-direction: column; align-items: flex-start; }

h1 {
  margin: 12px 0 10px;
  font-stretch: 102%;
  font-weight: 780;
  font-size: 32px;
  line-height: 1.08;
  letter-spacing: -0.035em;
  text-wrap: balance;
}
.lede { margin: 0; max-width: 56ch; color: var(--ink-2); font-size: 14.5px; line-height: 1.6; }
.path { margin: 0; color: var(--silk); font-size: var(--t-meta); word-break: break-all; }
code { font-family: var(--font-mono); font-size: 0.92em; }

.starts {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: var(--s-2);
  width: 100%;
  margin-top: var(--s-6);
}
@media (max-width: 900px) { .starts { grid-template-columns: 1fr; } }

.start {
  display: grid;
  gap: 4px;
  justify-items: start;
  padding: var(--s-3) var(--s-4) var(--s-4);
  border: 1px solid var(--line);
  border-radius: var(--r-md);
  background: var(--bg-2);
  box-shadow: var(--rim), var(--shadow-1);
  text-align: left;
  transition: border-color var(--dur) var(--ease), background var(--dur) var(--ease), transform var(--dur) var(--ease);
}
.start:hover {
  border-color: var(--accent-line);
  background: color-mix(in srgb, var(--accent) 8%, var(--bg-2));
}
.start:hover .ui-icon { color: var(--accent-text); }
.start:active { transform: translateY(1px); box-shadow: none; }
.start .ui-icon { margin-bottom: 10px; color: var(--ink-2); transition: color var(--dur) var(--ease); }
.what { font-weight: 650; font-size: 14px; }
.how { color: var(--silk); font-size: var(--t-small); line-height: 1.4; }

.recent { width: 100%; margin-top: var(--s-8); display: grid; gap: var(--s-2); }
.recent ul { list-style: none; margin: 0; padding: 0; border-top: 1px solid var(--line-soft); }
.recent li { border-bottom: 1px solid var(--line-soft); }
.recent-item {
  display: flex;
  align-items: center;
  gap: var(--s-2);
  width: 100%;
  height: 34px;
  padding: 0 var(--s-2);
  border-radius: var(--r-sm);
  text-align: left;
  color: var(--ink-2);
}
.recent-item:hover { background: var(--hover); color: var(--ink); }
.recent-item .name { font-weight: 600; color: var(--ink); flex: none; }
.recent-item .parent { color: var(--silk); font-size: var(--t-meta); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl; text-align: left; }

.readout {
  display: flex;
  gap: var(--s-8);
  width: 100%;
  margin-top: var(--s-6);
  padding: var(--s-4) 0;
  border-top: 1px solid var(--line);
  border-bottom: 1px solid var(--line);
}

.actions { display: flex; gap: var(--s-2); margin-top: var(--s-5); }

.shortcuts { margin-top: var(--s-8); display: grid; gap: var(--s-3); width: 100%; }
dl { display: grid; grid-template-columns: max-content 1fr; gap: var(--s-2) var(--s-5); margin: 0; align-items: center; }
dt { display: flex; gap: 4px; }
dd { margin: 0; color: var(--ink-2); font-size: var(--t-small); }
</style>
