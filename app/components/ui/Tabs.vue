<script setup lang="ts">
import type { TabItem } from '~/utils/ui'

const props = defineProps<{ items: TabItem[]; label: string }>()
const model = defineModel<string>({ required: true })
const buttons = ref<HTMLButtonElement[]>([])

/** Arrow keys move between tabs, as in any native tab strip. */
function onKey(event: KeyboardEvent, index: number) {
  const step = event.key === 'ArrowRight' ? 1 : event.key === 'ArrowLeft' ? -1 : 0
  if (!step) return
  event.preventDefault()
  const next = (index + step + props.items.length) % props.items.length
  model.value = props.items[next]!.key
  buttons.value[next]?.focus()
}
</script>

<template>
  <div class="ui-tabs" role="tablist" :aria-label="label">
    <button
      v-for="(item, index) in items"
      :key="item.key"
      ref="buttons"
      type="button"
      role="tab"
      class="ui-tab"
      :class="{ on: model === item.key }"
      :aria-selected="model === item.key"
      :tabindex="model === item.key ? 0 : -1"
      @click="model = item.key"
      @keydown="onKey($event, index)"
    >
      <span class="silk">{{ item.label }}</span>
      <span v-if="item.meta !== undefined && item.meta !== null && item.meta !== ''" class="meta mono">{{ item.meta }}</span>
      <span v-if="item.dot" class="dot" aria-label="has content" />
    </button>
    <div class="end">
      <slot name="end" />
    </div>
  </div>
</template>

<style scoped>
.ui-tabs {
  display: flex;
  align-items: stretch;
  gap: var(--s-5);
  height: 38px;
  padding: 0 var(--s-5);
  border-bottom: 1px solid var(--line);
  flex: none;
}

.ui-tab {
  position: relative;
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 0 1px;
  border-radius: 0;
}
/* The live tab is underlined in the accent, and the line glows a little. */
.ui-tab::after {
  content: "";
  position: absolute;
  left: -2px;
  right: -2px;
  bottom: -1px;
  height: 2px;
  border-radius: var(--r-full) var(--r-full) 0 0;
  background: transparent;
  transition: background var(--dur) var(--ease), box-shadow var(--dur) var(--ease);
}
.ui-tab:hover .silk { color: var(--ink-2); }
.ui-tab.on .silk { color: var(--ink); }
.ui-tab.on::after { background: var(--accent); box-shadow: 0 0 10px -1px var(--accent); }
.ui-tab:focus-visible { outline-offset: 3px; border-radius: var(--r-xs); }

.meta {
  display: inline-flex;
  align-items: center;
  height: 16px;
  padding: 0 5px;
  border-radius: var(--r-full);
  background: var(--hover);
  font-size: 10px;
  font-weight: 550;
  color: var(--silk);
}
.ui-tab.on .meta { color: var(--accent-text); background: var(--accent-tint); }
.dot { width: 5px; height: 5px; border-radius: 50%; background: var(--accent); box-shadow: 0 0 8px -1px var(--accent); }

.end { margin-left: auto; display: flex; align-items: center; gap: var(--s-2); }
</style>
