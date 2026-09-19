<script setup lang="ts">
const props = defineProps<{ parent: string | null; depth: number }>()
const store = useCollectionStore()
const { stopCreatingFolder } = useTreeMenu()

// Enter and blur can both fire for one commit; only act on the first.
let done = false

// Focused after mount, not through a function ref: Vue calls a function ref
// while the element's subtree is still being assembled, before it is in the
// document, and focus() on a detached input does nothing. The field then sat
// there with the caret nowhere, and everything typed went to the page.
const input = ref<HTMLInputElement>()
onMounted(() => input.value?.focus())

async function commit(value: string) {
  if (done) return
  done = true
  stopCreatingFolder()
  if (value.trim()) await store.createFolder(props.parent, value)
}

function cancel() {
  done = true
  stopCreatingFolder()
}
</script>

<template>
  <div class="new-folder" :style="{ paddingLeft: `${12 + depth * 16}px` }">
    <UiIcon name="folder-plus" :size="14" />
    <input
      ref="input"
      class="field"
      placeholder="Folder name"
      aria-label="New folder name"
      @keydown.enter="commit(($event.target as HTMLInputElement).value)"
      @keydown.esc="cancel"
      @blur="commit(($event.target as HTMLInputElement).value)"
    >
  </div>
</template>

<style scoped>
.new-folder {
  display: flex;
  align-items: center;
  gap: 7px;
  height: 32px;
  margin: 0 var(--s-2);
  padding-right: 4px;
  color: var(--silk);
}
.field { height: 24px; padding: 0 7px; }
</style>
