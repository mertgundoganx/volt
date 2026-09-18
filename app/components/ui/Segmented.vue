<script setup lang="ts" generic="T extends string">
import type { SegmentOption } from '~/utils/ui'

const props = withDefaults(
  defineProps<{ options: SegmentOption<T>[]; label: string; mono?: boolean; size?: 'sm' | 'md' }>(),
  { mono: false, size: 'md' },
)
const model = defineModel<T>({ required: true })
const buttons = ref<HTMLButtonElement[]>([])

function onKey(event: KeyboardEvent, index: number) {
  const step = ['ArrowRight', 'ArrowDown'].includes(event.key) ? 1 : ['ArrowLeft', 'ArrowUp'].includes(event.key) ? -1 : 0
  if (!step) return
  event.preventDefault()
  const next = (index + step + props.options.length) % props.options.length
  model.value = props.options[next]!.value
  buttons.value[next]?.focus()
}
</script>

<template>
  <div class="ui-seg" :class="[size, { mono }]" role="radiogroup" :aria-label="label">
    <button
      v-for="(option, index) in options"
      :key="option.value"
      ref="buttons"
      type="button"
      role="radio"
      class="opt"
      :class="{ on: model === option.value }"
      :aria-checked="model === option.value"
      :tabindex="model === option.value ? 0 : -1"
      @click="model = option.value"
      @keydown="onKey($event, index)"
    >
      <UiIcon v-if="option.icon" :name="option.icon" :size="14" />
      <span>{{ option.label }}</span>
    </button>
  </div>
</template>

<style scoped>
.ui-seg {
  display: inline-flex;
  flex: none;
  gap: 2px;
  padding: 3px;
  border: 1px solid var(--line);
  border-radius: var(--r-full);
  background: var(--well);
}

.opt {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 24px;
  padding: 0 var(--s-2);
  border-radius: var(--r-full);
  color: var(--ink-2);
  font-size: var(--t-small);
  font-weight: 600;
  white-space: nowrap;
  transition: background var(--dur) var(--ease), color var(--dur) var(--ease);
}
.opt:hover:not(.on) { background: var(--hover); color: var(--ink); }
.opt.on {
  background: var(--accent);
  color: var(--accent-ink);
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.3), 0 3px 12px -4px var(--accent);
}
.opt:focus-visible { outline-offset: 1px; }

.mono .opt { font-family: var(--font-mono); font-size: var(--t-meta); font-weight: 500; }
.sm .opt { height: 20px; padding: 0 7px; font-size: 11.5px; }
.sm.mono .opt { font-size: 11px; }
</style>
