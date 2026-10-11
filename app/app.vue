<script setup lang="ts">
import { listen } from '@tauri-apps/api/event'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import type { StreamEvent } from '~/types'

const store = useCollectionStore()
const editingEnv = ref(false)
const editingSettings = ref(false)
const palette = ref(false)

installShortcuts()
// The bottom pane follows the open request.
const bottom = computed(() => {
  const kind = store.request?.kind ?? 'http'
  if (kind === 'websocket' || kind === 'sse' || store.isSubscription) return 'stream'
  // A streaming gRPC call is a connection, and reads like one.
  if (kind === 'grpc') return store.grpcStreams ? 'stream' : 'grpc'
  return 'response'
})

// These register before the language is known, so the sheet translates the
// labels when it shows them.
useShortcut('mod+p', 'Open a request', () => (palette.value = true))
useShortcut('mod+e', 'Environments', () => (editingEnv.value = true))
useShortcut('mod+,', 'Settings', () => (editingSettings.value = true))
useShortcut('?', 'This list', () => (store.shortcutsSheet = true))

// A variable named in the request but defined nowhere opens the environment
// editor with that name ready to fill in.
watch(() => store.openEnvironmentWith, (name) => {
  if (name) editingEnv.value = true
})

// The open tabs survive a restart. Debounced: typing is many changes.
let persistTimer: ReturnType<typeof setTimeout> | undefined
watch(
  () => [store.tabs.length, store.activeTab, store.activeId, store.dirty, store.request],
  () => {
    clearTimeout(persistTimer)
    persistTimer = setTimeout(() => void store.persistTabs(), 500)
  },
  { deep: true },
)

// Pane sizes are the user's, so they persist.
const { value: sidebarWidth, reset: resetSidebar } = usePersistentNumber('volt.sidebarWidth', 264, 200, 480)
const { value: requestShare, reset: resetRequestShare } = usePersistentNumber('volt.requestShare', 0.5, 0.2, 0.8)
const workArea = ref<HTMLElement>()

// How the two panes lie: the response under the request, or beside it.
// Side by side needs room; in a narrow window the choice waits and the panes stack.
// "Room" is the width left for the two panes, not the window's: a wide
// sidebar in a medium window leaves as little as a narrow window does.
const ROOM_FOR_TWO = 720
const roomy = ref(true)
const mainArea = ref<HTMLElement>()
onMounted(() => {
  const watcher = new ResizeObserver(([entry]) => {
    if (entry) roomy.value = entry.contentRect.width >= ROOM_FOR_TWO
  })
  // The element is replaced when the language changes (the shell is keyed by
  // it), so follow the ref rather than observing whatever was there first.
  watch(mainArea, (now, was) => {
    if (was) watcher.unobserve(was)
    if (now) watcher.observe(now)
  }, { immediate: true })
  onUnmounted(() => watcher.disconnect())
})
const layout = ref<'rows' | 'cols'>('rows')
const lying = computed(() => (roomy.value ? layout.value : 'rows'))
try {
  if (localStorage.getItem('volt.layout') === 'cols') layout.value = 'cols'
} catch {
  // Storage is unavailable: the default stands.
}
watch(layout, (next) => {
  try {
    localStorage.setItem('volt.layout', next)
  } catch {
    // Not persisting is fine.
  }
})

function resizeRequest(delta: number) {
  const size = (lying.value === 'rows' ? workArea.value?.clientHeight : workArea.value?.clientWidth) ?? 0
  if (size > 0) requestShare.value += delta / size
}

const workStyle = computed(() =>
  lying.value === 'rows'
    ? { gridTemplateRows: `minmax(150px, ${requestShare.value}fr) var(--gap) minmax(120px, ${1 - requestShare.value}fr)` }
    : { gridTemplateColumns: `minmax(320px, ${requestShare.value}fr) var(--gap) minmax(280px, ${1 - requestShare.value}fr)` },
)

// Streams push events; the store keeps the open one's. The unlisten has to be
// registered outside the async callback: after an `await` there is no current
// instance, so `onUnmounted` there is dropped with a warning and the listener
// survives every hot reload.
let stopStream: (() => void) | undefined
onMounted(async () => {
  stopStream = await listen<StreamEvent>('volt://stream', (event) => store.noteStreamEvent(event.payload))
})
onUnmounted(() => stopStream?.())

// Files dragged onto the window. The webview reports the paths; a veil says
// what dropping will do while something is held over the window.
const dropping = ref(false)
let stopDrop: (() => void) | undefined
onMounted(async () => {
  try {
    stopDrop = await getCurrentWebview().onDragDropEvent((event) => {
      const payload = event.payload
      if (payload.type === 'enter') dropping.value = true
      else if (payload.type === 'leave') dropping.value = false
      else if (payload.type === 'drop') {
        dropping.value = false
        void store.importPaths(payload.paths)
      }
    })
  } catch {
    // Not inside a Tauri webview: nothing can be dropped, and nothing to undo.
  }
})
onUnmounted(() => stopDrop?.())

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
  <!-- Keyed by language, so choosing another one redraws every label at once. -->
  <div :key="uiLanguage" class="shell">
    <TopBar v-model:layout="layout" :can-split="roomy" @edit-environment="editingEnv = true" @open-settings="editingSettings = true" />

    <div class="bench">
        <Sidebar :style="{ width: `${sidebarWidth}px` }" />
        <UiSplitter axis="x" :label="t('Sidebar width')" @drag="sidebarWidth += $event" @reset="resetSidebar()" />

        <main ref="mainArea" class="main">
          <div v-if="store.crash" class="error-strip" role="alert">
            <span class="led bad" />
            <span class="message selectable">
              {{ t('volt closed unexpectedly last time. The report is at') }}
              <span class="mono">{{ store.crash.path }}</span> {{ t('— please attach it to a bug report.') }}
            </span>
            <button type="button" class="icon-btn quiet sm" :aria-label="t('Dismiss crash report')" :title="t('Dismiss')" @click="store.crash = null">
              <UiIcon name="x" :size="14" />
            </button>
          </div>

          <div v-if="store.error" class="error-strip" role="alert">
            <span class="led bad" />
            <span class="message selectable" :class="{ mono: !store.errorDetail }">
              <span class="said">{{ store.error }}</span>
              <span v-if="store.errorDetail" class="detail mono">{{ store.errorDetail }}</span>
            </span>
            <button
              v-if="store.errorCertificate && store.request"
              type="button"
              class="btn btn-sm"
              :title="t('Turns off certificate verification for this request (its Settings tab) and sends again')"
              @click="store.sendWithoutVerifying()"
            >
              {{ t('Send without verifying') }}
            </button>
            <button type="button" class="icon-btn quiet sm" :aria-label="t('Dismiss error')" :title="t('Dismiss')" @click="store.error = null">
              <UiIcon name="x" :size="14" />
            </button>
          </div>

          <RequestTabs />

          <div v-if="store.request" ref="workArea" class="work" :class="lying" :style="workStyle">
            <RequestPane class="panel" />
            <UiSplitter :axis="lying === 'rows' ? 'y' : 'x'" :label="t('Request and response size')" @drag="resizeRequest" @reset="resetRequestShare()" />
            <!-- The second pane follows the kind of request: a socket is not a
                 response, and a gRPC reply is not an HTTP one. -->
            <StreamPane v-if="bottom === 'stream'" class="panel" />
            <GrpcReplyPane v-else-if="bottom === 'grpc'" class="panel" />
            <ResponsePane v-else class="panel" />
          </div>

          <Welcome v-else class="panel" />
        </main>
    </div>

    <EnvironmentEditor v-if="editingEnv" @close="editingEnv = false" />
    <SettingsDialog v-if="editingSettings" @close="editingSettings = false" />
    <CommandPalette v-if="palette" @close="palette = false" />
    <CodeDialog v-if="store.codeDialog" @close="store.codeDialog = false" />
    <CookiesDialog v-if="store.cookiesDialog" @close="store.cookiesDialog = false" />
    <SyncDialog v-if="store.syncDialog" @close="store.syncDialog = false" />
    <RunnerDialog v-if="store.runnerDialog" @close="store.runnerDialog = false; store.runnerTarget = null" />
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
    <ShortcutsSheet v-if="store.shortcutsSheet" @close="store.shortcutsSheet = false" />
    <CopyToDialog v-if="store.copyToDialog" @close="store.copyToDialog = null" />
    <ImportReport v-if="store.importReport" />
    <CurlDialog v-if="store.curlDialog" />
    <Toast />

    <div v-if="dropping" class="drop-veil" aria-hidden="true">
      <div class="drop-card">
        <UiIcon name="import" :size="22" />
        <span class="drop-title">{{ t('Drop to import') }}</span>
        <span class="drop-hint">{{ t('A Postman, Insomnia, Bruno, OpenAPI or HAR export becomes a collection; a folder opens.') }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* The bench: a strip on top, and under it the panels with a gap between. */
.shell { display: flex; flex-direction: column; height: 100%; }
.bench { flex: 1; display: flex; min-height: 0; padding: 0 var(--gap) var(--gap); }
.main { flex: 1; display: flex; flex-direction: column; min-width: 0; min-height: 0; }

.work { flex: 1; display: grid; min-height: 0; min-width: 0; }
.work.rows { grid-template-columns: minmax(0, 1fr); }
.work.cols { grid-template-rows: minmax(0, 1fr); }

/* A message lies on the bench like a slip of paper: its own rounded piece. */
.error-strip {
  display: flex;
  align-items: flex-start;
  gap: var(--s-3);
  margin-bottom: var(--gap);
  padding: var(--s-2) var(--s-2) var(--s-2) var(--s-4);
  border-radius: var(--r-panel);
  background: color-mix(in srgb, var(--bad) 12%, var(--bg-1));
  flex: none;
}
.error-strip .led { margin-top: 6px; }
.error-strip .message.selectable { font-family: var(--font-ui); display: grid; gap: 3px; }
.error-strip .message.mono { font-family: var(--font-mono); }
.error-strip .detail { font-size: var(--t-meta); color: color-mix(in srgb, var(--bad) 70%, var(--ink)); opacity: 0.85; }
.error-strip .btn { flex: none; align-self: center; }

.drop-veil {
  position: fixed;
  inset: 0;
  z-index: 45;
  display: grid;
  place-items: center;
  background: var(--backdrop);
  pointer-events: none;
}
.drop-card {
  display: grid;
  justify-items: center;
  gap: 6px;
  padding: var(--s-6) var(--s-8);
  border: 2px dashed var(--focus);
  border-radius: var(--r-lg);
  background: var(--bg-3);
  color: var(--ink);
}
.drop-card .ui-icon { color: var(--ink); margin-bottom: 4px; }
.drop-title { font-weight: 600; font-size: 15px; }
.drop-hint { color: var(--silk); font-size: var(--t-small); max-width: 40ch; text-align: center; }
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
