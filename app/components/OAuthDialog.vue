<script setup lang="ts">
import type { OAuthConfig, OAuthToken } from '~/types'
import type { SegmentOption } from '~/utils/ui'

const store = useCollectionStore()
const emit = defineEmits<{ close: [] }>()

const grants: SegmentOption<string>[] = [
  { value: 'client_credentials', label: 'Client credentials' },
  { value: 'authorization_code', label: 'Authorization code' },
  { value: 'refresh_token', label: 'Refresh' },
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
})

const needsBrowser = computed(() => grant.value === 'authorization_code')

async function get() {
  working.value = true
  token.value = await store.getOAuthToken(config.value, grant.value, variable.value)
  working.value = false
}

function expires(at: number | null) {
  if (!at) return 'no expiry given'
  const minutes = Math.round((at * 1000 - Date.now()) / 60000)
  return minutes > 0 ? `expires in about ${minutes} min` : 'already expired'
}
</script>

<template>
  <UiDialog title="Get a token" eyebrow="OAuth 2.0" :width="720" @close="emit('close')">
    <p class="note">
      <UiIcon name="lock" :size="14" />
      <span>
        The token goes into the active environment as a secret variable, so requests
        use <code>{{ placeholder }}</code> like any other. It is not hidden inside an auth
        object and not fetched again on every send.
      </span>
    </p>

    <UiSegmented v-model="grant" :options="grants" label="Grant" size="sm" class="grants" />

    <div class="form">
      <span class="silk">Token URL</span>
      <input v-model="config.tokenUrl" class="field mono" aria-label="Token URL" placeholder="https://id.example.com/oauth/token" spellcheck="false">

      <template v-if="needsBrowser">
        <span class="silk">Authorize URL</span>
        <input v-model="config.authUrl" class="field mono" aria-label="Authorization URL" placeholder="https://id.example.com/authorize" spellcheck="false">
      </template>

      <span class="silk">Client ID</span>
      <input v-model="config.clientId" class="field mono" aria-label="Client ID" spellcheck="false">

      <span class="silk">Client secret</span>
      <input v-model="config.clientSecret" class="field mono" type="password" aria-label="Client secret" autocomplete="off">

      <span class="silk">Scope</span>
      <input v-model="config.scope" class="field mono" aria-label="Scope" placeholder="read write" spellcheck="false">

      <span class="silk">Audience</span>
      <input v-model="config.audience" class="field mono" aria-label="Audience" placeholder="optional" spellcheck="false">

      <span class="silk">Keep it as</span>
      <input v-model="variable" class="field mono" aria-label="Variable name" placeholder="token" spellcheck="false">

      <span class="silk">Send client as</span>
      <label class="switch-row">
        <input v-model="config.basicAuth" type="checkbox" class="switch">
        <span>{{ config.basicAuth ? 'Basic auth header' : 'Form fields' }} — providers disagree; this is the switch</span>
      </label>
    </div>

    <p v-if="needsBrowser" class="hint">
      <span>Opens your browser and waits up to three minutes for it to come back to a loopback address. PKCE is always used.</span>
    </p>

    <div v-if="token" class="got">
      <span class="led ok" />
      <span>Got a {{ token.tokenType }} token, {{ expires(token.expiresAt) }}.</span>
      <span class="silk">Kept as {{ variable || 'token' }}</span>
    </div>

    <template #footer>
      <span class="spacer" />
      <button type="button" class="btn btn-quiet" @click="emit('close')">Close</button>
      <button
        type="button"
        class="btn btn-primary"
        :disabled="working || !config.tokenUrl.trim() || !config.clientId.trim()"
        @click="get"
      >
        {{ working ? (needsBrowser ? 'Waiting for the browser' : 'Asking') : 'Get the token' }}
      </button>
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

.grants { margin-bottom: var(--s-4); }
.form { display: grid; grid-template-columns: 120px minmax(0, 1fr); gap: var(--s-2) var(--s-3); align-items: center; }
.switch-row { display: flex; align-items: center; gap: var(--s-2); font-size: var(--t-meta); color: var(--silk); cursor: pointer; }
.hint { display: flex; margin: var(--s-3) 0 0; color: var(--faint); font-size: var(--t-meta); }
.got { display: flex; align-items: center; gap: var(--s-3); margin-top: var(--s-4); font-size: var(--t-small); }
</style>
