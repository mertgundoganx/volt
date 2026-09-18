<script setup lang="ts">
import type { Auth, CollectionMeta, FolderMeta, KeyValue } from '~/types'
import type { SegmentOption } from '~/utils/ui'

const props = defineProps<{ folderId: string | null; title: string }>()
const emit = defineEmits<{ close: [] }>()
const store = useCollectionStore()

// One sheet for both: a folder and the collection carry the same three things,
// and the only difference is which file they are written to.
const headers = ref<KeyValue[]>([])
const vars = ref<KeyValue[]>([])
const auth = ref<Auth>({ type: 'inherit' })
const loading = ref(true)
const saving = ref(false)
const folder = ref<FolderMeta | null>(null)

const authKinds: SegmentOption<Auth['type']>[] = [
  { value: 'inherit', label: 'Inherit' },
  { value: 'none', label: 'None' },
  { value: 'bearer', label: 'Bearer' },
  { value: 'basic', label: 'Basic' },
  { value: 'apikey', label: 'API key' },
]

onMounted(async () => {
  if (props.folderId) {
    const meta = await store.loadFolder(props.folderId)
    folder.value = meta
    headers.value = meta?.headers ?? []
    vars.value = meta?.vars ?? []
    auth.value = meta?.auth ?? { type: 'inherit' }
  } else {
    const meta = store.collection?.meta
    headers.value = [...(meta?.headers ?? [])]
    vars.value = [...(meta?.vars ?? [])]
    // The collection has nothing above it, so "inherit" would mean nothing.
    auth.value = meta?.auth ?? { type: 'none' }
  }
  loading.value = false
})

const kinds = computed(() => (props.folderId ? authKinds : authKinds.filter((k) => k.value !== 'inherit')))

const authKind = computed({
  get: () => auth.value.type,
  set: (type: Auth['type']) => {
    const blank: Record<Auth['type'], Auth> = {
      none: { type: 'none' },
      inherit: { type: 'inherit' },
      bearer: { type: 'bearer', token: '' },
      basic: { type: 'basic', username: '', password: '' },
      apikey: { type: 'apikey', key: '', value: '', location: 'header' },
      digest: { type: 'digest', username: '', password: '' },
      ntlm: { type: 'ntlm', username: '', password: '', domain: '' },
      awssigv4: { type: 'awssigv4', key_id: '', secret: '', region: 'us-east-1', service: '', session_token: '' },
    }
    auth.value = auth.value.type === type ? auth.value : blank[type]
  },
})

const known = computed(() => store.environment?.vars.map((v) => v.name) ?? [])

async function save() {
  saving.value = true
  const clean = (rows: KeyValue[]) => rows.filter((row) => row.name.trim())

  const ok = props.folderId
    ? await store.saveFolder(props.folderId, {
        ...(folder.value ?? { name: null, seq: 1, headers: [], auth: { type: 'inherit' }, vars: [] }),
        headers: clean(headers.value),
        vars: clean(vars.value),
        auth: auth.value,
      })
    : await store.saveCollectionMeta({
        ...(store.collection?.meta as CollectionMeta),
        headers: clean(headers.value),
        vars: clean(vars.value),
        auth: auth.value,
      })

  saving.value = false
  if (ok) emit('close')
}
</script>

<template>
  <UiDialog :title="props.title" :eyebrow="props.folderId ? 'Folder' : 'Collection'" :width="760" @close="emit('close')">
    <p class="note">
      <UiIcon name="info" :size="14" />
      <span>
        Everything inside gets these. A request sets the same header name itself and its own
        wins; auth here is used by anything set to inherit.
      </span>
    </p>

    <p v-if="loading" class="empty">Reading…</p>
    <template v-else>
      <section>
        <h3 class="silk">Headers</h3>
        <KeyValueEditor v-model="headers" name-label="Header" add-label="Add header" :known="known" />
      </section>

      <section>
        <h3 class="silk">Variables</h3>
        <p class="hint">
          <span>Plain values, committed with the collection. An environment overrides them; secrets belong there, not here.</span>
        </p>
        <KeyValueEditor v-model="vars" name-label="Variable" add-label="Add variable" :known="known" />
      </section>

      <section>
        <h3 class="silk">Auth</h3>
        <UiSegmented v-model="authKind" :options="kinds" label="Auth type" size="sm" />

        <div v-if="auth.type === 'bearer'" class="auth-fields">
          <UiVarInput v-model="auth.token" label="Token" :known="known" placeholder="{{token}}" />
        </div>
        <div v-else-if="auth.type === 'basic'" class="auth-fields two">
          <UiVarInput v-model="auth.username" label="Username" :known="known" />
          <UiVarInput v-model="auth.password" label="Password" :known="known" />
        </div>
        <div v-else-if="auth.type === 'apikey'" class="auth-fields two">
          <UiVarInput v-model="auth.key" label="Key" :known="known" placeholder="X-API-Key" />
          <UiVarInput v-model="auth.value" label="Value" :known="known" placeholder="{{apiKey}}" />
          <UiSegmented
            v-model="auth.location"
            :options="[{ value: 'header', label: 'Header' }, { value: 'query', label: 'Query string' }]"
            label="Where the key goes"
            size="sm"
          />
        </div>
      </section>
    </template>

    <template #footer>
      <span class="spacer" />
      <button type="button" class="btn btn-quiet" @click="emit('close')">Cancel</button>
      <button type="button" class="btn btn-primary" :disabled="saving || loading" @click="save">
        {{ saving ? 'Saving' : 'Save' }}
      </button>
    </template>
  </UiDialog>
</template>

<style scoped>
.note {
  display: flex;
  gap: var(--s-2);
  margin: 0 0 var(--s-4);
  color: var(--silk);
  font-size: var(--t-small);
  line-height: 1.55;
}
.note .ui-icon { margin-top: 2px; flex: none; }
.empty { margin: var(--s-6) 0; color: var(--silk); text-align: center; }

section { margin-bottom: var(--s-5); }
section + section { border-top: 1px solid var(--line-soft); padding-top: var(--s-3); }
h3 { margin: 0 0 var(--s-2); }
.hint { display: flex; margin: 0 0 var(--s-2); color: var(--faint); font-size: var(--t-meta); }
section :deep(.kv) { padding-left: 0; padding-right: 0; }
.auth-fields { display: grid; gap: var(--s-2); margin-top: var(--s-3); max-width: 520px; }
.auth-fields.two { grid-template-columns: repeat(2, minmax(0, 1fr)); }
.auth-fields.two :deep(.ui-var-input:nth-child(n+3)) { grid-column: 1 / -1; }
</style>
