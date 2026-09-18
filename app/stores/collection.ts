import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import { parentOf, type DropPosition } from '~/composables/useTreeMenu'
import { ask, open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog'
import { load as loadStore } from '@tauri-apps/plugin-store'
import { check as checkUpdate, type Update } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'
import { getVersion } from '@tauri-apps/api/app'

import { fileBase } from '~/utils/curl'

import {
  defaultExecOptions,
  emptyRequest,
  normalizeExecOptions,
  normalizeRequest,
  type ApiRequest,
  type Collection,
  type CurlParsed,
  type CurlRendered,
  type Environment,
  type EnvVar,
  type ExecOptions,
  type HistoryEntry,
  type HistorySummary,
  type ImportOutcome,
  type ImportReport,
  type HttpResponse,
  type Node,
  type SendOutcome,
  type Cookie,
  type Example,
  type Generated,
  type Exported,
  type FolderMeta,
  type CollectionMeta,
  type MockRunning,
  type SyncStatus,
  type StreamEvent,
  type StreamOpen,
  type GrpcService,
  type GrpcMethod,
  type GrpcReply,
  type Run,
  type OAuthConfig,
  type OAuthToken,
  type CheckOutcome,
  type Deleted,
  type Comparison,
  type GraphQlSchema,
  type GraphQlProblem,
  type GraphQlCompletion,
} from '~/types'

/** Matches history::MAX_ENTRIES in Rust. */
const HISTORY_LIMIT = 200

const RECENT_KEY = 'recentCollections'
const OPTIONS_KEY = 'execOptions'
const THEME_KEY = 'theme'
const WORKSPACES_KEY = 'workspaces'
const WORKSPACE_KEY = 'workspace'
const MONITORS_KEY = 'monitors'
const AUTO_UPDATE_KEY = 'checkUpdates'

export type ThemePreference = 'system' | 'light' | 'dark'

/** What a pasted curl command became, shown under the request until it changes. */
export interface CurlNotice {
  notes: string[]
  /** Secret variables created for the command's credentials. */
  secrets: string[]
  environment: string | null
}

/** Fill the open request, or create a new file in a folder (`null` is the root). */
export type CurlTarget = { mode: 'replace' } | { mode: 'create'; parent: string | null }

export interface Toast {
  id: number
  text: string
  detail?: string
  tone: 'ok' | 'warn' | 'bad'
  /** Whether this toast is the one offering to undo a delete. Inferring it
   *  from the tone made every other warning offer to restore the last
   *  deleted node, whatever it was. */
  undo?: boolean
}

/** `system` stamps nothing, so only `prefers-color-scheme` decides (see tokens.css). */
export function applyTheme(theme: ThemePreference) {
  if (theme === 'system') delete document.documentElement.dataset.theme
  else document.documentElement.dataset.theme = theme
}

/** Say exactly what a delete will take with it, since it cannot be undone. */
function describeDeletion(node: Node): string {
  if (node.kind === 'request') return `Delete “${node.name}”? The file is removed from disk.`

  let requests = 0
  let folders = 0
  const count = (nodes: Node[]) => {
    for (const child of nodes) {
      if (child.kind === 'folder') {
        folders++
        count(child.children)
      } else {
        requests++
      }
    }
  }
  count(node.children)

  if (requests === 0 && folders === 0) return `Delete the empty folder “${node.name}”?`
  const parts = [
    requests ? `${requests} request${requests === 1 ? '' : 's'}` : '',
    folders ? `${folders} folder${folders === 1 ? '' : 's'}` : '',
  ].filter(Boolean)
  return `Delete “${node.name}” and the ${parts.join(' and ')} inside it? This removes them from disk.`
}

/** Turn whatever came back over the IPC boundary into something readable. */
function describe(e: unknown): string {
  if (typeof e === 'string') return e
  if (e instanceof Error) return e.message
  return JSON.stringify(e)
}

/** One open request. The active tab's values live in the store's own fields. */
export interface Tab {
  id: string | null
  request: ApiRequest | null
  dirty: boolean
  response: HttpResponse | null
  historyEntry: HistorySummary | null
  curlNotice: CurlNotice | null
  captureNotes: string[]
}

function blankTab(id: string | null): Tab {
  return {
    id,
    request: null,
    dirty: false,
    response: null,
    historyEntry: null,
    curlNotice: null,
    captureNotes: [],
  }
}


/** A named set of collection folders. Local to this machine, like Recent is. */
export interface Workspace {
  name: string
  collections: string[]
}

/**
 * A request sent on a schedule. It runs while volt is open and nowhere else —
 * there is no server here to keep watching after you close the app, and
 * pretending otherwise would be the one thing worse than not having it.
 */
export interface Monitor {
  id: string
  /** The collection this belongs to, so switching collections switches these. */
  root: string
  requestId: string
  name: string
  everySeconds: number
  paused: boolean
}

export interface MonitorRun {
  at: number
  monitorId: string
  name: string
  status: number | null
  durationMs: number | null
  error: string | null
}


/**
 * Timers live outside the store: they are not state to render, and putting a
 * timer id in a reactive object is how you end up with two of them running.
 */
const monitorTimers = new Map<string, ReturnType<typeof setInterval>>()

function scheduleMonitor(store: ReturnType<typeof useCollectionStore>, monitor: Monitor) {
  clearMonitor(monitor.id)
  monitorTimers.set(
    monitor.id,
    setInterval(() => store.runMonitor(monitor), monitor.everySeconds * 1000),
  )
}

function clearMonitor(id: string) {
  const timer = monitorTimers.get(id)
  if (timer) clearInterval(timer)
  monitorTimers.delete(id)
}

function clearAllMonitors() {
  for (const timer of monitorTimers.values()) clearInterval(timer)
  monitorTimers.clear()
}


export const useCollectionStore = defineStore('collection', {
  state: () => ({
    collection: null as Collection | null,
    recent: [] as string[],

    activeId: null as string | null,
    request: null as ApiRequest | null,
    dirty: false,

    activeEnvironment: null as string | null,

    response: null as HttpResponse | null,
    sending: false,

    /** How requests are executed. App-wide, not part of the collection. */
    options: defaultExecOptions() as ExecOptions,

    theme: 'system' as ThemePreference,
    /**
     * Whether to ask GitHub for a newer release on start.
     *
     * The only network request volt makes that the user did not ask for, so it
     * is a setting rather than a given, and Settings says exactly what it
     * sends. Off means no request at all, not a quieter one.
     */
    autoCheckUpdates: true,
    /** The release waiting to be installed, once a check has found one. */
    update: null as Update | null,
    /** Set while the download is running, so the button can say so. */
    installing: false,
    /** This build's version, read from Tauri rather than hard-coded here. */
    version: '',

    sidebarTab: 'tree' as 'tree' | 'history',
    /** Newest first. Kept by Rust in the app data directory, not the collection. */
    history: [] as HistorySummary[],
    /**
     * Set while the editor shows a request reopened from history. It is not
     * tied to a file, so there is nothing to Save until it is saved as new.
     */
    historyEntry: null as HistorySummary | null,

    /** Shown once after an import: what came across and what did not. */
    importReport: null as ImportReport | null,

    curlNotice: null as CurlNotice | null,
    /** What the last send's captures could not find. */
    captureNotes: [] as string[],
    /** How the open request's checks came out, for the response pane. */
    checkResults: [] as CheckOutcome[],
    /** Whatever the last Postman export could not take with it. */
    exportNotes: [] as string[],
    /** Named sets of collections, kept in this app's settings. */
    workspaces: [] as Workspace[],
    workspace: null as string | null,
    /** Scheduled sends, which only run while volt is open. */
    monitors: [] as Monitor[],
    monitorRuns: [] as MonitorRun[],
    /** The last thing deleted, so it can be put back in one click. */
    lastDeleted: null as Deleted | null,
    /** The open stream, when there is one, and what it has said. */
    stream: null as StreamOpen | null,
    /**
     * The request the open connection belongs to. There is one socket at a
     * time, and without knowing whose it is, pressing Send on a second socket
     * tab disconnected the first one instead of connecting — and the events
     * from one request were read as the other's.
     */
    streamOwner: null as string | null,
    streamEvents: [] as StreamEvent[],
    /** The last gRPC reply, which has its own shape. */
    grpcReply: null as GrpcReply | null,
    /**
     * The methods the open gRPC request can call, from the proto file or from
     * the server itself. Kept here because `go()` has to know whether the
     * chosen one streams before it decides what pressing Send means.
     */
    grpcMethods: [] as GrpcMethod[],
    /** The last run, and whether one is going. */
    lastRun: null as Run | null,
    running: false,
    /** Kept between OAuth fetches so a refresh has something to refresh. */
    oauthRefresh: null as string | null,
    /** The mock server, when it is serving. */
    mock: null as MockRunning | null,
    /** Every open request. The one on screen is `tabs[activeTab]`. */
    tabs: [] as Tab[],
    activeTab: 0,
    /** Dialogs the app shell renders. */
    codeDialog: false,
    cookiesDialog: false,
    scopeDialog: null as { id: string | null; title: string } | null,
    syncDialog: false,
    runnerDialog: false,
    trashDialog: false,
    oauthDialog: false,
    workspacesDialog: false,
    monitorsDialog: false,
    mockDialog: false,
    /** The "request from cURL" dialog, when open. */
    curlDialog: null as CurlTarget | null,

    /** A short confirmation, such as "Copied as cURL". */
    toast: null as Toast | null,

    /** Surfaced as a banner. Every failed action lands here. */
    error: null as string | null,
  }),

  getters: {
    root: (state) => state.collection?.root ?? null,
    isOpen: (state) => state.collection !== null,
    tree: (state): Node[] => state.collection?.tree ?? [],
    environments: (state): Environment[] => state.collection?.environments ?? [],

    /**
     * The open connection, but only when it belongs to the request on screen.
     * Everything that reads `stream` to decide what the UI shows or what Send
     * does goes through this, so a socket opened on one tab is not mistaken for
     * this tab's.
     */
    myStream(state): StreamOpen | null {
      return state.stream && state.streamOwner === state.activeId ? state.stream : null
    },

    /** Whether the method the open request names is a streaming one. */
    grpcStreams(): boolean {
      const body = this.request?.body
      if (body?.type !== 'grpc') return false
      const method = this.grpcMethods.find((one) => one.fullName === body.method)
      return Boolean(method && (method.clientStreaming || method.serverStreaming))
    },

    /** Every folder, flattened, for the "move to" picker. */
    folders(state): { id: string; name: string; depth: number }[] {
      const out: { id: string; name: string; depth: number }[] = []
      const walk = (nodes: Node[], depth: number) => {
        for (const node of nodes) {
          if (node.kind !== 'folder') continue
          out.push({ id: node.id, name: node.name, depth })
          walk(node.children, depth + 1)
        }
      }
      walk(state.collection?.tree ?? [], 0)
      return out
    },

    environment(state): Environment | null {
      const all = state.collection?.environments ?? []
      return all.find((e) => e.name === state.activeEnvironment) ?? all[0] ?? null
    },
  },

  actions: {
    /**
     * Every action goes through here. Without it a rejected `invoke` is an
     * unhandled promise rejection and the UI just sits there doing nothing.
     */
    async attempt<T>(fn: () => Promise<T>): Promise<T | undefined> {
      this.error = null
      try {
        return await fn()
      } catch (e) {
        this.error = describe(e)
        return undefined
      }
    },

    requireRoot(): string {
      if (!this.root) throw new Error('No collection is open. Use “Open…” or “New” first.')
      return this.root
    },

    async restore() {
      await this.attempt(async () => {
        const store = await loadStore('volt.json', { autoSave: true })
        this.recent = (await store.get<string[]>(RECENT_KEY)) ?? []
        this.options = normalizeExecOptions(await store.get<Partial<ExecOptions>>(OPTIONS_KEY))
        this.workspaces = (await store.get<Workspace[]>(WORKSPACES_KEY)) ?? []
        this.workspace = (await store.get<string | null>(WORKSPACE_KEY)) ?? null
        this.monitors = (await store.get<Monitor[]>(MONITORS_KEY)) ?? []
        const theme = await store.get<ThemePreference>(THEME_KEY)
        this.theme = theme === 'light' || theme === 'dark' ? theme : 'system'
        applyTheme(this.theme)

        this.autoCheckUpdates = (await store.get<boolean>(AUTO_UPDATE_KEY)) ?? true
        this.version = await getVersion().catch(() => '')
        // Quietly, and not in the way: whether there is a new release is not
        // worth blocking the collection the user came here to open.
        if (this.autoCheckUpdates) void this.checkForUpdate()

        const last = this.recent[0]
        if (!last) return

        try {
          await this.load(last)
        } catch {
          // A remembered path can be moved or deleted; forget it quietly.
          this.recent = this.recent.slice(1)
          await store.set(RECENT_KEY, this.recent)
        }
      })
    },

    /** Throws on failure — callers decide whether that is worth reporting. */
    async load(path: string) {
      // A socket belongs to the collection it was opened from, so it goes with
      // it. Left open it would keep pushing events at a pane showing something
      // else, and nothing would ever close it.
      try {
        await this.closeStream()
      } catch {
        // Already gone on the Rust side; nothing to close.
        this.stream = null
      }
      this.streamEvents = []
      this.streamOwner = null
      this.grpcReply = null

      this.collection = await invoke<Collection>('open_collection', { path })
      this.activeEnvironment = this.collection.environments[0]?.name ?? null
      // Another collection, so nothing that was open still belongs here.
      this.tabs = []
      this.activeTab = 0
      this.activeId = null
      this.request = null
      this.response = null
      this.historyEntry = null
      this.curlNotice = null
      this.captureNotes = []
      this.history = []

      // A file that would not parse is left out of the tree rather than
      // refusing the whole collection — but silence about it is how a request
      // goes missing without anybody noticing.
      const problems = this.collection.problems ?? []
      if (problems.length) {
        const many = problems.length > 1
        this.error = `${many ? 'These files' : 'This file'} could not be read and ${many ? 'are' : 'is'} not in the tree: ${problems.join(', ')}`
      }

      this.recent = [path, ...this.recent.filter((p) => p !== path)].slice(0, 8)
      const store = await loadStore('volt.json', { autoSave: true })
      await store.set(RECENT_KEY, this.recent)

      await this.refreshHistory()
      // Monitors belong to a collection, so opening another one stops theirs.
      this.restartMonitors()
    },

    async open(path: string) {
      await this.attempt(() => this.load(path))
    },

    async browseAndOpen() {
      await this.attempt(async () => {
        const path = await openDialog({ directory: true, multiple: false })
        if (typeof path !== 'string') return
        await this.load(path)
      })
    },

    async browseAndInit() {
      await this.attempt(async () => {
        const path = await openDialog({ directory: true, multiple: false })
        if (typeof path !== 'string') return

        // Windows dialogs return backslashes; splitting on `/` alone named the
        // collection after its entire path.
        const name = path.split(/[\\/]/).filter(Boolean).pop() ?? 'Untitled'
        await invoke<Collection>('init_collection', { path, name })
        await this.load(path)
      })
    },

    /**
     * Convert a Postman or Insomnia export. The collection is created as a new
     * folder inside the chosen one, so picking a busy directory is safe.
     */
    async browseAndImport() {
      await this.attempt(async () => {
        const source = await openDialog({
          title: 'Choose a Postman or Insomnia export',
          multiple: false,
          directory: false,
          filters: [{ name: 'Postman or Insomnia export', extensions: ['json', 'yaml', 'yml'] }],
        })
        if (typeof source !== 'string') return

        const into = await openDialog({
          title: 'Choose where to create the collection',
          multiple: false,
          directory: true,
        })
        if (typeof into !== 'string') return

        const outcome = await invoke<ImportOutcome>('import_collection', { source, into })
        await this.load(outcome.report.path)
        this.importReport = outcome.report
      })
    },

    async reload() {
      const root = this.requireRoot()
      this.collection = await invoke<Collection>('open_collection', { path: root })
    },

    async select(id: string) {
      await this.attempt(() => this.openRequest(id))
    },

    /** Throws, for callers already inside `attempt()`. */
    async openRequest(id: string) {
      // Snapshot first, so the active tab's id is current before the lookup
      // below decides whether this file is already open.
      this.snapshot()
      // Already open: go back to it rather than throwing its edits away.
      const open = this.tabs.findIndex((tab) => tab.id === id)
      if (open !== -1) return this.switchTab(open)

      const root = this.requireRoot()
      const raw = await invoke<ApiRequest>('get_request', { root, id })
      this.tabs.push(blankTab(id))
      this.activeTab = this.tabs.length - 1

      this.activeId = id
      this.request = normalizeRequest(raw)
      this.dirty = false
      this.response = null
      this.historyEntry = null
      this.curlNotice = null
      this.captureNotes = []
    },

    /**
     * The open tabs are snapshots; the live fields are the one on screen.
     * Keeping it that way means every action can go on writing `this.request`
     * as before, and only switching has to think about tabs at all.
     */
    snapshot() {
      const tab = this.tabs[this.activeTab]
      if (!tab) return
      tab.id = this.activeId
      tab.request = this.request
      tab.dirty = this.dirty
      tab.response = this.response
      tab.historyEntry = this.historyEntry
      tab.curlNotice = this.curlNotice
      tab.captureNotes = this.captureNotes
    },

    switchTab(index: number) {
      const next = this.tabs[index]
      if (!next || index === this.activeTab) {
        if (next) this.activeTab = index
        return
      }
      this.snapshot()
      this.activeTab = index
      this.activeId = next.id
      this.request = next.request
      this.dirty = next.dirty
      this.response = next.response
      this.historyEntry = next.historyEntry
      this.curlNotice = next.curlNotice
      this.captureNotes = next.captureNotes
    },

    /** Closing a tab with unsaved edits asks first; the file is never touched. */
    async closeTab(index: number) {
      const tab = index === this.activeTab ? this.liveTab() : this.tabs[index]
      if (!tab) return
      if (tab.dirty) {
        const name = tab.request?.name || 'this request'
        const confirmed = await ask(`Close “${name}”? Its unsaved changes are lost.`, {
          title: 'Close',
          kind: 'warning',
          okLabel: 'Close',
          cancelLabel: 'Keep open',
        })
        if (!confirmed) return
      }
      this.dropTab(index)
    },

    /** Close without asking — for a request that is no longer on disk. */
    dropTab(index: number) {
      const going = this.tabs[index]
      if (!going) return
      // A connection belongs to the request that opened it, so closing that
      // tab closes it. Otherwise it keeps running with nothing on screen and
      // no way to reach the Disconnect button.
      if (this.stream && this.streamOwner !== null && this.streamOwner === going.id) {
        void this.closeStream()
      }
      this.tabs.splice(index, 1)

      if (index < this.activeTab) {
        this.activeTab -= 1
        return
      }
      if (index > this.activeTab) return

      // The open one went: fall back to its neighbour, or to nothing.
      this.activeTab = Math.min(index, this.tabs.length - 1)
      const next = this.tabs[this.activeTab]
      this.activeId = next?.id ?? null
      this.request = next?.request ?? null
      this.dirty = next?.dirty ?? false
      this.response = next?.response ?? null
      this.historyEntry = next?.historyEntry ?? null
      this.curlNotice = next?.curlNotice ?? null
      this.captureNotes = next?.captureNotes ?? []
    },

    /** The active tab with the live fields written into it. */
    liveTab(): Tab | null {
      this.snapshot()
      return this.tabs[this.activeTab] ?? null
    },

    notify(text: string, detail?: string, tone: Toast['tone'] = 'ok', undo = false) {
      const id = Date.now()
      this.toast = { id, text, detail, tone, undo }
      setTimeout(() => {
        if (this.toast?.id === id) this.toast = null
      }, detail ? 4200 : 2600)
    },

    /**
     * Add secret variables to the active environment, creating `local` when
     * there is none, and return the environment's name. Throws.
     */
    async addSecrets(vars: EnvVar[]): Promise<string | null> {
      const current = this.environment
      if (!vars.length) return current?.name ?? null
      const root = this.requireRoot()
      const name = current?.name ?? 'local'
      const environment = { name, vars: [...(current?.vars ?? []), ...vars] }
      await invoke('save_environment', { root, environment, previous: current?.name ?? null })
      await this.reload()
      this.activeEnvironment = name
      return name
    },

    /**
     * Turn a curl command into a request. Credentials it held in plain text go
     * to secret variables first, so the request only ever sees `{{name}}`.
     * Resolves to whether it worked, so a dialog can stay open on failure.
     */
    async importCurl(command: string, target: CurlTarget): Promise<boolean> {
      const done = await this.attempt(async () => {
        const root = this.requireRoot()
        if (target.mode === 'replace' && !this.request) throw new Error('Open a request to fill it from curl.')

        const parsed = await invoke<CurlParsed>('parse_curl', { command, envVars: this.environment?.vars ?? [] })
        const incoming = normalizeRequest(parsed.request)
        const environment = await this.addSecrets(parsed.newSecrets)

        if (target.mode === 'replace' && this.request) {
          // The command describes what is sent; the name and notes are the user's.
          const request = this.request
          request.method = incoming.method
          request.url = incoming.url
          request.params = incoming.params
          request.headers = incoming.headers
          request.body = incoming.body
          request.auth = incoming.auth
          if (!request.name.trim() || request.name === emptyRequest().name) request.name = incoming.name
          this.touch()
        } else if (target.mode === 'create') {
          const id = this.freeId(target.parent, fileBase(incoming.name))
          await invoke('save_request', { root, id, request: incoming })
          await this.reload()
          await this.openRequest(id)
        }

        this.curlNotice = {
          notes: parsed.notes,
          secrets: parsed.newSecrets.map((v) => v.name),
          environment: parsed.newSecrets.length ? environment : null,
        }
        return true
      })
      return done === true
    },

    /**
     * Copy a request as a curl command, resolved the way Send resolves it.
     * Secrets stay as `{{name}}` unless asked for, so it is safe to share.
     */
    async copyCurl(includeSecrets: boolean, source?: ApiRequest, sourceId?: string) {
      await this.attempt(async () => {
        const root = this.requireRoot()
        const request = source ?? this.request
        if (!request) throw new Error('Open a request to copy it as curl.')

        const out = await invoke<CurlRendered>('export_curl', {
          root,
          request,
          envVars: this.environment?.vars ?? [],
          options: this.options,
          requestId: source ? sourceId : this.activeId,
          includeSecrets,
        })
        await navigator.clipboard.writeText(out.command)

        const detail = [
          out.hidden.length ? `Secrets kept as ${out.hidden.map((n) => `{{${n}}}`).join(', ')}` : '',
          out.undefined.length ? `No value for ${out.undefined.join(', ')}` : '',
        ]
          .filter(Boolean)
          .join(' · ')
        this.notify(
          includeSecrets ? 'Copied as cURL, with secret values' : 'Copied as cURL',
          detail || undefined,
          out.undefined.length ? 'warn' : 'ok',
        )
      })
    },

    /** From the tree: an open request copies what is in the editor, unsaved edits included. */
    async copyCurlFor(id: string) {
      if (id === this.activeId && this.request) return this.copyCurl(false)
      await this.attempt(async () => {
        const root = this.requireRoot()
        const raw = await invoke<ApiRequest>('get_request', { root, id })
        await this.copyCurl(false, normalizeRequest(raw), id)
      })
    },

    touch() {
      this.dirty = true
    },

    async save() {
      await this.attempt(async () => {
        const root = this.requireRoot()
        if (!this.activeId || !this.request) throw new Error('Nothing to save.')
        await invoke('save_request', { root, id: this.activeId, request: this.request })
        this.dirty = false
        await this.reload()
      })
    },

    /** Pick a filename that does not collide with something already in the tree. */
    freeId(folderId: string | null, base: string): string {
      const taken = new Set<string>()
      const walk = (nodes: Node[]) => {
        for (const node of nodes) {
          taken.add(node.id)
          if (node.kind === 'folder') walk(node.children)
        }
      }
      walk(this.tree)

      const prefix = folderId ? `${folderId}/` : ''
      let candidate = `${prefix}${base}.yaml`
      let n = 2
      while (taken.has(candidate)) candidate = `${prefix}${base}-${n++}.yaml`
      return candidate
    },

    async createRequest(folderId: string | null) {
      await this.attempt(async () => {
        const root = this.requireRoot()
        const id = this.freeId(folderId, 'new-request')
        await invoke('save_request', { root, id, request: emptyRequest() })
        await this.reload()
        await this.select(id)
      })
    },

    /**
     * Create a folder inside `parent` (null for the root). The typed name is the
     * display name; Rust picks a safe, unique directory name for it.
     */
    async createFolder(parent: string | null, name: string) {
      await this.attempt(async () => {
        const root = this.requireRoot()
        if (!name.trim()) throw new Error('Folder name cannot be empty.')
        await invoke<string>('create_folder', { root, parent, name: name.trim() })
        await this.reload()
      })
    },

    /**
     * Change a node's display name. The file keeps its own name, so `id` is
     * unaffected and git sees a one-line edit.
     */
    async rename(id: string, name: string) {
      await this.attempt(async () => {
        const root = this.requireRoot()
        const trimmed = name.trim()
        if (!trimmed) throw new Error('Name cannot be empty.')

        await invoke('rename_node', { root, id, name: trimmed })
        // Every tab holding this file, not only the one in front: a background
        // tab still carrying the old name would put it back on its next save.
        for (const tab of this.tabs) {
          if (tab.id === id && tab.request) tab.request.name = trimmed
        }
        if (this.activeId === id && this.request) this.request.name = trimmed
        await this.reload()
      })
    },

    /**
     * Throws on failure, so callers must be inside `attempt()`. Kept separate
     * from `moveNode` because a nested `attempt()` would swallow the error and
     * let the caller carry on against stale state.
     */
    async applyMove(id: string, newParent: string | null): Promise<string> {
      const root = this.requireRoot()
      const newId = await invoke<string>('move_node', { root, id, newParent })

      // Every open tab may be the moved node, or may live inside a moved
      // folder. An id that is not remapped points at a file that is no longer
      // there, and the next save would recreate it at the old path.
      const remap = (open: string | null) => {
        if (open === id) return newId
        if (open?.startsWith(`${id}/`)) return newId + open.slice(id.length)
        return open
      }
      this.activeId = remap(this.activeId)
      for (const tab of this.tabs) tab.id = remap(tab.id)

      // A monitor holds an id too, and it keeps firing. Left unmapped it hits a
      // file that is no longer there every interval, for ever, with a failure
      // toast each time.
      let moved = false
      this.monitors = this.monitors.map((monitor) => {
        if (monitor.root !== root) return monitor
        const next = remap(monitor.requestId)
        if (next === monitor.requestId) return monitor
        moved = true
        return { ...monitor, requestId: next as string }
      })
      if (moved) await this.saveMonitors()

      return newId
    },

    /** Move a node into another folder. `newParent` is null for the root. */
    async moveNode(id: string, newParent: string | null) {
      await this.attempt(async () => {
        await this.applyMove(id, newParent)
        await this.reload()
      })
    },

    /** The children of a folder, or the top level when `parent` is null. */
    siblingsOf(parent: string | null): Node[] {
      if (parent === null) return this.tree

      const find = (nodes: Node[]): Node[] | null => {
        for (const node of nodes) {
          if (node.kind !== 'folder') continue
          if (node.id === parent) return node.children
          const hit = find(node.children)
          if (hit) return hit
        }
        return null
      }
      return find(this.tree) ?? []
    },

    /**
     * Resolve a drag-and-drop. Dropping onto a folder moves into it; dropping
     * above or below a row places the node there, moving it between folders
     * first when the drop crossed a boundary.
     */
    async drop(dragId: string, targetId: string, position: DropPosition) {
      await this.attempt(async () => {
        const root = this.requireRoot()

        if (position === 'inside') {
          await this.applyMove(dragId, targetId)
          await this.reload()
          return
        }

        const parent = parentOf(targetId)
        let movedId = dragId
        if (parentOf(dragId) !== parent) {
          movedId = await this.applyMove(dragId, parent)
          await this.reload()
        }

        const ordered = this.siblingsOf(parent)
          .map((node) => node.id)
          .filter((id) => id !== movedId)

        const at = ordered.indexOf(targetId)
        if (at === -1) throw new Error('That row is no longer where it was; try again.')
        ordered.splice(position === 'before' ? at : at + 1, 0, movedId)

        await invoke('reorder', { root, parent, ordered })
        await this.reload()
      })
    },

    /** Delete a node after the user confirms. There is no undo, so always ask. */
    async remove(id: string) {
      await this.attempt(async () => {
        const root = this.requireRoot()
        const node = this.findNode(id)
        if (!node) throw new Error('That item is no longer in the collection.')

        const confirmed = await ask(describeDeletion(node), {
          title: 'Delete',
          kind: 'warning',
          okLabel: 'Delete',
          cancelLabel: 'Cancel',
        })
        if (!confirmed) return

        const deleted = await invoke<Deleted>('delete_node', { root, id })

        // A tab on a file that is no longer there would recreate it on its next
        // save, so those close — without asking, since it is already gone.
        for (let i = this.tabs.length - 1; i >= 0; i--) {
          const open = i === this.activeTab ? this.activeId : this.tabs[i]!.id
          if (open === id || open?.startsWith(`${id}/`)) this.dropTab(i)
        }
        await this.reload()

        // A monitor pointed at what was just deleted would fail on every tick
        // for ever. Stop it and say so, so nothing runs against a file that is
        // in the bin.
        const orphaned = this.monitors.filter(
          (monitor) =>
            monitor.root === root &&
            (monitor.requestId === id || monitor.requestId.startsWith(`${id}/`)),
        )
        if (orphaned.length) {
          for (const monitor of orphaned) clearMonitor(monitor.id)
          this.monitors = this.monitors.filter((monitor) => !orphaned.includes(monitor))
          await this.saveMonitors()
        }

        // Nothing was destroyed: it is in the collection's bin. Say so, and
        // offer the way back while the thought is still fresh.
        const also = orphaned.length
          ? `It is in the bin · ${orphaned.length} monitor${orphaned.length > 1 ? 's' : ''} stopped`
          : 'It is in the bin'
        this.notify(`Deleted ${deleted.name}`, also, 'warn', true)
        this.lastDeleted = deleted
      })
    },

    /** Put the last deleted node back, if nothing has taken its place. */
    async undoDelete() {
      const deleted = this.lastDeleted
      if (!deleted) return
      await this.attempt(async () => {
        const root = this.requireRoot()
        await invoke('restore_node', { root, deleted })
        this.lastDeleted = null
        await this.reload()
        this.notify(`${deleted.name} is back`)
      })
    },

    async loadTrash(): Promise<Deleted[]> {
      const listed = await this.attempt(async () => invoke<Deleted[]>('list_trash', { root: this.requireRoot() }))
      return listed ?? []
    },

    async restoreDeleted(deleted: Deleted): Promise<boolean> {
      const done = await this.attempt(async () => {
        await invoke('restore_node', { root: this.requireRoot(), deleted })
        await this.reload()
        return true
      })
      if (done && this.lastDeleted?.at === deleted.at) this.lastDeleted = null
      return done === true
    },

    /** The one delete that is not recoverable, so it asks. */
    async emptyTrash(): Promise<boolean> {
      const done = await this.attempt(async () => {
        const root = this.requireRoot()
        const confirmed = await ask('Empty the bin? What is in it is gone for good.', {
          title: 'Empty the bin',
          kind: 'warning',
          okLabel: 'Empty it',
          cancelLabel: 'Cancel',
        })
        if (!confirmed) return false
        await invoke('empty_trash', { root })
        this.lastDeleted = null
        return true
      })
      return done === true
    },

    findNode(id: string): Node | null {
      const walk = (nodes: Node[]): Node | null => {
        for (const node of nodes) {
          if (node.id === id) return node
          if (node.kind === 'folder') {
            const hit = walk(node.children)
            if (hit) return hit
          }
        }
        return null
      }
      return walk(this.tree)
    },

    /** Display names of the folders above a node, outermost first. */
    folderTrail(id: string): string[] {
      const trail: string[] = []
      let parent = parentOf(id)
      while (parent) {
        const node = this.findNode(parent)
        trail.unshift(node?.name ?? parent.slice(parent.lastIndexOf('/') + 1))
        parent = parentOf(parent)
      }
      return trail
    },

    /** How many requests and folders sit under `nodes`, at any depth. */
    countNodes(nodes?: Node[]): { requests: number; folders: number } {
      let requests = 0
      let folders = 0
      const walk = (list: Node[]) => {
        for (const node of list) {
          if (node.kind === 'folder') {
            folders++
            walk(node.children)
          } else {
            requests++
          }
        }
      }
      walk(nodes ?? this.tree)
      return { requests, folders }
    },

    /**
     * Requests matching every word of `query`, in name or in path, best first.
     * Both the sidebar filter and the palette use this so they cannot disagree
     * about what "matching" means.
     */
    searchRequests(query: string, limit = 60): { id: string; name: string; method: string }[] {
      const words = query.toLowerCase().split(/\s+/).filter(Boolean)
      if (!words.length) return []

      const found: { id: string; name: string; method: string; rank: number }[] = []
      const walk = (nodes: Node[]) => {
        for (const node of nodes) {
          if (node.kind === 'folder') {
            walk(node.children)
            continue
          }
          const name = node.name.toLowerCase()
          const id = node.id.toLowerCase()
          if (!words.every((word) => name.includes(word) || id.includes(word))) continue
          found.push({
            id: node.id,
            name: node.name,
            method: node.method,
            rank: name.startsWith(words[0]!) ? 0 : name.includes(words[0]!) ? 1 : 2,
          })
        }
      }
      walk(this.tree)

      return found
        .sort((a, b) => a.rank - b.rank || a.name.localeCompare(b.name))
        .slice(0, limit)
        .map(({ id, name, method }) => ({ id, name, method }))
    },

    /** Unlike the collection actions this touches no files, only app settings. */
    async saveOptions(options: ExecOptions) {
      await this.attempt(async () => {
        this.options = normalizeExecOptions(options)
        const store = await loadStore('volt.json', { autoSave: true })
        await store.set(OPTIONS_KEY, this.options)
      })
    },

    /**
     * Forget the cookies collected for this collection. There is no way to
     * empty a jar in reqwest, so Rust throws it away and starts another.
     */
    async clearCookies() {
      await this.attempt(async () => {
        const root = this.requireRoot()
        await invoke('clear_cookies', { root })
        this.notify('Cookies cleared', 'The next request starts with an empty jar.')
      })
    },

    async saveTheme(theme: ThemePreference) {
      await this.attempt(async () => {
        this.theme = theme
        applyTheme(theme)
        const store = await loadStore('volt.json', { autoSave: true })
        await store.set(THEME_KEY, theme)
      })
    },

    async saveAutoCheckUpdates(on: boolean) {
      await this.attempt(async () => {
        this.autoCheckUpdates = on
        const store = await loadStore('volt.json', { autoSave: true })
        await store.set(AUTO_UPDATE_KEY, on)
      })
    },

    /**
     * Ask whether there is a newer release.
     *
     * `loud` is the difference between the user pressing the button and the
     * check that runs on start. A check nobody asked for must not put a banner
     * up because the machine is offline, the endpoint is wrong, or GitHub is
     * having a day — so it fails in silence. The one the user pressed says what
     * happened, including "you are on the latest".
     */
    async checkForUpdate(loud = false): Promise<boolean> {
      try {
        const found = await checkUpdate()
        this.update = found ?? null
        if (found) {
          this.notify(`volt ${found.version} is available`, 'Install it from Settings')
          return true
        }
        if (loud) this.notify('volt is up to date', `You are on ${this.version}`)
        return false
      } catch (error) {
        if (loud) this.error = `Could not check for updates: ${String(error)}`
        return false
      }
    },

    /**
     * Download and install, then restart into it. Everything open is saved to
     * disk already — a collection is files — but an unsaved edit in a tab is
     * not, so the caller confirms first.
     */
    async installUpdate() {
      const update = this.update
      if (!update || this.installing) return
      this.installing = true
      try {
        await this.attempt(async () => {
          await update.downloadAndInstall()
          await relaunch()
        })
      } finally {
        this.installing = false
      }
    },

    /**
     * `previous` is the name the environment was opened under (null for a new
     * one), so a rename moves its files rather than leaving a duplicate.
     * Resolves to whether it saved, so an editor can stay open on failure.
     */
    async saveEnvironment(environment: Environment, previous: string | null): Promise<boolean> {
      const saved = await this.attempt(async () => {
        const root = this.requireRoot()
        await invoke('save_environment', { root, environment, previous })
        await this.reload()
        return true
      })
      return saved === true
    },

    /**
     * Delete an environment, after confirming. Resolves to whether it went,
     * so the editor can keep showing it when the user says no.
     */
    async deleteEnvironment(name: string): Promise<boolean> {
      const gone = await this.attempt(async () => {
        const root = this.requireRoot()
        const confirmed = await ask(
          `Delete “${name}”? Its environment file goes, and with it any secret values saved for it.`,
          { title: 'Delete environment', kind: 'warning', okLabel: 'Delete', cancelLabel: 'Cancel' },
        )
        if (!confirmed) return false

        await invoke('delete_environment', { root, name })
        if (this.activeEnvironment === name) this.activeEnvironment = null
        await this.reload()
        return true
      })
      return gone === true
    },

    /**
     * Put captured values into the active environment. They are written, not
     * held in memory: the point is that the next request — and the next run,
     * tomorrow — can use `{{name}}`, and invisible state is what volt exists
     * to avoid. A capture marked secret lands in `.env.<environment>`.
     */
    async applyCaptures(captured: EnvVar[]) {
      if (!captured.length) return
      const current = this.environment
      const name = current?.name ?? 'local'
      const vars = [...(current?.vars ?? [])]

      for (const value of captured) {
        const at = vars.findIndex((v) => v.name === value.name)
        if (at === -1) vars.push(value)
        else vars[at] = { ...vars[at]!, value: value.value, secret: vars[at]!.secret || value.secret }
      }

      const saved = await this.saveEnvironment({ name, vars }, current?.name ?? null)
      if (saved) {
        this.activeEnvironment = name
        this.notify(
          `Captured ${captured.length === 1 ? captured[0]!.name : `${captured.length} values`}`,
          `Saved in ${name}`,
        )
      }
    },

    // --- Cookies -------------------------------------------------------------

    async loadCookies(): Promise<Cookie[]> {
      const cookies = await this.attempt(async () =>
        invoke<Cookie[]>('list_cookies', { root: this.requireRoot() }),
      )
      return cookies ?? []
    },

    async deleteCookie(cookie: Cookie) {
      await this.attempt(async () => {
        const root = this.requireRoot()
        await invoke('delete_cookie', { root, name: cookie.name, domain: cookie.domain, path: cookie.path })
      })
    },

    // --- Saved examples ------------------------------------------------------

    async loadExamples(id: string): Promise<Example[]> {
      const examples = await this.attempt(async () =>
        invoke<Example[]>('list_examples', { root: this.requireRoot(), id }),
      )
      return examples ?? []
    },

    /** Keep the response on screen beside its request, as a file. */
    async saveExample(name: string): Promise<boolean> {
      const done = await this.attempt(async () => {
        const root = this.requireRoot()
        if (!this.activeId || !this.response) throw new Error('Send a request first, then keep its response.')
        await invoke<Example[]>('save_example', {
          root,
          id: this.activeId,
          name,
          response: this.response,
          envVars: this.environment?.vars ?? [],
        })
        this.notify('Example saved', `Kept as “${name}” beside the request`)
        return true
      })
      return done === true
    },

    async deleteExample(id: string, name: string): Promise<Example[]> {
      const left = await this.attempt(async () =>
        invoke<Example[]>('delete_example', { root: this.requireRoot(), id, name }),
      )
      return left ?? []
    },

    /**
     * How the response on screen differs from a kept example. Structure and
     * status, not lines: the question an example answers is whether the shape
     * is still the one that was reviewed.
     */
    async compareExample(id: string, name: string): Promise<Comparison | null> {
      if (!this.response) return null
      const out = await this.attempt(async () =>
        invoke<Comparison>('compare_example', {
          root: this.requireRoot(),
          id,
          name,
          response: this.response,
        }),
      )
      return out ?? null
    },


    // --- GraphQL --------------------------------------------------------------

    /**
     * Ask the endpoint what it can do. The answer is held in Rust for as long
     * as the app runs and never written to the collection: a schema belongs to
     * the server, and a stale copy committed next to the requests is a lie
     * waiting to be believed.
     */
    async introspect(): Promise<GraphQlSchema | null> {
      const schema = await this.attempt(async () => {
        const root = this.requireRoot()
        if (!this.request) throw new Error('Open a request first.')
        return await invoke<GraphQlSchema>('graphql_schema', {
          root,
          request: this.request,
          envVars: this.environment?.vars ?? [],
          options: this.options,
          requestId: this.activeId,
        })
      })
      return schema ?? null
    },

    /** Fields the query asks for that the schema does not have. */
    async graphqlProblems(query: string): Promise<GraphQlProblem[]> {
      try {
        return await invoke<GraphQlProblem[]>('graphql_check', { requestId: this.activeId, query })
      } catch {
        // Marking is a convenience; it never becomes an error strip.
        return []
      }
    },

    /** The fields that belong where the caret is. */
    async graphqlCompletions(query: string, at: number): Promise<GraphQlCompletion[]> {
      try {
        return await invoke<GraphQlCompletion[]>('graphql_complete', {
          requestId: this.activeId,
          query,
          at,
        })
      } catch {
        return []
      }
    },

    // --- Code, export, scopes -------------------------------------------------

    async generateCode(language: string, includeSecrets: boolean): Promise<Generated | null> {
      const out = await this.attempt(async () => {
        const root = this.requireRoot()
        if (!this.request) throw new Error('Open a request first.')
        return await invoke<Generated>('generate_code', {
          root,
          request: this.request,
          envVars: this.environment?.vars ?? [],
          options: this.options,
          requestId: this.activeId,
          language,
          includeSecrets,
        })
      })
      return out ?? null
    },

    /** Write the collection out as a Postman v2.1 file the user picks. */
    async exportPostman() {
      await this.attempt(async () => {
        const root = this.requireRoot()
        const out = await invoke<Exported>('export_postman', { root })
        const path = await saveDialog({
          defaultPath: `${this.collection?.meta.name ?? 'collection'}.postman_collection.json`,
          title: 'Export as Postman collection',
        })
        if (!path) return
        await invoke('save_body', { path, body: out.json, base64Encoded: false })
        this.notify('Exported for Postman', out.notes[0] ?? path)
        if (out.notes.length) this.exportNotes = out.notes
      })
    },

    async loadFolder(id: string): Promise<FolderMeta | null> {
      const folder = await this.attempt(async () =>
        invoke<FolderMeta>('get_folder', { root: this.requireRoot(), id }),
      )
      return folder ?? null
    },

    async saveFolder(id: string, folder: FolderMeta): Promise<boolean> {
      const done = await this.attempt(async () => {
        const root = this.requireRoot()
        await invoke('save_folder', { root, id, folder })
        await this.reload()
        return true
      })
      return done === true
    },

    /** The collection's own headers, auth and variables. */
    async saveCollectionMeta(meta: CollectionMeta): Promise<boolean> {
      const done = await this.attempt(async () => {
        const root = this.requireRoot()
        this.collection = await invoke<Collection>('save_collection_meta', { root, meta })
        return true
      })
      return done === true
    },

    // --- Workspaces ----------------------------------------------------------

    /**
     * A workspace is a named set of collection folders — the three APIs at
     * work, or the side project. It is a list of paths in this app's settings,
     * not a thing in the cloud: there is no account here to hang one on.
     */
    async saveWorkspaces() {
      const store = await loadStore('volt.json', { autoSave: true })
      await store.set(WORKSPACES_KEY, this.workspaces)
      await store.set(WORKSPACE_KEY, this.workspace)
    },

    async addWorkspace(name: string) {
      await this.attempt(async () => {
        const trimmed = name.trim()
        if (!trimmed) throw new Error('A workspace needs a name.')
        if (this.workspaces.some((w) => w.name.toLowerCase() === trimmed.toLowerCase())) {
          throw new Error(`There is already a workspace called “${trimmed}”.`)
        }
        // Whatever is open is what you are working on, so it starts there.
        this.workspaces = [...this.workspaces, { name: trimmed, collections: this.root ? [this.root] : [] }]
        this.workspace = trimmed
        await this.saveWorkspaces()
      })
    },

    async removeWorkspace(name: string) {
      await this.attempt(async () => {
        const confirmed = await ask(
          `Remove the workspace “${name}”? The collections in it are left exactly where they are.`,
          { title: 'Remove workspace', kind: 'warning', okLabel: 'Remove', cancelLabel: 'Cancel' },
        )
        if (!confirmed) return
        this.workspaces = this.workspaces.filter((w) => w.name !== name)
        if (this.workspace === name) this.workspace = null
        await this.saveWorkspaces()
      })
    },

    /** Put the open collection in a workspace, or take it out again. */
    async setInWorkspace(name: string, include: boolean) {
      await this.attempt(async () => {
        const root = this.requireRoot()
        this.workspaces = this.workspaces.map((workspace) =>
          workspace.name === name
            ? {
                ...workspace,
                collections: include
                  ? [...new Set([...workspace.collections, root])]
                  : workspace.collections.filter((path) => path !== root),
              }
            : workspace,
        )
        await this.saveWorkspaces()
      })
    },

    async useWorkspace(name: string | null) {
      await this.attempt(async () => {
        this.workspace = name
        await this.saveWorkspaces()
        const first = this.workspaces.find((w) => w.name === name)?.collections[0]
        if (first && first !== this.root) await this.open(first)
      })
    },


    // --- Monitors ------------------------------------------------------------

    async saveMonitors() {
      const store = await loadStore('volt.json', { autoSave: true })
      await store.set(MONITORS_KEY, this.monitors)
    },

    /** Watch a request on a schedule, for as long as volt is open. */
    async addMonitor(requestId: string, name: string, everySeconds: number) {
      await this.attempt(async () => {
        const root = this.requireRoot()
        const monitor: Monitor = {
          id: `${Date.now()}`,
          root,
          requestId,
          name,
          everySeconds: Math.max(10, Math.round(everySeconds)),
          paused: false,
        }
        this.monitors = [...this.monitors, monitor]
        await this.saveMonitors()
        scheduleMonitor(this, monitor)
      })
    },

    async removeMonitor(id: string) {
      await this.attempt(async () => {
        clearMonitor(id)
        this.monitors = this.monitors.filter((m) => m.id !== id)
        await this.saveMonitors()
      })
    },

    async setMonitorPaused(id: string, paused: boolean) {
      await this.attempt(async () => {
        this.monitors = this.monitors.map((m) => (m.id === id ? { ...m, paused } : m))
        await this.saveMonitors()
        const monitor = this.monitors.find((m) => m.id === id)
        if (!monitor) return
        if (paused) clearMonitor(id)
        else scheduleMonitor(this, monitor)
      })
    },

    /** Start the ones belonging to the open collection; stop everything else. */
    restartMonitors() {
      clearAllMonitors()
      for (const monitor of this.monitors) {
        if (monitor.root === this.root && !monitor.paused) scheduleMonitor(this, monitor)
      }
    },

    /**
     * Send a request by id without disturbing what is on screen. History
     * records it like any other send, which is where a monitor's evidence
     * lives — there is no separate log to go stale.
     */
    async runMonitor(monitor: Monitor) {
      try {
        const raw = await invoke<ApiRequest>('get_request', { root: monitor.root, id: monitor.requestId })
        const outcome = await invoke<SendOutcome>('send_request', {
          root: monitor.root,
          request: normalizeRequest(raw),
          envVars: this.environment?.vars ?? [],
          options: this.options,
          requestId: monitor.requestId,
          environment: this.environment?.name ?? null,
        })
        this.noteRun(monitor, outcome.response.status, outcome.response.durationMs, null)
        if (outcome.history) this.history = [outcome.history, ...this.history].slice(0, HISTORY_LIMIT)
      } catch (error) {
        this.noteRun(monitor, null, null, String((error as Error)?.message ?? error))
      }
    },

    noteRun(monitor: Monitor, status: number | null, durationMs: number | null, error: string | null) {
      const run: MonitorRun = { at: Date.now(), monitorId: monitor.id, name: monitor.name, status, durationMs, error }
      this.monitorRuns = [run, ...this.monitorRuns].slice(0, 50)

      // Only failures interrupt: a monitor that says "fine" every minute is a
      // monitor people turn off.
      const failed = error !== null || (status !== null && status >= 400)
      if (failed) this.notify(`${monitor.name} failed`, error ?? `Answered ${status}`, 'bad')
    },


    // --- Mock, docs, sync ----------------------------------------------------

    async startMock(port: number): Promise<MockRunning | null> {
      const running = await this.attempt(async () =>
        invoke<MockRunning>('start_mock', { root: this.requireRoot(), port }),
      )
      this.mock = running ?? null
      if (running) this.notify('Mock is serving', `http://127.0.0.1:${running.port}`)
      return this.mock
    },

    async stopMock() {
      await invoke('stop_mock')
      this.mock = null
    },

    async refreshMock() {
      this.mock = (await this.attempt(async () => invoke<MockRunning | null>('mock_status'))) ?? null
    },

    /** Write the collection's documentation as one HTML file the user picks. */
    async exportDocs() {
      await this.attempt(async () => {
        const root = this.requireRoot()
        const html = await invoke<string>('export_docs', { root })
        const path = await saveDialog({
          defaultPath: `${this.collection?.meta.name ?? 'collection'}.html`,
          title: 'Write the documentation',
        })
        if (!path) return
        await invoke('save_body', { path, body: html, base64Encoded: false })
        this.notify('Documentation written', path)
      })
    },

    async syncStatus(): Promise<SyncStatus | null> {
      const status = await this.attempt(async () =>
        invoke<SyncStatus>('sync_status', { root: this.requireRoot() }),
      )
      return status ?? null
    },

    async syncPull(): Promise<boolean> {
      const done = await this.attempt(async () => {
        const said = await invoke<string>('sync_pull', { root: this.requireRoot() })
        await this.reload()
        this.notify('Pulled', said.split('\n')[0] ?? '')
        return true
      })
      return done === true
    },

    async syncCommit(message: string, push: boolean): Promise<boolean> {
      const done = await this.attempt(async () => {
        const said = await invoke<string>('sync_commit', { root: this.requireRoot(), message, push })
        this.notify(push ? 'Committed and pushed' : 'Committed', said.split('\n')[0] ?? '')
        return true
      })
      return done === true
    },

    /** Write the `.env.<environment>.example` files a teammate fills in. */
    async writeEnvTemplates(): Promise<string[]> {
      const written = await this.attempt(async () =>
        invoke<string[]>('write_env_templates', { root: this.requireRoot() }),
      )
      if (written?.length) this.notify('Templates written', written.join(', '))
      else if (written) this.notify('Nothing to template', 'No environment has a secret in it yet')
      return written ?? []
    },


    // --- Streams, gRPC and runs -----------------------------------------------

    /** Ctrl+Enter means the same thing whatever kind of request is open. */
    async go() {
      const kind = this.request?.kind ?? 'http'
      const streaming = kind === 'websocket' || kind === 'sse' || (kind === 'grpc' && this.grpcStreams)

      if (streaming) {
        // Only this tab's own connection is ended by this tab's key. Pressing
        // Connect on a second socket used to disconnect the first one.
        if (this.myStream) return this.closeStream()
        if (this.stream) {
          // Through `attempt`, like every other action: a bare throw here is an
          // unhandled rejection, which reaches the strip only by accident.
          return this.attempt(async () => {
            throw new Error('Another request is connected. Disconnect it first.')
          })
        }
        return kind === 'grpc' ? this.openGrpcStream() : this.openStream(kind as 'websocket' | 'sse')
      }
      if (kind === 'grpc') return this.sendGrpc()
      return this.send()
    },

    async openStream(kind: 'websocket' | 'sse') {
      await this.attempt(async () => {
        const root = this.requireRoot()
        if (!this.request) throw new Error('Open a request first.')
        this.streamEvents = []
        const open = await invoke<StreamOpen>('open_stream', {
          root,
          request: this.request,
          envVars: this.environment?.vars ?? [],
          options: this.options,
          requestId: this.activeId,
          kind,
        })
        this.stream = open
        this.streamOwner = this.activeId
      })
    },

    async closeStream() {
      const open = this.stream
      if (!open) return
      await invoke('close_stream', { id: open.id })
      this.stream = null
    },

    async sendStreamText(text: string) {
      await this.attempt(async () => {
        const open = this.stream
        if (!open) throw new Error('Nothing is connected.')
        await invoke('send_stream', { id: open.id, text })
      })
    },

    /** Events arrive on one channel; this keeps the open one's. */
    noteStreamEvent(event: StreamEvent) {
      if (!this.stream || event.id !== this.stream.id) return
      this.streamEvents = [...this.streamEvents, event].slice(-500)
      // `streamOwner` is deliberately kept: the transcript of a connection that
      // has ended still belongs to the request that opened it, and clearing it
      // here made the events vanish the moment the server hung up.
      if (event.kind === 'closed') this.stream = null
    },

    async loadGrpcServices(proto: string): Promise<GrpcService[]> {
      const services = await this.attempt(async () =>
        invoke<GrpcService[]>('grpc_services', { root: this.requireRoot(), proto }),
      )
      this.grpcMethods = (services ?? []).flatMap((service) => service.methods)
      return services ?? []
    },

    /**
     * Ask the server what it serves, for an endpoint that offers reflection.
     * Nothing is written down: a service description belongs to the server, and
     * a stale copy committed next to the requests is a lie waiting to be
     * believed.
     */
    async reflectGrpc(): Promise<GrpcService[]> {
      const services = await this.attempt(async () => {
        const root = this.requireRoot()
        if (!this.request) throw new Error('Open a request first.')
        return await invoke<GrpcService[]>('grpc_reflect', {
          root,
          request: this.request,
          envVars: this.environment?.vars ?? [],
          options: this.options,
          requestId: this.activeId,
        })
      })
      this.grpcMethods = (services ?? []).flatMap((service) => service.methods)
      return services ?? []
    },

    /** Client, server or bidirectional streaming — the same pane a socket uses. */
    async openGrpcStream() {
      await this.attempt(async () => {
        const root = this.requireRoot()
        if (!this.request) throw new Error('Open a request first.')
        this.streamEvents = []
        this.stream = await invoke<StreamOpen>('open_grpc_stream', {
          root,
          request: this.request,
          envVars: this.environment?.vars ?? [],
          options: this.options,
          requestId: this.activeId,
        })
        this.streamOwner = this.activeId
      })
    },

    async sendGrpc() {
      this.sending = true
      try {
        const reply = await this.attempt(async () => {
          const root = this.requireRoot()
          if (!this.request) throw new Error('Open a request first.')
          return await invoke<GrpcReply>('send_grpc', {
            root,
            request: this.request,
            envVars: this.environment?.vars ?? [],
            options: this.options,
            requestId: this.activeId,
          })
        })
        this.grpcReply = reply ?? null
      } finally {
        this.sending = false
      }
    },

    /** Run a folder, or the whole collection when `target` is null. */
    async runCollection(target: string | null, stopOnFailure: boolean): Promise<Run | null> {
      this.running = true
      try {
        const run = await this.attempt(async () =>
          invoke<Run>('run_collection', {
            root: this.requireRoot(),
            target,
            envVars: this.environment?.vars ?? [],
            options: this.options,
            stopOnFailure,
          }),
        )
        this.lastRun = run ?? null
        return this.lastRun
      } finally {
        this.running = false
      }
    },

    /** Get an OAuth token and keep it in the environment, as a secret. */
    async getOAuthToken(config: OAuthConfig, grant: string, name: string): Promise<OAuthToken | null> {
      const token = await this.attempt(async () =>
        invoke<OAuthToken>('oauth_token', { config, grant, refreshToken: this.oauthRefresh }),
      )
      if (!token) return null

      this.oauthRefresh = token.refreshToken
      await this.applyCaptures([{ name: name.trim() || 'token', value: token.accessToken, secret: true }])
      return token
    },


    async send() {
      if (!this.request) return
      // Who this send belongs to, decided before it leaves. A send is slow and
      // the user is not: by the time it lands the collection may have been
      // switched, the tab closed, or another tab made active, and a response
      // written into whichever tab happens to be in front is a response
      // attributed to the wrong request — with its captured secrets written
      // into the wrong environment.
      this.snapshot()
      const root = this.requireRoot()
      const owner = this.tabs[this.activeTab] ?? null

      this.sending = true
      try {
        const result = await this.attempt(async () => {
          return await invoke<SendOutcome>('send_request', {
            root,
            request: this.request,
            envVars: this.environment?.vars ?? [],
            options: this.options,
            requestId: this.activeId,
            environment: this.environment?.name ?? null,
          })
        })

        // A different collection is open now; this answer is about a tab that
        // no longer exists.
        if (this.root !== root) return

        const stillActive = owner !== null && this.tabs[this.activeTab] === owner
        if (stillActive) {
          this.response = result?.response ?? null
          this.captureNotes = result?.captureNotes ?? []
          this.checkResults = result?.checks ?? []
        } else if (owner !== null && this.tabs.includes(owner)) {
          // The tab is still open, just not in front. Put the reading where it
          // belongs rather than on top of whatever the user moved to.
          owner.response = result?.response ?? null
          owner.captureNotes = result?.captureNotes ?? []
        }
        // Otherwise the tab was closed while it was in flight; there is nowhere
        // for the reading to go, and the history entry below still records it.

        if (result?.captured?.length) await this.applyCaptures(result.captured)

        if (result?.history) {
          this.history = [result.history, ...this.history].slice(0, HISTORY_LIMIT)
        } else if (!result) {
          // A failed send is recorded too, but the error came back instead of a
          // summary, so fetch the list to pick it up.
          await this.refreshHistory()
        }
      } finally {
        this.sending = false
      }
    },

    /**
     * Never throws and never touches `error`: it runs right after a failed send,
     * and clearing the banner there would hide the reason the send failed.
     */
    async refreshHistory() {
      if (!this.root) return
      try {
        this.history = await invoke<HistorySummary[]>('list_history', { root: this.root })
      } catch {
        // History is a convenience; an unreadable file must not break the app.
      }
    },

    /** Reopen a past request, with the response it got, detached from any file. */
    async openHistory(id: string) {
      await this.attempt(async () => {
        const root = this.requireRoot()
        const entry = await invoke<HistoryEntry>('get_history_entry', { root, id })
        // A replay has no file, so it gets a tab of its own rather than taking
        // over whichever request happened to be open.
        this.snapshot()
        this.tabs.push(blankTab(null))
        this.activeTab = this.tabs.length - 1
        this.activeId = null
        this.request = normalizeRequest(entry.request)
        this.response = entry.response
        this.dirty = false
        this.curlNotice = null
        this.historyEntry = this.history.find((h) => h.id === id) ?? {
          id: entry.id,
          at: entry.at,
          requestId: entry.requestId,
          environment: entry.environment,
          name: entry.request.name,
          method: entry.request.method,
          url: entry.request.url,
          status: entry.response?.status ?? null,
          durationMs: entry.response?.durationMs ?? null,
          error: entry.error,
        }
        // A failed send has no response to show, so show why instead.
        if (entry.error) this.error = `This request failed when it was sent: ${entry.error}`
      })
    },

    /** Write the request currently open from history into the collection. */
    async saveHistoryAsNew() {
      await this.attempt(async () => {
        const root = this.requireRoot()
        if (!this.request || !this.historyEntry) throw new Error('Nothing from history is open.')

        const id = this.freeId(null, fileBase(this.request.name, 'from-history'))
        const request = this.request
        await invoke('save_request', { root, id, request })
        await this.reload()
        this.activeId = id
        this.historyEntry = null
        this.dirty = false
        // The replay tab now owns the new id. Without this its snapshot still
        // says `null`, so opening the request from the tree makes a second tab
        // for the same file and the stale one overwrites it on its next save.
        this.snapshot()
      })
    },

    async clearHistory() {
      await this.attempt(async () => {
        const root = this.requireRoot()
        const confirmed = await ask('Clear the request history for this collection? This cannot be undone.', {
          title: 'Clear history',
          kind: 'warning',
          okLabel: 'Clear',
          cancelLabel: 'Cancel',
        })
        if (!confirmed) return
        await invoke('clear_history', { root })
        this.history = []
      })
    },
  },
})
