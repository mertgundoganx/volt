<script setup lang="ts">
import type { SyncStatus } from '~/types'

const store = useCollectionStore()
const emit = defineEmits<{ close: [] }>()

const status = ref<SyncStatus | null>(null)
const loading = ref(true)
const message = ref('')
const busy = ref(false)

async function refresh() {
  status.value = await store.syncStatus()
  loading.value = false
  if (status.value?.changed.length && !message.value) message.value = 'Update the API collection'
}
onMounted(refresh)

async function pull() {
  busy.value = true
  await store.syncPull()
  busy.value = false
  await refresh()
}

async function commit(push: boolean) {
  busy.value = true
  const ok = await store.syncCommit(message.value, push)
  busy.value = false
  if (ok) {
    message.value = ''
    await refresh()
  }
}
</script>

<template>
  <UiDialog title="Sync" eyebrow="This collection" :width="680" @close="emit('close')">
    <p class="note">
      <UiIcon name="info" :size="14" />
      <span>
        A collection is files, so sharing it is your repository's job, not a cloud
        service's. This is that repository, from here: pull what others changed,
        commit what you did, push it.
      </span>
    </p>

    <p v-if="loading" class="empty">Looking…</p>

    <template v-else-if="!status?.repository">
      <p class="empty">{{ status?.why ?? 'Nothing to sync with.' }}</p>
      <p class="hint">
        <span>
          Put the collection folder in a git repository and this page does the rest.
          Everything else in volt works exactly the same without one.
        </span>
      </p>
    </template>

    <template v-else>
      <div class="state">
        <span class="silk">Branch</span>
        <span class="mono">{{ status.branch }}</span>
        <span class="silk">Remote</span>
        <span class="mono remote">{{ status.remote ?? 'none' }}</span>
        <span class="silk">Standing</span>
        <span>
          <template v-if="status.ahead || status.behind">
            <span v-if="status.ahead" class="chip">{{ status.ahead }} to push</span>
            <span v-if="status.behind" class="chip">{{ status.behind }} to pull</span>
          </template>
          <span v-else class="silk">up to date</span>
        </span>
      </div>

      <p v-if="!status.secretsIgnored" class="warn-note" role="alert">
        <UiIcon name="warning" :size="14" />
        <span>
          <code>.gitignore</code> does not cover the <code>.env</code> files, so committing
          would publish secret values. Save any environment once and volt writes the rule.
        </span>
      </p>

      <h3 class="silk">Changed here</h3>
      <ul v-if="status.changed.length" class="changed mono">
        <li v-for="path in status.changed" :key="path">{{ path }}</li>
      </ul>
      <p v-else class="hint"><span>Nothing changed since the last commit.</span></p>

      <div class="commit">
        <input
          v-model="message"
          class="field"
          aria-label="Commit message"
          placeholder="What changed"
          :disabled="!status.changed.length"
          @keydown.enter.prevent="commit(true)"
        >
      </div>

      <p class="hint share">
        <span>
          Sharing with someone new? <button type="button" class="link" @click="store.writeEnvTemplates()">Write the .env templates</button>
          — the names they have to fill in, with none of your values.
        </span>
      </p>
    </template>

    <template #footer>
      <button
        v-if="status?.repository"
        type="button"
        class="btn btn-quiet btn-sm"
        :disabled="busy || !status.behind"
        @click="pull"
      >
        Pull
      </button>
      <span class="spacer" />
      <button type="button" class="btn btn-quiet" @click="emit('close')">Close</button>
      <button
        v-if="status?.repository"
        type="button"
        class="btn btn-quiet"
        :disabled="busy || !status.changed.length || !message.trim() || !status.secretsIgnored"
        @click="commit(false)"
      >
        Commit
      </button>
      <button
        v-if="status?.repository"
        type="button"
        class="btn btn-primary"
        :disabled="busy || !status.secretsIgnored || (!status.changed.length && !status.ahead)"
        @click="commit(true)"
      >
        {{ status.changed.length ? 'Commit and push' : 'Push' }}
      </button>
    </template>
  </UiDialog>
</template>

<style scoped>
.note, .warn-note, .hint {
  display: flex;
  gap: var(--s-2);
  font-size: var(--t-small);
  line-height: 1.55;
}
.note { margin: 0 0 var(--s-4); color: var(--silk); }
.note .ui-icon, .warn-note .ui-icon { margin-top: 2px; flex: none; }
.hint { margin: var(--s-2) 0 0; color: var(--faint); font-size: var(--t-meta); }
.share { margin-top: var(--s-4); }
.warn-note {
  margin: 0 0 var(--s-3);
  padding: var(--s-2) var(--s-3);
  border: 1px solid color-mix(in srgb, var(--warn) 45%, transparent);
  border-radius: var(--r-sm);
  background: var(--warn-tint);
  color: var(--warn);
}
.empty { margin: var(--s-5) 0; color: var(--silk); font-size: var(--t-small); text-align: center; }

.state {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  gap: 6px var(--s-4);
  align-items: baseline;
  padding-bottom: var(--s-3);
  border-bottom: 1px solid var(--line-soft);
  margin-bottom: var(--s-3);
  font-size: var(--t-small);
}
.remote { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
h3 { margin: 0 0 var(--s-2); }
.changed { list-style: none; margin: 0; padding: 0; max-height: 26vh; overflow: auto; font-size: var(--t-meta); }
.changed li { padding: 2px 0; color: var(--ink-2); }
.commit { margin-top: var(--s-3); }
.link { color: var(--ink); text-decoration: underline; }
</style>
