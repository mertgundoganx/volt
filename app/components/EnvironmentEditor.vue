<script setup lang="ts">
import type { EnvVar } from '~/types'

const store = useCollectionStore()
const emit = defineEmits<{ close: [] }>()

/**
 * One environment being edited. `previous` is the name it was loaded under —
 * null for one that has never been written, which is also how Delete knows
 * there is no file to remove. Everything is edited in memory; nothing touches
 * disk until Save.
 */
interface Draft {
  key: number
  previous: string | null
  name: string
  vars: EnvVar[]
  dirty: boolean
  /** Set when a new environment copied its variable names from another. */
  copiedFrom?: string
}

let nextKey = 1
const drafts = ref<Draft[]>(
  store.environments.map((env) => ({
    key: nextKey++,
    previous: env.name,
    name: env.name,
    vars: env.vars.map((v) => ({ ...v })),
    dirty: false,
  })),
)

const selected = ref(drafts.value.find((d) => d.name === store.environment?.name)?.key ?? drafts.value[0]?.key ?? 0)
const current = computed(() => drafts.value.find((d) => d.key === selected.value) ?? null)

/** Shown inside the dialog: the page's error strip sits behind the backdrop. */
const saveError = ref<string | null>(null)
const saving = ref(false)
const nameInput = ref<HTMLInputElement>()

/** Secret values are masked until asked for, one row at a time. */
const revealed = ref(new Set<number>())

const rows = computed(() => {
  const vars = current.value?.vars ?? []
  const last = vars[vars.length - 1]
  return last && !last.name ? vars : [...vars, { name: '', value: '', secret: false }]
})

const named = computed(() => (current.value?.vars ?? []).filter((v) => v.name))
const secretCount = computed(() => named.value.filter((v) => v.secret).length)

/** Two names that would fight over the same file, caught before asking Rust. */
const clash = computed(() => {
  const seen = new Set<string>()
  for (const draft of drafts.value) {
    const name = draft.name.trim().toLowerCase()
    if (!name) continue
    if (seen.has(name)) return draft.name.trim()
    seen.add(name)
  }
  return null
})
const unnamed = computed(() => drafts.value.some((d) => d.dirty && !d.name.trim()))
const problem = computed(() => {
  if (clash.value) return `Two environments are called “${clash.value}”. Give one of them a different name.`
  if (unnamed.value) return 'An environment needs a name before it can be saved.'
  return null
})

const pending = computed(() => drafts.value.filter((d) => d.dirty).length)

const name = computed({
  get: () => current.value?.name ?? '',
  set: (value: string) => {
    if (!current.value) return
    current.value.name = value
    current.value.dirty = true
  },
})

function select(key: number) {
  selected.value = key
  revealed.value = new Set()
  saveError.value = null
}

function update(index: number, patch: Partial<EnvVar>) {
  if (!current.value) return
  const next = [...rows.value]
  next[index] = { ...next[index]!, ...patch }
  current.value.vars = next.filter((row, i) => row.name || i === next.length - 1)
  current.value.dirty = true
}

function removeVar(index: number) {
  if (!current.value) return
  current.value.vars = rows.value.filter((_, i) => i !== index).filter((row) => row.name)
  current.value.dirty = true
  revealed.value = new Set()
}

function toggleReveal(index: number) {
  const next = new Set(revealed.value)
  next.has(index) ? next.delete(index) : next.add(index)
  revealed.value = next
}

/**
 * A new environment starts from the variable names on screen with blank
 * values: the point of a second environment is usually the same keys pointing
 * somewhere else.
 */
function addEnvironment() {
  const taken = new Set(drafts.value.map((d) => d.name.trim().toLowerCase()))
  let suggestion = ['local', 'staging', 'prod', 'dev'].find((n) => !taken.has(n))
  for (let n = 2; !suggestion; n++) {
    if (!taken.has(`environment-${n}`)) suggestion = `environment-${n}`
  }

  const source = current.value
  const vars = (source?.vars ?? []).filter((v) => v.name).map((v) => ({ name: v.name, value: '', secret: v.secret }))
  const draft: Draft = {
    key: nextKey++,
    previous: null,
    name: suggestion,
    vars,
    dirty: true,
    copiedFrom: vars.length ? source?.name : undefined,
  }
  drafts.value = [...drafts.value, draft]
  select(draft.key)
  nextTick(() => nameInput.value?.select())
}

async function removeEnvironment() {
  const draft = current.value
  if (!draft) return
  // Never written: there is no file to delete and nothing to confirm.
  if (draft.previous && !(await store.deleteEnvironment(draft.previous))) return

  const index = drafts.value.indexOf(draft)
  drafts.value = drafts.value.filter((d) => d !== draft)
  select(drafts.value[Math.max(0, index - 1)]?.key ?? 0)
}

async function save() {
  saveError.value = null
  if (problem.value) return

  saving.value = true
  // On failure keep the dialog open on the environment that refused to save;
  // closing would throw the edits away.
  for (const draft of drafts.value) {
    if (!draft.dirty) continue
    const environment = { name: draft.name.trim(), vars: draft.vars.filter((v) => v.name) }
    if (!(await store.saveEnvironment(environment, draft.previous))) {
      saving.value = false
      select(draft.key)
      saveError.value = store.error
      store.error = null
      return
    }
    draft.name = environment.name
    draft.vars = environment.vars
    draft.previous = environment.name
    draft.copiedFrom = undefined
    draft.dirty = false
  }
  saving.value = false
  if (current.value) store.activeEnvironment = current.value.name
  emit('close')
}
</script>

<template>
  <UiDialog :title="current?.name || 'Environments'" eyebrow="Environments" :width="780" @close="emit('close')">
    <template #title>
      <input
        v-if="current"
        ref="nameInput"
        v-model="name"
        class="env-name"
        placeholder="Environment name"
        aria-label="Environment name"
        spellcheck="false"
      >
      <h2 v-else class="env-title">No environment</h2>
    </template>

    <div class="layout">
      <nav class="rail" aria-label="Environments">
        <button
          v-for="draft in drafts"
          :key="draft.key"
          type="button"
          class="env-row"
          :class="{ on: draft.key === selected }"
          :aria-current="draft.key === selected ? 'true' : undefined"
          @click="select(draft.key)"
        >
          <span class="env-label mono">{{ draft.name.trim() || 'Untitled' }}</span>
          <span v-if="draft.dirty" class="dot" title="Unsaved" />
          <span v-else class="count num">{{ draft.vars.filter((v) => v.name).length }}</span>
        </button>

        <button type="button" class="env-row add" @click="addEnvironment">
          <UiIcon name="plus" :size="14" />
          <span class="env-label">New environment</span>
        </button>
      </nav>

      <div class="pane">
        <p class="note">
          <UiIcon name="lock" :size="14" />
          <span>
            Plain values are saved in <code>environments/*.yaml</code> and committed with the collection.
            Values marked secret go to a gitignored <code>.env.&lt;environment&gt;</code> file instead.
          </span>
        </p>

        <p v-if="problem" class="save-warn" role="status">{{ problem }}</p>
        <p v-if="saveError" class="save-error mono" role="alert">{{ saveError }}</p>
        <p v-if="current?.copiedFrom" class="copied">
          Variable names copied from <strong>{{ current.copiedFrom }}</strong>. Fill in the values this environment uses.
        </p>

        <div v-if="current" class="table" role="table" aria-label="Variables">
          <div class="row head" role="row">
            <span role="columnheader" class="silk">Variable</span>
            <span role="columnheader" class="silk">Value</span>
            <span role="columnheader" class="silk center">Secret</span>
            <span role="columnheader" />
          </div>

          <div v-for="(row, i) in rows" :key="i" class="row" role="row" :class="{ blank: !row.name }">
            <input
              class="field mono"
              :value="row.name"
              :placeholder="row.name ? '' : 'baseUrl'"
              aria-label="Variable name"
              spellcheck="false"
              @input="update(i, { name: ($event.target as HTMLInputElement).value })"
            >
            <div class="value">
              <input
                class="field mono"
                :value="row.value"
                :type="row.secret && !revealed.has(i) ? 'password' : 'text'"
                :placeholder="row.name ? '' : 'https://api.example.com'"
                aria-label="Value"
                spellcheck="false"
                autocomplete="off"
                @input="update(i, { value: ($event.target as HTMLInputElement).value })"
              >
              <button
                v-if="row.secret"
                type="button"
                class="icon-btn quiet sm reveal"
                :aria-label="revealed.has(i) ? 'Hide value' : 'Show value'"
                :title="revealed.has(i) ? 'Hide value' : 'Show value'"
                @click="toggleReveal(i)"
              >
                <UiIcon :name="revealed.has(i) ? 'eye-off' : 'eye'" :size="14" />
              </button>
            </div>
            <span class="center">
              <button
                type="button"
                class="secret"
                :class="{ on: row.secret }"
                :disabled="!row.name"
                :aria-pressed="row.secret"
                :aria-label="row.secret ? 'Secret: stored in .env' : 'Plain: stored in YAML'"
                :title="row.secret ? 'Secret — stored in the gitignored .env file' : 'Plain — stored in the environment YAML'"
                @click="update(i, { secret: !row.secret })"
              >
                <UiIcon :name="row.secret ? 'lock' : 'unlock'" :size="14" />
              </button>
            </span>
            <span class="center">
              <button v-if="row.name" type="button" class="icon-btn quiet sm" :aria-label="`Remove ${row.name}`" title="Remove" @click="removeVar(i)">
                <UiIcon name="x" :size="14" />
              </button>
            </span>
          </div>
        </div>

        <p v-else class="empty">Nothing to fill in yet. Add an environment to hold the variables your requests use.</p>
      </div>
    </div>

    <template #footer>
      <button
        v-if="current"
        type="button"
        class="btn btn-quiet btn-sm"
        :aria-label="`Delete ${current.name || 'environment'}`"
        @click="removeEnvironment"
      >
        <UiIcon name="trash" :size="14" />Delete
      </button>
      <span v-if="current" class="silk counts">{{ named.length }} {{ named.length === 1 ? 'variable' : 'variables' }} · {{ secretCount }} secret</span>
      <span class="spacer" />
      <button type="button" class="btn btn-quiet" @click="emit('close')">Cancel</button>
      <button type="button" class="btn btn-primary" :disabled="saving || !!problem || (!current && !pending)" @click="save">
        {{ saving ? 'Saving' : pending > 1 ? `Save ${pending} environments` : 'Save environment' }}
      </button>
    </template>
  </UiDialog>
</template>

<style scoped>
.env-name {
  width: 100%;
  margin-left: -7px;
  padding: 2px 6px;
  border: 1px solid transparent;
  border-radius: var(--r-sm);
  background: transparent;
  font-weight: 700;
  font-size: 18px;
  letter-spacing: -0.012em;
}
.env-name:hover { background: var(--hover); }
.env-name:focus { background: var(--well); border-color: var(--accent); }
.env-title { margin: 0; font-weight: 700; font-size: 18px; }

.layout { display: grid; grid-template-columns: 168px minmax(0, 1fr); gap: var(--s-5); align-items: start; }
.rail {
  display: flex;
  flex-direction: column;
  gap: 1px;
  padding-right: var(--s-4);
  border-right: 1px solid var(--line-soft);
}
.env-row {
  display: flex;
  align-items: center;
  gap: var(--s-2);
  width: 100%;
  padding: 6px var(--s-2);
  border: 1px solid transparent;
  border-radius: var(--r-sm);
  color: var(--ink-2);
  text-align: left;
  transition: background var(--dur) var(--ease), color var(--dur) var(--ease);
}
.env-row:hover { background: var(--hover); color: var(--ink); }
.env-row.on { background: var(--accent-tint); border-color: var(--accent-line); color: var(--ink); }
.env-label { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--t-small); }
.count { color: var(--faint); font-size: var(--t-meta); }
.dot { width: 6px; height: 6px; border-radius: 50%; background: var(--warn); flex: none; }
.add { margin-top: var(--s-2); color: var(--silk); }
.add .ui-icon { flex: none; }

.note {
  display: flex;
  gap: var(--s-2);
  margin: 0 0 var(--s-4);
  color: var(--silk);
  font-size: var(--t-small);
  line-height: 1.55;
}
.note .ui-icon { margin-top: 2px; flex: none; }
code { font-family: var(--font-mono); font-size: 0.95em; color: var(--ink-2); }

.save-error,
.save-warn {
  margin: 0 0 var(--s-4);
  padding: var(--s-2) var(--s-3);
  border-radius: var(--r-sm);
  font-size: var(--t-small);
  white-space: pre-wrap;
}
.save-error {
  border: 1px solid color-mix(in srgb, var(--bad) 45%, transparent);
  background: var(--bad-tint);
  color: var(--bad);
}
.save-warn {
  border: 1px solid color-mix(in srgb, var(--warn) 45%, transparent);
  background: var(--warn-tint);
  color: var(--warn);
}

.copied { margin: 0 0 var(--s-3); color: var(--silk); font-size: var(--t-small); }
.copied strong { color: var(--ink-2); font-weight: 600; }
.empty { padding: var(--s-6) 0; color: var(--silk); font-size: var(--t-small); text-align: center; }

.table { display: grid; }
.row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1.6fr) 56px 28px;
  gap: var(--s-2);
  align-items: center;
  padding: 5px 0;
}
.row.head { padding-bottom: var(--s-2); border-bottom: 1px solid var(--line-soft); margin-bottom: 4px; }
.center { display: grid; place-items: center; }
.counts { white-space: nowrap; }

.value { position: relative; }
.value .field { padding-right: 34px; }
/* Centred with margins, not a transform: `.icon-btn:active` sets its own
   transform, which would drop the button out from under the cursor mid-press. */
.reveal { position: absolute; right: 4px; top: 0; bottom: 0; margin: auto 0; }

.secret {
  display: grid;
  place-items: center;
  width: 30px;
  height: 26px;
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
  color: var(--silk);
  background: var(--well);
  transition: background var(--dur) var(--ease), color var(--dur) var(--ease), border-color var(--dur) var(--ease);
}
.secret:hover:not(:disabled) { color: var(--ink); border-color: var(--line-strong); }
.secret.on { background: var(--accent); border-color: var(--accent); color: var(--accent-ink); }
.secret:disabled { opacity: 0.35; }
</style>
