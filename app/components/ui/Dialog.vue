<script setup lang="ts">
const props = withDefaults(
  defineProps<{ title: string; eyebrow?: string; width?: number }>(),
  { eyebrow: undefined, width: 560 },
)
const emit = defineEmits<{ close: [] }>()

const sheet = ref<HTMLElement>()
const titleId = `dialog-${Math.random().toString(36).slice(2, 9)}`
let returnFocus: HTMLElement | null = null

const FOCUSABLE = 'button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex="-1"])'

onMounted(() => {
  returnFocus = document.activeElement as HTMLElement | null
  // Prefer what the dialog marks as its starting point, then its first field.
  const start =
    sheet.value?.querySelector<HTMLElement>('[autofocus]') ??
    sheet.value?.querySelector<HTMLElement>('input, select, textarea') ??
    sheet.value?.querySelector<HTMLElement>(FOCUSABLE)
  start?.focus()
})

// On the window, not on the backdrop. A click on a non-focusable part of a
// dialog moves focus to `document.body`, and from there no keydown reaches the
// backdrop at all — Escape stopped closing and Tab stopped being trapped.
onMounted(() => window.addEventListener('keydown', onKey))
onUnmounted(() => {
  window.removeEventListener('keydown', onKey)
  returnFocus?.focus?.()
})

function onKey(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    event.stopPropagation()
    emit('close')
    return
  }
  if (event.key !== 'Tab' || !sheet.value) return

  // Keep Tab inside the dialog.
  const items = [...sheet.value.querySelectorAll<HTMLElement>(FOCUSABLE)]
  if (!items.length) return
  const first = items[0]!
  const last = items[items.length - 1]!
  const inside = sheet.value.contains(document.activeElement)
  if (!inside) {
    // Focus fell out of the sheet — usually onto the body after a click on
    // something that does not take focus. Pull it back rather than letting
    // Tab walk into the page behind.
    event.preventDefault()
    ;(event.shiftKey ? last : first).focus()
    return
  }
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault()
    last.focus()
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault()
    first.focus()
  }
}
</script>

<template>
  <div class="ui-backdrop" @mousedown.self="emit('close')">
    <div
      ref="sheet"
      class="ui-dialog"
      role="dialog"
      aria-modal="true"
      :aria-labelledby="titleId"
      :style="{ '--dialog-w': `${props.width}px` }"
    >
      <header>
        <div class="heading">
          <span v-if="eyebrow" class="silk">{{ eyebrow }}</span>
          <slot name="title">
            <h2 :id="titleId">{{ title }}</h2>
          </slot>
        </div>
        <button type="button" class="icon-btn quiet" aria-label="Close" title="Close (Esc)" @click="emit('close')">
          <UiIcon name="x" />
        </button>
      </header>

      <div class="body">
        <slot />
      </div>

      <footer v-if="$slots.footer">
        <slot name="footer" />
      </footer>
    </div>
  </div>
</template>

<style scoped>
.ui-backdrop {
  position: fixed;
  inset: 0;
  z-index: 50;
  display: grid;
  place-items: center;
  padding: var(--s-6);
  background: var(--backdrop);
  animation: fade 140ms var(--ease);
}

.ui-dialog {
  display: flex;
  flex-direction: column;
  width: min(var(--dialog-w), 100%);
  max-height: min(86vh, 760px);
  background: var(--bg-3);
  border: 1px solid var(--line);
  border-radius: var(--r-lg);
  box-shadow: var(--rim), var(--shadow-pop);
  animation: rise 180ms var(--ease);
}

header {
  display: flex;
  align-items: flex-start;
  gap: var(--s-3);
  padding: var(--s-4) var(--s-4) var(--s-3) var(--s-5);
}
.heading { flex: 1; display: grid; gap: 7px; min-width: 0; padding-top: 4px; }
h2 {
  margin: 0;
  font-stretch: 100%;
  font-weight: 750;
  font-size: 17px;
  line-height: 1.2;
  letter-spacing: -0.02em;
}

.body { flex: 1; overflow: auto; padding: 0 var(--s-5) var(--s-5); min-height: 0; }

footer {
  display: flex;
  align-items: center;
  gap: var(--s-2);
  padding: var(--s-3) var(--s-4) var(--s-3) var(--s-5);
  border-top: 1px solid var(--line-soft);
}

@keyframes fade { from { opacity: 0; } }
@keyframes rise { from { opacity: 0; transform: translateY(10px) scale(0.985); } }
</style>
