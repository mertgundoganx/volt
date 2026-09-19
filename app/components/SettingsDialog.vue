<script setup lang="ts">
import { defaultExecOptions } from '~/types'
import type { ThemePreference } from '~/stores/collection'
import { applyTheme } from '~/stores/collection'

const store = useCollectionStore()
const emit = defineEmits<{ close: [] }>()

// Edit copies; nothing is persisted until Save.
const draft = ref({ ...store.options })
const theme = ref<ThemePreference>(store.theme)
const saving = ref(false)


// Updates. The toggle saves as soon as it is flipped rather than waiting for
// "Save settings", because it is about this app rather than about requests.
const autoCheck = computed({
  get: () => store.autoCheckUpdates,
  set: (on: boolean) => store.saveAutoCheckUpdates(on),
})
const checking = ref(false)
async function checkNow() {
  checking.value = true
  try {
    await store.checkForUpdate(true)
  } finally {
    checking.value = false
  }
}

// The theme previews as soon as it is picked, and goes back if cancelled.
watch(theme, (next) => applyTheme(next))
function cancel() {
  applyTheme(store.theme)
  emit('close')
}

// Each card wears its theme through `data-theme`, so the swatch is drawn
// from the real tokens rather than a second copy of the colours. System shows
// whichever of the two the operating system would pick right now.
const systemPick = window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
const swatchTheme = (value: ThemePreference) => (value === 'system' ? systemPick : value)
const themes: { value: ThemePreference; label: string; hint: string }[] = [
  { value: 'system', label: 'System', hint: 'Follows the operating system' },
  { value: 'light', label: 'Paper', hint: 'Warm white, ultramarine' },
  { value: 'dark', label: 'Graphite', hint: 'Warm dark' },
  { value: 'linen', label: 'Linen', hint: 'Cream, brick' },
  { value: 'mist', label: 'Mist', hint: 'Cool grey, teal' },
]

// Seconds read better than milliseconds, but Rust wants milliseconds.
const timeoutSeconds = computed({
  get: () => draft.value.timeoutMs / 1000,
  set: (value: number) => {
    draft.value.timeoutMs = Math.round(value * 1000)
  },
})
const timeoutInvalid = computed(() => !(draft.value.timeoutMs > 0))

// Stored as null when empty, so "no proxy" and "" cannot disagree.
const proxy = computed({
  get: () => draft.value.proxy ?? '',
  set: (value: string) => {
    draft.value.proxy = value.trim() || null
  },
})

function reset() {
  draft.value = defaultExecOptions()
  theme.value = 'system'
}

async function save() {
  if (timeoutInvalid.value) return
  saving.value = true
  await store.saveOptions(draft.value)
  await store.saveTheme(theme.value)
  saving.value = false
  emit('close')
}
</script>

<template>
  <UiDialog title="Settings" eyebrow="volt" :width="600" @close="cancel">
    <p class="scope">These apply to this app on this machine, not to any collection, and are never written to its files.</p>

    <section class="group" aria-labelledby="s-appearance">
      <h3 id="s-appearance" class="silk">Appearance</h3>
      <div class="setting themes">
        <div class="text">
          <span class="name">Theme</span>
          <span class="desc">Applies as you pick; Cancel puts it back.</span>
        </div>
        <div class="theme-grid" role="radiogroup" aria-label="Theme">
          <button
            v-for="one in themes"
            :key="one.value"
            type="button"
            role="radio"
            class="theme-card"
            :class="{ on: theme === one.value }"
            :aria-checked="theme === one.value"
            @click="theme = one.value"
          >
            <span class="swatch" :data-theme="swatchTheme(one.value)" aria-hidden="true">
              <i class="sw sw-chrome" /><i class="sw sw-paper" /><i class="sw sw-accent" /><i class="sw sw-ink" />
            </span>
            <span class="theme-name">{{ one.label }}</span>
            <span class="theme-hint">{{ one.hint }}</span>
          </button>
        </div>
      </div>
    </section>

    <section class="group" aria-labelledby="s-requests">
      <h3 id="s-requests" class="silk">Requests</h3>

      <div class="setting">
        <div class="text">
          <label class="name" for="timeout">Timeout</label>
          <span v-if="timeoutInvalid" class="desc bad">Must be greater than zero.</span>
          <span v-else class="desc">How long to wait for a response before giving up.</span>
        </div>
        <div class="with-unit">
          <input
            id="timeout"
            v-model.number="timeoutSeconds"
            class="field mono num"
            :class="{ invalid: timeoutInvalid }"
            type="number"
            min="0.1"
            step="0.5"
          >
          <span class="silk">sec</span>
        </div>
      </div>

      <div class="setting">
        <div class="text">
          <label class="name" for="redirects">Follow redirects</label>
          <span class="desc">
            {{ draft.followRedirects ? 'Follows up to 10 redirects, like curl -L.' : 'Returns the 3xx response itself, so its Location can be inspected.' }}
          </span>
        </div>
        <input id="redirects" v-model="draft.followRedirects" class="switch" type="checkbox">
      </div>
      <div class="setting">
        <div class="text">
          <label class="name" for="proxy">Proxy</label>
          <span class="desc">
            Every request goes through it, unless one overrides it in its Options tab.
            Leave it empty to use the system settings.
          </span>
        </div>
        <input
          id="proxy"
          v-model="proxy"
          class="field mono proxy"
          spellcheck="false"
          placeholder="http://127.0.0.1:8080"
        >
      </div>

      <div class="setting">
        <div class="text">
          <label class="name" for="cookies">Keep cookies</label>
          <span class="desc">
            Cookies a response sets are sent back on the next request to that host, one jar
            per collection. They are held in memory only, so closing volt forgets them.
          </span>
        </div>
        <div class="cookie-controls">
          <button v-if="store.isOpen" type="button" class="btn btn-quiet btn-sm" @click="store.clearCookies()">
            Clear now
          </button>
          <input id="cookies" v-model="draft.sendCookies" class="switch" type="checkbox">
        </div>
      </div>

      <div class="setting" :class="{ alarm: !draft.verifyTls }">
        <div class="text">
          <label class="name" for="tls">Verify TLS certificates</label>
          <span v-if="draft.verifyTls" class="desc">Rejects expired, self-signed and mismatched certificates.</span>
          <span v-else class="desc warn">
            <UiIcon name="warning" :size="13" />
            Any certificate is accepted, so traffic can be read or changed in transit. Only for hosts you control.
          </span>
        </div>
        <input id="tls" v-model="draft.verifyTls" class="switch" type="checkbox">
      </div>
    </section>

    <section class="group" aria-labelledby="s-keys">
      <h3 id="s-keys" class="silk">Keyboard</h3>
      <div class="setting">
        <div class="text">
          <span class="name">Shortcuts</span>
          <span class="desc">Send with {{ modKey }}+Enter, save with {{ modKey }}+S, jump to a request with {{ modKey }}+P. The full list is one key away: ?</span>
        </div>
        <button type="button" class="btn btn-sm" @click="emit('close'); store.shortcutsSheet = true">Show all</button>
      </div>
    </section>

    <section class="group" aria-labelledby="s-updates">
      <h3 id="s-updates" class="silk">Updates</h3>

      <div class="setting">
        <div class="text">
          <span class="name">This build</span>
          <span class="desc">volt {{ store.version || 'unknown' }}</span>
        </div>
        <button type="button" class="btn btn-sm" :disabled="checking" @click="checkNow">
          {{ checking ? 'Checking' : 'Check now' }}
        </button>
      </div>

      <div class="setting">
        <div class="text">
          <label class="name" for="auto-update">Check for updates on start</label>
          <span class="desc">
            Asks GitHub whether there is a newer release. It sends this build's version and, as any
            request does, your IP address — nothing about you or your collections. Off means no
            request is made at all.
          </span>
        </div>
        <input id="auto-update" v-model="autoCheck" class="switch" type="checkbox">
      </div>

      <div v-if="store.update" class="setting ready">
        <div class="text">
          <span class="name">volt {{ store.update.version }} is ready</span>
          <span class="desc">Installing restarts the app. Save anything you are in the middle of first.</span>
        </div>
        <button type="button" class="btn btn-primary btn-sm" :disabled="store.installing" @click="store.installUpdate()">
          {{ store.installing ? 'Installing' : 'Install and restart' }}
        </button>
      </div>
    </section>

    <template #footer>
      <button type="button" class="btn btn-quiet" @click="reset">Reset to defaults</button>
      <span class="spacer" />
      <button type="button" class="btn btn-quiet" @click="cancel">Cancel</button>
      <button type="button" class="btn btn-primary" :disabled="timeoutInvalid || saving" @click="save">Save settings</button>
    </template>
  </UiDialog>
</template>

<style scoped>
.scope { margin: 0 0 var(--s-2); color: var(--silk); font-size: var(--t-small); line-height: 1.5; }

.group { margin-top: var(--s-5); }
.group h3 { margin: 0 0 var(--s-2); }

.setting {
  display: flex;
  align-items: center;
  gap: var(--s-6);
  padding: var(--s-3) 0;
  border-top: 1px solid var(--line-soft);
}
.text { flex: 1; display: grid; gap: 3px; min-width: 0; }
.name { font-weight: 600; }
.desc { color: var(--silk); font-size: var(--t-small); line-height: 1.45; }
.desc.bad { color: var(--bad); }
.desc.warn { color: var(--warn); display: flex; gap: 6px; align-items: flex-start; }
.desc.warn .ui-icon { margin-top: 2px; }

.themes { flex-direction: column; align-items: stretch; gap: var(--s-3); }
.theme-grid { display: grid; grid-template-columns: repeat(5, minmax(0, 1fr)); gap: var(--s-2); }
.theme-card {
  display: grid;
  gap: 5px;
  padding: 6px;
  border: 1px solid var(--line);
  border-radius: var(--r-md);
  background: var(--bg-2);
  text-align: left;
  transition: border-color var(--dur) var(--ease), box-shadow var(--dur) var(--ease);
}
.theme-card:hover { border-color: var(--line-strong); }
.theme-card.on { border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent-tint); }
/* The swatch carries its own theme, so these tokens resolve to that theme's. */
.swatch {
  display: flex;
  height: 34px;
  border-radius: var(--r-sm);
  overflow: hidden;
  border: 1px solid var(--line);
  background: var(--bg-1);
}
.sw { flex: 1; }
.sw-chrome { background: var(--bg-0); }
.sw-paper { background: var(--bg-1); }
.sw-accent { background: var(--accent); }
.sw-ink { background: var(--ink); }
.theme-name { font-weight: 600; font-size: var(--t-small); }
.theme-hint { color: var(--silk); font-size: var(--t-label); line-height: 1.3; }

.with-unit { display: flex; align-items: center; gap: var(--s-2); }
.with-unit .field { width: 84px; text-align: right; }
.proxy { width: 240px; flex: none; }
.cookie-controls { display: flex; align-items: center; gap: var(--s-3); flex: none; }
.ready { background: var(--accent-tint); box-shadow: inset 0 0 0 1px var(--accent-line); border-radius: var(--r-sm); padding: var(--s-3); }
</style>
