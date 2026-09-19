<script setup lang="ts">
const store = useCollectionStore()
const { target, close, startRenaming, startCreatingFolder } = useTreeMenu()

const menu = ref<HTMLElement>()
const node = computed(() => target.value?.node ?? null)
const here = computed(() => (node.value ? parentOf(node.value.id) : null))

/**
 * Every folder plus the collection root. A target is disabled when moving there
 * would do nothing, or when it would put a folder inside itself.
 */
const targets = computed(() => {
  const current = node.value
  if (!current) return []

  const rows = [
    { id: null as string | null, label: 'Collection root', depth: 0 },
    ...store.folders.map((f) => ({ id: f.id as string | null, label: f.name, depth: f.depth + 1 })),
  ]

  return rows.map((row) => {
    let reason = ''
    if (row.id === here.value) {
      reason = 'here'
    } else if (current.kind === 'folder' && row.id !== null) {
      if (row.id === current.id) reason = 'itself'
      else if (row.id.startsWith(`${current.id}/`)) reason = 'inside'
    }
    return { ...row, reason }
  })
})

const WIDTH = 232

// Keep the menu inside the window when it opens near an edge.
const position = ref({ left: 0, top: 0 })
watch(target, async (at) => {
  if (!at) return
  position.value = { left: at.x, top: at.y }
  await nextTick()
  const height = menu.value?.offsetHeight ?? 0
  position.value = {
    left: Math.max(8, Math.min(at.x, window.innerWidth - WIDTH - 8)),
    top: Math.max(8, Math.min(at.y, window.innerHeight - height - 8)),
  }
  focusItem(0)
})

function items() {
  return [...(menu.value?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') ?? [])]
}

function focusItem(index: number) {
  const list = items()
  if (list.length) list[(index + list.length) % list.length]!.focus()
}

function onKey(event: KeyboardEvent) {
  const list = items()
  const current = list.indexOf(document.activeElement as HTMLButtonElement)
  if (event.key === 'ArrowDown') {
    event.preventDefault()
    focusItem(current + 1)
  } else if (event.key === 'ArrowUp') {
    event.preventDefault()
    focusItem(current - 1)
  } else if (event.key === 'Home') {
    event.preventDefault()
    focusItem(0)
  } else if (event.key === 'End') {
    event.preventDefault()
    focusItem(-1)
  } else if (event.key === 'Escape' || event.key === 'Tab') {
    event.preventDefault()
    close()
  }
}

async function moveTo(parent: string | null) {
  const current = node.value
  close()
  if (current) await store.moveNode(current.id, parent)
}

async function newRequestIn(folder: string) {
  close()
  await store.createRequest(folder)
}

function newRequestFromCurl(folder: string) {
  close()
  store.curlDialog = { mode: 'create', parent: folder }
}

async function duplicate(id: string) {
  close()
  await store.duplicate(id)
}
async function copyCurl(id: string) {
  close()
  await store.copyCurlFor(id)
}

function folderSettings(id: string, name: string) {
  close()
  store.scopeDialog = { id, title: name }
}

async function remove() {
  const current = node.value
  close()
  if (current) await store.remove(current.id)
}
</script>

<template>
  <div v-if="node" class="catcher" @mousedown.self="close" @contextmenu.prevent="close">
    <div
      ref="menu"
      class="menu"
      role="menu"
      :aria-label="`Actions for ${node.name}`"
      :style="{ left: `${position.left}px`, top: `${position.top}px`, width: `${WIDTH}px` }"
      @keydown="onKey"
    >
      <div class="heading">
        <span class="method" v-if="node.kind === 'request'" :data-method="node.method">{{ node.method }}</span>
        <UiIcon v-else name="folder" :size="13" />
        <span class="title">{{ node.name }}</span>
      </div>

      <template v-if="node.kind === 'folder'">
        <button type="button" role="menuitem" class="item" @click="newRequestIn(node.id)">
          <UiIcon name="file-plus" :size="15" /><span>New request here</span>
        </button>
        <button type="button" role="menuitem" class="item" @click="newRequestFromCurl(node.id)">
          <UiIcon name="import" :size="15" /><span>New request from cURL…</span>
        </button>
        <button type="button" role="menuitem" class="item" @click="startCreatingFolder(node.id)">
          <UiIcon name="folder-plus" :size="15" /><span>New folder here</span>
        </button>
        <button type="button" role="menuitem" class="item" @click="folderSettings(node.id, node.name)">
          <UiIcon name="sliders" :size="15" /><span>Folder settings…</span><span class="hint">headers, auth, variables</span>
        </button>
        <div class="sep" role="separator" />
      </template>

      <template v-else>
        <button type="button" role="menuitem" class="item" @click="duplicate(node.id)">
          <UiIcon name="copy" :size="15" /><span>Duplicate</span><span class="hint">{{ modKey }} D</span>
        </button>
        <button type="button" role="menuitem" class="item" @click="copyCurl(node.id)">
          <UiIcon name="copy" :size="15" /><span>Copy as cURL</span><span class="hint">secrets hidden</span>
        </button>
        <div class="sep" role="separator" />
      </template>

      <button type="button" role="menuitem" class="item" @click="startRenaming(node.id)">
        <UiIcon name="pencil" :size="15" /><span>Rename</span><span class="hint">Double-click</span>
      </button>

      <div class="sep" role="separator" />
      <div class="group silk">Move to</div>
      <div class="targets">
        <button
          v-for="row in targets"
          :key="row.id ?? '/'"
          type="button"
          role="menuitem"
          class="item target"
          :disabled="!!row.reason"
          :title="row.reason === 'here' ? 'Already here' : row.reason ? 'A folder cannot go inside itself' : ''"
          :style="{ paddingLeft: `${8 + row.depth * 14}px` }"
          @click="moveTo(row.id)"
        >
          <UiIcon :name="row.id === null ? 'collection' : 'folder'" :size="14" />
          <span class="name">{{ row.label }}</span>
          <span v-if="row.reason === 'here'" class="hint">current</span>
        </button>
      </div>

      <div class="sep" role="separator" />
      <button type="button" role="menuitem" class="item danger" @click="remove">
        <UiIcon name="trash" :size="15" /><span>Delete…</span>
      </button>
    </div>
  </div>
</template>

<style scoped>
.catcher { position: fixed; inset: 0; z-index: 60; }

.menu {
  position: fixed;
  display: flex;
  flex-direction: column;
  max-height: min(70vh, 520px);
  padding: 5px;
  background: var(--bg-3);
  border: 1px solid var(--line);
  border-radius: var(--r-md);
  box-shadow: var(--shadow-pop);
  animation: pop 130ms var(--ease);
}

.heading {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 6px 8px 8px;
  color: var(--silk);
  min-width: 0;
}
.heading .title { font-weight: 600; color: var(--ink); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

.item {
  display: flex;
  align-items: center;
  gap: 9px;
  width: 100%;
  height: 30px;
  padding: 0 8px;
  border-radius: var(--r-sm);
  text-align: left;
  color: var(--ink);
  flex: none;
}
.item .ui-icon { color: var(--ink-2); }
.item:hover:not(:disabled), .item:focus-visible { background: var(--accent-tint); outline: none; }
.item:hover:not(:disabled) .ui-icon, .item:focus-visible .ui-icon { color: var(--accent-text); }
.item:disabled { color: var(--faint); }
.item:disabled .ui-icon { color: var(--faint); }
.item span:not(.hint) { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.hint { font-size: 11px; color: var(--silk); flex: none; }
.item.danger, .item.danger .ui-icon { color: var(--bad); }
.item.danger:hover { background: var(--bad-tint); }
.item.danger:hover .ui-icon { color: var(--bad); }

.group { padding: 8px 8px 5px; }
.targets { overflow: auto; min-height: 0; }
.target { height: 28px; }

.sep { height: 1px; margin: 5px 4px; background: var(--line-soft); flex: none; }

@keyframes pop { from { opacity: 0; transform: translateY(-6px) scale(0.97); } }
</style>
