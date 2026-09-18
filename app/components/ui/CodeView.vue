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
const gutter = computed(() => `${String(lines.value.length).length + 1}ch`)

const scroller = ref<HTMLElement | null>(null)
const top = ref(0)
const height = ref(0)

const windowing = computed(() => !props.wrap && lines.value.length > WINDOW_FROM)
const capped = computed(() => props.wrap && lines.value.length > WRAP_CAP)

const first = computed(() => {
  if (!windowing.value) return 0
  return Math.max(0, Math.floor(top.value / ROW) - OVERSCAN)
})
const last = computed(() => {
  if (!windowing.value) return capped.value ? WRAP_CAP : lines.value.length
  const visible = Math.ceil((height.value || 600) / ROW)
  return Math.min(lines.value.length, first.value + visible + OVERSCAN * 2)
})
const shown = computed(() => lines.value.slice(first.value, last.value))
const above = computed(() => first.value * ROW)
const below = computed(() => Math.max(0, (lines.value.length - last.value) * ROW))

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
    if (windowing.value) {
      const at = lines.value.findIndex((line) => line.some((token) => token.hit === props.current))
      if (at !== -1 && scroller.value) {
        const wanted = at * ROW
        const view = scroller.value.clientHeight
        if (wanted < scroller.value.scrollTop || wanted > scroller.value.scrollTop + view - ROW) {
          scroller.value.scrollTop = Math.max(0, wanted - view / 2)
        }
        await nextTick()
      }
    }
    await nextTick()
    root.value?.querySelector('.hit.now')?.scrollIntoView({ block: 'nearest', inline: 'nearest' })
  },
)
</script>

<template>
  <div ref="root" class="ui-code mono selectable" :class="{ wrap }" :style="{ '--gutter': gutter }">
    <div v-if="above" class="spacer" :style="{ height: `${above}px` }" aria-hidden="true" />
    <div v-for="(line, index) in shown" :key="first + index" class="row">
      <span class="ln" aria-hidden="true">{{ first + index + 1 }}</span>
      <span class="src"><template v-for="(token, t) in line" :key="t"><span v-if="token.hit !== undefined" class="hit" :class="[token.kind !== 'text' ? token.kind : '', { now: token.hit === props.current }]">{{ token.text }}</span><span v-else-if="token.kind !== 'text'" :class="token.kind">{{ token.text }}</span><template v-else>{{ token.text }}</template></template></span>
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
  width: calc(var(--gutter) + var(--s-4));
  padding-right: var(--s-3);
  text-align: right;
  color: var(--faint);
  /* Opaque, so code scrolled sideways passes under the numbers. */
  background: var(--code-bg, var(--well));
  user-select: none;
  font-variant-numeric: tabular-nums;
}

.src { white-space: pre; padding-right: var(--s-4); }
.wrap .src { white-space: pre-wrap; word-break: break-word; min-width: 0; }

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
.hit.now { background: var(--accent); color: var(--accent-ink); box-shadow: 0 0 12px -2px var(--accent); }
</style>
