<script setup lang="ts">
import type { FormField } from '~/types'

const rows = defineModel<FormField[]>({ required: true })
const props = withDefaults(
  defineProps<{
    nameLabel?: string
    valueLabel?: string
    addLabel?: string
    known?: string[] | null
    /** Multipart only: a row may send a file instead of text. */
    files?: boolean
  }>(),
  { nameLabel: 'Key', valueLabel: 'Value', addLabel: 'Add', known: null, files: false },
)

/** Always keep one blank row at the bottom so there is no "add" button. */
const display = computed(() => {
  const last = rows.value[rows.value.length - 1]
  return last && !last.name && !last.value ? rows.value : [...rows.value, blank()]
})

function blank(): FormField {
  return { name: '', value: '', enabled: true }
}

function isBlank(row: FormField) {
  return !row.name && !row.value
}

function update(index: number, patch: Partial<FormField>) {
  const next = [...display.value]
  next[index] = { ...next[index]!, ...patch }
  // Drop rows the user has emptied out, but never the trailing blank.
  rows.value = next.filter((row, i) => row.name || row.value || i === next.length - 1)
}

/**
 * Bulk edit: the same rows as `name: value` lines, for pasting a block in or
 * out. A disabled row keeps a leading `#`, and only the first colon splits, so
 * `X-Next: https://a/b` keeps its value whole.
 */
const bulk = ref(false)
const text = ref('')

function openBulk() {
  text.value = rows.value
    .filter((row) => row.name || row.value)
    .map((row) => `${row.enabled ? '' : '# '}${row.name}: ${row.value}`)
    .join('\n')
  bulk.value = true
}

function parseBulk(input: string) {
  text.value = input
  rows.value = input
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => {
      const enabled = !line.startsWith('#')
      const body = enabled ? line : line.replace(/^#\s*/, '')
      const at = body.indexOf(':')
      return {
        name: (at === -1 ? body : body.slice(0, at)).trim(),
        value: (at === -1 ? '' : body.slice(at + 1)).trim(),
        enabled,
      }
    })
    .filter((row) => row.name || row.value)
}


function remove(index: number) {
  rows.value = display.value.filter((_, i) => i !== index).filter((r) => r.name || r.value)
}
</script>

<template>
  <div class="kv" :class="{ 'with-files': props.files }">
    <div class="bar">
      <button
        type="button"
        class="btn btn-quiet btn-sm mode"
        :aria-pressed="bulk"
        @click="bulk ? (bulk = false) : openBulk()"
      >
        {{ bulk ? 'Table' : 'Bulk edit' }}
      </button>
    </div>

    <textarea
      v-if="bulk"
      class="field mono bulk"
      :aria-label="`${props.nameLabel}s as text`"
      spellcheck="false"
      :placeholder="`${props.nameLabel}: value\n# a line starting with # is switched off`"
      :value="text"
      @input="parseBulk(($event.target as HTMLTextAreaElement).value)"
    />

    <div v-else class="table" role="table" :aria-label="`${props.nameLabel}s`">
      <div class="row head" role="row">
        <span role="columnheader"><span class="visually-hidden">Enabled</span></span>
        <span role="columnheader" class="silk">{{ props.nameLabel }}</span>
        <span role="columnheader" class="silk">{{ props.valueLabel }}</span>
        <span role="columnheader" />
      </div>

      <div
        v-for="(row, i) in display"
        :key="i"
        class="row"
        role="row"
        :class="{ off: !row.enabled && !isBlank(row), blank: isBlank(row) }"
      >
        <span class="check">
          <input
            type="checkbox"
            :checked="row.enabled"
            :disabled="isBlank(row)"
            :aria-label="`Send ${row.name || 'this row'}`"
            @change="update(i, { enabled: ($event.target as HTMLInputElement).checked })"
          >
        </span>
        <input
          class="field inline mono name"
          :value="row.name"
          :placeholder="isBlank(row) ? props.addLabel : ''"
          :aria-label="props.nameLabel"
          spellcheck="false"
          @input="update(i, { name: ($event.target as HTMLInputElement).value })"
        >
        <UiVarInput
          variant="inline"
          :model-value="row.value"
          :known="props.known"
          :label="props.valueLabel"
          @update:model-value="update(i, { value: $event })"
        />
        <span v-if="props.files" class="center">
          <button
            type="button"
            class="as-file"
            :class="{ on: row.file }"
            :disabled="isBlank(row)"
            :aria-pressed="!!row.file"
            :aria-label="row.file ? 'Sends a file' : 'Sends text'"
            :title="row.file ? 'A file: the value is a path, taken from the collection folder' : 'Text'"
            @click="update(i, { file: !row.file })"
          >
            <UiIcon :name="row.file ? 'file' : 'braces'" :size="13" />
          </button>
        </span>
        <span class="tools">
          <button
            v-if="!isBlank(row)"
            type="button"
            class="icon-btn quiet sm"
            :aria-label="`Remove ${row.name || 'row'}`"
            title="Remove"
            @click="remove(i)"
          >
            <UiIcon name="x" :size="14" />
          </button>
        </span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.kv { padding: var(--s-1) var(--s-5) var(--s-5); }

.row {
  display: grid;
  grid-template-columns: 28px minmax(0, 1fr) minmax(0, 1.7fr) 28px;
}
.kv.with-files .row {
  grid-template-columns: 28px minmax(0, 1fr) minmax(0, 1.7fr) 30px 28px;
  gap: var(--s-1);
  align-items: center;
  min-height: 34px;
  border-bottom: 1px solid var(--line-soft);
}
.row.head { min-height: 28px; }
/* Line the labels up with the text inside the inline fields (padding + border). */
.row.head .silk { padding-left: 9px; }
.row.blank { border-bottom: 0; }

.check { display: grid; place-items: center; }
/* Nothing to enable on the add row, so no box to misread as "on". */
.row.blank .check input { visibility: hidden; }

.name { font-size: var(--t-code); }
.off .name, .off :deep(.mirror) { color: var(--faint); text-decoration: line-through; text-decoration-color: color-mix(in srgb, var(--faint) 60%, transparent); }

.tools { display: grid; place-items: center; opacity: 0; transition: opacity var(--dur) var(--ease); }
.row:hover .tools, .row:focus-within .tools { opacity: 1; }

.bar { display: flex; justify-content: flex-end; padding-bottom: 2px; }
.mode { height: 22px; padding: 0 7px; font-size: var(--t-meta); color: var(--silk); }
.mode[aria-pressed="true"] { background: var(--accent-tint); color: var(--accent-text); }
.bulk {
  width: 100%;
  height: auto;
  min-height: 180px;
  padding: var(--s-2) var(--s-3);
  line-height: 1.6;
  resize: vertical;
}

.as-file {
  display: grid;
  place-items: center;
  width: 26px;
  height: 22px;
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
  color: var(--silk);
  background: var(--well);
  transition: background var(--dur) var(--ease), color var(--dur) var(--ease);
}
.as-file.on { background: var(--accent); border-color: var(--accent); color: var(--accent-ink); }
.as-file:disabled { opacity: 0.3; }
</style>
