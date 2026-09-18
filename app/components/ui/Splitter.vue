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
    :title="`${label} — drag to resize, double-click to reset`"
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

/* Between columns: a hairline with a wider grab area. */
.x { width: 1px; background: var(--line); cursor: col-resize; }
.x::before { content: ""; position: absolute; inset: 0 -4px; }
.x:hover, .x.active { background: var(--accent); box-shadow: 0 0 12px -2px var(--accent); }

/* Between rows: a thin chrome bar with grip lines. */
.y {
  height: 10px;
  display: grid;
  place-items: center;
  background: var(--bg-0);
  border-top: 1px solid var(--line);
  border-bottom: 1px solid var(--line);
  cursor: row-resize;
}
.y i {
  width: 26px;
  height: 4px;
  border-top: 1px solid var(--silk);
  border-bottom: 1px solid var(--silk);
  opacity: 0.6;
  transition: opacity var(--dur) var(--ease);
}
.y:hover i, .y.active i { opacity: 1; border-color: var(--accent); }
</style>
