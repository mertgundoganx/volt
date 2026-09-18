<script setup lang="ts">
/**
 * The GraphQL body: the query, its variables, and what the endpoint said it
 * can do.
 *
 * The schema is fetched on request and held for the session only — it belongs
 * to the server, not to the collection, so nothing about it is written to a
 * file. With one in hand the editor offers the fields that belong where the
 * caret is, and marks the ones the schema does not have.
 */
import type { GraphQlCompletion, GraphQlProblem, GraphQlSchema } from '~/types'

const props = defineProps<{ modelValue: { type: 'graphql', query: string, variables: string } }>()
const emit = defineEmits<{ 'update:modelValue': [value: typeof props.modelValue] }>()

const store = useCollectionStore()
const schema = ref<GraphQlSchema | null>(null)
const fetching = ref(false)
const problems = ref<GraphQlProblem[]>([])
const completions = ref<GraphQlCompletion[]>([])
const caret = ref(0)
const browsing = ref(false)
const openType = ref('')
const editor = ref<{ insert: (text: string, back?: number) => void } | null>(null)

const query = computed({
  get: () => props.modelValue.query,
  set: (value: string) => emit('update:modelValue', { ...props.modelValue, query: value }),
})
const variables = computed({
  get: () => props.modelValue.variables,
  set: (value: string) => emit('update:modelValue', { ...props.modelValue, variables: value }),
})

async function introspect() {
  fetching.value = true
  try {
    schema.value = await store.introspect()
    if (schema.value) await reread()
  } finally {
    fetching.value = false
  }
}

/** The marks and the offer, both from the same walk in Rust. */
async function reread() {
  if (!schema.value) {
    problems.value = []
    completions.value = []
    return
  }
  problems.value = await store.graphqlProblems(query.value)
  completions.value = await store.graphqlCompletions(query.value, caret.value)
}

let pending: ReturnType<typeof setTimeout> | undefined
function later() {
  clearTimeout(pending)
  pending = setTimeout(reread, 120)
}

function onCaret(at: number) {
  caret.value = at
  later()
}

watch(query, later)
watch(
  () => store.activeId,
  () => {
    schema.value = null
    problems.value = []
    completions.value = []
  },
)
onBeforeUnmount(() => clearTimeout(pending))

function take(completion: GraphQlCompletion) {
  editor.value?.insert(completion.name, completion.prefix.length)
  store.touch()
}

/** `{ me { emial } }` → `2:8`, which is how an editor says where. */
function place(at: number) {
  const before = query.value.slice(0, at)
  const line = before.split('\n').length
  const column = at - (before.lastIndexOf('\n') + 1) + 1
  return `${line}:${column}`
}

const rootTypes = computed(() => {
  if (!schema.value) return []
  return [schema.value.queryType, schema.value.mutationType, schema.value.subscriptionType].filter(Boolean)
})
</script>

<template>
  <div class="graphql">
    <div class="pane-half">
      <div class="head">
        <span class="silk">Query</span>
        <span v-if="schema" class="chip">{{ schema.types.length }} types</span>
        <!-- A short pane can hide the list below, so the count is in the header too. -->
        <span v-if="problems.length" class="chip bad">{{ problems.length }} not in the schema</span>
        <span class="spacer" />
        <button
          v-if="schema"
          type="button"
          class="btn btn-quiet btn-sm"
          :aria-expanded="browsing"
          @click="browsing = !browsing"
        >
          <UiIcon name="braces" :size="14" />Schema
        </button>
        <button type="button" class="btn btn-sm" :disabled="fetching" @click="introspect">
          {{ fetching ? 'Asking' : schema ? 'Refresh schema' : 'Fetch schema' }}
        </button>
      </div>

      <UiCodeEditor
        ref="editor"
        v-model="query"
        label="GraphQL query"
        placeholder="query { me { id name } }"
        @caret="onCaret"
        @update:model-value="store.touch()"
      />

      <div v-if="schema && completions.length" class="offer" aria-label="Fields that belong here">
        <span class="silk">Next</span>
        <button
          v-for="field in completions.slice(0, 12)"
          :key="field.name"
          type="button"
          class="chip offer-item"
          :title="field.description || `${field.name}: ${field.typeName}`"
          @click="take(field)"
        >
          <span class="mono">{{ field.name }}</span>
          <span class="of mono">{{ field.typeName }}</span>
        </button>
      </div>

      <ul v-if="problems.length" class="problems" aria-label="Fields the schema does not have">
        <li v-for="problem in problems" :key="problem.at">
          <span class="led bad" />
          <span><span class="mono where">{{ place(problem.at) }}</span> {{ problem.message }}</span>
        </li>
      </ul>
      <p v-else-if="schema" class="agrees">
        <span class="led ok" /><span>Every field in this query is in the schema.</span>
      </p>
    </div>

    <div class="pane-half">
      <template v-if="browsing && schema">
        <div class="head">
          <span class="silk">Schema</span>
          <span class="spacer" />
          <button type="button" class="icon-btn quiet sm" aria-label="Back to the variables" @click="browsing = false">
            <UiIcon name="x" :size="14" />
          </button>
        </div>
        <div class="browser">
          <div v-for="kind in schema.types" :key="kind.name" class="kind">
            <button
              type="button"
              class="kind-head"
              :aria-expanded="openType === kind.name"
              @click="openType = openType === kind.name ? '' : kind.name"
            >
              <UiIcon :name="openType === kind.name ? 'chevron-down' : 'chevron-right'" :size="14" />
              <span class="mono">{{ kind.name }}</span>
              <span v-if="rootTypes.includes(kind.name)" class="chip">root</span>
            </button>
            <ul v-if="openType === kind.name" class="fields">
              <li v-if="kind.description" class="says">{{ kind.description }}</li>
              <li v-for="field in kind.fields" :key="field.name">
                <span class="mono">{{ field.name }}<template v-if="field.args.length">({{ field.args.join(', ') }})</template></span>
                <span class="of mono">{{ field.typeName }}</span>
              </li>
            </ul>
          </div>
        </div>
      </template>

      <template v-else>
        <span class="silk">Variables</span>
        <UiCodeEditor
          v-model="variables"
          label="GraphQL variables"
          placeholder="{ }"
          @update:model-value="store.touch()"
        />
      </template>
    </div>
  </div>
</template>

<style scoped>
/* When the request pane is shorter than the editor needs, this scrolls
   rather than squashing the query, the offer and the marks into each other. */
.graphql {
  flex: 1;
  display: grid;
  grid-template-columns: minmax(0, 1.4fr) minmax(0, 1fr);
  gap: var(--s-4);
  min-height: 0;
  overflow: auto;
  padding: var(--s-3) var(--s-4) var(--s-4);
}
.pane-half { display: flex; flex-direction: column; gap: var(--s-2); min-height: 220px; min-width: 0; }
.pane-half :deep(.ui-editor) { flex: 1; min-height: 110px; border: 1px solid var(--line-soft); border-radius: var(--r-sm); }
.head, .offer, .problems, .agrees { flex: none; }
.head { display: flex; align-items: center; gap: var(--s-2); flex-wrap: wrap; }
.spacer { flex: 1; }

.offer { display: flex; align-items: center; gap: var(--s-2); flex-wrap: wrap; }
.offer-item { display: inline-flex; align-items: center; gap: var(--s-2); cursor: pointer; }
.offer-item:active { transform: translateY(1px); }
.offer-item:hover { background: var(--hover); }
.of { color: var(--faint); font-size: var(--t-meta); }

.problems { list-style: none; margin: 0; padding: 0; display: grid; gap: 4px; max-height: 96px; overflow: auto; }
.problems li, .agrees { display: flex; align-items: center; gap: var(--s-2); font-size: var(--t-small); margin: 0; }
.where { color: var(--faint); font-size: var(--t-meta); }

.browser { overflow: auto; min-height: 0; display: grid; gap: 1px; align-content: start; }
.kind-head {
  display: flex;
  align-items: center;
  gap: var(--s-2);
  width: 100%;
  padding: 4px var(--s-2);
  border: 0;
  border-radius: var(--r-sm);
  background: none;
  color: var(--ink);
  font-size: var(--t-small);
  text-align: left;
  cursor: pointer;
}
.kind-head:hover { background: var(--hover); }
.fields { list-style: none; margin: 0 0 var(--s-2) 26px; padding: 0; display: grid; gap: 2px; }
.fields li { display: flex; align-items: baseline; gap: var(--s-2); font-size: var(--t-small); }
.says { color: var(--silk); font-size: var(--t-meta); }
</style>
