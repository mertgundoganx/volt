<script setup lang="ts">
import type { Cookie } from '~/types'

const store = useCollectionStore()
const emit = defineEmits<{ close: [] }>()

const cookies = ref<Cookie[]>([])
const loading = ref(true)

async function refresh() {
  cookies.value = await store.loadCookies()
  loading.value = false
}
onMounted(refresh)

/** Grouped by host, which is how anyone thinks about them. */
const byDomain = computed(() => {
  const groups = new Map<string, Cookie[]>()
  for (const cookie of cookies.value) {
    const list = groups.get(cookie.domain) ?? []
    list.push(cookie)
    groups.set(cookie.domain, list)
  }
  return [...groups.entries()]
})

function when(expires: number | null) {
  if (!expires) return 'session'
  return new Date(expires * 1000).toLocaleString()
}

async function drop(cookie: Cookie) {
  await store.deleteCookie(cookie)
  await refresh()
}

async function clearAll() {
  await store.clearCookies()
  await refresh()
}
</script>

<template>
  <UiDialog title="Cookies" eyebrow="This collection" :width="720" @close="emit('close')">
    <p class="note">
      <UiIcon name="lock" :size="14" />
      <span>
        Kept in memory for as long as volt is open, never written to the collection —
        a session cookie is a credential. Closing volt forgets them.
      </span>
    </p>

    <p v-if="loading" class="empty">Reading the jar…</p>
    <p v-else-if="!cookies.length" class="empty">
      No cookies yet. They arrive when a response sets one.
    </p>

    <div v-for="[domain, list] in byDomain" v-else :key="domain" class="group">
      <h3 class="mono">{{ domain }}</h3>
      <table class="cookies">
        <thead>
          <tr>
            <th scope="col" class="silk">Name</th>
            <th scope="col" class="silk">Value</th>
            <th scope="col" class="silk">Path</th>
            <th scope="col" class="silk">Expires</th>
            <th scope="col" />
          </tr>
        </thead>
        <tbody>
          <tr v-for="cookie in list" :key="`${cookie.name}-${cookie.path}`">
            <td class="mono name">{{ cookie.name }}</td>
            <td class="mono value" :title="cookie.value">{{ cookie.value }}</td>
            <td class="mono">{{ cookie.path }}</td>
            <td class="expires">{{ when(cookie.expires) }}</td>
            <td>
              <span v-if="cookie.secure" class="chip">TLS</span>
              <span v-if="cookie.httpOnly" class="chip">HTTP</span>
              <button type="button" class="icon-btn quiet sm" :aria-label="`Remove ${cookie.name}`" title="Remove" @click="drop(cookie)">
                <UiIcon name="x" :size="14" />
              </button>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <template #footer>
      <button type="button" class="btn btn-quiet btn-sm" :disabled="!cookies.length" @click="clearAll">
        <UiIcon name="trash" :size="14" />Forget all
      </button>
      <span class="silk">{{ cookies.length }} {{ cookies.length === 1 ? 'cookie' : 'cookies' }}</span>
      <span class="spacer" />
      <button type="button" class="btn btn-primary" @click="emit('close')">Done</button>
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
.empty { margin: var(--s-6) 0; color: var(--silk); font-size: var(--t-small); text-align: center; }

.group { margin-bottom: var(--s-4); }
h3 { margin: 0 0 var(--s-2); font-size: var(--t-small); font-weight: 600; color: var(--ink-2); }

.cookies { width: 100%; border-collapse: collapse; font-size: var(--t-small); }
.cookies th { text-align: left; padding-bottom: 4px; border-bottom: 1px solid var(--line-soft); }
.cookies td { padding: 5px 0; border-bottom: 1px solid var(--line-soft); vertical-align: middle; }
.cookies td:last-child { display: flex; gap: 4px; align-items: center; justify-content: flex-end; }
.name { max-width: 16ch; overflow: hidden; text-overflow: ellipsis; }
.value { max-width: 26ch; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--ink-2); }
.expires { color: var(--silk); white-space: nowrap; }
</style>
