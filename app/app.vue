<script setup lang="ts">
import { listen } from '@tauri-apps/api/event'
import type { StreamEvent } from '~/types'

const store = useCollectionStore()
const editingEnv = ref(false)
const editingSettings = ref(false)
const palette = ref(false)
const shortcuts = ref(false)

installShortcuts()
// The bottom pane follows the open request.
const bottom = computed(() => {
  const kind = store.request?.kind ?? 'http'
  if (kind === 'websocket' || kind === 'sse') return 'stream'
  // A streaming gRPC call is a connection, and reads like one.
  if (kind === 'grpc') return store.grpcStreams ? 'stream' : 'grpc'
  return 'response'
})

useShortcut('mod+p', 'Open a request', () => (palette.value = true))
useShortcut('mod+e', 'Environments', () => (editingEnv.value = true))
useShortcut('mod+,', 'Settings', () => (editingSettings.value = true))
useShortcut('?', 'This list', () => (shortcuts.value = true))

// Pane sizes are the user's, so they persist.
const { value: sidebarWidth, reset: resetSidebar } = usePersistentNumber('volt.sidebarWidth', 272, 220, 460)
const { value: requestShare, reset: resetRequestShare } = usePersistentNumber('volt.requestShare', 0.5, 0.2, 0.8)
const workArea = ref<HTMLElement>()

function resizeRequest(delta: number) {
  const height = workArea.value?.clientHeight ?? 0
  if (height > 0) requestShare.value += delta / height
}

// Streams push events; the store keeps the open one's. The unlisten has to be
// registered outside the async callback: after an `await` there is no current
// instance, so `onUnmounted` there is dropped with a warning and the listener
// survives every hot reload.
let stopStream: (() => void) | undefined
onMounted(async () => {
  stopStream = await listen<StreamEvent>('volt://stream', (event) => store.noteStreamEvent(event.payload))
})
onUnmounted(() => stopStream?.())

onMounted(() => {
  store.restore()
  // The mock outlives a reload of the webview, so ask whether it is serving.
  store.refreshMock()

  // Anything that escapes an action still has to be visible somewhere.
  const onRejection = (event: PromiseRejectionEvent) => {
    store.error = String(event.reason?.message ?? event.reason)
  }
  const onError = (event: ErrorEvent) => {
    store.error = event.message
  }
  window.addEventListener('unhandledrejection', onRejection)
  window.addEventListener('error', onError)
  onUnmounted(() => {
    window.removeEventListener('unhandledrejection', onRejection)
    window.removeEventListener('error', onError)
  })
})
</script>

<template>
  <div class="shell">
    <AppBar
      :brand-width="sidebarWidth"
      @edit-environment="editingEnv = true"
      @open-settings="editingSettings = true"
    />

    <div class="body">
      <Sidebar :style="{ width: `${sidebarWidth}px` }" />
      <UiSplitter axis="x" label="Sidebar width" @drag="sidebarWidth += $event" @reset="resetSidebar()" />

      <main class="main">
        <div v-if="store.error" class="error-strip" role="alert">
          <span class="led bad" />
          <span class="message mono selectable">{{ store.error }}</span>
          <button type="button" class="icon-btn quiet sm" aria-label="Dismiss error" title="Dismiss" @click="store.error = null">
            <UiIcon name="x" :size="14" />
          </button>
        </div>

        <RequestTabs />

        <div
          v-if="store.request"
          ref="workArea"
          class="work"
          :style="{ gridTemplateRows: `minmax(150px, ${requestShare}fr) auto minmax(120px, ${1 - requestShare}fr)` }"
        >
          <RequestPane />
          <UiSplitter axis="y" label="Request and response height" @drag="resizeRequest" @reset="resetRequestShare()" />
          <!-- The bottom pane follows the kind of request: a socket is not a
               response, and a gRPC reply is not an HTTP one. -->
          <StreamPane v-if="bottom === 'stream'" />
          <GrpcReplyPane v-else-if="bottom === 'grpc'" />
          <ResponsePane v-else />
        </div>

        <Welcome v-else />
      </main>
    </div>

    <EnvironmentEditor v-if="editingEnv" @close="editingEnv = false" />
    <SettingsDialog v-if="editingSettings" @close="editingSettings = false" />
    <CommandPalette v-if="palette" @close="palette = false" />
    <CodeDialog v-if="store.codeDialog" @close="store.codeDialog = false" />
    <CookiesDialog v-if="store.cookiesDialog" @close="store.cookiesDialog = false" />
    <SyncDialog v-if="store.syncDialog" @close="store.syncDialog = false" />
    <RunnerDialog v-if="store.runnerDialog" @close="store.runnerDialog = false" />
    <TrashDialog v-if="store.trashDialog" @close="store.trashDialog = false" />
    <OAuthDialog v-if="store.oauthDialog" @close="store.oauthDialog = false" />
    <WorkspacesDialog v-if="store.workspacesDialog" @close="store.workspacesDialog = false" />
    <MonitorsDialog v-if="store.monitorsDialog" @close="store.monitorsDialog = false" />
    <MockDialog v-if="store.mockDialog" @close="store.mockDialog = false" />
    <ScopeEditor
      v-if="store.scopeDialog"
      :folder-id="store.scopeDialog.id"
      :title="store.scopeDialog.title"
      @close="store.scopeDialog = null"
    />
    <ShortcutsSheet v-if="shortcuts" @close="shortcuts = false" />
    <ImportReport v-if="store.importReport" />
    <CurlDialog v-if="store.curlDialog" />
    <Toast />
  </div>
</template>

<style scoped>
.shell { display: flex; flex-direction: column; height: 100%; }
.body { flex: 1; display: flex; min-height: 0; }
.main { flex: 1; display: flex; flex-direction: column; min-width: 0; min-height: 0; background: var(--bg-1); }

.work { flex: 1; display: grid; min-height: 0; }

.error-strip {
  display: flex;
  align-items: flex-start;
  gap: var(--s-3);
  padding: var(--s-2) var(--s-2) var(--s-2) var(--s-4);
  border-bottom: 1px solid color-mix(in srgb, var(--bad) 35%, var(--line));
  background: var(--bad-tint);
  flex: none;
}
.error-strip .led { margin-top: 6px; }
.message {
  flex: 1;
  padding-top: 2px;
  color: var(--bad);
  font-size: var(--t-small);
  line-height: 1.5;
  white-space: pre-wrap;
  word-break: break-word;
}
</style>
