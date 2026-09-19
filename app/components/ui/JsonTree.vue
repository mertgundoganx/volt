<script setup lang="ts">
/**
 * A JSON body as a tree: fold what you are not looking at, and take a value
 * into a capture by its path. Paths are the subset `capture::read` understands
 * — `$.data.items[0].id` — so a path clicked here is a path that will work.
 */
const props = defineProps<{ text: string }>()
const emit = defineEmits<{ capture: [path: string, key: string]; copy: [path: string] }>()

/** Above this many nodes the tree is slower to read than the text. */
const NODE_LIMIT = 20_000

interface Node {
  path: string
  key: string
  depth: number
  kind: 'object' | 'array' | 'str' | 'num' | 'lit'
  /** For a leaf: what to print. For a container: how many children. */
  text: string
  size: number
  children?: Node[]
}

const parsed = computed<{ root: Node | null; count: number; error: string | null }>(() => {
  let value: unknown
  try {
    value = JSON.parse(props.text)
  } catch (e) {
    return { root: null, count: 0, error: String((e as Error).message) }
  }
  let count = 0
  const build = (v: unknown, key: string, path: string, depth: number): Node => {
    count += 1
    if (Array.isArray(v)) {
      const children = count > NODE_LIMIT ? [] : v.map((item, i) => build(item, String(i), `${path}[${i}]`, depth + 1))
      return { path, key, depth, kind: 'array', text: '', size: v.length, children }
    }
    if (v && typeof v === 'object') {
      const entries = Object.entries(v as Record<string, unknown>)
      const children = count > NODE_LIMIT ? [] : entries.map(([k, item]) => build(item, k, `${path}.${k}`, depth + 1))
      return { path, key, depth, kind: 'object', text: '', size: entries.length, children }
    }
    if (typeof v === 'string') return { path, key, depth, kind: 'str', text: JSON.stringify(v), size: 0 }
    if (typeof v === 'number') return { path, key, depth, kind: 'num', text: String(v), size: 0 }
    return { path, key, depth, kind: 'lit', text: String(v), size: 0 }
  }
  const root = build(value, '', '$', 0)
  return { root, count, error: null }
})

const tooLarge = computed(() => parsed.value.count > NODE_LIMIT)

// Folded paths. Everything is open to start with except lists deeper than
// the second level, which is where a hundred rows would otherwise unfold.
const folded = ref(new Set<string>())
watch(
  () => parsed.value.root,
  (root) => {
    const next = new Set<string>()
    const walk = (node: Node) => {
      if (node.children) {
        if (node.depth >= 2 && node.size > 3) next.add(node.path)
        for (const child of node.children) walk(child)
      }
    }
    if (root) walk(root)
    folded.value = next
  },
  { immediate: true },
)

function toggle(path: string) {
  const next = new Set(folded.value)
  next.has(path) ? next.delete(path) : next.add(path)
  folded.value = next
}

/** The rows on screen: a depth-first walk that stops at folded containers. */
const rows = computed<Node[]>(() => {
  const out: Node[] = []
  const walk = (node: Node) => {
    out.push(node)
    if (node.children && !folded.value.has(node.path)) for (const child of node.children) walk(child)
  }
  if (parsed.value.root && !tooLarge.value) walk(parsed.value.root)
  return out
})

/** A capture wants a variable name; the last path segment is the obvious one. */
function nameFor(node: Node) {
  return node.key && !/^\d+$/.test(node.key) ? node.key : 'value'
}
</script>

<template>
  <div class="ui-json-tree mono">
    <p v-if="parsed.error" class="note">Not JSON: {{ parsed.error }}</p>
    <p v-else-if="tooLarge" class="note">
      {{ parsed.count.toLocaleString() }} values is more than a tree reads well. Use Pretty for this one.
    </p>
    <template v-else>
      <div
        v-for="node in rows"
        :key="node.path"
        class="row"
        :class="{ folded: node.children && folded.has(node.path) }"
        :style="{ paddingLeft: `${12 + node.depth * 16}px` }"
      >
        <button
          v-if="node.children"
          type="button"
          class="fold"
          :aria-label="folded.has(node.path) ? `Unfold ${node.path}` : `Fold ${node.path}`"
          @click="toggle(node.path)"
        >
          <UiIcon :name="folded.has(node.path) ? 'chevron-right' : 'chevron-down'" :size="12" />
        </button>
        <span v-else class="fold-space" />

        <template v-if="node.depth > 0"><span class="key">{{ node.key }}</span><span class="punc">: </span></template><span v-if="node.kind === 'object'" class="punc">{{ folded.has(node.path) ? `{…} ${node.size} ${node.size === 1 ? 'field' : 'fields'}` : '{' }}</span>
        <span v-else-if="node.kind === 'array'" class="punc">{{ folded.has(node.path) ? `[…] ${node.size} ${node.size === 1 ? 'item' : 'items'}` : '[' }}</span>
        <span v-else class="value selectable" :class="node.kind">{{ node.text }}</span>

        <span class="tools">
          <span class="path">{{ node.path }}</span>
          <button
            v-if="node.depth > 0"
            type="button"
            class="act"
            :title="`Capture ${node.path} into the environment as {{${nameFor(node)}}}`"
            @click="emit('capture', node.path, nameFor(node))"
          >
            <UiIcon name="key" :size="13" />Capture
          </button>
          <button type="button" class="act" title="Copy the path" @click="emit('copy', node.path)">
            <UiIcon name="copy" :size="13" />
          </button>
        </span>
      </div>
    </template>
  </div>
</template>

<style scoped>
.ui-json-tree { padding: var(--s-2) 0 var(--s-4); font-size: var(--t-code); line-height: 20px; color: var(--ink); }
.note { margin: var(--s-4); color: var(--silk); font-family: var(--font-ui); font-size: var(--t-small); }

.row { display: flex; align-items: center; min-height: 22px; padding-right: var(--s-3); white-space: pre; }
.row:hover { background: var(--hover); }

.fold { display: grid; place-items: center; width: 16px; height: 16px; margin-right: 2px; border-radius: 3px; color: var(--faint); flex: none; }
.fold:hover { color: var(--ink); background: var(--press); }
.fold-space { width: 18px; flex: none; }

.key { color: var(--syn-key); }
.punc { color: var(--punc); }
.value.str { color: var(--syn-str); }
.value.num { color: var(--syn-num); }
.value.lit { color: var(--syn-lit); }
.value { overflow: hidden; text-overflow: ellipsis; max-width: 60ch; }

.tools { display: none; align-items: center; gap: 6px; margin-left: auto; padding-left: var(--s-4); flex: none; }
.row:hover .tools { display: inline-flex; }
.path { color: var(--faint); font-size: var(--t-meta); }
.act {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 20px;
  padding: 0 6px;
  border: 1px solid var(--line);
  border-radius: var(--r-xs);
  background: var(--bg-2);
  color: var(--ink-2);
  font-family: var(--font-ui);
  font-size: var(--t-label);
  font-weight: 500;
}
.act:hover { color: var(--accent-text); border-color: var(--accent-line); }
</style>
