<script setup lang="ts">
import type { Node } from '~/types'

const props = defineProps<{ nodes: Node[]; depth: number }>()
const store = useCollectionStore()
const { renamingId, creatingFolder, openAt, openBelow, startRenaming, stopRenaming } = useTreeMenu()
const { dragId, dropTarget, start, over, end, markerFor } = useTreeDrag()
const collapsed = ref(new Set<string>())

/** Tree rows show the short form; colour still comes from the full method. */
const SHORT: Record<string, string> = { DELETE: 'DEL', OPTIONS: 'OPT' }
const short = (method: string) => SHORT[method] ?? method

const indent = computed(() => `${12 + props.depth * 16}px`)

function onDrop(targetId: string) {
  const from = dragId.value
  const target = dropTarget.value
  end()
  if (from && target?.id === targetId) store.drop(from, targetId, target.position)
}

function toggle(id: string) {
  const next = new Set(collapsed.value)
  next.has(id) ? next.delete(id) : next.add(id)
  collapsed.value = next
}

/** Select the text so typing replaces the old name straight away. */
function focusInput(el: Element | ComponentPublicInstance | null) {
  const input = el as HTMLInputElement | null
  if (!input) return
  input.focus()
  input.select()
}

async function commitRename(id: string, value: string, previous: string) {
  // Removing the input fires blur after Enter or Escape has already handled
  // it. Without this, Enter renames twice and Escape saves instead of cancelling.
  if (renamingId.value !== id) return
  stopRenaming()
  if (value.trim() && value.trim() !== previous) await store.rename(id, value)
}

/** Requests anywhere inside a folder, for the count beside its name. */
function requestCount(node: Node) {
  return node.kind === 'folder' ? store.countNodes(node.children).requests : 0
}
</script>

<template>
  <!-- The guide lines up with the parent's chevron: row margin + indent + half the icon. -->
  <ul class="tree" :class="{ nested: depth > 0 }" :style="{ '--guide': `${6 + 12 + (depth - 1) * 16 + 6}px` }">
    <li v-for="node in nodes" :key="node.id">
      <template v-if="node.kind === 'folder'">
        <div
          class="row folder"
          :class="[markerFor(node.id), { dragging: dragId === node.id }]"
          :style="{ paddingLeft: indent }"
          :draggable="renamingId !== node.id"
          :aria-expanded="!collapsed.has(node.id)"
          @click="toggle(node.id)"
          @contextmenu="openAt($event, node)"
          @dragstart.stop="start($event, node.id)"
          @dragend="end()"
          @dragover="over($event, node.id, true)"
          @drop.prevent="onDrop(node.id)"
        >
          <UiIcon :name="collapsed.has(node.id) ? 'chevron-right' : 'chevron-down'" :size="12" class="chev" />
          <UiIcon :name="collapsed.has(node.id) ? 'folder' : 'folder-open'" :size="14" class="folder-glyph" />

          <input
            v-if="renamingId === node.id"
            :ref="focusInput"
            class="field rename"
            :value="node.name"
            aria-label="Folder name"
            @click.stop
            @keydown.stop.enter="commitRename(node.id, ($event.target as HTMLInputElement).value, node.name)"
            @keydown.esc="stopRenaming()"
            @blur="commitRename(node.id, ($event.target as HTMLInputElement).value, node.name)"
          >
          <span v-else class="label" @dblclick.stop="startRenaming(node.id)">{{ node.name }}</span>

          <span class="count mono num">{{ requestCount(node) }}</span>
          <span class="tools">
            <button type="button" class="icon-btn quiet sm" aria-label="New request here" title="New request here" @click.stop="store.createRequest(node.id)">
              <UiIcon name="plus" :size="14" />
            </button>
            <button type="button" class="icon-btn quiet sm" :aria-label="`More for ${node.name}`" title="More" @click.stop="openBelow($event.currentTarget as HTMLElement, node)">
              <UiIcon name="more" :size="14" />
            </button>
          </span>
        </div>

        <!-- Shown even when collapsed, so the input is never created out of sight. -->
        <NewFolderInput v-if="creatingFolder?.parent === node.id" :parent="node.id" :depth="depth + 1" />
        <TreeNode v-if="!collapsed.has(node.id) && node.children.length" :nodes="node.children" :depth="depth + 1" />
      </template>

      <div
        v-else
        class="row request"
        :class="[markerFor(node.id), { active: store.activeId === node.id, dragging: dragId === node.id }]"
        :style="{ paddingLeft: indent }"
        :draggable="renamingId !== node.id"
        :aria-current="store.activeId === node.id ? 'true' : undefined"
        :title="node.id"
        @click="store.select(node.id)"
        @contextmenu="openAt($event, node)"
        @dragstart.stop="start($event, node.id)"
        @dragend="end()"
        @dragover="over($event, node.id, false)"
        @drop.prevent="onDrop(node.id)"
      >
        <span class="method" :data-method="node.method">{{ short(node.method) }}</span>

        <input
          v-if="renamingId === node.id"
          :ref="focusInput"
          class="field rename"
          :value="node.name"
          aria-label="Request name"
          @click.stop
          @keydown.stop.enter="commitRename(node.id, ($event.target as HTMLInputElement).value, node.name)"
          @keydown.esc="stopRenaming()"
          @blur="commitRename(node.id, ($event.target as HTMLInputElement).value, node.name)"
        >
        <span v-else class="label" @dblclick.stop="startRenaming(node.id)">{{ node.name }}</span>

        <span class="tools">
          <button type="button" class="icon-btn quiet sm" :aria-label="`More for ${node.name}`" title="More" @click.stop="openBelow($event.currentTarget as HTMLElement, node)">
            <UiIcon name="more" :size="14" />
          </button>
        </span>
      </div>
    </li>
  </ul>
</template>

<style scoped>
.tree { list-style: none; margin: 0; padding: 0; position: relative; }

/* An indent guide under each open folder. */
.tree.nested::before {
  content: "";
  position: absolute;
  left: var(--guide);
  top: 0;
  bottom: 4px;
  border-left: 1px solid var(--line);
  pointer-events: none;
}

.row {
  position: relative;
  display: flex;
  align-items: center;
  gap: 6px;
  height: var(--h-row);
  margin: 0 6px;
  padding-right: 4px;
  border-radius: var(--r-sm);
  color: var(--ink-2);
  transition: background var(--dur) var(--ease), color var(--dur) var(--ease);
}
.row:hover { background: var(--hover); color: var(--ink); }

.folder { color: var(--ink); }
.folder .label { font-weight: 500; }
.chev { color: var(--faint); margin-right: -2px; }
.folder-glyph { color: var(--silk); }

.request .method { width: 38px; flex: none; text-align: left; margin-left: 20px; }

/* The request that is open. */
.request.active { background: var(--accent-tint); color: var(--ink); }
.request.active .label { font-weight: 500; color: var(--accent-text); }

.label { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--t-small); }

.count { color: var(--faint); font-size: var(--t-micro); padding-right: 4px; }

.tools { display: none; gap: 1px; }
.row:hover .tools, .row:focus-within .tools { display: flex; }
.row:hover .count, .row:focus-within .count { display: none; }
/* While renaming, the field gets the whole row. */
.row:has(.rename) .tools { display: none; }

.rename { flex: 1; min-width: 0; height: 22px; padding: 0 6px; font-size: var(--t-small); }

/* Drag and drop: a line where the row will land, a fill to go inside. */
.row.dragging { opacity: 0.4; }
.row.drop-before::after,
.row.drop-after::after {
  content: "";
  position: absolute;
  left: 4px;
  right: 4px;
  height: 2px;
  background: var(--accent);
  pointer-events: none;
}
.row.drop-before::before,
.row.drop-after::before {
  content: "";
  position: absolute;
  left: 0;
  width: 6px;
  height: 6px;
  border: 2px solid var(--accent);
  border-radius: 50%;
  background: var(--bg-0);
  pointer-events: none;
}
.row.drop-before::after { top: -1px; }
.row.drop-before::before { top: -3px; }
.row.drop-after::after { bottom: -1px; }
.row.drop-after::before { bottom: -3px; }
.row.drop-inside { background: var(--accent-tint); box-shadow: inset 0 0 0 1.5px var(--accent); }
</style>
