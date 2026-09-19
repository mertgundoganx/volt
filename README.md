# volt

An open source API client that keeps your collections as plain YAML files
inside your repository. No account, no cloud sync, no telemetry.

The one request volt makes on its own is an update check against this
repository's GitHub releases, which sends the build version and nothing about
you or your collections. It is a switch in Settings, and off means no request
at all.

Built with [Tauri](https://tauri.app) (Rust) and [Nuxt](https://nuxt.com).

## Why another one

Most API clients store your collections in their own cloud and hand you an
opaque blob if you want to export them. `volt` has no storage layer of its own:
a collection *is* a directory of YAML files. You review changes in a pull
request, you resolve conflicts in your editor, and the requests live next to
the code they exercise.

Requests are executed by the Rust process rather than the webview, so there are
no CORS restrictions, redirects and TLS behave the way `curl` does, and the
timings you see are real.

## How it looks

Like paper. Warm off-white surfaces, ink, hairline rules, and one colour —
ultramarine — kept for the things that act: the Send key, the request you are
on, a `{{variable}}`, the focus ring. Nothing glows. The method on a request
and the status on a response are stamped, a block of colour with the text cut
out of it. A warning is amber and a failure is red; blue is never a status.
There is a dark theme — the same paper at night, warm graphite rather than
blue-black — and two more light ones, Linen and Mist, all measured to the same
contrast floor. The choice lives in Settings and follows the OS until you make
one.

On first launch volt opens a collection of its own, kept under your documents
folder, so the first thing you can do is make a request. Opening a folder
inside a repository replaces it whenever you are ready.

## Signing without a script

What a Postman pre-request script is usually for — a signature, an
encoding — is a helper here, and a request that uses one still reads as a
request in its YAML:

```yaml
headers:
  - name: X-Signature
    value: "{{$hmacSha256(apiSecret, $body)}}"
```

`$body` is the body as it will be sent, with its own variables already
filled in. Arguments are variable names or `"quoted"` literals. The set is
fixed: `$base64`, `$sha256`, `$md5`, `$hmacSha256`, `$hmacSha256Base64`,
`$urlencode`, `$upper`, `$lower`, beside `$timestamp`, `$isoTimestamp`,
`$guid` (also `$uuid`) and `$randomInt`. A helper that cannot be computed —
an argument nobody defined, a call nobody knows — stays visible in the
request and is named in the warning, the way a missing variable is.

Run a single folder from its menu (**Run this folder…**), and paste a block
of `name=value` lines into an environment with **Bulk edit** — `# secret`
at the end of a line keeps that value out of the YAML.

## Small things that add up

The URL bar shows the query and fills the Params table when you paste one.
Open tabs come back after a restart, unsaved edits included. A send can be
cancelled from the same key. A tab shows the status of its last send. Errors
say what happened in words — "Could not connect to api.test: nothing is
listening there" — with the raw message underneath, and a refused
certificate offers to send without verifying, for that request only. An
undefined `{{name}}` in the warning is a link into the environment editor.
Search looks inside requests — headers, bodies, notes — not only at names.
Tabs drag, and a right-click closes the others. A curl command dropped on a
folder becomes a request there. A request can be copied, or moved, to
another collection from its menu. Large JSON folds. Shortcuts are one key
away (?) and a click away in Settings.

## Reading a response

Pretty, Tree and Raw. In the tree, every row shows its path and a **Capture**
key that adds `$.data.token` to the request's captures, named after the field
— the next send fills it. An HTML response gets a **Preview**, rendered in a
sandbox that allows nothing. The diff key compares this send with the
previous one, line by line.

Typing `{{` anywhere a variable can go lists the names that exist — yours,
the collection's, and the ones volt supplies.

## Coming from Postman

Most of it is where you expect it. What differs, differs for a reason:

| In Postman | In volt |
| --- | --- |
| Workspace → Collections in the sidebar | One collection open at a time; switch with the name in the top bar. A *workspace* here is a named group of collection folders, in the collection menu. |
| Environment quick look | The environment picker in the top bar; the pencil beside it edits values |
| Tests tab, `pm.test(...)` | **Tests** tab: a value from the response, a comparison, a value — no script. `volt-run` fails on them in CI. |
| `pm.environment.set(...)` | **Captures** tab: a path into the response, saved into the environment as a secret |
| Save as example | **Examples** on the response, kept as a file beside the request and redacted |
| Mock server (hosted) | Mock server on this machine, from the saved examples |
| Collection Runner | **Run…** in the collection menu, and the same from a terminal |
| Duplicate, Ctrl+D | The same |
| Sync to the cloud | The collection is files in your repository; **Sync…** pulls, commits and pushes it |

Import a Postman export from the collection menu; the scripts it cannot read
are listed, line by line, rather than dropped.

## The file format

```
my-api/
├── collection.yaml       # name, shared headers, auth and variables
├── .env.local            # secret values for local — gitignored
├── .env.prod             # secret values for prod — gitignored
├── .examples/            # kept example responses, mirroring the tree
│   └── users/list-users.yaml
├── environments/
│   ├── local.yaml
│   └── prod.yaml
├── auth/
│   ├── folder.yaml       # display name, ordering, and what this folder passes down
│   └── login.yaml
└── users/
    ├── list-users.yaml
    └── create-user.yaml
```

One request per file:

```yaml
name: List users
seq: 1
method: GET
url: '{{baseUrl}}/users'
params:
  - name: _limit
    value: '3'
    enabled: true
body:
  type: none
auth:
  type: inherit
captures:
  - name: firstId
    from: $.items[0].id
    enabled: true
options:
  timeout_ms: 5000
```

Lists are used instead of maps so duplicate headers survive a round-trip and
diffs stay line-by-line readable. `seq` controls ordering, so reordering a
request touches one number rather than rewriting the file.

A complete example lives in [`examples/sample-collection`](examples/sample-collection).

Deleting a request or a folder moves it to a gitignored `.trash/` inside the
collection rather than removing it. The toast that says so offers **Undo**, and
**Bin** in the collection menu lists what is in there and puts any of it back.

### Variables

`{{name}}` is resolved from the active environment. Values may themselves
contain variables (resolved up to five levels deep). A name with no definition
is left visible in the request as `{{name}}` and reported in the response bar,
rather than being silently replaced with an empty string.

Four values are always defined without you doing anything: `{{$timestamp}}`
(Unix seconds), `{{$isoTimestamp}}`, `{{$guid}}` and `{{$randomInt}}`. They are
worked out once per send, so two uses in the same request match. Define a
variable with one of those names and yours wins.


### Chaining

Log in once. A request can carry *captures*: a variable name and where to read
it from the response — `status`, `header:Location`, `body`, or a path into a
JSON body such as `$.data.token`. After the send, the value goes into the
active environment, so the next request just uses `{{token}}`. Mark it secret
and it lands in the gitignored `.env` file rather than the committed YAML.

There is no scripting engine, on purpose: captures are three fields in the
request's YAML, so they read in a pull request like everything else.

### Inheriting

`collection.yaml` and each `folder.yaml` can carry headers, auth and variables.
Everything inside gets them, a request that sets the same header name wins, and
auth marked `inherit` is answered by the nearest folder that has an opinion. An
environment variable overrides one from a folder or the collection, which is
what makes switching environments useful.

### Secrets

An environment variable marked `secret: true` is written to a gitignored
dotenv file for that environment — `.env.prod` for `environments/prod.yaml` —
and only a name-with-empty-value placeholder goes into the environment YAML.
Committing a collection therefore never commits a token, and the same secret
can hold a different value in each environment.

**Environments** lists every environment in the collection: switch between
them, add one (which starts from the variable names of the one you were
looking at, with blank values), rename, and delete. Deleting an environment
removes its YAML file and its secret values together, after asking.

volt checks that the collection's `.gitignore` covers these files every time it
writes one, and adds `.env.*` if it does not. Collections from earlier versions
kept every secret in a single shared `.env`; it is still read, and the first
time you save an environment its values are moved into per-environment files.


### Checks and running

A request can carry *checks*: what has to be true of the response. They read
the same places a capture does — `status`, `time`, `header:Name`, `body`, a
JSON path — and compare with `is`, `contains`, `exists`, `under` and friends.

**Run…** sends every request in a folder or the whole collection in order,
carrying captured values forward, and reports which checks failed. The same run
happens from a terminal:

```bash
volt-run ./my-api --env prod --stop-on-failure
```

It exits 0 when everything passed and 1 when something did not, so CI can use
it. Captured values live for the length of the run and are not written
anywhere: a CI job should not leave a token behind it.

### Other protocols

A request has a kind: **HTTP**, **WebSocket**, **SSE** or **gRPC**. The URL,
headers and auth resolve the same way for all of them.

A WebSocket or SSE request opens a connection and shows what arrives as it
arrives; a WebSocket can be typed into. A gRPC request points at a `.proto`,
which is read to find out what can be called, and sends a JSON message that is
turned into protobuf. **Ask the server** instead if the endpoint offers
reflection — then there is no file to point at at all.

Streaming methods work in every direction. A server-streaming call shows each
message as it lands, a client-streaming one keeps the request open so you can
send more and then say **Done sending**, and a bidirectional one does both, on
the same pane a WebSocket uses. volt speaks HTTP/2 itself for this, which is
also how it reads the `grpc-status` that follows a body — the place a gRPC call
actually says whether it worked.

**GraphQL** is a body kind rather than a protocol: a query and its variables,
sent as the JSON POST the specification describes. **Fetch schema** asks the
endpoint what it can do; after that the editor offers the fields that belong
where the cursor is, marks any the endpoint does not have, and can show the
whole schema beside the query. The schema is kept for the session only — it
belongs to the server, and a stale copy committed next to your requests would
be a lie waiting to be believed.

### Auth

Bearer, basic, API key, digest, NTLM, AWS SigV4 and client certificates.
Digest and NTLM are answered after the server challenges; SigV4 signs the
finished request, which is why a copied cURL command works for as long as the
signature lives.

**Get an OAuth token…** fetches one — client credentials, authorization code
with PKCE, or a refresh — and puts it in the environment as a secret variable.
It is not hidden inside an auth object: requests use `{{token}}` like anything
else, and you can see it, reuse it and share it as a name.

### Importing

**Drop a file on the window** to import it: a Postman, Insomnia or OpenAPI
export becomes a collection beside your personal one and opens; a folder
opens as it is.

**Import…** converts a Postman (Collection v2.0 / v2.1), Insomnia (v4 JSON or
v5 YAML) or OpenAPI/Swagger (3.x or 2.0, JSON or YAML) file into a new
collection folder. A folder keeps its own auth and headers, so what a request
inherits still reads the way it did, and Insomnia's `{{ _.name }}` becomes
`{{name}}`.

Test scripts are read as far as they mean something volt already has:
`pm.environment.set("token", …)` becomes a capture and `pm.test("status is
200", …)` becomes a check. Nothing beyond that is guessed at — no control flow,
no computed values — and every line that was not understood is quoted back in
the report, so you can see exactly what you still have to do by hand.

Exports often contain live tokens in plain text. The importer moves literal
bearer tokens, passwords, API keys and credential-looking headers and fields
into secret variables, so the imported YAML is safe to commit. Anything that
could not be converted — file uploads, gRPC and WebSocket requests, unsupported
auth types — is listed in a report at the end.

### curl

Paste a curl command into the URL field and it fills the whole request:
method, URL and query, headers, body and auth. Multi-line commands and a
browser's "Copy as cURL" work in both the bash and the Windows cmd form, and
**New request from cURL…** in a folder's menu creates a request from one.
Tokens, passwords, API keys and cookies in the command become secret variables
in the active environment rather than plain text in the request. Anything that
could not be applied — a proxy, a file the command would read, `-k` — is listed
under the request instead of being dropped quietly.

The other way, **cURL → Copy as cURL** prints the request exactly as Send would
send it, with variables and inherited auth resolved. Secret values stay as
`{{name}}` so the command is safe to share; **Copy with secret values** gives a
command ready to run.


### Cookies

Cookies a response sets are sent back on the next request to that host, one jar
per collection. **Cookies** in the sidebar shows what is being held, per
domain, and lets you drop one or all of them. Jars live in memory only — a
session cookie is a credential, and a collection is meant to be committable —
so closing volt signs you out.

### Examples

Keep a response beside its request — "this is what a 404 looks like here" — and
it is written under `.examples/`, reviewed and shared with the collection.
Secret values are replaced with their `{{name}}` first.

An example is also something to check against later. Compare the reading on
screen with one, and volt says what changed in *shape*: the status, the content
type, which paths appeared or vanished, and which hold a different kind of thing
now. Not a line diff — two responses differ on every line the moment an id
changes, and the question an example answers is whether the shape is still the
one you reviewed.

### Code

**Generate code…** prints the request as fetch, axios, Python requests, Go,
C#, PHP or Ruby, resolved exactly the way Send resolves it. Secrets stay as
placeholders unless you ask for them.

### Exporting

**Export for Postman** writes the collection as a Postman v2.1 file. What
Postman has no place for — collection-level headers, folder headers — is
flattened onto the requests rather than lost, and anything that could not come
across is listed. Environments are not exported: their values live outside the
collection and stay on your machine.


### Mock server

**Mock server…** serves the responses you kept as examples, on
`http://127.0.0.1:<port>`, while volt is open. A path segment that came from a
variable matches anything, so `{{baseUrl}}/users/{{id}}` answers `/users/42`.
A request that gives a query parameter or a top-level JSON field a value of its
own makes it a condition too, so two requests on the same path are told apart;
the dialog shows each route's conditions beside it. Add `?example=Name` to ask
for a particular one — the 404 on purpose. It is on loopback only, and there is
no hosted URL, because there is no volt service to host one.

### Documentation

**Write documentation…** produces one self-contained HTML file: every request,
what it sends, and the examples kept beside it. It opens offline in any
browser, and it belongs in the repository next to the collection, where it is
reviewed like everything else. Nothing to publish, nothing to sign in to,
nothing that goes stale because someone forgot to press a button.

### Sync and sharing

A collection is files, so sharing it is your repository's job. **Sync…** shows
where the collection stands against it — branch, what changed, what is waiting
to push or pull — and does the three things worth a button: pull (fast-forward
only), commit, push. Every command is scoped to the collection folder, so a
collection inside a larger repository never sweeps up someone else's work, and
a commit is refused while `.gitignore` does not cover the `.env` files.

Sharing with someone new also writes `.env.<environment>.example`: the names
they have to fill in, with none of your values.

### Workspaces

A workspace is a named set of collection folders — the three APIs at work, or
the side project. It is a list of paths in the app's own settings, so nothing
is uploaded and the collections stay exactly where they are.

### Monitors

Watch a request on a schedule and hear about it when it fails; every run is
recorded in History like any other send. Monitors run while volt is open and
nowhere else — there is no server here to keep watching after you close the
app, and pretending otherwise would be worse than not offering it.

### History

Every request you send is recorded with its response, including failures.
History is kept in the app's own data directory, never in the collection, and
secret values are replaced with their `{{name}}` before anything is written.
Reopen an entry to look at it again, resend it, or save it as a new request.

## Development

Requires [Rust](https://rustup.rs) and Node 20+ with pnpm.

There are no prebuilt binaries yet — the first tagged release will attach
installers for macOS, Linux and Windows.

```bash
pnpm install
pnpm app          # run the desktop app in development
pnpm app:build    # produce a release bundle
pnpm test         # Rust test suite
pnpm typecheck    # Vue + TypeScript

# Run a collection from a terminal, the way CI would
cargo run --manifest-path src-tauri/Cargo.toml --bin volt-run -- ./my-api
```

The Rust side has 239 tests — the auth schemes are checked against their
published vectors, and the mock server, the WebSocket, SSE and gRPC paths are
tested over real loopback sockets rather than mocks. The UI is driven in a real
browser with Playwright.

[CONTRIBUTING.md](CONTRIBUTING.md) has the rest: the toolchain notes, how the
UI is verified in a real browser, and the pins that are load-bearing.
[ARCHITECTURE.md](ARCHITECTURE.md) is the "why" — the conventions each part
follows and what went wrong to put them there. Read it before changing anything
structural.

## Layout

| Path | What lives there |
| --- | --- |
| `src-tauri/src/model.rs` | The on-disk file format |
| `src-tauri/src/collection.rs` | Reading and writing a collection directory |
| `src-tauri/src/http.rs` | Resolving and sending a request |
| `src-tauri/src/curl.rs` | curl import and export |
| `src-tauri/src/vars.rs` | `{{variable}}` interpolation |
| `src-tauri/src/secrets.rs` | Per-environment secret files |
| `src-tauri/src/history.rs` | Request history |
| `src-tauri/src/import.rs` | Postman and Insomnia import |
| `src-tauri/src/capture.rs` | Values taken out of a response |
| `src-tauri/src/checks.rs` | Assertions on a response |
| `src-tauri/src/runner.rs` | Running a folder or a collection |
| `src-tauri/src/openapi.rs` | OpenAPI and Swagger import |
| `src-tauri/src/aws.rs`, `digest.rs`, `ntlm.rs`, `oauth.rs` | The auth schemes that are not a header |
| `src-tauri/src/stream.rs` | WebSocket and server-sent events |
| `src-tauri/src/grpc.rs` | gRPC from a `.proto`: unary and streaming |
| `src-tauri/src/wire.rs` | HTTP/2 itself — frames and trailers |
| `src-tauri/src/reflection.rs` | Asking a server what it serves |
| `src-tauri/src/graphql.rs` | A GraphQL schema, and what a query asks it for |
| `src-tauri/src/scripts.rs` | Captures and checks out of an imported script |
| `src-tauri/src/codegen.rs` | Printing a request as code |
| `src-tauri/src/cookies.rs` | The cookie jar |
| `src-tauri/src/examples.rs` | Saved example responses |
| `src-tauri/src/export.rs` | Export to Postman |
| `src-tauri/src/docs.rs` | Documentation as one HTML file |
| `src-tauri/src/git.rs` | Sync through the collection's repository |
| `src-tauri/src/mock.rs` | The mock server |
| `src-tauri/src/lib.rs` | The commands the UI can call |
| `app/` | Nuxt SPA — components, Pinia store, types |

`app/types.ts` mirrors `src-tauri/src/model.rs`; change both together.

## Status

Early, but wide. REST, GraphQL, WebSocket, server-sent events and gRPC —
unary and every kind of streaming, with server reflection for endpoints that
describe themselves. Collections and folders that pass headers, auth and
variables down to what is inside them; environments with secret handling;
bearer, basic, API-key, digest, NTLM, AWS SigV4 and client certificates, with
OAuth 2.0 as a step that puts a token in the environment; JSON / text / XML /
form / urlencoded / binary bodies with file fields in multipart; a GraphQL
schema browser with completion and marking; a response viewer you can search,
save from and read as a timeline; kept example responses, and a comparison
against one; captures and checks, with a runner in the app and on the command
line; a cookie jar you can look inside; send settings per app and per request;
tabs; a delete that goes to a recoverable bin; searching the collection;
keyboard shortcuts; request history; import from Postman, Insomnia and OpenAPI
— with the shapes a test script shares with volt read as captures and checks —
and export back to Postman; curl in both directions; code generation for seven
languages; a mock server that matches on query and body as well as path;
documentation as one HTML file; sync through the collection's own git
repository; workspaces; and monitors.

Besides the variables you define, `{{$timestamp}}`, `{{$isoTimestamp}}`,
`{{$guid}}` (or `{{$uuid}}`) and `{{$randomInt}}` are always available. Each is worked out once
per send, so two uses in one request agree with each other.

Renaming changes the `name:` in the file rather than the filename, so a request
keeps its path and its git history, and the diff stays one line.

What it deliberately will not do: run a service of its own. No accounts, no
cloud copy of your collections, no hosted mock URLs or published documentation
pages, no checks that run while the app is closed.
Not because they are bad ideas — because volt is a desktop app over your own
files, and the moment it needs a server to be useful it stops being that. Each
one of them has a local, file-shaped version here already: the mock server on
loopback, documentation as one HTML file, sync through the collection's own
repository, and monitors that run while volt is open and say so.

## License

MIT — see [LICENSE](LICENSE).

The bundled IBM Plex Sans and IBM Plex Mono typefaces are under the SIL Open Font
License 1.1; their notices are in [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).
