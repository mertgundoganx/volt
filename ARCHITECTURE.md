# Architecture

volt is a desktop API client whose collections are plain YAML files in the
user's own repository. There is no storage layer of its own and no cloud: a
collection *is* a directory. Tauri 2 (Rust) owns the filesystem and the network;
Nuxt 4 runs as a pure SPA inside the webview and owns nothing but the UI.

This file is the "why" behind the code. For how to build and test it, see
[CONTRIBUTING.md](CONTRIBUTING.md); for what volt does, see the
[README](README.md).

## Layout

| Path | What lives there |
| --- | --- |
| `src-tauri/src/model.rs` | The on-disk file format — every struct maps 1:1 to YAML |
| `src-tauri/src/collection.rs` | Reading/writing a collection directory, secrets, path safety |
| `src-tauri/src/http.rs` | `plan` (resolve a request) and `execute` (send it with reqwest) |
| `src-tauri/src/curl.rs` | Pasted curl → request, and request → curl command |
| `src-tauri/src/vars.rs` | `{{variable}}` interpolation |
| `src-tauri/src/capture.rs` | Taking a value out of a response into a variable |
| `src-tauri/src/checks.rs` | Assertions on a response |
| `src-tauri/src/runner.rs` | Running a folder or a collection, in order |
| `src-tauri/src/openapi.rs` | OpenAPI and Swagger import |
| `src-tauri/src/aws.rs` | AWS SigV4 signing |
| `src-tauri/src/digest.rs` | HTTP Digest authentication |
| `src-tauri/src/ntlm.rs` | NTLMv2 |
| `src-tauri/src/oauth.rs` | OAuth 2.0 token grants |
| `src-tauri/src/stream.rs` | WebSocket and server-sent events |
| `src-tauri/src/grpc.rs` | gRPC, from a `.proto` at runtime: unary and streaming |
| `src-tauri/src/wire.rs` | HTTP/2 itself — frames, flow control, trailers, and the one TLS config |
| `src-tauri/src/reflection.rs` | Asking a server what it serves, when there is no `.proto` |
| `src-tauri/src/graphql.rs` | A fetched schema, and what a query asks it for |
| `src-tauri/src/scripts.rs` | Reading captures and checks out of an imported script |
| `src-tauri/src/bin/volt-run.rs` | The runner as a command line tool |
| `src-tauri/src/codegen.rs` | Printing a request as code, in seven languages |
| `src-tauri/src/cookies.rs` | The cookie jar: an RFC 6265 subset we can read back |
| `src-tauri/src/examples.rs` | Saved example responses, under `.examples/` |
| `src-tauri/src/export.rs` | Writing a collection out as Postman v2.1 |
| `src-tauri/src/docs.rs` | The collection as one self-contained HTML file |
| `src-tauri/src/git.rs` | Sync and sharing through the collection's repository |
| `src-tauri/src/mock.rs` | Serving the saved examples on loopback |
| `src-tauri/src/secrets.rs` | `.env.<environment>` files: dotenv parsing, legacy `.env`, `.gitignore` coverage |
| `src-tauri/src/history.rs` | Request history — JSON Lines in the app data dir, never the collection |
| `src-tauri/src/import.rs` | Postman v2.0/v2.1 and Insomnia v4/v5 import |
| `src-tauri/src/error.rs` | One error enum, serialized to a string across IPC |
| `src-tauri/src/lib.rs` | The `#[tauri::command]` surface the UI can call |
| `app/types.ts` | TypeScript mirror of `model.rs` and every IPC payload |
| `app/stores/collection.ts` | Pinia store — all app state and every `invoke` |
| `app/composables/useTreeMenu.ts` | Shared tree UI state: context menu, inline rename, new folder, drag |
| `app/composables/useUi.ts` | `modKey` (⌘/Ctrl), persisted pane sizes, `humanSize` |
| `app/composables/useShortcuts.ts` | The keyboard registry every binding goes through |
| `app/assets/tokens.css` | Every colour, radius, space, type size — the design system's source of truth |
| `app/assets/base.css` | Element resets and the shared classes (`.silk`, `.btn`, `.field`, `.led`, …) |
| `app/utils/icons.ts` | The hand-drawn icon set |
| `app/utils/syntax.ts` | JSON pretty-printing and highlighting (tokens, never HTML) |
| `app/components/ui/` | Primitives: Icon, Tabs, Segmented, Dialog, MenuButton, Splitter, VarInput, CodeView, CodeEditor, Select, Measure |
| `app/components/` | Screens: Rail, AppBar, Sidebar, TreeNode, TreeMenu, NewFolderInput, HistoryList, Welcome, RequestTabs, RequestPane, KeyValueEditor, ResponsePane, EnvironmentEditor, ScopeEditor, SettingsDialog, CookiesDialog, CodeDialog, ImportReport, CurlDialog, CommandPalette, ShortcutsSheet, MockDialog, SyncDialog, WorkspacesDialog, MonitorsDialog, RunnerDialog, OAuthDialog, StreamPane, GrpcEditor, GrpcReplyPane, GraphQlEditor, TrashDialog, Toast |
| `examples/sample-collection/` | Fixture the Rust tests load — changing it breaks tests |

## Conventions that matter

**`app/types.ts` mirrors `src-tauri/src/model.rs`.** Nothing enforces this.
Change a struct and you must change the interface in the same commit, or the UI
silently reads `undefined`.

**Serde tagging.** `Body` and `Auth` are `#[serde(tag = "type")]` enums, so they
cross IPC as `{ type: 'json', content: '…' }` and the TS side is a discriminated
union on `type`. Adding a variant means touching both sides plus the `bodyKinds`
/ `authKinds` arrays in `RequestPane.vue`.

**Rust serializes camelCase, YAML stays snake_case.** `HttpResponse` and
`ExecOptions` carry `#[serde(rename_all = "camelCase")]` because they only ever
travel over IPC. The `model.rs` structs do not, because they are also the file
format and the YAML should read naturally.

**Lists, never maps.** Headers and params are `Vec<KeyValue>` so duplicates
survive a round-trip and diffs stay line-by-line readable in a pull request.

**`seq` drives ordering.** Sorting is `(seq, name.to_lowercase())`.
`collection::reorder` numbers a folder's children `1..n` but writes only the
files whose number actually changed, so dragging one row does not show up as a
diff on every sibling. `FolderMeta` has a hand-written `Default` because the
derived one would give `seq: 0` and disagree with the serde default — if you add
a numeric field with a `#[serde(default = ...)]`, check the two agree.

**Renaming does not touch filenames.** `rename_node` edits the `name:` field
(and creates `folder.yaml` for a folder); the file keeps its own name, so `id`
is stable and git sees a one-line edit rather than a rename. Moving is the
separate operation that actually relocates a file. Keep them separate.

**`id` is the collection-relative path** (`users/list-users.yaml`), which is why
there is no index file. Every id from the UI goes through `collection::resolve`,
which rejects `.`, `..` and empty segments, and then `reserved_check`, which
keeps `collection.yaml` and `folder.yaml` from being treated as nodes. Do not
bypass either. A move must also refuse a folder landing inside itself, or the
whole subtree is detached.

**An `id` changes when a node moves.** `store.applyMove` remaps `activeId`,
including the case where the open request merely lives *inside* a moved folder.
Anything else holding an id across a move needs the same treatment.

**Secrets never reach the YAML.** An `EnvVar` with `secret: true` has its value
written to `.env.<stem>` at the collection root, where `<stem>` is the name of
the environment's YAML file (`environments/prod.yaml` → `.env.prod`). Only the
name goes into the YAML. `load_environments` re-attaches the value at read time.
Any new code path that writes an environment must go through
`write_environment` to keep this split.

**How the secrets files work** (`secrets.rs`, `collection::write_environment`):
- The YAML file's stem is an environment's identity on disk, not its `name`.
  An unchanged name keeps its file even if it was named by hand; a new or
  changed name gets `slug(name)`.
- `save_environment` takes `previous`, the name the environment was opened
  under. A rename moves the YAML and the secrets file; saving onto a file that
  belongs to another environment is refused.
- Files are edited line by line, not regenerated: hand-written comments and
  keys survive. Keys the environment declared (before or now) that are no
  longer secret are removed; a file with nothing worth keeping is deleted.
- Values are always written double-quoted with `\\ \" \n \r \t` escaped, so
  multi-line keys round-trip.
- **Every write checks the collection's `.gitignore` covers the file** and
  appends `.env.*` if not. Collections created before this format only ignore
  `.env`; without the check their secrets would be committed. Never write a
  secrets file without `secrets::write`, which does this.
- Legacy: older collections have one shared `.env` with one value per name. It
  is still read as a fallback (with the old parsing rules — no escapes). The
  first save of any environment copies shared values into every environment's
  own file, then removes from `.env` the names some environment declares,
  deleting it once only its old header is left. Keys nobody declares stay.
- `EnvironmentEditor` edits every environment at once: one draft per
  environment, kept in memory while the sheet is open, and Save writes only the
  drafts that changed. `delete_environment` removes the YAML and the matching
  `.env.<stem>`, leaving the shared legacy `.env` alone because another
  environment may still be reading it. A draft that was never saved is dropped
  without asking — there is no file to delete.

**Other paths that must keep secrets out of files:**
- History (`history.rs`) stores the request as authored, with `{{vars}}`
  unresolved, and `Entry::redact` replaces every secret value (4+ chars) with its
  `{{name}}` across the whole entry — resolved URL, echoed headers, body.
- An imported folder keeps its own headers and auth in `folder.yaml` rather
  than having them copied onto every request inside it, so the tree that comes
  out reads like one somebody wrote. `scrub_nodes` therefore scrubs a folder
  as well as a request: a credential can now live on either.
- Import (`import.rs`) moves literal credentials — bearer tokens, basic
  passwords, API key values, `Authorization` and `Cookie` headers, and headers,
  params or form fields named like a credential — into secret variables. JSON
  bodies are not rewritten, only warned about.
- A pasted curl command goes through the same rule via
  `import::extract_request_secrets`: new secret variables are added to the
  active environment *before* the request is filled, so the editor only ever
  holds `{{name}}`. Names never clash with existing variables, and a value an
  existing variable already holds reuses it.
- "Copy as cURL" leaves secret variables as `{{name}}` unless the user picks
  "Copy with secret values".

**Folder names typed by the user are display names.** `create_folder` stores
the typed name in `folder.yaml` and derives the directory with
`folder_dir_name`, which transliterates accented Latin letters before
slugging. It is deliberately separate from `slug`: environment filenames come
from `slug`, and changing its output would orphan existing environment files.

**A delete asks, and is undoable.** `store.remove` confirms through the dialog
plugin's `ask` (which travels as `plugin:dialog|message`, already allowed by
`dialog:default`) and closes the editor if the open request lived inside what
was deleted — otherwise its next save would recreate the file.

**What it deletes goes to `.trash/`, not away.** `collection::delete_node`
moves the file or folder to `.trash/<stamp>-<name>` inside the collection and
returns a `Deleted` describing it; `restore_node` puts it back, refusing if
something has taken the id since. `ensure_trash_ignored` appends `.trash/` to
the collection's `.gitignore` on every delete, because a deleted secret must
not arrive in a commit by the back door. The toast offers Undo and the bin is
in the collection menu. This is deliberately not an in-memory undo stack: an
undo that only lasts until the app closes is worse than none, because people
come to rely on it.

**Inline inputs must guard against blur.** Removing a focused input fires
`blur` after Enter or Escape already handled it. `commitRename` checks that the
rename is still pending, and `NewFolderInput` has a `done` flag; without that,
Enter commits twice and Escape saves instead of cancelling.

**Requests run in Rust, not the webview.** That is the point — no CORS, real
redirect and TLS control, honest timings. Never move a request into `fetch()`.

**One plan for sending and for printing.** `http::plan` is the only place a
request is resolved: variables, inherited collection auth, collection headers
overridden by request headers (case-insensitively), params and a query API key
appended, the body's default Content-Type. `http::execute` sends a plan and
`curl::export` prints one, which is why "Copy as cURL" matches what Send sends.
Any change to how a request goes out belongs in `plan`, never in only one of
the two. `lenient_url` lets a URL with an unresolved `{{var}}` be printed while
still failing to send.

**curl, both ways** (`curl.rs`):
- `parse` takes bash (quotes, `$'…'`, `\` continuations) and Windows cmd (`^`
  escapes, C runtime argv rules) — the two flavours of a browser's "Copy as
  cURL". Options that take a value are listed explicitly so their value is
  never mistaken for the URL; add to those tables rather than guessing.
- The request is faithful to the command: `-d` without `-X` is POST, `-G` moves
  data to the query, curl's default form Content-Type is kept for non-pair
  data, an `Authorization: Bearer/Basic` header becomes auth. Browser-only
  headers (`sec-fetch-*`, `sec-ch-*`, `priority`) are kept but disabled;
  `Content-Length` and `Accept-Encoding` are dropped because volt sets them
  (reqwest cannot decode the `zstd` browsers ask for).
- Nothing is dropped silently: unsupported transport options, files the command
  would read, `-k`, other auth schemes all come back in `notes`, which the UI
  shows under the request.
- Pasting a curl command into the URL field fills the open request (unsaved);
  the cURL menu and the folder menu open `CurlDialog` for multi-line commands.
**Variables volt supplies itself.** `{{$timestamp}}`, `{{$isoTimestamp}}`,
`{{$guid}}` (also `{{$uuid}}`) and `{{$randomInt}}` come from `vars::dynamics()`, which
`collection::env_context` lays down *before* the environment — so a collection
that defines one of those names wins. They are generated once per resolution,
not per occurrence, so two `{{$guid}}` in one request agree; a later send gets
new ones. `env_context` is the only place the map is built, which is how
`preview`, Send and "Copy as cURL" stay in step. The randomness is splitmix64
over the clock: fine for test data, never for anything that must be unguessable.

**Helpers, not scripts.** `{{$fn(arg, arg)}}` is a value computed from
others — `$hmacSha256(apiSecret, $body)`, `$base64("user:pass")` — which is
what a Postman pre-request script is usually written for. The set is
`vars::HELPERS`, fixed and small, mirrored by `SUPPLIED` in the store for
completion; adding one means both. Arguments are variable names (resolved,
and reported by name when missing) or double-quoted literals; a defined
variable that happens to look like a call still wins. `$body` is the body as
it will be sent: `http::plan` resolves the body *first*, lays it into a copy
of the variables, and only then resolves the URL, query and headers — so a
header can sign the payload and the payload cannot be defined in terms of
itself (`$body` inside the body is reported missing). The `preview` command
has no body to offer, so the UI drops `$body` from what it calls undefined.
A helper that cannot be computed stays in the request verbatim and names
what was missing — the argument, or the call — exactly like a variable.
There is no script, deliberately: the value of a request in YAML is that it
can be read.

**Two option structs, on purpose.** `http::ExecOptions` is the app's settings
and is IPC-only, so camelCase. `model::RequestOptions` is what a request
carries in its own YAML, so snake_case and every field `Option` — absent means
"whatever Settings says". `ExecOptions::for_request` merges them and is called
by both `http::execute` and `curl::export`, so a per-request timeout shows up
as `--max-time` in the printed command as well as on the wire. An empty proxy
string is not "nothing said": it means go direct, and switches off the
`HTTP_PROXY` variables reqwest would otherwise read.

**Cookies live in memory, per collection.** `lib.rs` holds a jar per collection
root and hands it to `execute`; without it every send builds a fresh client and
a session cookie never survives to the next request. Nothing is written to
disk — a session cookie is a credential and a collection is meant to be
committable — so closing volt signs you out, which is the safe direction.
`reqwest::cookie::Jar` cannot be emptied or read back, so `clear_cookies`
throws the jar away and starts another.

**Shortcuts are registered, not hard-coded.** `useShortcut(keys, label, run)`
in the component that owns the action; `installShortcuts()` runs once in
`app.vue`. That is what lets the `?` sheet list what actually exists. Two rules
the listener enforces: nothing fires while a dialog is open (it has its own Esc
and focus trap), and a binding without `mod` is ignored while a field has
focus, or `/` would type itself into whatever is being edited.

**Scopes: what a request inherits.** `collection::Scopes` is the collection
plus every folder between it and the request, outermost first, read from disk
by `collection::scopes(root, id)`. Headers apply in that order and a nearer one
replaces the same name (case-insensitively); auth walks the other way and the
nearest scope that is not `inherit` answers; variables are laid down weakest
first — the ones volt supplies, then the collection, then each folder, then the
environment, which wins because switching environments has to be able to
override a default. `http::plan` is the only place this resolves, which is why
Send, "Copy as cURL" and a generated snippet cannot disagree. A request with no
file yet (a history replay) has no folders, only the collection.

**Captures, not scripts.** A request can carry `captures`: `name` plus a
`from` that is `status`, `header:Name`, `body`, or a path into a JSON body
(`$.data.token`). `send_request` returns what they found and the UI writes it
into the active environment — marked secret, so a captured token lands in
`.env.<environment>` and never in the committed YAML. The JSONPath subset is
deliberate: filters and wildcards would make "what will this capture" a
question you cannot answer by reading it.

**Examples are files, and redacted.** A kept response is written to
`.examples/<the request's path>`; the tree skips dotted names, so it never
appears as a request. They go through `history::redacted` with the active
environment, because an example is committed and a token in a committed file is
the thing volt exists to prevent.

**A `Set-Cookie` in history keeps everything but its value.**
`Entry::redact` replaces the values of variables marked secret, which cannot
help with a session cookie the user never declared. So `hide_cookie_value`
rewrites `session=abc123; Path=/; HttpOnly` as `session=…; Path=/; HttpOnly`:
the name and the attributes stay, because "why am I not getting a cookie" is
the reason to look at history in the first place, and only the value goes.

**A response can be read three ways, and one of them writes back.** Pretty
and Raw are text. Tree (`UiJsonTree`) is the parsed body with folds, and
every row carries its path in the subset `capture::read` understands —
`$.data.items[0].id` — so the Capture key on a row adds exactly the capture
that will work, named after the last segment and kept secret when the name
says token, key, password or the like. A body of more than 20,000 values says
so instead of building a tree. `text/html` adds Preview: an `<iframe
sandbox="">` with `srcdoc`, which allows nothing — no scripts, no forms, no
navigation — because the body is whatever some server sent. Beside those, the
diff key compares this send with the previous one on the same tab
(`previousResponse`, carried in the tab like the response itself):
`utils/diff.ts` is a plain line LCS, capped at 2,500 lines a side so it can
never freeze the window, with unchanged runs folded.

**An example is something to compare against.** `examples::compare` reads the
kept response and the one on screen as *shapes*: the status, the content type,
which paths appeared or vanished (`$.data.items[].id`), and which hold a
different kind of thing now. A list contributes one path per field across all
of its items, so a hundred rows read as one shape; items that disagree are
`mixed` rather than whichever came last; and a branch that is gone is reported
once instead of once per field under it. Not a line diff: two JSON bodies
differ on every line the moment an id changes, and the question an example
answers is whether the shape is still the one that was reviewed.

**A GraphQL schema belongs to the server.** `graphql_schema` introspects and
`graphql::read` keeps the answer in the `Schemas` map in `lib.rs`, per request,
for as long as the app runs — never in a file. A cache committed next to the
requests is a lie waiting to be believed. `graphql::check` walks a query against
it and `graphql::complete` answers with the fields that belong where the caret
is; both come from the same walk, which follows selection sets, aliases and
inline fragments and skips arguments, directives and variable definitions
whole. A named fragment spread is not followed — its definition is checked
where it is written, where `on Type` says what it is about. Where the walk
cannot follow, it says nothing: a red mark under valid GraphQL is worse than no
mark at all.

**The importer reads the shapes a script shares with volt.** Most of a Postman
test script is not a script: `pm.environment.set("token", …)` is a capture and
`pm.test("status is 200", …)` is a check, and `scripts::translate` reads both,
plus the variable a script binds the parsed body to. Everything else comes back
in `Translated::left` and the report names it line by line, quoted. The line is
deliberate: no control flow, no computed values, no fragment of a JavaScript
interpreter. A check that quietly does not mean what the script meant is a test
that lies — so `not contains`, a list index and a path volt's JSONPath subset
has no way to write are all left for the report rather than approximated. Only
after-response scripts are read; a pre-request script runs before the send and
volt has nothing that does. What a script captures is marked secret, because
the safe mistake is the one that keeps a value out of the committed YAML.

**gRPC speaks HTTP/2 itself.** `wire.rs` opens the connection over `h2` —
already in the binary through reqwest — because reqwest hands back a body and
gRPC needs three things that are not in one: the messages as they arrive, a
request half that stays open, and the **trailers**. The `grpc-status` that
follows a body is where a gRPC call says whether it worked; without it a failed
streaming call reads as a successful one. `wire::Exchange` is split into a
`Sender` and a `Receiver` on purpose, so a bidirectional call can have a task
on each without one `&mut` over both, and `Sender::send` waits for flow-control
capacity rather than assuming it. `grpc::open_stream` registers in the same
`stream::Streams` a WebSocket uses and pushes the same events, because to the
UI it is the same kind of thing; sending an empty message means "nothing more
from me", which is how a client-streaming call asks for its reply.

**A stream says `closed` once, and not before it is.** Every task that owns a
connection — WebSocket, server-sent events, gRPC — calls `Streams::forget`
and only then emits `closed`, exactly once. Announcing it first leaves a
window in which the UI has been told the connection ended while the registry
still lists it as open, and a second `closed` behind the first shows up in the
transcript as a socket that closed twice. Both were real: the WebSocket task
emitted the server's reason inside the loop and an empty one after, and the
gRPC task announced before it forgot.

**Reflection is written by hand, and has to be.** Calling `ServerReflection`
would normally mean compiling its `.proto`, which is circular: volt reaches for
reflection precisely because it has no descriptor pool yet. So `reflection.rs`
encodes and reads those four messages against the protobuf wire format
directly. It asks for the service list, then for the file containing each
symbol, then for any import nothing has supplied yet, until the set closes over
itself — and what comes back is a `FileDescriptorSet`, exactly what `protox`
produces from a file on disk, so a reflected service and a compiled one are the
same thing to everything downstream. The v1 name is tried first and the
`v1alpha` one after it.

**Cookies are ours, not reqwest's.** `cookies::Jar` implements reqwest's
`CookieStore` so it can be listed, deleted from and cleared — reqwest's own jar
can do none of those, and a jar you cannot look inside is a debugging trap. It
is an RFC 6265 subset: domain matching refuses a `Domain` that is not the
host's own and refuses a dotless one (no public suffix list, so `co.uk` is
missed), longest path wins, `Max-Age` beats `Expires`, and an expiry in the
past deletes. Jars live in memory, one per collection, and are never written.

**Code generation is another printer of the same plan.** `codegen::render`
takes a `Plan` exactly as `curl::render` does; secrets stay `{{name}}` unless
asked for. Adding a language means one function and one entry in
`Language::ALL`. Its quoting helpers are written with `'\u{5c}'`-style escapes
on purpose — the functions are about escaping, and a source file full of
leaning toothpicks is how the wrong number of them ships.

**Tabs keep the live fields.** `store.tabs` is a list of snapshots and the
request on screen lives in the store's own fields (`request`, `dirty`,
`response`, …). `snapshot()` writes the live fields into the active tab and
`switchTab` reads the next one out, which is why every action can go on writing
`this.request` the way it always did. Anything that changes an id — a move, a
delete — has to walk `tabs` as well as `activeId`, or a tab points at a file
that is not there and its next save recreates it.

**The mock is a real socket, not a stub.** `mock.rs` listens on loopback and
answers from the saved examples: routes come from each request's URL with any
`{{variable}}` segment turned into a wildcard, longest path first. The HTTP is
hand-written on tokio because a mock needs one request line, some headers and a
response — and because a server framework would be the largest dependency in
the project by some way. It serves while volt runs and nowhere else; the app
bar shows a badge so a forgotten one is visible.

**The mock matches on what the request declares.** Beyond the method and the
shape of the path, a route also carries the query parameters and the top-level
JSON body fields the request gives a literal value — a `{{variable}}` is not a
condition, because it matches anything. A call has to carry all of them; extra
parameters are the caller's business. Within one path shape the route that asks
for more is tried first. That is where the matching stops: every rule beyond it
is a rule somebody has to reason about when the mock answers the wrong thing,
and a matching language is a product of its own. `MockDialog` shows each
route's conditions as chips, so two routes on one path are visibly different.


**Documentation is a file, not a page.** `docs::html` writes one
self-contained HTML document — no `<script>`, no `<link>`, so it opens from an
email attachment on a plane. Everything written into it goes through `escape`,
including example bodies, which are whatever some server sent.

**Sync is the collection's own repository.** `git.rs` shells out to the `git`
already on the machine. Three rules it keeps: every command is scoped with
`-- .` so a collection inside a larger repository never stages someone else's
work; a pull is `--ff-only`, because resolving a merge belongs in the user's
editor; and a commit is refused while `.gitignore` does not cover the `.env`
files. Missing git, or a folder that is not a repository, is not an error —
`status` says so and the rest of the app carries on.

**The first request is already there.** `ensure_default` writes one
`hello.yaml` — a GET to a public JSON endpoint with a status test — into the
personal collection the one time it creates it, so the first thing on screen
is a Send key that does something. Deleting it is a delete like any other; a
later launch never puts it back, because the check is for `collection.yaml`,
not for the sample.

**Files dropped on the window need no dialog.** The webview reports drops
(`onDragDropEvent`); an export — Postman, Insomnia, OpenAPI — is converted
into `<documents>/volt/` beside the personal collection (`imports_dir`) and
opened with its report, and a folder is opened as it is. Asking where to put
a dropped file would undo the point of dropping it.

**`volt <folder>` opens that folder.** `startup_collection` reads the first
command-line argument, or `VOLT_COLLECTION`, and `restore` opens it instead
of the last collection when it holds a `collection.yaml`. It is what the
real end-to-end test uses to keep out of the user's collections, and it is
how a terminal or a shortcut opens the right one.

**A panic leaves a report.** `install_crash_hook` writes the panic and its
backtrace to `crash.log` in the app data folder before the default hook runs;
`last_crash` hands it over once on the next start, renaming it to
`crash-<stamp>.log`, and the shell shows a strip saying where it is. It is
its own state (`store.crash`) rather than `store.error`, because every
action clears `error` on its way in and the mock refresh on start would
have wiped it before anyone read it.

**A first launch opens a collection, not a dialog.** With nothing remembered,
`restore` asks `default_collection` for volt's own folder —
`<documents>/volt/personal`, made by `collection::ensure_default` the first
time and never reinitialised after — and opens it. Before this, the first
screen was a file picker and "New request" was disabled until a folder had
been chosen, which is a question nobody can answer before they have made a
request. The store keeps the path as `defaultRoot` so the start page can say
what the folder is; a collection inside a repository is opened over it like
any other, and the personal one stays where it is.

**Workspaces and monitors are settings, not collection data.** Both live in
the app's own store: a workspace is a list of paths on this machine, and a
monitor only runs while volt is open. Monitor timers are kept in a module-level
`Map`, outside the store — a timer id is not state to render, and putting one
in a reactive object is how two of them end up running. Every monitor run is
recorded in History like any other send, so there is no second log to go stale.

**Checks are captures with a comparison.** A `Check` is `from`, `op`, `value`,
and `from` is the same vocabulary a capture reads — `status`, `time`,
`header:Name`, `body`, `$.data.id`. Both go through `capture::read`, so an
assertion and a capture can never disagree about where a value comes from. `op`
defaults to `is`, because that is what most checks are. On screen they are
called **Tests** — the word people arrive with from every other client — and
only the file format and the code say `checks`.

**A run carries its variables forward.** `runner.rs` walks the tree in order,
and a step's captures feed the next step's context. They stay in memory for the
length of the run: a run is a check, not an edit, and a CI job should not leave
a token in a file behind it. `volt-run` is the same code from a terminal, which
is why `pub mod runner` is the only module the binary can see.

**Two auth schemes that are not headers.** SigV4 signs the finished request, so
`aws::sign` runs between `plan` and `execute` (and again in `curl::export`, so
a copied command works). Digest and NTLM are conversations: the first send gets
a 401, `execute` answers it and sends again. NTLM authenticates a *connection*,
so a request using it gets a client with `http1_only` and one pooled
connection — without that the three legs land on different sockets and the
handshake never completes.

**OAuth is a step, not a kind of auth.** `oauth.rs` fetches a token and the UI
writes it into the environment as a secret; requests then use `{{token}}` like
anything else. A token hidden inside an auth object is a token nobody can see,
reuse or share as a `{{name}}`.

**A request has a kind.** `Kind` is `http`, `websocket`, `sse` or `grpc`, and
it decides which pane the shell shows and what the Send key does. A gRPC
request is read twice over: a unary method sends and shows a reply, a streaming
one opens a connection and shows the stream pane, which is why the store keeps
`grpcMethods` — `go()` has to know which before it can decide. gRPC carries
its call in `Body::Grpc` rather than in `method`, because the RPC name and the
HTTP method are different things and one field cannot be both.

**OpenAPI import is a translation with opinions**, and they are listed at the
top of `openapi.rs`: paths group into folders by their first segment,
`servers[0]` becomes `{{baseUrl}}`, a path parameter becomes `{{name}}`, a
schema becomes a body worth editing, and `securitySchemes` become auth with
placeholders — never a token, because a token in a spec is a mistake nobody
should copy forward.


**`{{` completes.** `UiVarInput` watches the text before the caret for an
unclosed `{{name` and offers what `store.variableNames` knows — the values
volt supplies, the collection's variables, the environment's — filtered by
prefix. Enter or Tab inserts the name and closes the braces unless they are
already typed; Escape closes the list and keeps it closed until the text
changes; Enter with no list open still sends. The list is `position: fixed`
because the URL bar clips its overflow. The same list is what marks a
variable undefined, so marking and completing cannot disagree — with one
gap, folder variables, which the UI does not load; Rust's `preview` remains
the authority on what the URL resolves to.

**Missing variables stay visible.** `vars::interpolate` leaves `{{name}}`
verbatim and reports it in `missing_vars` rather than substituting an empty
string. Preserve that; silent empty strings are how people ship broken requests.

**One TLS provider, chosen out loud.** `wire::ensure_crypto_provider` installs
`ring` once, before anything builds a `rustls::ClientConfig`. Two backends are
compiled in — volt asks for `ring`, reqwest's `rustls` feature drags in
`aws-lc-rs` — and rustls does not pick between them: `ClientConfig::builder()`
**panics**. A panic in a command unwinds the task, so the promise in the webview
never settles and the UI waits for ever with no banner. That is how every
`grpcs://` call and every `wss://` socket failed silently. `wire::tls_config` is
the one place a config is built, so the WebSocket path cannot quietly verify
while the app bar says TLS is off.

**Send before you wait, on HTTP/2.** A gRPC server reads the request message
before it answers. `Connection::start` sends the headers and hands back the two
halves; `wire::receive` waits for the response. Awaiting the response first is a
deadlock against every real server — and the test server in `grpc.rs` now reads
a DATA frame before it responds, precisely so that mistake fails a test instead
of shipping.

**Offsets across the IPC boundary are UTF-16.** A textarea counts in UTF-16
code units; Rust slices bytes. `graphql::complete` converts on the way in and
`graphql::check` converts on the way out. Treating one as the other panics on
the first non-ASCII character — `{ ıııı }` was enough — and a panic in a command
takes the window with it. Anything new that takes a caret position from the UI
has the same obligation.

**Randomness has two kinds and they are not interchangeable.** `vars` has a
splitmix64 over the clock for `{{$guid}}` and friends: fine for test data, and
documented as such. Anything that has to be *unguessable* — the PKCE verifier,
the OAuth `state`, the NTLM client challenge — comes from `getrandom`, the OS
source. A verifier recoverable from the `state` sitting beside it in a proxy log
is a verifier that protects nothing.

**Files are written through `collection::write_atomic`.** A temp beside the
target, then a rename. `fs::write` truncates first, so a crash between the
truncate and the write leaves an empty request. The temp is named
`<name>.tmp` rather than `.<name>.tmp` on purpose: the dotted form of a secrets
file (`..env.prod.tmp`) falls outside the `.gitignore` rule, and a crash would
leave a credential in a file git is willing to commit.

**Every parser bounds its own work.** `openapi::sample` carries a node budget as
well as a depth limit — ten self-referencing properties produced 286 MB from a
twenty-line spec. `wire::Receiver` refuses a message over 16 MB rather than
believing a server's length prefix. `import::v4_children` threads the ancestor
path so two resources sharing an `_id` cannot recurse for ever; a stack overflow
cannot be caught, so it takes the whole app with it. `cookies::parse_http_date`
bounds the year before the civil-date arithmetic, which overflows long before
`i64` runs out.

**A reply belongs to the tab that asked for it.** `send()` snapshots the owning
tab and the root before the invoke and checks both after: a different collection
means the answer is dropped, a different active tab means it is written into the
tab that issued it rather than the one in front. Without that, a slow request
answered into whatever the user had moved to — and its captured token was
written into the wrong environment. `myStream` is the same rule for connections:
one socket at a time, and it belongs to the request that opened it.

**A panic in a `#[tauri::command]` is a crashed app, not an error.** There is no
catch: the task unwinds, the promise never settles, and `attempt()` never sees a
rejection, so the UI sits there. That is why every indexing expression, slice
range and integer cast on a path that touches a server's bytes, an imported
document or the webview's numbers is a bug until it is bounded.

**The URL bar shows the query; the file keeps it apart.** What the field
shows is `url` with the enabled params appended, and what is typed into it is
split back: the part before `?` becomes `url`, the pairs become the enabled
params, disabled ones are kept behind them. People expect to paste a whole
address and see the table fill, and to see the table in the address; the
YAML stays `url` plus a list, which is what diffs well and what `plan` joins.

**A send can be stopped.** Every send carries a token; `send_request` parks a
oneshot under it in `Cancels` and `execute_or_cancel` selects between the
send and that channel. Dropping the future drops the connection, so a
cancelled send stops sending rather than finishing in the background. A
cancelled send is not history — it says nothing about the server — and the
UI, which set `sendToken` back to null before asking Rust, ignores whatever
comes back under the old token and shows no error. The Send key is the Cancel
key while a send is out; there is no second button to find.

**Errors are said in words, with the original underneath.** `utils/errors.ts`
maps the messages reqwest and the OS produce — refused, unknown host, timed
out, reset, certificate — to a sentence with the host in it and what to do,
and keeps the raw message under it on the strip: the sentence is for the
person, the raw text is for the bug report. A certificate failure also sets
`errorCertificate`, which is what puts "Send without verifying" on the strip;
it turns off `verify_tls` on that request's own Options and sends again, so
the choice is saved with the request rather than made for the whole app.

**The open tabs come back.** `persistTabs` writes each collection's tabs —
ids, which are dirty, and the dirty ones' edits — into the app's settings
under `openTabs`, debounced from a deep watch on the live request;
`restoreTabs` opens them again after `load`, putting the unsaved edit back
on top of the file. `restoringTabs` holds `persistTabs` off while that
happens, or the empty tab list of a collection just opened would overwrite
the one about to be restored. History replays are not kept; they have no
file.

**Search reads the files.** The sidebar's name search stays in the store;
`search_collection` (`collection::search`) reads every request and reports
where the text was found — `header X-Api-Key`, `body`, `docs` — so a hit
says why it is a hit. Auth values are not searched: they are `{{names}}` by
design, and a literal there is the one thing not worth making easy to find.

**A request can leave for another collection.** `copy_request_to` writes it
at the other collection's root under its own file name, or the first free
numbered one; a move is that copy followed by a delete into this
collection's bin. The dialog offers the collections opened lately, because
those are the ones a request is ever moved between.

**A curl command dropped on a folder is a request in it.** The tree's drag
code lets text from outside through (`over` accepts a `text/plain` drag
when no row of ours is being dragged) and `onDrop` hands anything that
`isCurl` to `importCurl` with the folder as parent; the empty part of the
sidebar means the root. The row handler stops the event, or the root handler
would import it a second time.

**A tip is said once.** `tip(key, …)` shows a sticky toast the first time
and records the key under `tips` in the app's settings. The one that exists
explains the sample request's green mark after its first send. Tips are for
things that are surprising once and obvious after; a second one should have
to argue its way in.

**The updater is the one thing that reaches out on its own.** volt asks
GitHub's releases for a newer version on start, and nothing else about the app
contacts a server the user did not point it at. So it is a setting, not a
given: `autoCheckUpdates` defaults to on, Settings says exactly what the
request carries — this build's version, and the IP any request carries — and
off means no request is made at all rather than a quieter one.

`checkForUpdate(loud)` splits the two cases on purpose. The check that runs on
start is silent when it fails: being offline, a wrong endpoint or a bad day at
GitHub must not put an error in front of somebody who only wanted to open a
collection. The check the user pressed says what happened either way, including
"you are on the latest".

An update is only offered, never applied: installing restarts the app, and a
tab with unsaved edits in it is not something to take away from someone without
asking. Releases are published as **drafts**, and a draft is not served at
`/releases/latest/`, so nothing reaches anybody's updater until the release is
published by hand.

**Errors surface or they do not exist.** Every store action wraps its body in
`attempt()`, which parks the message in `store.error` for the banner in
`app.vue`. An `invoke` outside `attempt()` becomes an unhandled rejection and the
UI just sits there.

## Design system — "Paper"

Light first. volt is a sheet of warm paper with ink on it, and one colour —
ultramarine — kept for the things that act. Follow this for every new screen:
it is what keeps the app from looking like every other client, and each part
of it is deliberate.

**The idea.** A request is a file, and the screen should feel like a well-set
page rather than a cockpit. Warm off-white surfaces, near-black ink, hairline
rules, and nothing that glows. The collection is visibly files: request ids
such as `users/list-users.yaml` are printed next to the name, not hidden. Two
things are *stamped* — the method on the request bar and the status on the
response — a solid block of colour with the text cut out of it, which is the
one motif the app owns.

**The one colour.** Ultramarine means **acts**, and that is the only thing it
means. It marks the Send key, the request you are on, a `{{variable}}`, the
focus ring, the underline of the open tab, the rail key that is on. It is never
decoration, and it is never a status: a warning is amber (`--warn`), a failure
is red (`--bad`), a success is green (`--ok`). There is one filled accent
button on a screen — the one that does the thing. `.btn-primary` is that button
and the Send key is its own case of it.

**Two tones of accent, and they are not interchangeable.** `--accent` is a
fill: white reads on it at 7:1. `--accent-text` is the same colour as text on
paper, darkened on light and lifted on dark so it clears 4.5:1 on the surface.
A component that writes accent-coloured *text* uses `--accent-text`; a
component that *fills* uses `--accent`. Getting them the wrong way round is
the difference between a control and a smudge.

**Surfaces**, from the frame inward: `--bg-0` (the rail, the sidebar, the tab
strip), `--bg-1` (the working surface), `--bg-2` (buttons, cards), `--bg-3`
(menus and dialogs, the only things that float), `--well` (inputs, code, the
response body). Light is on bare `:root`; dark is the same paper at night —
warm graphite, not blue-black — and redefines tokens only, under
`prefers-color-scheme: dark` while no theme is chosen and under an explicit
`data-theme="dark"`. Two more sheets of paper, Linen (cream, brick) and Mist
(cool grey, teal), are named themes as well. Every named theme is selected by
`[data-theme=…]` on *any* element, not only `:root`: that is how the cards in
Settings show each theme's real colours — a swatch wears the attribute and
draws `var(--bg-0)`, `var(--accent)`, `var(--ink)` — without a second copy of
the palette in a component. The user's choice (System, Paper, Graphite, Linen,
Mist) lives in Settings and is applied by `applyTheme`. If you add a token,
add it to all five blocks and check contrast: text >= 4.5:1 on every surface,
`--faint` and `--punc` >= 3:1. Every hex in `tokens.css` was measured, not
eyeballed.

**Type.** IBM Plex Sans for the interface and IBM Plex Mono for anything a
machine reads: URLs, keys, values, bodies, paths, numbers. They are one family,
which is most of why the screen reads as designed rather than assembled. Both
are bundled through `@fontsource`; the app is offline and its CSP only allows
same-origin, so never link a font CDN. Sizes come from `--t-*`. A label is
`.silk`: 11.5px, medium weight, sentence case, in the label colour. It does
not uppercase and it does not track — the label should be quieter than the
value, not louder.

**Shape.** Radius follows role: `--r-xs` 4 (checkbox, kbd, stamps, variable
marks), `--r-sm` 6 (buttons, inputs, rows), `--r-md` 8 (menus, the URL bar),
`--r-lg` 10 (dialogs). Structure is drawn with hairline rules, not boxes:
panes, tables and sections are separated by `--line`, and only the things
that float get `--shadow-pop`. No other shadow exists. No gradient exists. Do
not wrap content in cards.

**Materials.** A control is a rectangle with a hairline and a fill. Under the
pointer it darkens one step (`--hover`), pressed another (`--press`). Nothing
moves on press and nothing lights up. `UiSegmented` is a strip of choices with
the chosen one lifted to the surface; the chosen one is not filled with the
accent, because a choice is not an action.

**The layout.** A rail on the left edge — the mark, then one key per section
(Collection, History, Env, Settings) with its name under it, because an icon
that needs a tooltip is a puzzle and a rail with names is the thing people
already know from every other tool. Beside it the sidebar for the section that
is on: search and New on top, then the collection itself as the root row with
its menu, then the tree — the shape someone arriving from Postman expects, so
nothing has to be explained. Above both a slim bar holding the collection
switcher and the environment. The request is one bar: the method stamp, the address, and Send
welded to its right end, because to the person typing it is one thing — where,
and go. The response puts its reading in the tab row — the status stamp, the
time, the size — rather than in a readout of its own.

**Shared vocabulary** (`base.css`): `.btn` (+ `.btn-primary`, `.btn-quiet`,
`.btn-danger`, `.btn-sm`), `.icon-btn` (+ `.quiet`, `.sm`, `.on`), `.field`
(+ `.mono`, `.inline`, `.invalid`), native checkboxes and `.switch` are styled
globally and go accent when checked, `.led` (+ `ok/warn/bad/live/off`, a plain
dot), `.method[data-method]` (coloured text; + `.tag` for the stamp, whose
text is `--on-method`), `.var` (+ `.missing`), `.kbd`, `.chip` (+ `.accent/
.ok/.warn/.bad`), `.silk`, `.mono`, `.num` (tabular figures).

**Primitives** (`components/ui/`, used as `<UiX>`): reach for these before
writing markup. `UiVarInput` for any single-line field that can hold
`{{variables}}` (marks undefined ones in amber); `UiCodeView` for any body
shown to the user; `UiDialog` for any modal (Esc, focus trap, return focus);
`UiTabs`/`UiSegmented` for switching (arrow keys work); `UiMeasure` for a
labelled reading; `UiSplitter` for resizable panes; `UiMenuButton` for a
button that opens a short menu (items with icon and hint, keyboard navigation;
`variant` picks a quiet, solid or primary face). For a passing confirmation
call `store.notify(text, detail?, tone)`, which the `Toast` shows; errors
still go through `attempt()` and the error strip.

**Icons and the mark.** Only from `utils/icons.ts`: drawn for this app on a
16px grid, 1.5px stroke, round caps. No icon libraries (Lucide and friends are
the most recognisable sign of a generated UI). The mark is the bolt in that
set, filled; the app icon is the same bolt, white on ultramarine, from
`src-tauri/app-icon.svg` — regenerate every size with
`pnpm tauri icon src-tauri/app-icon.svg -o src-tauri/icons` and delete the
`android/` and `ios/` folders it also writes. Prefer a word when an icon would
need a tooltip to be understood; the rail's keys carry their names.

**Motion.** Short and functional only: 120ms colour transitions on `--ease`,
a 120–160ms fade for dialogs and menus, stripes across the Send key while a
request is in flight, a sweep on the response strip while waiting.
`prefers-reduced-motion` turns it all off (`base.css`).

**Copy.** Say what happens, from the user's side: "Save environment", "No
response yet. Send the request", "Undefined: orderId". Errors say what went
wrong and what to do. No eyebrow labels over headings, no taglines.

**Avoid**, because they are what makes a UI read as generated: a dark
background with one neon accent, glow and halo effects, glass and blur,
gradients, Inter, emoji as icons, a rounded card with a shadow around
everything, centred landing-page layouts for tool screens, tracked uppercase
labels on everything, decorative numbering.

**Pitfalls already hit:**
- A pane that stacks a header, an editor and a strip or two needs a
  `min-height` on itself and `overflow: auto` on its container. Without both,
  a short request pane does not scroll — it draws the children on top of each
  other, and the result reads as a rendering bug rather than a tight fit. The
  GraphQL editor hit this at the 900×600 minimum. While you are there: put the
  reading that matters in the header as well, because the bottom of a short
  pane is the part nobody sees.
- `.silk` is for a *label*, never a value. It is styled as the name of a
  thing, quieter than the thing; a value in it looks like a caption for
  nothing.
- A component root class that matches a global class inherits its styles
  (`VarInput` with class `field` got a second border). Prefix variant classes
  (`is-field`).
- A function ref (`:ref="el => el.focus()"`) runs while the element's subtree
  is still being assembled, before it is in the document, and `focus()` on a
  detached input does nothing. The new-folder field sat there with the caret
  nowhere and everything typed went to the page. Focus in `onMounted`, from a
  template ref. (A function ref works for the rename field only because that
  input is patched into a row that is already on screen.)
- The updater's `Update` is a class with private fields. Put it in Pinia
  state as it is and Vue wraps it in a proxy, and `downloadAndInstall` then
  throws "Cannot read private member from an object whose class did not
  declare it" — so Install and restart did nothing, quietly, in 0.1.0 and
  0.2.0. It is kept with `markRaw`. Anything else with `#fields` gets the
  same treatment.
- A multi-root component (`UiMenuButton` is a button plus a menu) does not
  pass `class` through. It sets `inheritAttrs: false` and binds `$attrs` to
  the trigger; do the same in any primitive that renders more than one root.
- `display: flex` with `gap` on a paragraph splits its text around inline
  `<code>` into separate flex items. Wrap the text in one `<span>`.
- A `@container` block adds no specificity; put it after the base rules it
  overrides, or it silently loses.
- Never centre an absolutely positioned `.btn`/`.icon-btn` with
  `top: 50%; transform: translateY(-50%)` if the button has an `:active`
  transform of its own — pressing it drops it out from under the cursor.
  Centre with `top: 0; bottom: 0; margin: auto 0` instead.
- The response tab row holds three things — tabs, the reading, the tools —
  and at the 900px minimum with the sidebar open it is 588px wide. The
  container queries at the end of `ResponsePane.vue` drop the measurements,
  the tab counts, the status text and the Pretty/Raw switch in that order.
  Add something to that row and check it at 900px, or the tools fall off the
  right edge.
- Tree rows also carry the class `request`; select the request pane as
  `section.request`.

**Checking a UI change.** Look at it, in both themes, at the 900×600 minimum
window Tauri allows, including empty, loading, error and disabled states.
Overflow checks miss squeezing; screenshots do not.

## Status

Works: REST, GraphQL, WebSocket, server-sent events and gRPC — unary and every
kind of streaming; collections and folders with headers, auth and variables
inherited down the tree; environments with secret handling (add, rename,
delete, switch); query params; JSON/text/XML/form/urlencoded/binary bodies
including file fields in multipart; bearer, basic, API-key, digest, NTLM, AWS
SigV4 and client certificates, with OAuth 2.0 as a step that puts a token in
the environment; a response viewer with JSON highlighting, find, a timeline,
saved examples and a comparison against one; captures and checks, and a runner
that carries captured values forward — in the app and from a terminal with
`volt-run`; a cookie jar you can look inside; per-app and per-request send
settings (timeout, redirects, TLS, proxy); tabs; organising the tree, with a
delete that goes to a recoverable bin; searching the collection and a Ctrl+P
palette; keyboard shortcuts with a `?` sheet; bulk text editing of headers and
params; request history; import from Postman, Insomnia and OpenAPI/Swagger —
with the shapes a test script shares with volt read as captures and checks —
and export back to Postman; curl in both directions; code generation in seven
languages; a GraphQL schema browser with completion and marking; gRPC server
reflection for endpoints that describe themselves; a mock server that matches
on query and body as well as path; one HTML file of documentation; sync and
sharing through the collection's own git repository; workspaces; monitors that
run while the app is open; resizable panes; `{{variable}}` marking; and the
`{{$timestamp}}` / `{{$isoTimestamp}}` / `{{$guid}}` / `{{$randomInt}}` values
volt supplies.

There is nothing open. What volt deliberately will not do is anything that
needs it to run a service — accounts, a cloud copy of a collection, hosted
mock URLs, published documentation pages, checks that run while the app is
closed — because each of them has a local, file-shaped version above instead.

The codebase has been through a full bug, security and performance audit across
nine dimensions, with every finding adversarially re-checked before it was
acted on. What it turned up is in the conventions above; the short version is
that the things which bite are panics in commands (they crash the app rather
than raising an error), redaction paths that are applied to the local artefact
and not the committed one, and offsets that cross the IPC boundary in a
different unit than the one they are used in.

