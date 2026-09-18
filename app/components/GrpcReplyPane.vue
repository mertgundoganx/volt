<script setup lang="ts">
import type { TabItem } from '~/utils/ui'

const store = useCollectionStore()
const reply = computed(() => store.grpcReply)
const tab = ref<'message' | 'metadata'>('message')

const tabs = computed<TabItem[]>(() => [
  { key: 'message', label: 'Message' },
  {
    key: 'metadata',
    label: 'Metadata',
    meta: (reply.value?.headers?.length ?? 0) + (reply.value?.trailers?.length ?? 0) || null,
  },
])
</script>

<template>
  <section class="grpc-reply" aria-label="gRPC reply" aria-live="polite">
    <div class="state">
      <template v-if="reply">
        <span class="led" :class="reply.status === 0 ? 'ok' : 'bad'" />
        <span class="code num">{{ reply.status }}</span>
        <span class="silk">{{ reply.statusText }}</span>
        <span v-if="reply.message" class="message">{{ reply.message }}</span>
        <span class="spacer" />
        <UiMeasure label="Time" :value="String(reply.durationMs)" unit="ms" />
      </template>
      <template v-else>
        <span class="led off" />
        <span class="blank">No reply yet. Call the method <span class="kbd">{{ modKey }} ↵</span></span>
      </template>
    </div>

    <UiTabs v-if="reply" v-model="tab" :items="tabs" label="Reply sections" />

    <div class="viewer">
      <template v-if="reply && tab === 'metadata'">
        <!-- Both, and labelled: the status of a gRPC call usually arrives in
             the trailers, and knowing which side said what is the point. -->
        <div class="metadata">
          <span class="silk">Headers</span>
          <table v-if="reply.headers?.length" class="pairs">
            <tbody>
              <tr v-for="(header, i) in reply.headers ?? []" :key="`h-${i}`">
                <th scope="row">{{ header.name }}</th>
                <td>{{ header.value }}</td>
              </tr>
            </tbody>
          </table>
          <p v-else class="none">None.</p>

          <span class="silk">Trailers</span>
          <table v-if="reply.trailers?.length" class="pairs">
            <tbody>
              <tr v-for="(trailer, i) in reply.trailers ?? []" :key="`t-${i}`">
                <th scope="row">{{ trailer.name }}</th>
                <td>{{ trailer.value }}</td>
              </tr>
            </tbody>
          </table>
          <p v-else class="none">None — the status came with the headers.</p>
        </div>
      </template>
      <UiCodeView v-else-if="reply?.body" :text="reply.body" language="json" wrap />
      <p v-else-if="reply" class="blank-note">The call returned an empty message.</p>
      <p v-else class="blank-note">The reply shows up here, as JSON.</p>
    </div>
  </section>
</template>

<style scoped>
.grpc-reply { display: flex; flex-direction: column; min-height: 0; background: var(--bg-1); }

.state {
  display: flex;
  align-items: center;
  gap: var(--s-3);
  padding: var(--s-3) var(--s-4);
  border-bottom: 1px solid var(--line-soft);
}
.code { font-size: var(--t-title); font-weight: 700; }
.message { color: var(--bad); font-size: var(--t-small); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.blank { color: var(--silk); font-size: var(--t-small); display: flex; align-items: center; gap: var(--s-2); }

.viewer { flex: 1; overflow: auto; background: var(--well); --code-bg: var(--well); }
.blank-note { margin: var(--s-6) var(--s-4); color: var(--silk); font-size: var(--t-small); text-align: center; }

.metadata { display: grid; gap: var(--s-2); padding: var(--s-3) var(--s-4) var(--s-4); align-content: start; }
.pairs { width: 100%; border-collapse: collapse; font-size: var(--t-small); }
.pairs th, .pairs td { text-align: left; padding: 4px 0; border-bottom: 1px solid var(--line-soft); vertical-align: top; }
.pairs th { width: 220px; padding-right: var(--s-3); font-weight: 500; color: var(--ink-2); font-family: var(--font-mono); font-size: var(--t-meta); }
.pairs td { font-family: var(--font-mono); font-size: var(--t-meta); word-break: break-all; }
.none { margin: 0; color: var(--silk); font-size: var(--t-small); }
</style>
