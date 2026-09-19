<script setup lang="ts">
/** What changed between two bodies, line by line. */
import { diffLines } from '~/utils/diff'

const props = defineProps<{ before: string; after: string }>()
const diff = computed(() => diffLines(props.before, props.after))
</script>

<template>
  <div class="ui-diff mono">
    <p v-if="diff.tooLarge" class="note">These bodies are too long to compare line by line.</p>
    <p v-else-if="!diff.changed" class="note">Same body as the previous send.</p>
    <template v-else>
      <div v-for="(row, i) in diff.rows" :key="i" class="row" :class="row.kind">
        <template v-if="row.kind === 'skip'">
          <span class="ln" /><span class="ln" />
          <span class="mark" /><span class="src fold">{{ row.count }} unchanged {{ row.count === 1 ? 'line' : 'lines' }}</span>
        </template>
        <template v-else>
          <span class="ln">{{ row.before ?? '' }}</span>
          <span class="ln">{{ row.after ?? '' }}</span>
          <span class="mark">{{ row.kind === 'gone' ? '−' : row.kind === 'new' ? '+' : '' }}</span>
          <span class="src selectable">{{ row.text }}</span>
        </template>
      </div>
    </template>
  </div>
</template>

<style scoped>
.ui-diff { min-width: max-content; padding: var(--s-2) 0 var(--s-4); font-size: var(--t-code); line-height: 20px; color: var(--ink); }
.note { margin: var(--s-4); color: var(--silk); font-family: var(--font-ui); font-size: var(--t-small); }
.row { display: flex; min-height: 20px; }
.ln { flex: none; width: 4ch; padding-right: 6px; text-align: right; color: var(--faint); user-select: none; font-variant-numeric: tabular-nums; }
.mark { flex: none; width: 2ch; text-align: center; color: var(--silk); user-select: none; }
.src { white-space: pre; padding-right: var(--s-4); }
.row.gone { background: var(--bad-tint); }
.row.gone .mark { color: var(--bad); }
.row.new { background: var(--ok-tint); }
.row.new .mark { color: var(--ok); }
.fold { color: var(--silk); font-family: var(--font-ui); font-size: var(--t-small); }
</style>
