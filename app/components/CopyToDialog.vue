<script setup lang="ts">
/**
 * Put a request into another collection: one of the ones opened lately. A
 * copy lands at the other collection's root; a move also puts this one in
 * the bin, where it can be got back.
 */
const store = useCollectionStore()
const emit = defineEmits<{ close: [] }>()

const target = computed(() => store.copyToDialog)
const move = ref(false)
const busy = ref(false)

/** The last path segment names the folder; the rest is where it lives. */
function splitPath(path: string) {
  const parts = path.split(/[\\/]/).filter(Boolean)
  const name = parts.pop() ?? path
  return { name, parent: parts.join(path.includes('\\') ? '\\' : '/') }
}

const choices = computed(() => store.recent.filter((path) => path !== store.root).map((path) => ({ path, ...splitPath(path) })))

async function pick(path: string) {
  if (!target.value || busy.value) return
  busy.value = true
  const done = await store.copyRequestTo(target.value.id, path, move.value)
  busy.value = false
  if (done) emit('close')
}
</script>

<template>
  <UiDialog :title="move ? 'Move to another collection' : 'Copy to another collection'" :eyebrow="target?.name ?? 'Request'" :width="520" @close="emit('close')">
    <p class="note">
      <span>
        The request lands at that collection's root, as its own file. Open it from there to put it in a folder.
      </span>
    </p>

    <label class="switch-row">
      <input v-model="move" type="checkbox" class="switch" aria-label="Move instead of copying">
      <span>Move — put this one in the bin afterwards</span>
    </label>

    <ul v-if="choices.length" class="list" aria-label="Collections">
      <li v-for="item in choices" :key="item.path">
        <button type="button" class="line" :title="item.path" :disabled="busy" @click="pick(item.path)">
          <UiIcon name="folder" :size="14" class="glyph" />
          <span class="main">{{ item.name }}</span>
          <span class="side mono">{{ item.parent }}</span>
        </button>
      </li>
    </ul>
    <p v-else class="empty">No other collection has been opened yet. Open one first; it will be listed here.</p>

    <template #footer>
      <span class="spacer" />
      <button type="button" class="btn btn-quiet" @click="emit('close')">Cancel</button>
    </template>
  </UiDialog>
</template>

<style scoped>
.note { display: flex; margin: 0 0 var(--s-3); color: var(--silk); font-size: var(--t-small); line-height: 1.5; }
.switch-row { display: flex; align-items: center; gap: var(--s-2); margin-bottom: var(--s-3); font-size: var(--t-small); cursor: pointer; }
.list { list-style: none; margin: 0; padding: 0; border-top: 1px solid var(--line); }
.list li { border-bottom: 1px solid var(--line-soft); }
.line { display: flex; align-items: center; gap: var(--s-3); width: 100%; height: 36px; padding: 0 var(--s-2); text-align: left; color: var(--ink-2); }
.line:hover:not(:disabled) { background: var(--hover); color: var(--ink); }
.line .glyph { color: var(--silk); }
.line .main { flex: none; font-weight: 500; color: var(--ink); }
.line .side { flex: 1; min-width: 0; color: var(--faint); font-size: var(--t-meta); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.empty { margin: var(--s-4) 0; color: var(--silk); font-size: var(--t-small); }
</style>
