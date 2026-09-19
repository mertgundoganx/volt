<script setup lang="ts">
/**
 * A body, with line numbers and syntax colour.
 *
 * Only the rows near the viewport are in the DOM. A 20 MB JSON response is
 * about 1.8 million lines, and one `<div>` per line is a frozen window and a
 * gigabyte of nodes — so the rest of the height is held by two spacers and the
 * rows are rendered as they come into view. Rows are a fixed 20px tall, which
 * is what makes the arithmetic exact; when the text is wrapped a row can be
 * taller than that, so windowing is off and a very long body is capped instead,
 * with a line saying so.
 *
 * JSON folds: a line that opens `{` or `[` carries a chevron in its gutter,
 * and folding it hides everything down to the matching close. The hidden
 * lines simply leave the visible list; line numbers stay the original ones.
 */
import { jsonLines, markMatches, plainLines, type Line } from '~/utils/syntax'

const props = withDefaults(
  defineProps<{
    text: string
    language?: 'json' | 'text'
    wrap?: boolean
    /** Literal, case-insensitive; every occurrence is marked. */
    find?: string
    /** Which occurrence is the current one, counted from 0. */
    current?: number
  }>(),
  { language: 'text', wrap: false, find: '', current: 0 },
)

const ROW = 20
/** Rows kept on each side of the viewport, so scrolling does not flicker. */
const OVERSCAN = 30
/** Below this, windowing costs more than it saves. */
const WINDOW_FROM = 400
/** With wrapping on, row heights vary, so this is a cap rather than a window. */
const WRAP_CAP = 3000

const root = ref<HTMLElement>()

// Tokenising is the expensive half and depends only on the text, so a keystroke
// in the find box re-marks but does not re-parse.
const parsed = computed<Line[]>(() =>
  props.language === 'json' ? jsonLines(props.text) : plainLines(props.text),
)
const lines = computed<Line[]>(() => markMatches(parsed.value, props.find))

// --- Folds -------------------------------------------------------------------
// Which line closes each line that opens a container, from the brackets in
// the punctuation tokens. Only pretty-printed JSON puts one open per line, so
// the map is built from what is there rather than assumed.
const foldEnds = computed<Map<number, number>>(() => {
  const ends = new Map<number, number>()
  if (props.language !== 'json') return ends
  const stack: number[] = []
  parsed.value.forEach((line, index) => {
    for (const token of line) {
      if (token.kind !== 'punc') continue
      for (const ch of token.text) {
        if (ch === '{' || ch === '[') stack.push(index)
        else if (ch === '}' || ch === ']') {
          const open = stack.pop()
          if (open !== undefined && open !== index) ends.set(open, index)
        }
      }
    }
  })
  return ends
})
const folded = ref(new Set<number>())
// A new body, or a search, unfolds everything: a hit inside a fold would be
// counted and not seen.
watch(() => [props.text, props.find], () => (folded.value = new Set()))

function toggleFold(index: number) {
  const next = new Set(folded.value)
  next.has(index) ? next.delete(index) : next.add(index)
  folded.value = next
}

/** The original index of each visible line, with folded ranges left out. */
const originals = computed<number[]>(() => {
  const all = lines.value.length
  if (!folded.value.size) return Array.from({ length: all }, (_, i) => i)
  const out: number[] = []
  for (let i = 0; i < all; i++) {
    out.push(i)
    if (folded.value.has(i)) {
      const end = foldEnds.value.get(i)
      if (end !== undefined) i = end
    }
  }
  return out
})
const gutter = computed(() => `${String(lines.value.length).length + 1}ch`)

const scroller = ref<HTMLElement | null>(null)
const top = ref(0)
const height = ref(0)

const windowing = computed(() => !props.wrap && originals.value.length > WINDOW_FROM)
const capped = computed(() => props.wrap && originals.value.length > WRAP_CAP)

const first = computed(() => {
  if (!windowing.value) return 0
  return Math.max(0, Math.floor(top.value / ROW) - OVERSCAN)
})
const last = computed(() => {
  if (!windowing.value) return capped.value ? WRAP_CAP : originals.value.length
  const visible = Math.ceil((height.value || 600) / ROW)
  return Math.min(originals.value.length, first.value + visible + OVERSCAN * 2)
})
const shown = computed(() => originals.value.slice(first.value, last.value).map((original) => ({ original, line: lines.value[original]! })))
const above = computed(() => first.value * ROW)
const below = computed(() => Math.max(0, (originals.value.length - last.value) * ROW))

/** What a folded line stands for: how many lines, and what closes them. */
function foldSummary(original: number) {
  const end = foldEnds.value.get(original)
  if (end === undefined) return ''
  const closing = lines.value[end]?.map((t) => t.text).join('').trim().replace(/,$/, '') ?? ''
  return ` … ${closing} ${end - original - 1} ${end - original - 1 === 1 ? 'line' : 'lines'}`
}

/** The nearest ancestor that actually scrolls; the body lives inside a pane. */
function findScroller(from: HTMLElement | undefined): HTMLElement | null {
  let node = from?.parentElement ?? null
  while (node) {
    const overflow = getComputedStyle(node).overflowY
    if (overflow === 'auto' || overflow === 'scroll') return node
    node = node.parentElement
  }
  return null
}

function measure() {
  const el = scroller.value
  if (!el) return
  top.value = el.scrollTop
  height.value = el.clientHeight
}

let observer: ResizeObserver | undefined
onMounted(() => {
  scroller.value = findScroller(root.value)
  const el = scroller.value
  if (!el) return
  el.addEventListener('scroll', measure, { passive: true })
  observer = new ResizeObserver(measure)
  observer.observe(el)
  measure()
})
onBeforeUnmount(() => {
  scroller.value?.removeEventListener('scroll', measure)
  observer?.disconnect()
})

// A new body starts at the top, and the window has to be recomputed for it.
watch(
  () => props.text,
  () => {
    top.value = 0
    nextTick(measure)
  },
)

// Keep the current match on screen as the user steps through. With windowing on
// the row may not be rendered yet, so scroll to where it will be first.
watch(
  () => [props.current, props.find, props.text],
  async () => {
    if (!props.find) return
    const el = scroller.value
    if (!el) return
    const at = lines.value.findIndex((line) => line.some((token) => token.hit === props.current))
    if (at === -1) return
    if (windowing.value) {
      const y = at * ROW
      if (y < el.scrollTop || y + ROW > el.scrollTop + el.clientHeight) el.scrollTop = Math.max(0, y - el.clientHeight / 2)
      measure()
    }
    await nextTick()
    root.value?.querySelector('.hit.now')?.scrollIntoView({ block: 'nearest', inline: 'nearest' })
  },
)
</script>

<template>
  <div ref="root" class="ui-code mono selectable" :class="{ wrap }" :style="{ '--gutter': gutter }">
    <div v-if="above" class="spacer" :style="{ height: `${above}px` }" aria-hidden="true" />
    <div v-for="{ original, line } in shown" :key="original" class="row" :class="{ foldable: foldEnds.has(original), folded: folded.has(original) }">
      <span class="ln" aria-hidden="true">
        <button
          v-if="foldEnds.has(original)"
          type="button"
          class="fold"
          tabindex="-1"
          :aria-label="folded.has(original) ? `Unfold line ${original + 1}` : `Fold line ${original + 1}`"
          @click="toggleFold(original)"
        >
          <UiIcon :name="folded.has(original) ? 'chevron-right' : 'chevron-down'" :size="11" />
        </button>{{ original + 1 }}
      </span>
      <span class="src"><template v-for="(token, t) in line" :key="t"><span v-if="token.hit !== undefined" class="hit" :class="[token.kind !== 'text' ? token.kind : '', { now: token.hit === props.current }]">{{ token.text }}</span><span v-else-if="token.kind !== 'text'" :class="token.kind">{{ token.text }}</span><template v-else>{{ token.text }}</template></template><button v-if="folded.has(original)" type="button" class="summary" @click="toggleFold(original)">{{ foldSummary(original) }}</button></span>
    </div>
    <div v-if="below" class="spacer" :style="{ height: `${below}px` }" aria-hidden="true" />
    <p v-if="capped" class="capped">
      Showing the first {{ WRAP_CAP.toLocaleString() }} of {{ lines.length.toLocaleString() }} lines.
      Turn wrapping off to scroll all of it.
    </p>
  </div>
</template>

<style scoped>
.ui-code {
  min-width: max-content;
  padding: var(--s-2) 0 var(--s-4);
  font-size: var(--t-code);
  line-height: 20px;
  color: var(--ink);
}
.ui-code.wrap { min-width: 0; }

/* min-height keeps empty lines without putting invisible characters in a copy. */
.row { display: flex; min-height: 20px; }
.spacer { flex: none; }

.ln {
  position: sticky;
  left: 0;
  flex: none;
  display: inline-flex;
  justify-content: flex-end;
  align-items: center;
  gap: 2px;
  width: calc(var(--gutter) + var(--s-4) + 14px);
  padding-right: var(--s-3);
  text-align: right;
  color: var(--faint);
  /* Opaque, so code scrolled sideways passes under the numbers. */
  background: var(--code-bg, var(--well));
  user-select: none;
  font-variant-numeric: tabular-nums;
}
.fold {
  display: grid;
  place-items: center;
  width: 14px;
  height: 14px;
  border-radius: 3px;
  color: var(--faint);
  opacity: 0;
  transition: opacity var(--dur) var(--ease);
}
.row:hover .fold, .row.folded .fold { opacity: 1; }
.fold:hover { color: var(--ink); background: var(--press); }

.src { white-space: pre; padding-right: var(--s-4); }
.wrap .src { white-space: pre-wrap; word-break: break-word; min-width: 0; }
.summary {
  margin-left: 4px;
  padding: 0 6px;
  border-radius: var(--r-xs);
  background: var(--hover);
  color: var(--silk);
  font: inherit;
  font-size: var(--t-meta);
}
.summary:hover { background: var(--press); color: var(--ink); }

.key { color: var(--syn-key); }
.str { color: var(--syn-str); }
.num { color: var(--syn-num); }
.lit { color: var(--syn-lit); }
.punc { color: var(--punc); }

.capped {
  margin: var(--s-3) var(--s-4) 0;
  color: var(--silk);
  font-family: var(--font-ui);
  font-size: var(--t-small);
}

/* A match is not a status, so it is the accent rather than ok or warn. */
.hit { border-radius: var(--r-xs); background: var(--accent-tint); box-shadow: 0 0 0 1px var(--accent-line); }
.hit.now { background: var(--accent); color: var(--accent-ink); }
</style>
