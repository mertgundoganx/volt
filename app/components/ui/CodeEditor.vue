<script setup lang="ts">
import { jsonLines, plainLines, type TokenKind } from '~/utils/syntax'

/**
 * A textarea with a line-number gutter, for request bodies. The colours are
 * drawn by a mirror layer behind transparent text, the way `VarInput` does
 * it: the textarea keeps the native caret, selection, undo and IME, and the
 * mirror only has to agree with it on font, padding and scroll.
 */
const props = withDefaults(
  defineProps<{
    modelValue: string
    placeholder?: string
    label: string
    /** `json` is coloured; anything else only has its `{{variables}}` marked. */
    language?: string
    /** Defined variable names. `null` means "unknown", so nothing is marked missing. */
    known?: string[] | null
  }>(),
  { placeholder: '', language: 'text', known: null },
)
const emit = defineEmits<{ 'update:modelValue': [value: string], caret: [at: number] }>()

const area = ref<HTMLTextAreaElement>()
const scrollTop = ref(0)
const scrollLeft = ref(0)
const count = computed(() => Math.max(1, props.modelValue.split('\n').length))

interface Piece { text: string; kind: TokenKind | 'var' | 'missing' }

const VARIABLE = /\{\{\s*([^{}]*?)\s*\}\}/g

/** A `{{name}}` is marked wherever it sits, inside a string or outside one. */
function withVariables(text: string, kind: TokenKind): Piece[] {
  if (!text.includes('{{')) return [{ text, kind }]
  const out: Piece[] = []
  let last = 0
  for (const match of text.matchAll(VARIABLE)) {
    const start = match.index ?? 0
    if (start > last) out.push({ text: text.slice(last, start), kind })
    const name = match[1] ?? ''
    const call = name.startsWith('$') && name.includes('(') ? name.slice(0, name.indexOf('(') + 1) : null
    const defined = props.known === null || props.known.includes(name) || (call !== null && props.known.includes(call))
    out.push({ text: match[0], kind: defined ? 'var' : 'missing' })
    last = start + match[0].length
  }
  if (last < text.length) out.push({ text: text.slice(last), kind })
  return out
}

/**
 * Above this the body is drawn plain. The whole text is re-read on every
 * keystroke, and typing into a large body has to stay instant; colour on
 * sixty thousand characters of JSON is not what anyone is looking at.
 */
const COLOUR_LIMIT = 60_000

const lines = computed<Piece[][]>(() => {
  if (props.modelValue.length > COLOUR_LIMIT) return plainLines(props.modelValue)
  const base = props.language === 'json' ? jsonLines(props.modelValue) : plainLines(props.modelValue)
  return base.map((line) => line.flatMap((token) => withVariables(token.text, token.kind)))
})

function onInput(event: Event) {
  const el = event.target as HTMLTextAreaElement
  emit('update:modelValue', el.value)
  emit('caret', el.selectionStart)
}

/** Where the caret is, for anything that completes what is being typed. */
function onCaret() {
  if (area.value) emit('caret', area.value.selectionStart)
}

function onScroll(event: Event) {
  const el = event.target as HTMLTextAreaElement
  scrollTop.value = el.scrollTop
  scrollLeft.value = el.scrollLeft
}

/**
 * Put `text` in, replacing the `back` characters before the caret — which is
 * how a completion replaces the part of the name already typed.
 */
function insert(text: string, back = 0) {
  const el = area.value
  if (!el) return
  const at = el.selectionStart
  const before = el.value.slice(0, Math.max(0, at - back))
  el.value = before + text + el.value.slice(el.selectionEnd)
  el.selectionStart = el.selectionEnd = before.length + text.length
  emit('update:modelValue', el.value)
  emit('caret', el.selectionStart)
  el.focus()
}

defineExpose({ insert, focus: () => area.value?.focus() })

const PAIRS: Record<string, string> = { '{': '}', '[': ']', '"': '"' }

/**
 * Replace `from..to` with `text` the way typing would. Through the browser's
 * editing command rather than by assigning `value`: an assignment throws the
 * undo history away, so Ctrl+Z would stop at the last Enter, and it does not
 * scroll the caret back into view.
 */
function edit(el: HTMLTextAreaElement, from: number, to: number, text: string) {
  el.setSelectionRange(from, to)
  if (document.execCommand('insertText', false, text)) return
  // No editing command here (it is deprecated, though every webview has it).
  el.value = el.value.slice(0, from) + text + el.value.slice(to)
  emit('update:modelValue', el.value)
}

/**
 * Tab indents — the caret by two spaces, a selection line by line, and
 * Shift+Tab takes it back; Enter keeps the line's indent, and one more level
 * after an opening bracket; a bracket or a quote typed in JSON brings its
 * partner.
 */
function onKeydown(event: KeyboardEvent) {
  const el = area.value
  // A key pressed while an input method is composing belongs to it: Enter
  // there picks a candidate, it does not start a line.
  if (!el || event.isComposing || event.keyCode === 229) return
  if (event.ctrlKey || event.metaKey || event.altKey) return
  const { selectionStart: start, selectionEnd: end, value } = el
  const put = (text: string, caret: number) => {
    event.preventDefault()
    edit(el, start, end, text)
    el.selectionStart = el.selectionEnd = start + caret
    emit('caret', el.selectionStart)
  }

  if (event.key === 'Tab') {
    const lineStart = value.lastIndexOf('\n', start - 1) + 1
    if (start === end && !event.shiftKey) return put('  ', 2)
    // A selection, or Shift+Tab: move whole lines, and keep them selected.
    event.preventDefault()
    const block = value.slice(lineStart, end)
    const moved = event.shiftKey ? block.replace(/^ {1,2}/gm, '') : block.replace(/^(?=.)/gm, '  ')
    if (moved === block) return
    edit(el, lineStart, end, moved)
    const first = event.shiftKey ? -Math.min(2, /^ */.exec(block)![0].length, start - lineStart) : 2
    el.setSelectionRange(Math.max(lineStart, start + first), lineStart + moved.length)
    emit('caret', el.selectionStart)
    return
  }

  if (event.key === 'Enter' && !event.shiftKey) {
    const lineStart = value.lastIndexOf('\n', start - 1) + 1
    const indent = /^[ \t]*/.exec(value.slice(lineStart, start))?.[0] ?? ''
    const before = value[start - 1]
    const after = value[end]
    if ((before === '{' && after === '}') || (before === '[' && after === ']')) {
      return put(`\n${indent}  \n${indent}`, indent.length + 3)
    }
    const deeper = before === '{' || before === '[' ? '  ' : ''
    return put(`\n${indent}${deeper}`, 1 + indent.length + deeper.length)
  }

  if (props.language !== 'json' || start !== end) return
  const next = value[start]
  // Typing the closer that is already there steps over it.
  if ((event.key === '}' || event.key === ']' || event.key === '"') && next === event.key && value[start - 1] !== '\\') {
    event.preventDefault()
    el.selectionStart = el.selectionEnd = start + 1
    return
  }
  const partner = PAIRS[event.key]
  if (!partner || !(next === undefined || /[\s,}\]:]/.test(next))) return
  if (event.key === '"') {
    // A quote typed inside an open string is closing it, not starting one.
    const lineStart = value.lastIndexOf('\n', start - 1) + 1
    const quotes = value.slice(lineStart, start).replace(/\\./g, '').split('"').length - 1
    if (quotes % 2 === 1) return
  }
  put(event.key + partner, 1)
}
</script>

<template>
  <div class="ui-editor">
    <div class="gutter mono" aria-hidden="true">
      <div :style="{ transform: `translateY(${-scrollTop}px)` }">
        <div v-for="n in count" :key="n">{{ n }}</div>
      </div>
    </div>
    <div class="stage">
      <!-- One line of markup: inside <pre>, the template's own whitespace would show. -->
      <pre class="mirror mono" aria-hidden="true" :style="{ transform: `translate(${-scrollLeft}px, ${-scrollTop}px)` }"><template v-for="(line, i) in lines" :key="i"><span v-for="(piece, j) in line" :key="j" :class="piece.kind">{{ piece.text }}</span>{{ '\n' }}</template></pre>
      <textarea
        ref="area"
        class="mono"
        :value="modelValue"
        :placeholder="placeholder"
        :aria-label="label"
        spellcheck="false"
        wrap="off"
        @input="onInput"
        @keydown="onKeydown"
        @keyup="onCaret"
        @click="onCaret"
        @select="onCaret"
        @scroll="onScroll"
      />
    </div>
  </div>
</template>

<style scoped>
.ui-editor {
  display: flex;
  flex: 1;
  min-height: 0;
  background: var(--well);
}

.gutter {
  flex: none;
  min-width: 44px;
  padding: var(--s-3) var(--s-3) var(--s-3) var(--s-2);
  overflow: hidden;
  text-align: right;
  color: var(--faint);
  font-size: var(--t-code);
  line-height: 20px;
  font-variant-numeric: tabular-nums;
  user-select: none;
}

.stage { position: relative; flex: 1; min-width: 0; overflow: hidden; }

.mirror, textarea {
  margin: 0;
  padding: var(--s-3) var(--s-4) var(--s-3) var(--s-1);
  border: 0;
  font-size: var(--t-code);
  line-height: 20px;
  white-space: pre;
  tab-size: 2;
  letter-spacing: 0;
}
.mirror {
  position: absolute;
  top: 0;
  left: 0;
  min-width: 100%;
  color: var(--ink);
  pointer-events: none;
  user-select: none;
}
textarea {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  background: transparent;
  /* The mirror shows the text; this keeps only the caret and the selection. */
  color: transparent;
  caret-color: var(--ink);
  resize: none;
}
textarea::placeholder { color: var(--faint); }
textarea:focus-visible { outline: none; }
.ui-editor:focus-within { box-shadow: inset 2px 0 0 var(--focus); }

.key { color: var(--syn-key); }
.str { color: var(--syn-str); }
.num { color: var(--syn-num); }
.lit { color: var(--syn-lit); }
.punc { color: var(--punc); }
.var { color: var(--var); background: var(--var-bg); border-radius: 3px; }
.missing {
  color: var(--warn);
  background: var(--warn-tint);
  border-radius: 3px;
  text-decoration: underline wavy;
  text-decoration-color: color-mix(in srgb, var(--warn) 60%, transparent);
  text-underline-offset: 3px;
}
</style>
