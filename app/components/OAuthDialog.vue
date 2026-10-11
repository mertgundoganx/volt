<script setup lang="ts">
import type { OAuthConfig, OAuthDevice, OAuthToken } from '~/types'

const store = useCollectionStore()
const emit = defineEmits<{ close: [] }>()

/** The ways to a token, each with the one line that tells them apart. */
const grants = [
  { value: 'client_credentials', label: t('Client credentials'), hint: t('A service signing in as itself: an id and a secret, no person involved.') },
  { value: 'authorization_code', label: t('Authorization code'), hint: t('Opens your browser to sign in, and waits up to three minutes for it to come back. PKCE is always used.') },
  { value: 'password', label: t('Password'), hint: t('A user name and password sent straight to the token endpoint. Only for providers that still allow it.') },
  { value: 'device', label: t('Device code'), hint: t('Shows a short code to type at an address, on this or another device, and waits for you to approve it.') },
  { value: 'refresh_token', label: t('Refresh'), hint: t('Trades the refresh token from the last fetch for a new access token.') },
]

// Inline in the template the inner braces would close the interpolation.
const placeholder = '{{' + 'token' + '}}'

const grant = ref('client_credentials')
const variable = ref('token')
const working = ref(false)
const token = ref<OAuthToken | null>(null)

const config = ref<OAuthConfig>({
  tokenUrl: '',
  authUrl: '',
  clientId: '',
  clientSecret: '',
  scope: '',
  audience: '',
  basicAuth: false,
  username: '',
  password: '',
  deviceUrl: '',
})

// What was typed last time for this collection comes back, without the secrets.
onMounted(async () => {
  const saved = await store.loadOAuthSetup()
  if (!saved) return
  if (grants.some((one) => one.value === saved.grant)) grant.value = saved.grant
  variable.value = saved.variable || 'token'
  config.value = { ...config.value, ...saved.config }
})

const hint = computed(() => grants.find((one) => one.value === grant.value)?.hint ?? '')
const ready = computed(() => {
  const c = config.value
  if (!c.tokenUrl.trim() || !c.clientId.trim()) return false
  if (grant.value === 'authorization_code') return !!c.authUrl.trim()
  if (grant.value === 'password') return !!c.username.trim()
  if (grant.value === 'device') return !!c.deviceUrl.trim()
  if (grant.value === 'refresh_token') return !!store.oauthRefresh
  return true
})

// --- The device grant: a code to show, then waiting -------------------------
const device = ref<OAuthDevice | null>(null)
const deviceNote = ref('')
let pollTimer: ReturnType<typeof setTimeout> | undefined
let gone = false
onUnmounted(() => {
  gone = true
  clearTimeout(pollTimer)
  device.value = null
})

function stopWaiting() {
  clearTimeout(pollTimer)
  device.value = null
  working.value = false
}

async function waitForDevice(every: number, until: number) {
  const current = device.value
  if (!current) return
  if (Date.now() > until) {
    deviceNote.value = t('The code ran out before it was approved. Start again.')
    return stopWaiting()
  }
  const poll = await store.pollOAuthDevice(config.value, current.deviceCode, variable.value)
  if (gone || device.value !== current) return
  if (!poll || poll.status === 'denied' || poll.status === 'expired') {
    // A poll that failed outright left its reason in the error strip, which
    // sits behind this dialog; say it here as well.
    deviceNote.value = poll?.status === 'denied' ? t('The request was refused.') : poll ? t('The code ran out before it was approved. Start again.') : (store.error ?? '')
    return stopWaiting()
  }
  if (poll.status === 'done') {
    token.value = poll.token ?? null
    return stopWaiting()
  }
  // The server may ask for patience; give it five more seconds each time.
  const next = poll.status === 'slow_down' ? every + 5 : every
  pollTimer = setTimeout(() => void waitForDevice(next, until), next * 1000)
}

async function copyCode() {
  if (!device.value) return
  try {
    await navigator.clipboard.writeText(device.value.userCode)
    store.notify(t('Code copied'), device.value.userCode)
  } catch {
    store.error = t('Could not copy to the clipboard.')
  }
}

async function get() {
  token.value = null
  deviceNote.value = ''
  working.value = true
  void store.saveOAuthSetup(grant.value, variable.value, config.value)

  if (grant.value === 'device') {
    const started = await store.startOAuthDevice(config.value)
    if (gone) return
    if (!started) {
      working.value = false
      return
    }
    device.value = started
    const every = Math.max(1, started.interval || 5)
    pollTimer = setTimeout(() => void waitForDevice(every, Date.now() + started.expiresIn * 1000), every * 1000)
    return
  }

  token.value = await store.getOAuthToken(config.value, grant.value, variable.value)
  working.value = false
}

function expires(at: number | null) {
  if (!at) return t('no expiry given')
  const minutes = Math.round((at * 1000 - Date.now()) / 60000)
  return minutes > 0 ? t('expires in about {n} min', { n: minutes }) : t('already expired')
}

const action = computed(() => {
  if (!working.value) return t('Get the token')
  if (grant.value === 'authorization_code') return t('Waiting for the browser')
  if (grant.value === 'device') return t('Waiting for approval')
  return t('Asking')
})
</script>

<template>
  <UiDialog :title="t('Get a token')" eyebrow="OAuth 2.0" :width="720" @close="emit('close')">
    <p class="note">
      <UiIcon name="lock" :size="14" />
      <span>
        {{ t('The token goes into the active environment as a secret variable, so requests use') }} <code>{{ placeholder }}</code> {{ t('like any other.') }}
      </span>
    </p>

    <div class="form">
      <span class="silk">{{ t('How') }}</span>
      <UiSelect v-model="grant" :options="grants" :label="t('Grant')" class="grant" />
      <span />
      <p class="hint">{{ hint }}</p>

      <span class="silk">{{ t('Token URL') }}</span>
      <input v-model="config.tokenUrl" class="field mono" :aria-label="t('Token URL')" placeholder="https://id.example.com/oauth/token" spellcheck="false">

      <template v-if="grant === 'authorization_code'">
        <span class="silk">{{ t('Authorize URL') }}</span>
        <input v-model="config.authUrl" class="field mono" :aria-label="t('Authorization URL')" placeholder="https://id.example.com/authorize" spellcheck="false">
      </template>

      <template v-if="grant === 'device'">
        <span class="silk">{{ t('Device URL') }}</span>
        <input v-model="config.deviceUrl" class="field mono" :aria-label="t('Device authorization URL')" placeholder="https://id.example.com/oauth/device/code" spellcheck="false">
      </template>

      <span class="silk">{{ t('Client ID') }}</span>
      <input v-model="config.clientId" class="field mono" :aria-label="t('Client ID')" spellcheck="false">

      <span class="silk">{{ t('Client secret') }}</span>
      <input v-model="config.clientSecret" class="field mono" type="password" :aria-label="t('Client secret')" autocomplete="off" :placeholder="grant === 'device' || grant === 'authorization_code' ? t('optional') : ''">

      <template v-if="grant === 'password'">
        <span class="silk">{{ t('User name') }}</span>
        <input v-model="config.username" class="field mono" :aria-label="t('User name')" spellcheck="false" autocomplete="off">
        <span class="silk">{{ t('Password') }}</span>
        <input v-model="config.password" class="field mono" type="password" :aria-label="t('Password')" autocomplete="off">
      </template>

      <span class="silk">{{ t('Scope') }}</span>
      <input v-model="config.scope" class="field mono" :aria-label="t('Scope')" placeholder="read write" spellcheck="false">

      <span class="silk">{{ t('Audience') }}</span>
      <input v-model="config.audience" class="field mono" :aria-label="t('Audience')" :placeholder="t('optional')" spellcheck="false">

      <span class="silk">{{ t('Keep it as') }}</span>
      <input v-model="variable" class="field mono" :aria-label="t('Variable name')" placeholder="token" spellcheck="false">

      <span class="silk">{{ t('Send client as') }}</span>
      <label class="switch-row">
        <input v-model="config.basicAuth" type="checkbox" class="switch">
        <span>{{ t('{how} — providers disagree; this is the switch', { how: config.basicAuth ? t('Basic auth header') : t('Form fields') }) }}</span>
      </label>
    </div>

    <p v-if="grant === 'refresh_token' && !store.oauthRefresh" class="hint solo">
      {{ t('There is no refresh token yet: fetch a token another way first, and the provider has to hand one out.') }}
    </p>

    <div v-if="device" class="device">
      <span class="silk">{{ t('Type this code') }}</span>
      <button type="button" class="code mono selectable" :title="t('Copy the code')" @click="copyCode">{{ device.userCode }}</button>
      <span class="silk">{{ t('at') }}</span>
      <span class="where mono selectable">{{ device.verificationUri }}</span>
      <button type="button" class="btn btn-quiet btn-sm" @click="stopWaiting">{{ t('Stop waiting') }}</button>
    </div>
    <p v-else-if="deviceNote" class="hint solo warn">{{ deviceNote }}</p>

    <div v-if="token" class="got">
      <span class="led ok" />
      <span>{{ t('Got a {type} token, {expiry}.', { type: token.tokenType, expiry: expires(token.expiresAt) }) }}</span>
      <span class="silk">{{ t('Kept as {name}', { name: variable || 'token' }) }}</span>
    </div>

    <template #footer>
      <span class="silk">{{ t('The form is remembered for this collection; the secrets are not') }}</span>
      <span class="spacer" />
      <button type="button" class="btn btn-quiet" @click="emit('close')">{{ t('Close') }}</button>
      <button type="button" class="btn btn-primary" :disabled="working || !ready" @click="get">{{ action }}</button>
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

.form { display: grid; grid-template-columns: minmax(120px, max-content) minmax(0, 1fr); gap: var(--s-2) var(--s-3); align-items: center; }
.grant { justify-self: start; }
.grant :deep(select) { width: 240px; }
.switch-row { display: flex; align-items: center; gap: var(--s-2); font-size: var(--t-meta); color: var(--silk); cursor: pointer; }
.hint { margin: 0 0 var(--s-2); color: var(--silk); font-size: var(--t-small); line-height: 1.5; }
.hint.solo { margin: var(--s-3) 0 0; }
.hint.warn { color: var(--warn); }

.device {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: var(--s-2) var(--s-3);
  margin-top: var(--s-4);
  padding: var(--s-3) var(--s-4);
  border-radius: var(--r-md);
  background: var(--accent-tint);
}
.code { padding: 4px 12px; border-radius: var(--r-sm); background: var(--bg-2); font-size: 18px; font-weight: 600; letter-spacing: 0.12em; }
.where { flex: 1; min-width: 0; font-size: var(--t-small); word-break: break-all; }

.got { display: flex; align-items: center; gap: var(--s-3); margin-top: var(--s-4); font-size: var(--t-small); }
</style>
