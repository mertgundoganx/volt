<script setup lang="ts">
const props = defineProps<{ parent: string | null; depth: number }>()
const store = useCollectionStore()
const { stopCreatingFolder } = useTreeMenu()

// Enter and blur can both fire for one commit; only act on the first.
let done = false

function focusInput(el: Element | ComponentPublicInstance | null) {
  ;(el as HTMLInputElement | null)?.focus()
}

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
      :ref="focusInput"
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
