<script setup lang="ts">
import { isCurl } from '~/utils/curl'

const store = useCollectionStore()
const target = computed(() => store.curlDialog!)

const command = ref('')
const busy = ref(false)
const error = ref<string | null>(null)

const where = computed(() => {
  if (target.value.mode === 'replace') return null
  const parent = target.value.parent
  return parent ? (store.findNode(parent)?.name ?? parent) : 'the collection root'
})

const looksRight = computed(() => isCurl(command.value))
const typed = computed(() => command.value.trim().length > 0)

function close() {
  store.curlDialog = null
}

async function submit() {
  if (!looksRight.value || busy.value) return
  busy.value = true
  error.value = null
  // On failure keep the dialog and the pasted text; the reason shows here.
  const ok = await store.importCurl(command.value, target.value)
  busy.value = false
  if (ok) {
    close()
    return
  }
  error.value = store.error
  store.error = null
}

function onKey(event: KeyboardEvent) {
  if (event.key === 'Enter' && (event.ctrlKey || event.metaKey)) {
    event.preventDefault()
    submit()
  }
}
</script>

<template>
  <UiDialog
    :title="target.mode === 'replace' ? 'Fill this request from curl' : 'New request from curl'"
    eyebrow="cURL"
    :width="680"
    @close="close"
  >
    <p class="lede">
      Paste a command, including multi-line ones and a browser's "Copy as cURL" in either the bash or the
      Windows form.
      <template v-if="where">The request is saved in <b>{{ where }}</b>.</template>
      <template v-else>The method, URL, headers, body and auth are replaced; the name and notes stay.</template>
    </p>

    <div class="editor" :class="{ wrong: typed && !looksRight }" @keydown="onKey">
      <UiCodeEditor v-model="command" label="curl command" placeholder="curl 'https://api.example.com/users' -H 'Accept: application/json'" />
    </div>

    <p v-if="error" class="error mono" role="alert">{{ error }}</p>
    <p v-else-if="typed && !looksRight" class="hint warn">It should start with <code>curl</code>.</p>
    <p v-else class="hint">
      <UiIcon name="lock" :size="13" />
      <span>Tokens, passwords, API keys and cookies in the command go to secret variables, never into the YAML.</span>
    </p>

    <template #footer>
      <span class="kbd">{{ modKey }} ↵</span>
      <span class="spacer" />
      <button type="button" class="btn btn-quiet" @click="close">Cancel</button>
      <button type="button" class="btn btn-primary" :disabled="!looksRight || busy" @click="submit">
        {{ busy ? 'Reading' : target.mode === 'replace' ? 'Fill request' : 'Create request' }}
      </button>
    </template>
  </UiDialog>
</template>

<style scoped>
.lede { margin: 0 0 var(--s-3); color: var(--ink-2); font-size: var(--t-small); line-height: 1.55; }
.lede b { font-weight: 600; color: var(--ink); }
code { font-family: var(--font-mono); font-size: 0.95em; }

.editor {
  display: flex;
  height: 220px;
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
  overflow: hidden;
}
.editor:focus-within { border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-tint); }
.editor.wrong { border-color: var(--warn); }
.editor :deep(.ui-editor:focus-within) { box-shadow: none; }

.hint { display: flex; align-items: center; gap: 7px; margin: var(--s-3) 0 0; color: var(--silk); font-size: var(--t-small); }
.hint.warn { color: var(--warn); }

.error {
  margin: var(--s-3) 0 0;
  padding: var(--s-2) var(--s-3);
  border: 1px solid color-mix(in srgb, var(--bad) 45%, transparent);
  border-radius: var(--r-sm);
  background: var(--bad-tint);
  color: var(--bad);
  font-size: var(--t-small);
  white-space: pre-wrap;
}
</style>
