<script setup lang="ts">
/**
 * A single-line mono input that marks `{{variables}}`: defined ones in the
 * variable colour, undefined ones in amber, so a typo is visible before Send.
 *
 * The text is drawn by a mirror layer behind a transparent input. The two
 * share one font, size and padding, and the mirror follows the input's
 * horizontal scroll; the input keeps native caret, selection and IME.
 */
const props = withDefaults(
  defineProps<{
    modelValue: string
    /** Defined variable names. `null` means "unknown", so nothing is marked missing. */
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
    const defined = props.known === null || props.known.includes(name)
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

function onInput(event: Event) {
  emit('update:modelValue', (event.target as HTMLInputElement).value)
  sync()
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
      @input="onInput"
      @scroll="sync"
      @keyup="sync"
      @mouseup="sync"
      @focus="sync"
      @keydown.enter="emit('enter')"
      @paste="emit('paste', $event)"
    >
  </div>
</template>

<style scoped>
.ui-var-input {
  position: relative;
  min-width: 0;
  --pad: var(--s-3);
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
.is-field .real:focus { border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-tint); }

.is-inline { --pad: var(--s-2); --h: 26px; border-radius: var(--r-sm); }
.is-inline .real { border-color: transparent; }
.is-inline:hover { background: var(--hover); }
.is-inline:focus-within { background: var(--well); }
.is-inline .real:focus { border-color: var(--accent); }

.is-bare { --pad: var(--s-3); --h: 100%; height: 100%; }
.is-bare .real { border: 0; border-radius: 0; }
.is-bare .mirror { border: 0; }
</style>
