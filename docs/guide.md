# The volt guide

Everything volt does, one section each. The [README](../README.md) is the short version.

**Contents**

- [Signing without a script](#signing-without-a-script)
- [Small things that add up](#small-things-that-add-up)
- [Writing a body](#writing-a-body)
- [Reading a response](#reading-a-response)
- [Coming from Postman](#coming-from-postman)
- [The file format](#the-file-format)
- [Variables](#variables)
- [Chaining](#chaining)
- [Inheriting](#inheriting)
- [Secrets](#secrets)
- [Checks and running](#checks-and-running)
- [Other protocols](#other-protocols)
- [Auth](#auth)
- [Certificates and proxies](#certificates-and-proxies)
- [Importing](#importing)
- [curl](#curl)
- [Cookies](#cookies)
- [Examples](#examples)
- [Code](#code)
- [Exporting](#exporting)
- [Mock server](#mock-server)
- [Documentation](#documentation)
- [Sync and sharing](#sync-and-sharing)
- [Workspaces](#workspaces)
- [Monitors](#monitors)
- [History](#history)

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

A `ws://` address makes the request a WebSocket as you type it. A **File**
body and a multipart file field each have a **Choose…** that picks the file,
kept relative when it is inside the collection so it works on a teammate's
machine too. History has a filter: words match the URL and name, `404`,
`5xx` and `error` match how it came back.

## Writing a body

The body editor colours JSON as you type it, marks every `{{variable}}` —
amber when nothing defines it — keeps the indent on Enter and closes the
bracket you opened. **Format** pretty-prints. It is still a plain text field
underneath: undo, selection and your input method work as they do anywhere.

## Reading a response

Pretty, Tree and Raw. In the tree, every row shows its path and a **Capture**
key that adds `$.data.token` to the request's captures, named after the field
— the next send fills it. An HTML response gets a **Preview**, rendered in a
sandbox that allows nothing. An image is drawn, with its size. The funnel
takes a path — `$.items[*].id`, `$..email` — and shows only what it matches.
The diff key compares this send with the previous one, line by line.

Typing `{{` anywhere a variable can go lists the names that exist — yours,
the collection's, and the ones volt supplies.

## Coming from Postman

Most of it is where you expect it. What differs, differs for a reason:

| In Postman | In volt |
| --- | --- |
| Collections in the sidebar | The same: the open one is the tree, the others are listed under it and a click opens one. Its tabs come back when you return. A *workspace* is a named group of them, under **Tools**. |
| Environment quick look | The environment menu at the top right: pick one, or **Manage environments…** |
| The method dropdown | The same, with WebSocket, SSE and gRPC under the methods — one picker for what a request is |
| Two-pane view | The pane key at the top right puts the response beside the request or under it |
| Runner, mock servers, monitors, cookies | All under **Tools** at the top right |
| Tests tab, `pm.test(...)` | **Tests** tab: a value from the response, a comparison, a value — no script. `volt-run` fails on them in CI. |
| `pm.environment.set(...)` | **Captures** tab: a path into the response, saved into the environment as a secret |
| Save as example | **Examples** on the response, kept as a file beside the request and redacted |
| Mock server (hosted) | Mock server on this machine, from the saved examples |
| Collection Runner, with a data file | **Run…** under Tools with **Data file…** (CSV or JSON, one pass per row), and the same from a terminal with `--data` |
| An exported environment file | Drop it on the window, or **Import…** it: it becomes an environment of the open collection, secrets kept out of the YAML |
| `{{$randomEmail}}` and the other `$random…` values | The same names, made up the same way |
| Duplicate, Ctrl+D | The same |
| Sync to the cloud | The collection is files in your repository; **Sync…** pulls, commits and pushes it |

Import a Postman export from the arrow beside **New**; the scripts it cannot
read are listed, line by line, rather than dropped.

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

A complete example lives in [`examples/sample-collection`](../examples/sample-collection).

Deleting a request or a folder moves it to a gitignored `.trash/` inside the
collection rather than removing it. The toast that says so offers **Undo**, and
**Bin** in the collection's own menu lists what is in there and puts any of it back.

## Variables

`{{name}}` is resolved from the active environment. Values may themselves
contain variables (resolved up to five levels deep). A name with no definition
is left visible in the request as `{{name}}` and reported in the response bar,
rather than being silently replaced with an empty string.

Some values are always defined without you doing anything: `{{$timestamp}}`
(Unix seconds), `{{$isoTimestamp}}`, `{{$guid}}` (also `{{$uuid}}`) and `{{$randomInt}}`. They are
worked out once per send, so two uses in the same request match. Define a
variable with one of those names and yours wins.

For test data there are Postman's made-up values under Postman's names:
`{{$randomFirstName}}`, `{{$randomLastName}}`, `{{$randomFullName}}`,
`{{$randomUserName}}`, `{{$randomEmail}}`, `{{$randomPhoneNumber}}`,
`{{$randomCity}}`, `{{$randomCountry}}`, `{{$randomStreetAddress}}`,
`{{$randomCompanyName}}`, `{{$randomWord}}`, `{{$randomLoremSentence}}`,
`{{$randomAlphaNumeric}}`, `{{$randomBoolean}}`, `{{$randomUUID}}`,
`{{$randomHexColor}}`, `{{$randomIP}}`, `{{$randomUrl}}`, `{{$randomPrice}}`,
`{{$randomDatePast}}` and `{{$randomDateFuture}}`. The email belongs to the
user name, which belongs to the name, so one request reads as one person.

## Chaining

Log in once. A request can carry *captures*: a variable name and where to read
it from the response — `status`, `header:Location`, `body`, or a path into a
JSON body such as `$.data.token`. After the send, the value goes into the
active environment, so the next request just uses `{{token}}`. Mark it secret
and it lands in the gitignored `.env` file rather than the committed YAML.

There is no scripting engine, on purpose: captures are three fields in the
request's YAML, so they read in a pull request like everything else.

## Inheriting

`collection.yaml` and each `folder.yaml` can carry headers, auth and variables.
Everything inside gets them, a request that sets the same header name wins, and
auth marked `inherit` is answered by the nearest folder that has an opinion. An
environment variable overrides one from a folder or the collection, which is
what makes switching environments useful.

## Secrets

An environment variable marked `secret: true` is written to a gitignored
dotenv file for that environment — `.env.prod` for `environments/prod.yaml` —
and only a name-with-empty-value placeholder goes into the environment YAML.
Committing a collection therefore never commits a token, and the same secret
can hold a different value in each environment.

**Manage environments…**, in the environment menu, lists every environment in the collection: switch between
them, add one (which starts from the variable names of the one you were
looking at, with blank values), rename, and delete. Deleting an environment
removes its YAML file and its secret values together, after asking.

volt checks that the collection's `.gitignore` covers these files every time it
writes one, and adds `.env.*` if it does not. Collections from earlier versions
kept every secret in a single shared `.env`; it is still read, and the first
time you save an environment its values are moved into per-environment files.

## Checks and running

A request can carry *checks*: what has to be true of the response. They read
the same places a capture does — `status`, `time`, `header:Name`, `body`, a
JSON path — and compare with `is`, `contains`, `matches` (a regular
expression), `exists`, `under` and friends.

**Run…** sends every request in a folder or the whole collection in order,
carrying captured values forward, and reports which checks failed. The same run
happens from a terminal:

```bash
volt-run ./my-api --env prod --stop-on-failure
```

It exits 0 when everything passed and 1 when something did not, so CI can use
it. Captured values live for the length of the run and are not written
anywhere: a CI job should not leave a token behind it.

A run can take a **data file**: a CSV with a header line, or a JSON array of
objects. The requests run once per row, with that row's columns as
`{{variables}}` — over the environment's, for that pass — and the report says
which row each result came from. Each pass starts clean, so one row's captures
cannot leak into the next.

```bash
volt-run ./my-api --env staging --data users.csv
```

WebSocket, SSE and gRPC requests in a folder are skipped by a run, and say so:
a run sends HTTP requests and judges them.

## Other protocols

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

A GraphQL **subscription** is sent the same way as a query — the key says
Subscribe — and each result arrives as a message in the pane a WebSocket
uses, over the `graphql-transport-ws` protocol.

**GraphQL** is a body kind rather than a protocol: a query and its variables,
sent as the JSON POST the specification describes. **Fetch schema** asks the
endpoint what it can do; after that the editor offers the fields that belong
where the cursor is, marks any the endpoint does not have, and can show the
whole schema beside the query. The schema is kept for the session only — it
belongs to the server, and a stale copy committed next to your requests would
be a lie waiting to be believed.

## Auth

Bearer, basic, API key, digest, NTLM, AWS SigV4, OAuth 1.0 and client
certificates. Digest and NTLM are answered after the server challenges; SigV4
and OAuth 1.0 (HMAC-SHA1 or HMAC-SHA256) sign the finished request, which is
why a copied cURL command works for as long as the signature lives.

**Get an OAuth token…** fetches an OAuth 2.0 token — client credentials,
authorization code with PKCE, password, device code, or a refresh — and puts
it in the environment as a secret variable. The device grant shows the code
to type and waits for you to approve it. It is not hidden inside an auth
object: requests use `{{token}}` like anything else, and you can see it,
reuse it and share it as a name. The form is remembered per collection; the
client secret and the password are not.

## Certificates and proxies

A server signed by an authority your machine does not know — a company's
own, a staging box — is trusted by naming that authority's PEM file under
**Trusted authorities** in Settings, or for one request in its Settings tab.
That is the alternative to turning verification off. A **client
certificate** is a PEM with its key, or a `.p12` / `.pfx` with its
passphrase (the passphrase stays in this app's settings and is never written
into a collection). Both, and the proxy — `http://`, `https://` or
`socks5://`, with credentials in the address if it wants them — apply to
every kind of request: HTTP, WebSocket, SSE and gRPC alike.

## Importing

**Drop a file on the window** to import it: a Postman, Insomnia or OpenAPI
export, a HAR file saved from a browser's network tab, or a Bruno collection
folder becomes a collection beside your personal one and opens; a volt
folder opens as it is.

A **HAR** file becomes one request per call, grouped by host, with the
browser's own headers and its cookies left out, repeats folded together and
images, fonts, scripts and stylesheets skipped. A **Bruno** collection (the
folder with `bruno.json` and `.bru` files) comes across with its folders,
environments, auth, bodies and docs; `assert` lines become tests where there
is an equivalent, and whatever has none is quoted in the report.

A Postman **environment** export is not a collection, and is not treated as
one: dropped on the window or imported, it becomes an environment of the
collection that is open, switched to, with `secret`-typed values and
credential-looking names kept in the gitignored file.

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

## curl

Paste a curl command into the URL field and it fills the whole request:
method, URL and query, headers, body and auth. Multi-line commands and a
browser's "Copy as cURL" work in both the bash and the Windows cmd form, and
**New request from cURL…** in a folder's menu creates a request from one.
Tokens, passwords, API keys and cookies in the command become secret variables
in the active environment rather than plain text in the request. Anything that
could not be applied — a proxy, a file the command would read, `-k` — is listed
under the request instead of being dropped quietly.

The other way, **Code → Copy as cURL** prints the request exactly as Send would
send it, with variables and inherited auth resolved. Secret values stay as
`{{name}}` so the command is safe to share; **Copy with secret values** gives a
command ready to run.

## Cookies

Cookies a response sets are sent back on the next request to that host, one jar
per collection. **Cookies…** under Tools shows what is being held, per
domain, and lets you drop one or all of them. Jars live in memory only — a
session cookie is a credential, and a collection is meant to be committable —
so closing volt signs you out.

## Examples

Keep a response beside its request — "this is what a 404 looks like here" — and
it is written under `.examples/`, reviewed and shared with the collection.
Secret values are replaced with their `{{name}}` first.

An example is also something to check against later. Compare the reading on
screen with one, and volt says what changed in *shape*: the status, the content
type, which paths appeared or vanished, and which hold a different kind of thing
now. Not a line diff — two responses differ on every line the moment an id
changes, and the question an example answers is whether the shape is still the
one you reviewed.

## Code

**Generate code…** prints the request as fetch, axios, Python requests, Go,
C#, PHP or Ruby, resolved exactly the way Send resolves it. Secrets stay as
placeholders unless you ask for them. What a snippet cannot carry — digest,
NTLM or SigV4 auth, a multipart or file body — is said under it, so it is not
mistaken for a copy that authenticates.

## Exporting

**Export for Postman** writes the collection as a Postman v2.1 file. What
Postman has no place for — collection-level headers, folder headers — is
flattened onto the requests rather than lost, and anything that could not come
across is listed. Environments are not exported: their values live outside the
collection and stay on your machine.

**Export as OpenAPI** writes an OpenAPI 3.0 document: a path per address, an
operation per request with its parameters and body as examples, and
responses built from the request's saved examples — so a collection that has
been used is most of a spec already.

## Mock server

**Mock server…** serves the responses you kept as examples, on
`http://127.0.0.1:<port>`, while volt is open. A path segment that came from a
variable matches anything, so `{{baseUrl}}/users/{{id}}` answers `/users/42`.
A request that gives a query parameter or a top-level JSON field a value of its
own makes it a condition too, so two requests on the same path are told apart;
the dialog shows each route's conditions beside it. Add `?example=Name` to ask
for a particular one — the 404 on purpose. It is on loopback only, and there is
no hosted URL, because there is no volt service to host one.

## Documentation

**Write documentation…** produces one self-contained HTML file: every request,
what it sends, and the examples kept beside it. It opens offline in any
browser, and it belongs in the repository next to the collection, where it is
reviewed like everything else. Nothing to publish, nothing to sign in to,
nothing that goes stale because someone forgot to press a button.

## Sync and sharing

A collection is files, so sharing it is your repository's job. **Sync…** shows
where the collection stands against it — branch, what changed, what is waiting
to push or pull — and does the three things worth a button: pull (fast-forward
only), commit, push. Every command is scoped to the collection folder, so a
collection inside a larger repository never sweeps up someone else's work, and
a commit is refused while `.gitignore` does not cover the `.env` files.

Sharing with someone new also writes `.env.<environment>.example`: the names
they have to fill in, with none of your values.

## Workspaces

A workspace is a named set of collection folders — the three APIs at work, or
the side project. It is a list of paths in the app's own settings, so nothing
is uploaded and the collections stay exactly where they are.

## Monitors

Watch a request on a schedule and hear about it when it fails; every run is
recorded in History like any other send. Monitors run while volt is open and
nowhere else — there is no server here to keep watching after you close the
app, and pretending otherwise would be worse than not offering it.

## History

Every request you send is recorded with its response, including failures.
History is kept in the app's own data directory, never in the collection, and
secret values are replaced with their `{{name}}` before anything is written.
Reopen an entry to look at it again, resend it, or save it as a new request.

## What volt will not do

Run a service of its own. No accounts, no cloud copy of your collections, no
hosted mock URLs or published documentation pages, no checks that run while
the app is closed. Not because they are bad ideas — because volt is a desktop
app over your own files, and the moment it needs a server to be useful it
stops being that. Each of them has a local, file-shaped version here already:
the mock server on loopback, documentation as one HTML file, sync through the
collection's own repository, and monitors that run while volt is open and say
so.

The one request volt makes on its own is an update check against this
repository's GitHub releases, which sends the build version and nothing about
you or your collections. It is a switch in Settings, and off means no request
at all.
