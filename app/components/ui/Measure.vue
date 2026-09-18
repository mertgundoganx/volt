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
.ui-measure { display: grid; gap: 7px; align-content: start; flex: none; }
.ui-measure .silk { white-space: nowrap; }
.value { font-size: 15px; font-weight: 500; line-height: 1; white-space: nowrap; }
.value small { margin-left: 3px; font-size: 10.5px; font-weight: 500; color: var(--silk); }
.ok { color: var(--ok); }
.warn { color: var(--warn); }
.bad { color: var(--bad); }
</style>
