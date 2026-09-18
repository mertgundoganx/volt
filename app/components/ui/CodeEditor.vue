<script setup lang="ts">
/** A plain textarea with a line-number gutter, for request bodies. */
const props = withDefaults(defineProps<{ modelValue: string; placeholder?: string; label: string }>(), {
  placeholder: '',
})
const emit = defineEmits<{ 'update:modelValue': [value: string], caret: [at: number] }>()

const area = ref<HTMLTextAreaElement>()
const scrollTop = ref(0)
const count = computed(() => Math.max(1, props.modelValue.split('\n').length))

function onInput(event: Event) {
  const el = event.target as HTMLTextAreaElement
  emit('update:modelValue', el.value)
  emit('caret', el.selectionStart)
}

/** Where the caret is, for anything that completes what is being typed. */
function onCaret() {
  if (area.value) emit('caret', area.value.selectionStart)
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

/** Tab indents with two spaces instead of leaving the editor. */
function onKeydown(event: KeyboardEvent) {
  if (event.key !== 'Tab' || event.shiftKey || event.ctrlKey || event.metaKey) return
  const el = area.value
  if (!el) return
  event.preventDefault()
  const { selectionStart: start, selectionEnd: end, value } = el
  el.value = `${value.slice(0, start)}  ${value.slice(end)}`
  el.selectionStart = el.selectionEnd = start + 2
  emit('update:modelValue', el.value)
}
</script>

<template>
  <div class="ui-editor">
    <div class="gutter mono" aria-hidden="true">
      <div :style="{ transform: `translateY(${-scrollTop}px)` }">
        <div v-for="n in count" :key="n">{{ n }}</div>
      </div>
    </div>
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
      @scroll="scrollTop = ($event.target as HTMLTextAreaElement).scrollTop"
    />
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
  border-right: 1px solid var(--line-soft);
  user-select: none;
}

textarea {
  flex: 1;
  min-width: 0;
  padding: var(--s-3) var(--s-4);
  border: 0;
  background: transparent;
  color: var(--ink);
  font-size: var(--t-code);
  line-height: 20px;
  resize: none;
  white-space: pre;
  tab-size: 2;
}
textarea::placeholder { color: var(--faint); }
textarea:focus-visible { outline: none; }
.ui-editor:focus-within { box-shadow: inset 2px 0 0 var(--accent); }
</style>
