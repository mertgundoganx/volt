<script setup lang="ts">
/** One reading on the instrument: a silkscreen label over a value and its unit. */
defineProps<{ label: string; value: string | number; unit?: string; tone?: 'ok' | 'warn' | 'bad' }>()
</script>

<template>
  <div class="ui-measure">
    <span class="silk">{{ label }}</span>
    <span class="value mono num" :class="tone">{{ value }}<small v-if="unit">{{ unit }}</small></span>
  </div>
</template>

<style scoped>
.ui-measure { display: grid; gap: 3px; align-content: start; flex: none; }
.ui-measure .silk { white-space: nowrap; }
.value { font-size: var(--t-body); font-weight: 500; line-height: 1; white-space: nowrap; }
.value small { margin-left: 2px; font-size: var(--t-micro); font-weight: 500; color: var(--silk); }
.ok { color: var(--ok); }
.warn { color: var(--warn); }
.bad { color: var(--bad); }
</style>
