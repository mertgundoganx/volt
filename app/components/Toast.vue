<script setup lang="ts">
const store = useCollectionStore()
</script>

<template>
  <div class="toast-slot" aria-live="polite">
    <Transition name="toast">
      <div v-if="store.toast" :key="store.toast.id" class="toast" role="status">
        <span class="led" :class="store.toast.tone" />
        <div class="text">
          <span class="title">{{ store.toast.text }}</span>
          <span v-if="store.toast.detail" class="detail mono">{{ store.toast.detail }}</span>
        </div>
        <button
          v-if="store.lastDeleted && store.toast.undo"
          type="button"
          class="btn btn-quiet btn-sm undo"
          @click="store.undoDelete(); store.toast = null"
        >
          Undo
        </button>
        <button type="button" class="icon-btn quiet sm" aria-label="Dismiss" @click="store.toast = null">
          <UiIcon name="x" :size="13" />
        </button>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.toast-slot { position: fixed; right: var(--s-4); bottom: var(--s-4); z-index: 70; pointer-events: none; }

.toast {
  display: flex;
  align-items: flex-start;
  gap: var(--s-3);
  max-width: 420px;
  padding: 10px 8px 10px 14px;
  background: var(--bg-3);
  border: 1px solid var(--line);
  border-radius: var(--r-md);
  box-shadow: var(--shadow-pop);
  pointer-events: auto;
}
.led { margin-top: 5px; }
.undo { flex: none; }
.text { flex: 1; display: grid; gap: 3px; min-width: 0; }
.title { font-weight: 600; }
.detail { font-size: var(--t-meta); color: var(--silk); overflow-wrap: anywhere; }

.toast-enter-active, .toast-leave-active { transition: opacity 160ms var(--ease), transform 160ms var(--ease); }
.toast-enter-from, .toast-leave-to { opacity: 0; transform: translateY(12px) scale(0.98); }
</style>
