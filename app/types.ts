// Mirrors src-tauri/src/model.rs. Keep the two in sync.

export interface KeyValue {
  name: string
  value: string
  enabled: boolean
  description?: string | null
}

export type RequestKind = 'http' | 'websocket' | 'sse' | 'grpc'

export type Body =
  | { type: 'none' }
  | { type: 'text'; content: string }
  | { type: 'json'; content: string }
  | { type: 'xml'; content: string }
  | { type: 'form'; fields: FormField[] }
  | { type: 'urlencoded'; fields: KeyValue[] }
  | { type: 'binary'; path: string }
  | { type: 'graphql'; query: string; variables: string }
  | { type: 'grpc'; proto: string; method: string; message: string }

export type Auth =
  | { type: 'none' }
  | { type: 'digest'; username: string; password: string }
  | { type: 'ntlm'; username: string; password: string; domain: string }
  | { type: 'awssigv4'; key_id: string; secret: string; region: string; service: string; session_token: string }
  | { type: 'inherit' }
  | { type: 'bearer'; token: string }
  | { type: 'basic'; username: string; password: string }
  | { type: 'apikey'; key: string; value: string; location: 'header' | 'query' }

export interface ApiRequest {
  name: string
  /** Absent means . */
  kind?: RequestKind
  seq: number
  method: string
  url: string
  headers: KeyValue[]
  params: KeyValue[]
  body: Body
  auth: Auth
  captures?: Capture[]
  checks?: Check[]
  options?: RequestOptions | null
  docs?: string | null
}

/** Mirrors RequestOptions in src-tauri/src/model.rs — file format, so
  * snake_case, unlike the camelCase ExecOptions that only crosses IPC. */
export interface RequestOptions {
  timeout_ms?: number | null
  follow_redirects?: boolean | null
  verify_tls?: boolean | null
  /** An empty string means "go direct", overriding a proxy in Settings. */
  proxy?: string | null
  /** A PEM holding a client certificate and its key. */
  client_cert?: string | null
}

export interface EnvVar {
  name: string
  value: string
  secret: boolean
}

export interface Environment {
  name: string
  vars: EnvVar[]
}

export interface CollectionMeta {
  name: string
  version: number
  headers: KeyValue[]
  auth: Auth
  /** Defaults that travel with the collection; an environment overrides them. */
  vars?: KeyValue[]
}

/** Mirrors FolderMeta in src-tauri/src/model.rs — one `folder.yaml`. */
export interface FolderMeta {
  name?: string | null
  seq: number
  headers: KeyValue[]
  auth: Auth
  vars: KeyValue[]
}

export type Node =
  | { kind: 'folder'; id: string; name: string; seq: number; children: Node[] }
  | { kind: 'request'; id: string; name: string; seq: number; method: string }

export interface Collection {
  root: string
  meta: CollectionMeta
  tree: Node[]
  environments: Environment[]
  /** Files under the root that did not parse, left out of the tree and named. */
  problems?: string[]
}

/** Mirrors ExecOptions in src-tauri/src/http.rs. */
export interface ExecOptions {
  timeoutMs: number
  followRedirects: boolean
  verifyTls: boolean
  proxy?: string | null
  sendCookies: boolean
  clientCert?: string | null
}

export function defaultExecOptions(): ExecOptions {
  return { timeoutMs: 30_000, followRedirects: true, verifyTls: true, proxy: null, sendCookies: true, clientCert: null }
}

/** A value read back from disk may predate a field, so fill the gaps. */
export function normalizeExecOptions(raw: Partial<ExecOptions> | null | undefined): ExecOptions {
  const fallback = defaultExecOptions()
  if (!raw) return fallback

  // A zero or negative timeout would make every request fail instantly.
  const timeout = Number(raw.timeoutMs)
  return {
    timeoutMs: Number.isFinite(timeout) && timeout > 0 ? Math.round(timeout) : fallback.timeoutMs,
    followRedirects: raw.followRedirects ?? fallback.followRedirects,
    verifyTls: raw.verifyTls ?? fallback.verifyTls,
    proxy: raw.proxy?.trim() || null,
    sendCookies: raw.sendCookies ?? fallback.sendCookies,
    clientCert: raw.clientCert?.trim() || null,
  }
}

export interface HttpResponse {
  status: number
  statusText: string
  headers: KeyValue[]
  body: string
  bodyIsBase64: boolean
  sizeBytes: number
  durationMs: number
  timeToFirstByteMs: number
  finalUrl: string
  sentUrl: string
  /** For the timeline. Older history entries do not have these. */
  sentMethod?: string
  sentHeaders?: KeyValue[]
  sentBodyBytes?: number
  redirects?: string[]
  version?: string
  missingVars: string[]
}

/** Mirrors curl::Parsed in src-tauri/src/curl.rs. */
export interface CurlParsed {
  request: ApiRequest
  /** Secret variables for the credentials the command held in plain text. */
  newSecrets: EnvVar[]
  notes: string[]
}

/** Mirrors curl::Rendered in src-tauri/src/curl.rs. */
export interface CurlRendered {
  command: string
  /** Secret variables left as `{{name}}`. */
  hidden: string[]
  /** Variables with no value, also left as `{{name}}`. */
  undefined: string[]
}

/** Mirrors import::Report in src-tauri/src/import.rs. */
export interface ImportReport {
  format: string
  /** Where the new collection was created. */
  path: string
  requests: number
  folders: number
  environments: string[]
  /** Variables whose values went to `.env.<environment>` files instead of the YAML. */
  secrets: string[]
  warnings: string[]
}

/** Mirrors import::Outcome in src-tauri/src/import.rs. */
export interface ImportOutcome {
  collection: Collection
  report: ImportReport
}

/** Mirrors SendOutcome in src-tauri/src/lib.rs. */
export interface SendOutcome {
  response: HttpResponse
  /** Null when history could not be written; the send itself still worked. */
  history: HistorySummary | null
  /** Values the request's captures took out of the response. */
  captured: EnvVar[]
  /** A line for every capture that found nothing. */
  captureNotes: string[]
  /** How the request's own checks came out. */
  checks: CheckOutcome[]
}

/** Mirrors history::Summary in src-tauri/src/history.rs. */
export interface HistorySummary {
  id: string
  /** Unix milliseconds. */
  at: number
  requestId: string | null
  environment: string | null
  name: string
  method: string
  url: string
  status: number | null
  durationMs: number | null
  error: string | null
}

/** Mirrors history::Entry in src-tauri/src/history.rs. */
export interface HistoryEntry {
  id: string
  at: number
  requestId: string | null
  environment: string | null
  request: ApiRequest
  response: HttpResponse | null
  bodyTruncated: boolean
  error: string | null
}

export const METHODS =['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS'] as const

export function emptyRequest(name = 'New request'): ApiRequest {
  return {
    name,
    seq: 1,
    method: 'GET',
    url: '',
    headers: [],
    params: [],
    body: { type: 'none' },
    auth: { type: 'inherit' },
    docs: null,
  }
}

/** Rust omits empty collections from its JSON; fill them back in. */
export function normalizeRequest(raw: Partial<ApiRequest>): ApiRequest {
  return {
    ...emptyRequest(),
    ...raw,
    headers: raw.headers ?? [],
    params: raw.params ?? [],
    body: raw.body ?? { type: 'none' },
    auth: raw.auth ?? { type: 'inherit' },
  }
}

/** Mirrors FormField in src-tauri/src/model.rs. */
export interface FormField extends KeyValue {
  /** The value is a path to upload rather than the text to send. */
  file?: boolean
}

/** Mirrors Capture in src-tauri/src/model.rs. */
export interface Capture {
  name: string
  /** `status`, `header:Name`, `body`, or `$.a.b[0]`. */
  from: string
  enabled: boolean
  secret?: boolean
}

/** Mirrors cookies::Cookie in src-tauri/src/cookies.rs. */
export interface Cookie {
  name: string
  value: string
  domain: string
  path: string
  secure: boolean
  httpOnly: boolean
  hostOnly: boolean
  expires: number | null
}

/** Mirrors collection::SearchHit in src-tauri/src/collection.rs. */
export interface SearchHit {
  id: string
  name: string
  method: string
  /** "URL", "param limit", "header X-Api-Key", "body", "docs". */
  foundIn: string
}

/** Mirrors examples::Example in src-tauri/src/examples.rs. */
export interface Example {
  name: string
  at: number
  status: number
  statusText: string
  headers: KeyValue[]
  body: string
  bodyIsBase64: boolean
}

/** Mirrors examples::Comparison in src-tauri/src/examples.rs. */
export interface Comparison {
  example: string
  /** `[kept, now]` when the status is not the one that was kept. */
  status: [number, number] | null
  contentType: [string, string] | null
  /** Paths into the body, as `$.data.items[].id`. */
  added: string[]
  removed: string[]
  retyped: { path: string, was: string, now: string }[]
  same: boolean
  note: string | null
}

/** Mirrors graphql::Schema and its parts in src-tauri/src/graphql.rs. */
export interface GraphQlSchema {
  queryType: string
  mutationType: string
  subscriptionType: string
  types: GraphQlType[]
}

export interface GraphQlType {
  name: string
  description: string
  fields: GraphQlField[]
}

export interface GraphQlField {
  name: string
  /** As the schema writes it: `[User!]!`. */
  typeName: string
  /** The named type under the wrappers. */
  of: string
  description: string
  args: string[]
}

/** A field the query asks for that the schema does not have. */
export interface GraphQlProblem {
  at: number
  len: number
  message: string
}

export interface GraphQlCompletion {
  name: string
  typeName: string
  description: string
  args: string[]
  /** What is already typed, which the editor replaces. */
  prefix: string
}

/** Mirrors codegen::Generated in src-tauri/src/codegen.rs. */
export interface Generated {
  code: string
  syntax: string
  hidden: string[]
  undefined: string[]
}

/** Mirrors export::Exported in src-tauri/src/export.rs. */
export interface Exported {
  json: string
  notes: string[]
}

/** Mirrors mock::Route and mock::Running in src-tauri/src/mock.rs. */
export interface MockRoute {
  id: string
  name: string
  method: string
  path: string
  example: string
  status: number
  /** What a call has to carry as well as the path, as `name=value`. */
  conditions: string[]
}

export interface MockRunning {
  port: number
  routes: MockRoute[]
}

/** Mirrors git::Status in src-tauri/src/git.rs. */
export interface SyncStatus {
  repository: boolean
  why: string | null
  branch: string
  remote: string | null
  ahead: number
  behind: number
  changed: string[]
  secretsIgnored: boolean
}

/** Mirrors Check in src-tauri/src/model.rs. */
export interface Check {
  from: string
  op: 'is' | 'isnot' | 'contains' | 'exists' | 'missing' | 'under' | 'over'
  value?: string
  enabled: boolean
}

/** Mirrors checks::Outcome in src-tauri/src/checks.rs. */
export interface CheckOutcome {
  ok: boolean
  from: string
  op: string
  expected: string
  actual: string | null
  note: string | null
}

/** Mirrors runner::Step and runner::Run in src-tauri/src/runner.rs. */
export interface RunStep {
  id: string
  name: string
  method: string
  status: number | null
  durationMs: number | null
  error: string | null
  checks: CheckOutcome[]
  captured: string[]
  ok: boolean
}

export interface Run {
  started: number
  durationMs: number
  steps: RunStep[]
  passed: number
  failed: number
}

/** Mirrors stream::Event in src-tauri/src/stream.rs. */
export interface StreamEvent {
  id: string
  kind: 'open' | 'message' | 'sent' | 'error' | 'closed'
  at: number
  data?: string
  name?: string
}

export interface StreamOpen {
  id: string
  /** A streaming gRPC call is the same kind of thing as a socket. */
  kind: 'websocket' | 'sse' | 'grpc'
  url: string
}

/** Mirrors oauth::Config and oauth::Token in src-tauri/src/oauth.rs. */
export interface OAuthConfig {
  tokenUrl: string
  authUrl: string
  clientId: string
  clientSecret: string
  scope: string
  audience: string
  basicAuth: boolean
}

export interface OAuthToken {
  accessToken: string
  tokenType: string
  expiresAt: number | null
  refreshToken: string | null
  scope: string | null
}

/** Mirrors grpc::Service, grpc::Method and grpc::Reply in src-tauri/src/grpc.rs. */
export interface GrpcMethod {
  name: string
  fullName: string
  input: string
  output: string
  clientStreaming: boolean
  serverStreaming: boolean
  example: string
}

export interface GrpcService {
  name: string
  methods: GrpcMethod[]
}

export interface GrpcReply {
  status: number
  statusText: string
  message: string | null
  body: string
  durationMs: number
  headers: KeyValue[]
  /** What came after the body — where a gRPC call says whether it worked. */
  trailers: KeyValue[]
}

/** Mirrors collection::Deleted in src-tauri/src/collection.rs. */
export interface Deleted {
  id: string
  name: string
  folder: boolean
  /** Where it is now, under `.trash/`. */
  at: string
}
