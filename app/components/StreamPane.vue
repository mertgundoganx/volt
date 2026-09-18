<script setup lang="ts">
const store = useCollectionStore()
const outgoing = ref('')
const list = ref<HTMLElement>()

// The events of this tab's own connection. A socket opened on another tab
// keeps running, but its traffic is not this request's story.
const events = computed(() => (store.myStream || store.streamOwner === store.activeId ? store.streamEvents : []))

const isWebsocket = computed(() => store.myStream?.kind === 'websocket')
const isGrpc = computed(() => store.myStream?.kind === 'grpc')
const clientStreaming = computed(() => {
  const body = store.request?.body
  if (body?.type !== 'grpc') return false
  return Boolean(store.grpcMethods.find((one) => one.fullName === body.method)?.clientStreaming)
})

// Only a call that keeps sending has anything to say after the first message.
const canSay = computed(() => isWebsocket.value || (isGrpc.value && clientStreaming.value))

const label = computed(() => {
  if (!store.myStream) return 'Not connected'
  if (isWebsocket.value) return 'WebSocket'
  return isGrpc.value ? 'gRPC' : 'Server-sent events'
})

/**
 * An empty message means "nothing more from me", which is how a
 * client-streaming call asks for its reply.
 */
async function doneSending() {
  await store.sendStreamText('')
}

async function send() {
  const text = outgoing.value
  if (!text.trim()) return
  await store.sendStreamText(text)
  outgoing.value = ''
}

function when(at: number) {
  return new Date(at).toLocaleTimeString()
}

// New messages arrive at the bottom, which is where the eye already is.
watch(
  () => events.value.length,
  async () => {
    await nextTick()
    const el = list.value
    if (el) el.scrollTop = el.scrollHeight
  },
)
</script>

<template>
  <section class="stream" aria-label="Connection">
    <div class="state">
      <span class="led" :class="store.myStream ? 'ok' : 'off'" />
      <span class="silk">{{ label }}</span>
      <span v-if="store.myStream" class="url mono">{{ store.myStream.url }}</span>
      <span class="spacer" />
      <span class="silk num">{{ events.length }} events</span>
      <button v-if="store.myStream" type="button" class="btn btn-quiet btn-sm" @click="store.closeStream()">Disconnect</button>
    </div>

    <div ref="list" class="events">
      <p v-if="!events.length" class="blank">
        Nothing yet. Connect, and whatever the server sends shows up here as it arrives.
      </p>
      <div v-for="(event, i) in events" :key="i" class="event" :class="event.kind">
        <span class="at silk num">{{ when(event.at) }}</span>
        <span class="mark">{{ event.kind === 'sent' ? '→' : event.kind === 'message' ? '←' : '·' }}</span>
        <span class="body mono selectable">
          <span v-if="event.name" class="name">{{ event.name }}</span>{{ event.data || event.kind }}
        </span>
      </div>
    </div>

    <form v-if="canSay" class="say" @submit.prevent="send">
      <input
        v-model="outgoing"
        class="field mono"
        aria-label="Message to send"
        placeholder="Type a message and press enter"
        spellcheck="false"
      >
      <button type="submit" class="btn btn-sm" :disabled="!outgoing.trim()">Send</button>
      <button v-if="isGrpc" type="button" class="btn btn-quiet btn-sm" @click="doneSending">Done sending</button>
    </form>
  </section>
</template>

<style scoped>
.stream { display: flex; flex-direction: column; min-height: 0; background: var(--bg-1); }

.state {
  display: flex;
  align-items: center;
  gap: var(--s-3);
  padding: var(--s-2) var(--s-4);
  border-bottom: 1px solid var(--line-soft);
}
.url { font-size: var(--t-meta); color: var(--ink-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

.events { flex: 1; overflow: auto; padding: var(--s-2) var(--s-4) var(--s-4); background: var(--well); }
.blank { margin: var(--s-6) 0; color: var(--silk); font-size: var(--t-small); text-align: center; }

.event { display: grid; grid-template-columns: 70px 16px minmax(0, 1fr); gap: var(--s-2); padding: 2px 0; align-items: baseline; }
.at { font-size: var(--t-meta); }
.mark { color: var(--faint); text-align: center; }
.body { font-size: var(--t-code); white-space: pre-wrap; word-break: break-word; }
.event.sent .body { color: var(--ink-2); }
.event.error .body { color: var(--bad); }
.event.open .body, .event.closed .body { color: var(--silk); }
.name { display: inline-block; margin-right: var(--s-2); padding: 0 5px; border-radius: var(--r-xs); background: var(--press); font-size: var(--t-meta); }

.say { display: flex; gap: var(--s-2); padding: var(--s-2) var(--s-4); border-top: 1px solid var(--line-soft); }
.say .field { flex: 1; }
</style>
