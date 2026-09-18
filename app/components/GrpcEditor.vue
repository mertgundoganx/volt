<script setup lang="ts">
import type { GrpcMethod, GrpcService } from '~/types'

const store = useCollectionStore()
const body = defineModel<{ type: 'grpc'; proto: string; method: string; message: string }>({ required: true })

const services = ref<GrpcService[]>([])
const loading = ref(false)
const error = ref<string | null>(null)

const methods = computed(() => services.value.flatMap((service) => service.methods))
const chosen = computed(() => methods.value.find((method) => method.fullName === body.value.method) ?? null)

/** Read the proto: the file is the source of what can be called. */
async function load() {
  const proto = body.value.proto.trim()
  if (!proto) return
  await reading(() => store.loadGrpcServices(proto), 'No services in that file.')
}

/**
 * Ask the server instead. Some endpoints describe themselves, which is the
 * case where there is no `.proto` on disk to point at — and nothing about the
 * answer is written down, because a schema belongs to the server.
 */
async function reflect() {
  await reading(() => store.reflectGrpc(), 'The server listed no services.')
}

async function reading(ask: () => Promise<GrpcService[]>, blank: string) {
  loading.value = true
  error.value = null
  services.value = await ask()
  if (!services.value.length) error.value = store.error ?? blank
  store.error = null
  loading.value = false
}

watch(() => body.value.proto, () => { services.value = [] })
onMounted(() => { if (body.value.proto.trim()) load() })

function pick(fullName: string) {
  body.value = { ...body.value, method: fullName }
  const method = methods.value.find((one) => one.fullName === fullName)
  // Start from the shape of the message rather than an empty editor.
  if (method && !body.value.message.trim()) body.value = { ...body.value, message: method.example }
  store.touch()
}

function describe(method: GrpcMethod) {
  const shape = `${method.input} → ${method.output}`
  if (method.clientStreaming && method.serverStreaming) return `${shape} · both ways, streaming`
  if (method.clientStreaming) return `${shape} · streaming in`
  if (method.serverStreaming) return `${shape} · streaming out`
  return shape
}
</script>

<template>
  <div class="grpc">
    <div class="pick">
      <span class="silk">Proto</span>
      <input
        :value="body.proto"
        class="field mono"
        aria-label="Path to the proto file"
        placeholder="protos/orders.proto"
        spellcheck="false"
        @input="body = { ...body, proto: ($event.target as HTMLInputElement).value }; store.touch()"
      >
      <button type="button" class="btn btn-sm" :disabled="loading || !body.proto.trim()" @click="load">
        {{ loading ? 'Reading' : 'Read it' }}
      </button>
      <button type="button" class="btn btn-quiet btn-sm" :disabled="loading" title="For a server that describes itself" @click="reflect">
        Ask the server
      </button>
    </div>

    <div v-if="methods.length" class="pick">
      <span class="silk">Method</span>
      <UiSelect
        :model-value="body.method"
        :options="methods.map((method) => ({ value: method.fullName, label: method.fullName }))"
        label="Method to call"
        mono
        class="method"
        @update:model-value="pick($event)"
      />
      <span v-if="chosen" class="shape mono">{{ describe(chosen) }}</span>
    </div>
    <p v-else-if="error" class="error mono">{{ error }}</p>
    <p v-else class="hint">
      <span>Point at a <code>.proto</code> and read it, or ask the server if it describes itself. Either way, the services are what can be called.</span>
    </p>

    <div class="message">
      <span class="silk">Message</span>
      <UiCodeEditor
        :model-value="body.message"
        label="Request message"
        placeholder="{ }"
        @update:model-value="body = { ...body, message: $event }; store.touch()"
      />
    </div>
  </div>
</template>

<style scoped>
.grpc { flex: 1; display: grid; grid-template-rows: auto auto minmax(0, 1fr); gap: var(--s-3); min-height: 0; padding: var(--s-3) var(--s-4) var(--s-4); }
.pick { display: flex; align-items: center; gap: var(--s-3); }
.pick .field { flex: 1; max-width: 420px; }
.method { flex: 1; max-width: 420px; }
.shape { font-size: var(--t-meta); color: var(--silk); }
.hint { display: flex; margin: 0; color: var(--faint); font-size: var(--t-meta); }
.error { margin: 0; color: var(--bad); font-size: var(--t-small); }
.message { display: grid; grid-template-rows: auto minmax(0, 1fr); gap: 4px; min-height: 0; }
.message :deep(.ui-editor) { border: 1px solid var(--line-soft); border-radius: var(--r-sm); }
</style>
