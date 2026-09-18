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
  <div v-if="store.tabs.length" class="tabs" role="tablist" aria-label="Open requests">
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
  </div>
</template>

<style scoped>
.tabs {
  display: flex;
  align-items: stretch;
  gap: 2px;
  padding: 5px var(--s-3) 0;
  border-bottom: 1px solid var(--line);
  background: var(--bg-0);
  overflow-x: auto;
  scrollbar-width: none;
}
.tabs::-webkit-scrollbar { display: none; }

.tab {
  position: relative;
  display: flex;
  align-items: center;
  flex: none;
  max-width: 230px;
  border: 1px solid transparent;
  border-bottom: 0;
  border-radius: var(--r-sm) var(--r-sm) 0 0;
  color: var(--silk);
  transition: background var(--dur) var(--ease), color var(--dur) var(--ease);
}
.tab:hover { background: var(--hover); color: var(--ink-2); }
/* The open request's tab is the panel itself, pulled up over the rule, with the
   current running along its top edge. */
.tab.on {
  background: var(--bg-1);
  border-color: var(--line);
  color: var(--ink);
}
.tab.on::after {
  content: "";
  position: absolute;
  left: -1px;
  right: -1px;
  top: -1px;
  height: 2px;
  border-radius: var(--r-full);
  background: var(--accent);
  box-shadow: 0 0 10px -1px var(--accent);
}

.face {
  display: flex;
  align-items: center;
  gap: var(--s-2);
  min-width: 0;
  padding: 8px var(--s-2) 8px var(--s-3);
  color: inherit;
}
.label { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--t-small); }
.tab.on .label { font-weight: 600; }
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
.dot { width: 6px; height: 6px; border-radius: 50%; background: var(--warn); box-shadow: 0 0 8px -1px var(--warn); }
</style>
