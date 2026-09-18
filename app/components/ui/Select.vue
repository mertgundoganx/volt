<script setup lang="ts">
defineProps<{ options: { value: string; label: string }[]; label: string; mono?: boolean }>()
const model = defineModel<string>({ required: true })
</script>

<template>
  <div class="ui-select">
    <select v-model="model" class="field" :class="{ mono }" :aria-label="label">
      <option v-for="option in options" :key="option.value" :value="option.value">{{ option.label }}</option>
    </select>
    <UiIcon class="chev" name="chevron-down" :size="14" />
  </div>
</template>

<style scoped>
.ui-select { position: relative; display: inline-flex; min-width: 0; }
select { appearance: none; padding-right: 30px; }
.chev { position: absolute; right: 10px; top: 50%; transform: translateY(-50%); color: var(--silk); pointer-events: none; }
</style>
