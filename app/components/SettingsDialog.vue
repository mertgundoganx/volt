<script setup lang="ts">
import { defaultExecOptions } from '~/types'
import type { ThemePreference } from '~/stores/collection'
import { applyTheme } from '~/stores/collection'
import { LANGUAGES, type Language } from '~/composables/useI18n'

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

// The language applies on Save: the whole window is redrawn in it.
const chosenLanguage = ref<string>(uiLanguage.value)
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
  { value: 'system', label: t('System'), hint: t('Follows the operating system') },
  { value: 'light', label: t('Day'), hint: t('White, lime marker') },
  { value: 'dark', label: t('Night'), hint: t('Near black, lime') },
  { value: 'cobalt', label: t('Cobalt'), hint: t('Paper, ultramarine') },
  { value: 'linen', label: t('Linen'), hint: t('Cream, brick') },
  { value: 'mist', label: t('Mist'), hint: t('Cool grey, teal') },
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

const clientCert = computed({
  get: () => draft.value.clientCert ?? '',
  set: (value: string) => {
    draft.value.clientCert = value.trim() || null
  },
})

async function chooseCert() {
  const { open } = await import('@tauri-apps/plugin-dialog')
  const picked = await open({
    title: t('Choose the client certificate: a PEM with its key, or a .p12 / .pfx'),
    multiple: false,
    directory: false,
    filters: [{ name: t('Certificate'), extensions: ['pem', 'crt', 'key', 'p12', 'pfx'] }],
  })
  if (typeof picked === 'string') clientCert.value = picked
}
/** A PKCS#12 file is locked with a passphrase; a PEM is not. */
const certNeedsPassword = computed(() => /\.(p12|pfx)$/i.test(clientCert.value))
const certPassword = computed({
  get: () => draft.value.clientCertPassword ?? '',
  set: (value: string) => {
    draft.value.clientCertPassword = value || null
  },
})

const caCert = computed({
  get: () => draft.value.caCert ?? '',
  set: (value: string) => {
    draft.value.caCert = value.trim() || null
  },
})
async function chooseCa() {
  const { open } = await import('@tauri-apps/plugin-dialog')
  const picked = await open({
    title: t('Choose a PEM file of certificate authorities to trust'),
    multiple: false,
    directory: false,
    filters: [{ name: 'PEM', extensions: ['pem', 'crt', 'cer'] }],
  })
  if (typeof picked === 'string') caCert.value = picked
}

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
  // Last, and after closing: a new language redraws the whole window, this
  // dialog included, and what is unmounted can no longer ask to be closed.
  if (chosenLanguage.value !== uiLanguage.value) await store.saveLanguage(chosenLanguage.value as Language)
}
</script>

<template>
  <UiDialog :title="t('Settings')" eyebrow="volt" :width="600" @close="cancel">
    <p class="scope">{{ t('These apply to this app on this machine, not to any collection, and are never written to its files.') }}</p>

    <section class="group" aria-labelledby="s-appearance">
      <h3 id="s-appearance" class="silk">{{ t('Appearance') }}</h3>
      <div class="setting themes">
        <div class="text">
          <span class="name">{{ t('Theme') }}</span>
          <span class="desc">{{ t('Applies as you pick; Cancel puts it back.') }}</span>
        </div>
        <div class="theme-grid" role="radiogroup" :aria-label="t('Theme')">
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
      <div class="setting">
        <div class="text">
          <span class="name">{{ t('Language') }}</span>
          <span class="desc">{{ t('The words on the buttons and labels. What a server sends back is shown as it came.') }}</span>
        </div>
        <UiSelect v-model="chosenLanguage" :options="LANGUAGES" :label="t('Language')" class="language" />
      </div>
    </section>

    <section class="group" aria-labelledby="s-requests">
      <h3 id="s-requests" class="silk">{{ t('Requests') }}</h3>

      <div class="setting">
        <div class="text">
          <label class="name" for="timeout">{{ t('Timeout') }}</label>
          <span v-if="timeoutInvalid" class="desc bad">{{ t('Must be greater than zero.') }}</span>
          <span v-else class="desc">{{ t('How long to wait for a response before giving up.') }}</span>
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
          <span class="silk">{{ t('sec') }}</span>
        </div>
      </div>

      <div class="setting">
        <div class="text">
          <label class="name" for="redirects">{{ t('Follow redirects') }}</label>
          <span class="desc">
            {{ draft.followRedirects ? t('Follows up to 10 redirects, like curl -L.') : t('Returns the 3xx response itself, so its Location can be inspected.') }}
          </span>
        </div>
        <input id="redirects" v-model="draft.followRedirects" class="switch" type="checkbox">
      </div>
      <div class="setting">
        <div class="text">
          <label class="name" for="proxy">{{ t('Proxy') }}</label>
          <span class="desc">
            {{ t('Every request goes through it, unless one overrides it in its Settings tab.') }}
            {{ t('Leave it empty to use the system settings.') }}
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
          <label class="name" for="client-cert">{{ t('Client certificate') }}</label>
          <span class="desc">
            {{ t('For servers that ask who you are (mutual TLS): a PEM file with the certificate and its key, or a .p12 / .pfx.') }}
            {{ t("A request's Settings tab can name another.") }}
          </span>
        </div>
        <span class="cert">
          <input
            id="client-cert"
            v-model="clientCert"
            class="field mono proxy"
            spellcheck="false"
            :placeholder="t('none')"
          >
          <button type="button" class="btn btn-sm" @click="chooseCert">{{ t('Choose…') }}</button>
        </span>
      </div>
      <div v-if="certNeedsPassword" class="setting">
        <div class="text">
          <label class="name" for="client-cert-password">{{ t('Certificate passphrase') }}</label>
          <span class="desc">{{ t("Unlocks the .p12 / .pfx above. Kept in this app's settings on this machine, never in a collection.") }}</span>
        </div>
        <input id="client-cert-password" v-model="certPassword" class="field mono proxy" type="password" autocomplete="off">
      </div>
      <div class="setting">
        <div class="text">
          <label class="name" for="ca-cert">{{ t('Trusted authorities') }}</label>
          <span class="desc">
            {{ t("A PEM file of certificate authorities to trust besides the usual ones — a company's own, or the one that signed a staging server.") }}
            {{ t('Safer than turning verification off.') }}
          </span>
        </div>
        <span class="cert">
          <input
            id="ca-cert"
            v-model="caCert"
            class="field mono proxy"
            spellcheck="false"
            :placeholder="t('none')"
          >
          <button type="button" class="btn btn-sm" @click="chooseCa">{{ t('Choose…') }}</button>
        </span>
      </div>

      <div class="setting">
        <div class="text">
          <label class="name" for="cookies">{{ t('Keep cookies') }}</label>
          <span class="desc">
            {{ t('Cookies a response sets are sent back on the next request to that host, one jar per collection.') }}
            {{ t('They are held in memory only, so closing volt forgets them.') }}
          </span>
        </div>
        <div class="cookie-controls">
          <button v-if="store.isOpen" type="button" class="btn btn-quiet btn-sm" @click="store.clearCookies()">
            {{ t('Clear now') }}
          </button>
          <input id="cookies" v-model="draft.sendCookies" class="switch" type="checkbox">
        </div>
      </div>

      <div class="setting" :class="{ alarm: !draft.verifyTls }">
        <div class="text">
          <label class="name" for="tls">{{ t('Verify TLS certificates') }}</label>
          <span v-if="draft.verifyTls" class="desc">{{ t('Rejects expired, self-signed and mismatched certificates.') }}</span>
          <span v-else class="desc warn">
            <UiIcon name="warning" :size="13" />
            {{ t('Any certificate is accepted, so traffic can be read or changed in transit. Only for hosts you control.') }}
          </span>
        </div>
        <input id="tls" v-model="draft.verifyTls" class="switch" type="checkbox">
      </div>
    </section>

    <section class="group" aria-labelledby="s-keys">
      <h3 id="s-keys" class="silk">{{ t('Keyboard') }}</h3>
      <div class="setting">
        <div class="text">
          <span class="name">{{ t('Shortcuts') }}</span>
          <span class="desc">{{ t('Send with {mod}+Enter, save with {mod}+S, jump to a request with {mod}+P. The full list is one key away: ?', { mod: modKey }) }}</span>
        </div>
        <button type="button" class="btn btn-sm" @click="emit('close'); store.shortcutsSheet = true">{{ t('Show all') }}</button>
      </div>
    </section>

    <section class="group" aria-labelledby="s-updates">
      <h3 id="s-updates" class="silk">{{ t('Updates') }}</h3>

      <div class="setting">
        <div class="text">
          <span class="name">{{ t('This build') }}</span>
          <span class="desc">volt {{ store.version || t('unknown') }}</span>
        </div>
        <button type="button" class="btn btn-sm" :disabled="checking" @click="checkNow">
          {{ checking ? t('Checking') : t('Check now') }}
        </button>
      </div>

      <div class="setting">
        <div class="text">
          <label class="name" for="auto-update">{{ t('Check for updates on start') }}</label>
          <span class="desc">
            {{ t('Asks GitHub whether there is a newer release.') }}
            {{ t("It sends this build's version and, as any request does, your IP address — nothing about you or your collections.") }}
            {{ t('Off means no request is made at all.') }}
          </span>
        </div>
        <input id="auto-update" v-model="autoCheck" class="switch" type="checkbox">
      </div>

      <div v-if="store.update" class="setting ready">
        <div class="text">
          <span class="name">{{ t('volt {version} is ready', { version: store.update.version }) }}</span>
          <span class="desc">{{ t('Installing restarts the app. Save anything you are in the middle of first.') }}</span>
        </div>
        <button type="button" class="btn btn-primary btn-sm" :disabled="store.installing" @click="store.installUpdate()">
          {{ store.installing ? t('Installing') : t('Install and restart') }}
        </button>
      </div>
    </section>

    <template #footer>
      <button type="button" class="btn btn-quiet" @click="reset">{{ t('Reset to defaults') }}</button>
      <span class="spacer" />
      <button type="button" class="btn btn-quiet" @click="cancel">{{ t('Cancel') }}</button>
      <button type="button" class="btn btn-primary" :disabled="timeoutInvalid || saving" @click="save">{{ t('Save settings') }}</button>
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
.language :deep(select) { width: 150px; }
.theme-grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: var(--s-2); }
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
.theme-card.on { border-color: var(--focus); box-shadow: 0 0 0 2px var(--accent-tint); }
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
.cert { display: flex; gap: var(--s-1); flex: none; }
.cert .proxy { width: 170px; }
.cookie-controls { display: flex; align-items: center; gap: var(--s-3); flex: none; }
.ready { background: var(--accent-tint); box-shadow: inset 0 0 0 1px var(--accent-line); border-radius: var(--r-sm); padding: var(--s-3); }
</style>
