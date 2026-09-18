<script setup lang="ts">
const store = useCollectionStore()
const emit = defineEmits<{ close: [] }>()

const newName = ref('')

function has(name: string) {
  const workspace = store.workspaces.find((w) => w.name === name)
  return !!store.root && !!workspace?.collections.includes(store.root)
}

async function create() {
  const name = newName.value
  newName.value = ''
  await store.addWorkspace(name)
}

function shortPath(path: string) {
  const parts = path.split(/[\/]/).filter(Boolean)
  return parts.slice(-2).join('/')
}
</script>

<template>
  <UiDialog title="Workspaces" eyebrow="On this machine" :width="680" @close="emit('close')">
    <p class="note">
      <UiIcon name="info" :size="14" />
      <span>
        A workspace is a named set of collection folders — the three APIs at work,
        or the side project. It is a list of paths in this app's settings, so
        nothing is uploaded and nothing is shared; the collections stay where they are.
      </span>
    </p>

    <div class="add">
      <input
        v-model="newName"
        class="field"
        aria-label="New workspace name"
        placeholder="Work, Side project, Client A…"
        @keydown.enter.prevent="create"
      >
      <button type="button" class="btn btn-primary btn-sm" :disabled="!newName.trim()" @click="create">
        <UiIcon name="plus" :size="14" />New workspace
      </button>
    </div>

    <ul v-if="store.workspaces.length" class="list">
      <li v-for="workspace in store.workspaces" :key="workspace.name" :class="{ on: workspace.name === store.workspace }">
        <div class="head">
          <button type="button" class="name" @click="store.useWorkspace(workspace.name)">
            <span class="led" :class="workspace.name === store.workspace ? 'ok' : 'off'" />
            {{ workspace.name }}
          </button>
          <span class="silk">{{ workspace.collections.length }} {{ workspace.collections.length === 1 ? 'collection' : 'collections' }}</span>
          <label v-if="store.isOpen" class="include">
            <input
              type="checkbox"
              :checked="has(workspace.name)"
              @change="store.setInWorkspace(workspace.name, ($event.target as HTMLInputElement).checked)"
            >
            <span>this one</span>
          </label>
          <button type="button" class="icon-btn quiet sm" :aria-label="`Remove ${workspace.name}`" title="Remove" @click="store.removeWorkspace(workspace.name)">
            <UiIcon name="x" :size="14" />
          </button>
        </div>
        <ul class="paths">
          <li v-for="path in workspace.collections" :key="path">
            <button type="button" class="path mono" :class="{ open: path === store.root }" :title="path" @click="store.open(path)">
              {{ shortPath(path) }}
            </button>
          </li>
        </ul>
      </li>
    </ul>
    <p v-else class="empty">No workspaces yet. The one you are in now can be the first.</p>

    <template #footer>
      <button type="button" class="btn btn-quiet btn-sm" :disabled="!store.workspace" @click="store.useWorkspace(null)">
        Leave the workspace
      </button>
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

.add { display: flex; gap: var(--s-2); align-items: center; padding-bottom: var(--s-3); border-bottom: 1px solid var(--line-soft); }
.add .field { flex: 1; max-width: 320px; }

.list { list-style: none; margin: var(--s-3) 0 0; padding: 0; display: grid; gap: var(--s-3); }
.head { display: flex; align-items: center; gap: var(--s-3); }
.name { display: flex; align-items: center; gap: var(--s-2); font-size: var(--t-body); font-weight: 600; }
.include { display: flex; align-items: center; gap: 5px; margin-left: auto; font-size: var(--t-meta); color: var(--silk); cursor: pointer; }

.paths { list-style: none; margin: 4px 0 0 var(--s-4); padding: 0; display: grid; gap: 1px; }
.path { padding: 3px var(--s-2); border-radius: var(--r-sm); color: var(--silk); font-size: var(--t-meta); }
.path:hover { background: var(--hover); color: var(--ink); }
.path.open { color: var(--ink); }
.empty { margin: var(--s-6) 0; color: var(--silk); font-size: var(--t-small); text-align: center; }
</style>
