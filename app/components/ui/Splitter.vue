<script setup lang="ts">
/**
 * A drag handle between two panes. It only reports movement; the parent owns
 * the sizes and their limits. `x` sits between columns, `y` between rows.
 */
const props = defineProps<{ axis: 'x' | 'y'; label: string }>()
const emit = defineEmits<{ drag: [delta: number]; reset: [] }>()

const active = ref(false)
let last = 0

function start(event: PointerEvent) {
  const target = event.currentTarget as HTMLElement
  target.setPointerCapture(event.pointerId)
  active.value = true
  last = props.axis === 'x' ? event.clientX : event.clientY
}

function move(event: PointerEvent) {
  if (!active.value) return
  const now = props.axis === 'x' ? event.clientX : event.clientY
  emit('drag', now - last)
  last = now
}

function stop(event: PointerEvent) {
  active.value = false
  ;(event.currentTarget as HTMLElement).releasePointerCapture?.(event.pointerId)
}

function onKey(event: KeyboardEvent) {
  const back = props.axis === 'x' ? 'ArrowLeft' : 'ArrowUp'
  const forward = props.axis === 'x' ? 'ArrowRight' : 'ArrowDown'
  if (event.key !== back && event.key !== forward) return
  event.preventDefault()
  emit('drag', (event.key === forward ? 1 : -1) * (event.shiftKey ? 64 : 16))
}
</script>

<template>
  <div
    class="ui-splitter"
    :class="[axis, { active }]"
    role="separator"
    tabindex="0"
    :aria-orientation="axis === 'x' ? 'vertical' : 'horizontal'"
    :aria-label="label"
    :title="t('{label} — drag to resize, double-click to reset', { label })"
    @pointerdown="start"
    @pointermove="move"
    @pointerup="stop"
    @pointercancel="stop"
    @dblclick="emit('reset')"
    @keydown="onKey"
  >
    <i v-if="axis === 'y'" />
  </div>
</template>

<style scoped>
.ui-splitter { position: relative; flex: none; z-index: 2; }
.ui-splitter:focus-visible { outline-offset: -2px; }
/*
 * The gap between two panels is the handle. Nothing is drawn until the
 * pointer is on it; then a short bar says it can be dragged.
 */
.ui-splitter { display: grid; place-items: center; }
.ui-splitter::after {
  content: "";
  border-radius: 2px;
  background: transparent;
  transition: background var(--dur) var(--ease);
}
.x { width: var(--gap); cursor: col-resize; }
.x::after { width: 3px; height: 36px; }
.y { height: var(--gap); cursor: row-resize; }
.y::after { width: 36px; height: 3px; }
.ui-splitter:hover::after, .ui-splitter.active::after { background: var(--ink-2); }
.y i { display: none; }
</style>
