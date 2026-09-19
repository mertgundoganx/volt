<script setup lang="ts">
/**
 * A single-line mono input that marks `{{variables}}`: defined ones in the
 * variable colour, undefined ones in amber, so a typo is visible before Send.
 * Typing `{{` opens the names that exist; arrows pick, Enter or Tab inserts.
 *
 * The text is drawn by a mirror layer behind a transparent input. The two
 * share one font, size and padding, and the mirror follows the input's
 * horizontal scroll; the input keeps native caret, selection and IME.
 */
const props = withDefaults(
  defineProps<{
    modelValue: string
    /** Defined variable names. `null` means "unknown", so nothing is marked missing or offered. */
    known?: string[] | null
    placeholder?: string
    label: string
    variant?: 'field' | 'inline' | 'bare'
  }>(),
  { known: null, placeholder: '', variant: 'field' },
)
const emit = defineEmits<{
  'update:modelValue': [value: string]
  enter: []
  /** Emitted before the text lands, so a parent can take it over with preventDefault(). */
  paste: [event: ClipboardEvent]
}>()

const input = ref<HTMLInputElement>()
const offset = ref(0)

interface Piece { text: string; kind: 'text' | 'var' | 'missing' }

const pieces = computed<Piece[]>(() => {
  const out: Piece[] = []
  const pattern = /\{\{\s*([^{}]*?)\s*\}\}/g
  let last = 0
  for (const match of props.modelValue.matchAll(pattern)) {
    const start = match.index ?? 0
    if (start > last) out.push({ text: props.modelValue.slice(last, start), kind: 'text' })
    const name = match[1] ?? ''
    // A call is defined when its function is: `$hmacSha256(k, $body)` against `$hmacSha256(`.
    const call = name.startsWith('$') && name.includes('(') ? name.slice(0, name.indexOf('(') + 1) : null
    const defined = props.known === null || props.known.includes(name) || (call !== null && props.known.includes(call))
    out.push({ text: match[0], kind: defined ? 'var' : 'missing' })
    last = start + match[0].length
  }
  if (last < props.modelValue.length) out.push({ text: props.modelValue.slice(last), kind: 'text' })
  return out
})

function sync() {
  requestAnimationFrame(() => {
    offset.value = input.value?.scrollLeft ?? 0
  })
}

// --- Completion --------------------------------------------------------------
// An unclosed `{{name` just before the caret is what is being completed.
interface Context { prefix: string; start: number; at: number }
const context = ref<Context | null>(null)
const active = ref(0)
/** The text Escape was pressed on; the list stays closed until it changes. */
let dismissedFor: string | null = null
const box = ref({ top: 0, left: 0, width: 0 })

const suggestions = computed(() => {
  const ctx = context.value
  if (!ctx || props.known === null) return []
  const prefix = ctx.prefix.toLowerCase()
  return props.known.filter((name) => name.toLowerCase().startsWith(prefix)).slice(0, 8)
})
const open = computed(() => suggestions.value.length > 0)

function refresh() {
  const el = input.value
  if (!el || document.activeElement !== el) {
    context.value = null
    return
  }
  const at = el.selectionStart ?? el.value.length
  const match = el.value.slice(0, at).match(/\{\{\s*([A-Za-z0-9_$.-]*)$/)
  if (!match || el.value === dismissedFor) {
    context.value = null
    return
  }
  const prefix = match[1] ?? ''
  const wasOpen = !!context.value
  context.value = { prefix, start: at - prefix.length, at }
  if (!wasOpen) active.value = 0
  const rect = el.getBoundingClientRect()
  box.value = { top: rect.bottom + 4, left: rect.left, width: Math.max(180, Math.min(320, rect.width)) }
}

function accept(name: string) {
  const el = input.value
  const ctx = context.value
  if (!el || !ctx) return
  const after = el.value.slice(ctx.at)
  const closed = after.startsWith('}}')
  // A helper is offered as `$fn(`; it lands as `$fn()` with the caret inside.
  const call = name.endsWith('(')
  const insert = call ? `${name})` : name
  const next = `${el.value.slice(0, ctx.start)}${insert}${closed ? '' : '}}'}${after}`
  const caret = ctx.start + (call ? name.length : name.length + 2)
  emit('update:modelValue', next)
  context.value = null
  nextTick(() => {
    el.setSelectionRange(caret, caret)
    el.focus()
    sync()
  })
}

function onKeydown(event: KeyboardEvent) {
  if (!open.value) {
    if (event.key === 'Enter') emit('enter')
    return
  }
  const count = suggestions.value.length
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    event.preventDefault()
    active.value = (active.value + (event.key === 'ArrowDown' ? 1 : -1) + count) % count
  } else if (event.key === 'Enter' || event.key === 'Tab') {
    event.preventDefault()
    event.stopPropagation()
    accept(suggestions.value[active.value]!)
  } else if (event.key === 'Escape') {
    event.preventDefault()
    event.stopPropagation()
    dismissedFor = input.value?.value ?? null
    context.value = null
  }
}

function onInput(event: Event) {
  dismissedFor = null
  emit('update:modelValue', (event.target as HTMLInputElement).value)
  sync()
  nextTick(refresh)
}

defineExpose({ focus: () => input.value?.focus() })
</script>

<template>
  <!-- `is-` prefixed: a bare `field` class would pick up the global .field box. -->
  <div class="ui-var-input" :class="`is-${variant}`">
    <div class="mirror mono" aria-hidden="true">
      <span class="track" :style="{ transform: `translateX(${-offset}px)` }">
        <template v-for="(piece, i) in pieces" :key="i">
          <span v-if="piece.kind === 'text'">{{ piece.text }}</span>
          <span v-else class="var" :class="{ missing: piece.kind === 'missing' }">{{ piece.text }}</span>
        </template>
      </span>
    </div>
    <input
      ref="input"
      class="real mono"
      type="text"
      :value="modelValue"
      :placeholder="placeholder"
      :aria-label="label"
      spellcheck="false"
      autocomplete="off"
      autocapitalize="off"
      :aria-expanded="open"
      aria-autocomplete="list"
      @input="onInput"
      @scroll="sync"
      @keyup="sync(); refresh()"
      @mouseup="sync(); refresh()"
      @focus="sync"
      @blur="context = null"
      @keydown="onKeydown"
      @paste="emit('paste', $event)"
    >
    <ul
      v-if="open"
      class="complete"
      role="listbox"
      aria-label="Variables"
      :style="{ top: `${box.top}px`, left: `${box.left}px`, width: `${box.width}px` }"
    >
      <li
        v-for="(name, i) in suggestions"
        :key="name"
        role="option"
        class="option mono"
        :class="{ on: i === active }"
        :aria-selected="i === active"
        @mousedown.prevent="accept(name)"
        @mousemove="active = i"
      >
        <span class="var">{{ name }}</span>
      </li>
    </ul>
  </div>
</template>

<style scoped>
.ui-var-input {
  position: relative;
  min-width: 0;
  --pad: 10px;
  --h: var(--h-control);
}

.mirror,
.real {
  font-family: var(--font-mono);
  font-size: var(--t-code);
  font-variant-ligatures: none;
  letter-spacing: 0;
}

.mirror {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  padding: 0 var(--pad);
  overflow: hidden;
  white-space: pre;
  pointer-events: none;
  color: var(--ink);
  /* The mirror sits inside the border, so offset it by the border width. */
  border: 1px solid transparent;
}
.track { display: inline-block; white-space: pre; }

.real {
  position: relative;
  width: 100%;
  height: var(--h);
  padding: 0 var(--pad);
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
  background: transparent;
  color: transparent;
  caret-color: var(--ink);
  transition: border-color var(--dur) var(--ease), box-shadow var(--dur) var(--ease), background var(--dur) var(--ease);
}
.real::placeholder { color: var(--faint); }
.real::selection { color: transparent; background: var(--selection); }

.is-field { background: var(--well); border-radius: var(--r-sm); }
.is-field .real:hover { border-color: var(--line-strong); }
.is-field .real:focus { border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent-tint); }

.is-inline { --pad: var(--s-2); --h: 26px; border-radius: var(--r-sm); }
.is-inline .real { border-color: transparent; }
.is-inline:hover { background: var(--hover); }
.is-inline:focus-within { background: var(--well); }
.is-inline .real:focus { border-color: var(--accent); }

.is-bare { --pad: var(--s-3); --h: 100%; height: 100%; }
.is-bare .real { border: 0; border-radius: 0; }
.is-bare .mirror { border: 0; }

/* Fixed, so it escapes a bar that clips its overflow. */
.complete {
  position: fixed;
  z-index: 40;
  margin: 0;
  padding: 4px;
  list-style: none;
  background: var(--bg-3);
  border: 1px solid var(--line);
  border-radius: var(--r-md);
  box-shadow: var(--shadow-pop);
}
.option {
  display: flex;
  align-items: center;
  height: 26px;
  padding: 0 8px;
  border-radius: var(--r-sm);
  font-size: var(--t-code);
}
.option.on { background: var(--hover); }
</style>
