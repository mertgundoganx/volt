<script setup lang="ts">
/**
 * The left edge of the window: the mark, and one key per section. The two
 * that switch the sidebar are tabs; the two that open a sheet are buttons.
 */
const emit = defineEmits<{ editEnvironment: []; openSettings: [] }>()
const store = useCollectionStore()
</script>

<template>
  <nav class="rail" aria-label="Sections">
    <div class="mark" title="volt" aria-hidden="true">
      <UiIcon name="bolt" :size="22" />
    </div>

    <div class="group" role="tablist" aria-label="Sidebar">
      <button
        type="button"
        role="tab"
        class="key"
        :class="{ on: store.sidebarTab === 'tree' }"
        :aria-selected="store.sidebarTab === 'tree'"
        aria-label="Collection"
        @click="store.sidebarTab = 'tree'"
      >
        <UiIcon name="requests" :size="18" />
        <span>Collection</span>
      </button>
      <button
        type="button"
        role="tab"
        class="key"
        :class="{ on: store.sidebarTab === 'history' }"
        :aria-selected="store.sidebarTab === 'history'"
        aria-label="History"
        @click="store.sidebarTab = 'history'"
      >
        <UiIcon name="history" :size="18" />
        <span>History</span>
      </button>
    </div>

    <button type="button" class="key" aria-label="Environments" :disabled="!store.isOpen" @click="emit('editEnvironment')">
      <UiIcon name="globe" :size="18" />
      <span>Env</span>
    </button>

    <span class="spacer" />

    <button type="button" class="key" aria-label="Keyboard shortcuts" title="Every shortcut — or press ?" @click="store.shortcutsSheet = true">
      <UiIcon name="keyboard" :size="18" />
      <span>Keys</span>
    </button>
    <button type="button" class="key" aria-label="Settings" @click="emit('openSettings')">
      <UiIcon name="sliders" :size="18" />
      <span>Settings</span>
    </button>
  </nav>
</template>

<style scoped>
.rail {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2px;
  width: var(--w-rail);
  padding: 0 0 var(--s-2);
  background: var(--bg-0);
  border-right: 1px solid var(--line);
  flex: none;
}

.mark {
  display: grid;
  place-items: center;
  width: 100%;
  height: var(--h-bar);
  color: var(--accent);
  flex: none;
}

.group { display: flex; flex-direction: column; gap: 2px; width: 100%; align-items: center; }

.key {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
  width: 52px;
  padding: 8px 0 7px;
  border-radius: var(--r-sm);
  color: var(--silk);
  font-size: 10px;
  font-weight: 500;
  line-height: 1;
  transition: background var(--dur) var(--ease), color var(--dur) var(--ease);
}
.key:hover:not(:disabled) { background: var(--hover); color: var(--ink); }
.key.on { background: var(--accent-tint); color: var(--accent-text); }
.key:disabled { opacity: 0.4; }
.key:focus-visible { outline-offset: -2px; }
</style>
