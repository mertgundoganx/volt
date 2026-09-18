<script setup lang="ts">
import type { Generated } from '~/types'
import type { SegmentOption } from '~/utils/ui'

const store = useCollectionStore()
const emit = defineEmits<{ close: [] }>()

// Kept in step with codegen::Language in Rust, which serializes kebab-case.
const languages: SegmentOption<string>[] = [
  { value: 'js-fetch', label: 'fetch' },
  { value: 'js-axios', label: 'axios' },
  { value: 'python', label: 'Python' },
  { value: 'go', label: 'Go' },
  { value: 'csharp', label: 'C#' },
  { value: 'php', label: 'PHP' },
  { value: 'ruby', label: 'Ruby' },
]

const language = ref(languages[0]!.value)
const withSecrets = ref(false)
const generated = ref<Generated | null>(null)
const copied = ref(false)

async function regenerate() {
  generated.value = await store.generateCode(language.value, withSecrets.value)
}
watch([language, withSecrets], regenerate)
onMounted(regenerate)

// Inline in the template, the inner braces would close the interpolation.
const asVars = (names: string[]) => names.map((n) => `{{` + n + `}}`).join(", ")

async function copy() {
  if (!generated.value) return
  try {
    await navigator.clipboard.writeText(generated.value.code)
    copied.value = true
    setTimeout(() => (copied.value = false), 1400)
  } catch {
    store.error = 'Could not copy to the clipboard.'
  }
}
</script>

<template>
  <UiDialog title="Generate code" :eyebrow="store.request?.name ?? 'Request'" :width="760" @close="emit('close')">
    <div class="bar">
      <UiSegmented v-model="language" :options="languages" label="Language" size="sm" />
      <span class="spacer" />
      <label class="secrets">
        <input v-model="withSecrets" type="checkbox">
        <span>Include secret values</span>
      </label>
    </div>

    <div class="code">
      <UiCodeView v-if="generated" :text="generated.code" language="text" wrap />
      <p v-else class="empty">Generating…</p>
    </div>

    <p v-if="generated?.hidden.length" class="hint">
      <UiIcon name="lock" :size="13" />
      <span>Secrets left as placeholders: {{ asVars(generated.hidden) }}. Safe to paste anywhere.</span>
    </p>
    <p v-if="generated?.undefined.length" class="hint warn">
      <UiIcon name="warning" :size="13" />
      <span>No value anywhere for {{ generated.undefined.join(', ') }} — the snippet keeps the placeholder.</span>
    </p>

    <template #footer>
      <span class="silk">Resolved the way Send resolves it</span>
      <span class="spacer" />
      <button type="button" class="btn btn-quiet" @click="emit('close')">Close</button>
      <button type="button" class="btn btn-primary" :disabled="!generated" @click="copy">
        <UiIcon :name="copied ? 'check' : 'copy'" :size="14" />{{ copied ? 'Copied' : 'Copy' }}
      </button>
    </template>
  </UiDialog>
</template>

<style scoped>
.bar { display: flex; align-items: center; gap: var(--s-3); margin-bottom: var(--s-3); }
.secrets { display: flex; align-items: center; gap: var(--s-2); font-size: var(--t-small); cursor: pointer; }

.code {
  max-height: 46vh;
  overflow: auto;
  border: 1px solid var(--line-soft);
  border-radius: var(--r-sm);
  background: var(--well);
  --code-bg: var(--well);
}
.empty { margin: var(--s-6); color: var(--silk); text-align: center; }

.hint { display: flex; align-items: flex-start; gap: var(--s-2); margin: var(--s-3) 0 0; color: var(--silk); font-size: var(--t-meta); }
.hint.warn { color: var(--warn); }
.hint .ui-icon { margin-top: 1px; flex: none; }
</style>
