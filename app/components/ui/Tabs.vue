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
    <div v-if="$slots.start" class="start">
      <slot name="start" />
    </div>
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
      <span class="silk name">{{ item.label }}</span>
      <span v-if="item.meta !== undefined && item.meta !== null && item.meta !== ''" class="meta mono">{{ item.meta }}</span>
      <span v-if="item.dot" class="dot" :aria-label="t('has content')" />
    </button>
    <div class="end">
      <slot name="end" />
    </div>
  </div>
</template>

<style scoped>
.ui-tabs {
  display: flex;
  align-items: center;
  gap: 2px;
  height: 40px;
  padding: 0 var(--s-3);
  flex: none;
  min-width: 0;
  /* In a narrow pane the row scrolls sideways rather than cutting a tab off. */
  overflow-x: auto;
  scrollbar-width: none;
}
.ui-tabs::-webkit-scrollbar { display: none; }

/* A word; the one you are on is run over with the marker. */
.ui-tab {
  display: flex;
  align-items: center;
  gap: 6px;
  height: 26px;
  padding: 0 10px;
  border-radius: var(--r-full);
  flex: none;
  transition: background var(--dur) var(--ease);
}
.ui-tab .name { font-size: var(--t-small); font-weight: 500; color: var(--silk); transition: color var(--dur) var(--ease); }
.ui-tab:hover { background: var(--hover); }
.ui-tab:hover .name { color: var(--ink); }
.ui-tab.on { background: var(--accent-tint); }
.ui-tab.on .name { color: var(--ink); font-weight: 600; }
.ui-tab:focus-visible { outline-offset: 1px; border-radius: var(--r-full); }

.meta {
  font-size: var(--t-micro);
  font-weight: 500;
  color: var(--faint);
}
.ui-tab.on .meta { color: var(--ink-2); }
.dot { width: 5px; height: 5px; border-radius: 50%; background: var(--ink-2); }

.start { display: flex; align-items: center; gap: var(--s-2); margin-right: var(--s-2); flex: none; }
.end { margin-left: auto; display: flex; align-items: center; gap: var(--s-1); padding-left: var(--s-2); flex: none; }
</style>
