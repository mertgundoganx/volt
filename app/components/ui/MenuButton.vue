<script setup lang="ts">
import type { MenuItem } from '~/utils/ui'

/** A button that opens a small menu below it. Keyboard: arrows, Home/End, Esc. */
const props = defineProps<{ items: MenuItem[]; label: string; align?: 'start' | 'end' }>()
const emit = defineEmits<{ select: [key: string] }>()

const open = ref(false)
const trigger = ref<HTMLButtonElement>()
const menu = ref<HTMLElement>()
const position = ref({ top: 0, left: 0 })
const WIDTH = 248

async function toggle() {
  if (open.value) return close()
  const box = trigger.value!.getBoundingClientRect()
  const left = props.align === 'end' ? box.right - WIDTH : box.left
  position.value = { top: box.bottom + 4, left: Math.max(8, Math.min(left, window.innerWidth - WIDTH - 8)) }
  open.value = true
  await nextTick()
  focusItem(0)
}

function close(refocus = true) {
  open.value = false
  if (refocus) trigger.value?.focus()
}

function buttons() {
  return [...(menu.value?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') ?? [])]
}

function focusItem(index: number) {
  const list = buttons()
  if (list.length) list[(index + list.length) % list.length]!.focus()
}

function onKey(event: KeyboardEvent) {
  const list = buttons()
  const current = list.indexOf(document.activeElement as HTMLButtonElement)
  const moves: Record<string, number> = { ArrowDown: current + 1, ArrowUp: current - 1, Home: 0, End: -1 }
  if (event.key in moves) {
    event.preventDefault()
    focusItem(moves[event.key]!)
  } else if (event.key === 'Escape' || event.key === 'Tab') {
    event.preventDefault()
    close()
  }
}

function choose(item: MenuItem) {
  close(false)
  emit('select', item.key)
}
</script>

<template>
  <button
    ref="trigger"
    type="button"
    class="btn btn-quiet btn-sm ui-menu-trigger"
    :aria-label="label"
    aria-haspopup="menu"
    :aria-expanded="open"
    @click="toggle"
  >
    <slot>{{ label }}</slot>
    <UiIcon name="chevron-down" :size="12" class="chev" />
  </button>

  <div v-if="open" class="catcher" @mousedown.self="close(false)">
    <div
      ref="menu"
      class="menu"
      role="menu"
      :aria-label="label"
      :style="{ top: `${position.top}px`, left: `${position.left}px`, width: `${WIDTH}px` }"
      @keydown="onKey"
    >
      <template v-for="item in items" :key="item.key">
        <div v-if="item.divided" class="sep" role="separator" />
        <button type="button" role="menuitem" class="item" :disabled="item.disabled" @click="choose(item)">
          <UiIcon v-if="item.icon" :name="item.icon" :size="15" />
          <span class="text">
            <span class="name">{{ item.label }}</span>
            <span v-if="item.hint" class="hint">{{ item.hint }}</span>
          </span>
        </button>
      </template>
    </div>
  </div>
</template>

<style scoped>
.ui-menu-trigger .chev { color: var(--silk); margin-left: -2px; }

.catcher { position: fixed; inset: 0; z-index: 60; }

.menu {
  position: fixed;
  display: flex;
  flex-direction: column;
  padding: 5px;
  background: var(--bg-3);
  border: 1px solid var(--line);
  border-radius: var(--r-md);
  box-shadow: var(--rim), var(--shadow-pop);
  animation: pop 130ms var(--ease);
}

.item {
  display: flex;
  align-items: flex-start;
  gap: 9px;
  width: 100%;
  padding: 7px 8px;
  border-radius: var(--r-sm);
  text-align: left;
  color: var(--ink);
}
.item .ui-icon { color: var(--ink-2); margin-top: 1px; }
.item:hover:not(:disabled), .item:focus-visible { background: var(--accent-tint); color: var(--ink); outline: none; }
.item:hover:not(:disabled) .ui-icon, .item:focus-visible .ui-icon { color: var(--accent-text); }
.item:disabled { color: var(--faint); }
.text { display: grid; gap: 2px; min-width: 0; }
.name { font-weight: 550; }
.hint { font-size: 11.5px; color: var(--silk); line-height: 1.35; }

.sep { height: 1px; margin: 5px 4px; background: var(--line-soft); }

@keyframes pop { from { opacity: 0; transform: translateY(-6px) scale(0.97); } }
</style>
