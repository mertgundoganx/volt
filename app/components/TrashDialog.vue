<script setup lang="ts">
import type { Deleted } from '~/types'

const store = useCollectionStore()
const emit = defineEmits<{ close: [] }>()

const items = ref<Deleted[]>([])
const loading = ref(true)

async function refresh() {
  items.value = await store.loadTrash()
  loading.value = false
}
onMounted(refresh)

async function restore(deleted: Deleted) {
  if (await store.restoreDeleted(deleted)) await refresh()
}

async function empty() {
  if (await store.emptyTrash()) await refresh()
}

/** `.trash/1737052800000-list.yaml` → when it went. */
function when(at: string) {
  const stamp = Number(at.split('/').pop()?.split('-')[0] ?? 0)
  return stamp ? new Date(stamp).toLocaleString() : ''
}
</script>

<template>
  <UiDialog title="Bin" eyebrow="This collection" :width="680" @close="emit('close')">
    <p class="note">
      <UiIcon name="trash" :size="14" />
      <span>
        A deleted request or folder is moved here rather than removed, so it can be
        put back. The bin is a gitignored <code>.trash/</code> folder inside the
        collection; emptying it is the only thing that actually deletes.
      </span>
    </p>

    <p v-if="loading" class="empty">Looking…</p>
    <p v-else-if="!items.length" class="empty">Nothing has been deleted.</p>

    <ul v-else class="items">
      <li v-for="item in items" :key="item.at">
        <UiIcon :name="item.folder ? 'folder' : 'file'" :size="14" class="what" />
        <span class="name">{{ item.name }}</span>
        <span class="at silk">{{ when(item.at) }}</span>
        <button type="button" class="btn btn-quiet btn-sm" @click="restore(item)">Put it back</button>
      </li>
    </ul>

    <template #footer>
      <button type="button" class="btn btn-quiet btn-sm" :disabled="!items.length" @click="empty">
        <UiIcon name="trash" :size="14" />Empty the bin
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
.empty { margin: var(--s-6) 0; color: var(--silk); font-size: var(--t-small); text-align: center; }

.items { list-style: none; margin: 0; padding: 0; }
.items li {
  display: grid;
  grid-template-columns: 18px minmax(0, 1fr) auto auto;
  gap: var(--s-3);
  align-items: center;
  padding: 6px 0;
  border-bottom: 1px solid var(--line-soft);
  font-size: var(--t-small);
}
.what { color: var(--silk); }
.name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.at { font-size: var(--t-meta); white-space: nowrap; }
</style>
