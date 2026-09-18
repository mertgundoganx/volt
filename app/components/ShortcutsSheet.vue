<script setup lang="ts">
const emit = defineEmits<{ close: [] }>()
const shortcuts = useShortcutList()
</script>

<template>
  <UiDialog title="Keyboard" eyebrow="Shortcuts" :width="460" @close="emit('close')">
    <dl class="keys">
      <template v-for="binding in shortcuts" :key="binding.keys">
        <dt>{{ binding.label }}</dt>
        <dd>
          <span v-for="part in shortcutLabel(binding.keys).split(' ')" :key="part" class="kbd">{{ part }}</span>
        </dd>
      </template>
    </dl>
    <p class="note">Shortcuts are off while a dialog like this one is open.</p>
  </UiDialog>
</template>

<style scoped>
.keys { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 2px var(--s-4); margin: 0; align-items: center; }
dt { padding: 5px 0; font-size: var(--t-small); border-top: 1px solid var(--line-soft); }
dd { display: flex; gap: 4px; margin: 0; padding: 5px 0; justify-content: flex-end; border-top: 1px solid var(--line-soft); }
dt:first-of-type, dt:first-of-type + dd { border-top: 0; }
.note { margin: var(--s-4) 0 0; color: var(--silk); font-size: var(--t-meta); }
</style>
