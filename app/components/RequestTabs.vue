<script setup lang="ts">
const store = useCollectionStore()

// The active tab's values live in the store's own fields, so read those for it
// and the snapshot for every other one.
function requestOf(index: number) {
  return index === store.activeTab ? store.request : store.tabs[index]?.request
}

const nameOf = (index: number) => requestOf(index)?.name?.trim() || 'Untitled'
const methodOf = (index: number) => requestOf(index)?.method?.toUpperCase() ?? 'GET'

function isDirty(index: number) {
  return index === store.activeTab ? store.dirty : !!store.tabs[index]?.dirty
}

function isReplay(index: number) {
  return index === store.activeTab ? !!store.historyEntry : !!store.tabs[index]?.historyEntry
}

useShortcut('mod+w', 'Close the request', () => store.closeTab(store.activeTab))
</script>

<template>
  <div v-if="store.isOpen" class="tabs" role="tablist" aria-label="Open requests">
    <div v-for="(tab, i) in store.tabs" :key="i" class="tab" :class="{ on: i === store.activeTab }">
      <button
        type="button"
        role="tab"
        class="face"
        :aria-selected="i === store.activeTab"
        :title="tab.id ?? 'Not saved to a file'"
        @click="store.switchTab(i)"
        @auxclick.middle.prevent="store.closeTab(i)"
      >
        <span class="method" :data-method="methodOf(i)">{{ methodOf(i) }}</span>
        <span class="label">{{ nameOf(i) }}</span>
        <UiIcon v-if="isReplay(i)" name="history" :size="12" class="replay" />
      </button>
      <button
        type="button"
        class="close"
        :aria-label="`Close ${nameOf(i)}`"
        :title="isDirty(i) ? 'Unsaved changes' : 'Close'"
        @click="store.closeTab(i)"
      >
        <span v-if="isDirty(i)" class="dot" />
        <UiIcon v-else name="x" :size="12" />
      </button>
    </div>
    <button type="button" class="add" aria-label="New request" title="New request" @click="store.createRequest(null)">
      <UiIcon name="plus" :size="14" />
    </button>
  </div>
</template>

<style scoped>
.tabs {
  display: flex;
  align-items: stretch;
  height: 36px;
  padding: 0 var(--s-2) 0 0;
  border-bottom: 1px solid var(--line);
  background: var(--bg-0);
  overflow-x: auto;
  scrollbar-width: none;
  flex: none;
}
.tabs::-webkit-scrollbar { display: none; }

.tab {
  position: relative;
  display: flex;
  align-items: center;
  flex: none;
  max-width: 220px;
  margin-bottom: -1px;
  border-right: 1px solid var(--line);
  color: var(--silk);
  transition: background var(--dur) var(--ease), color var(--dur) var(--ease);
}
.tab:hover { background: var(--hover); color: var(--ink-2); }
/* The open request's tab is the surface itself, joined to the pane below. */
.tab.on {
  background: var(--bg-1);
  color: var(--ink);
  border-bottom: 1px solid var(--bg-1);
}
.tab.on::before {
  content: "";
  position: absolute;
  left: 0;
  right: 0;
  top: 0;
  height: 2px;
  background: var(--accent);
}

.face {
  display: flex;
  align-items: center;
  gap: var(--s-2);
  min-width: 0;
  height: 100%;
  padding: 0 4px 0 var(--s-3);
  color: inherit;
}
.label { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--t-small); }
.tab.on .label { font-weight: 500; }
.replay { flex: none; color: var(--faint); }

.close {
  display: grid;
  place-items: center;
  width: 20px;
  height: 20px;
  margin-right: 6px;
  border-radius: var(--r-xs);
  color: var(--faint);
}
.close:hover { background: var(--press); color: var(--ink); }
.dot { width: 7px; height: 7px; border-radius: 50%; background: var(--warn); }

.add {
  display: grid;
  place-items: center;
  width: 32px;
  flex: none;
  color: var(--silk);
}
.add:hover { color: var(--ink); background: var(--hover); }
</style>
