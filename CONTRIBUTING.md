# Contributing

volt is a Tauri 2 (Rust) desktop app with a Nuxt 4 SPA inside the webview.
The code is organised and argued for in [ARCHITECTURE.md](ARCHITECTURE.md) —
read the conventions there before changing anything structural, because most of
them exist because something went wrong once.

## Proposing something

Open an issue describing the problem before the solution — what you were trying
to do, and what volt made you do instead.

Two rules apply to anything that lands, and they are not negotiable:

- **Secrets never reach a YAML file or the history.** If a change touches a
  credential, say in the issue where the value lives and who can read it.
- **Anything that changes how a request goes out belongs in `http::plan`**, so
  that Send, "Copy as cURL" and a generated snippet cannot drift apart.

What volt will not take is anything that needs it to run a service of its own:
accounts, a cloud copy of a collection, hosted mock URLs, published
documentation pages, checks that run while the app is closed. Each has a
local, file-shaped version already, and that is the trade the app is built on.

## Commands

```bash
pnpm install      # dependencies
pnpm app          # run the desktop app (Tauri + Nuxt dev server)
pnpm app:build    # release bundle
pnpm dev          # Nuxt alone in a browser — no Tauri, so every invoke() fails
pnpm test         # Rust test suite
pnpm typecheck    # vue-tsc over the whole app
```

`pnpm test` runs `cargo test`. Network tests are `#[ignore]`d; run them with
`cargo test --manifest-path src-tauri/Cargo.toml -- --ignored`.

## Local toolchain notes (Windows)

- `pnpm` is installed through the user-level npm prefix
  (`%APPDATA%\npm`), not corepack — corepack needs admin to write into
  `C:\Program Files\nodejs`.
- Build-script approvals live in `pnpm-workspace.yaml` under `allowBuilds`.
  pnpm 12 ignores the old `pnpm.onlyBuiltDependencies` field in `package.json`.
- Rust needs the MSVC linker from Visual Studio Build Tools (C++ workload). Git
  Bash ships an unrelated `link.exe` that shadows it; if a build fails with
  `link: extra operand`, the MSVC toolchain is missing or not on PATH.
- TypeScript is pinned to `~5.9.3`. TypeScript 7 (the native port) dropped the
  `./lib/tsc` export that `vue-tsc` 3.x requires, so `pnpm typecheck` crashes on
  it. Revisit when vue-tsc ships TS 7 support.
- Tauri stays on 2.x. 3.0 is alpha only.
- The minimum Rust version in `Cargo.toml` is 1.88, which is what the
  dependency tree asks for rather than what volt's own code needs. CI builds
  on exactly that version, so the claim cannot rot quietly.
- `default-run = "volt"` in `src-tauri/Cargo.toml` is load-bearing. `tauri dev`
  shells out to a bare `cargo run`, and with `volt-run` beside it that is
  ambiguous — `pnpm app` fails with "could not determine which binary to run".
  Adding a third binary means keeping that key correct.
- `reqwest` is on 0.13, which renamed `rustls-tls` to `rustls` and split `form`
  (needed by `builder.form()` for urlencoded bodies) into its own feature. The
  crate is built with `default-features = false`, so a newly used part of the
  reqwest API usually means adding a feature rather than fixing code.
- `getrandom` is a direct dependency for the OS random source. It was already
  in the tree through rustls, so it costs nothing in the binary.
- `h2`, `http`, `bytes`, `rustls`, `tokio-rustls` and `webpki-roots` are named
  in `Cargo.toml` for `wire.rs`, but reqwest already brings every one of them
  into the binary — naming them costs nothing but pins the versions this code
  was written against. Keep `rustls` on the same version reqwest resolves to.
  Both `ring` and `aws-lc-rs` end up enabled on it, which is why
  `wire::ensure_crypto_provider` exists — see the convention above. A test in
  `wire.rs` builds a `ClientConfig`, so a dependency bump that breaks this
  fails a test rather than every TLS connection at runtime.
- The gRPC trio — `protox`, `prost-reflect` and `prost` — has to resolve to one
  `prost-types`. They were pinned apart for a while because a mismatched pair
  pulled in two of it; `cargo tree -d` is how you check after bumping any of
  them.
- RustCrypto is on the `digest` 0.11 line (`sha2` 0.11, `hmac` 0.13, `md-5` and
  `md4` 0.11). `Mac::new_from_slice` moved behind the `KeyInit` trait there, so
  `aws.rs` and `ntlm.rs` import it explicitly. The AWS SigV4 published vector
  and the RFC 7616 Digest example are both tests, so a bump that changes what
  the hashes produce fails loudly rather than in the field.
- YAML goes through `serde_yaml_ng` (a maintained fork of the now-deprecated
  `serde_yaml`). `collection.rs` imports it as `use serde_yaml_ng as yaml;` and
  is the only file that names it, so swapping the backend again is a one-line
  change. Code elsewhere that needs to read YAML (the Insomnia v5 importer) goes
  through `collection::yaml_value` to keep it that way. The fork was verified to
  emit byte-identical YAML across every `Body` and `Auth` variant before the
  switch — if you ever swap it again, prove that
  first, because these files are the user's committed file format and a
  reformat would show up as churn in their diffs.

## Continuous integration

`.github/workflows/ci.yml` runs on every push to `main` and every pull request:

| Job | What it runs |
| --- | --- |
| **Rust** | `cargo test` and `cargo clippy -- -D warnings`, on Ubuntu and Windows |
| **Frontend** | `pnpm install --frozen-lockfile`, `pnpm typecheck`, `pnpm generate` |
| **Minimum Rust version** | `cargo build --locked` on the exact `rust-version` in `Cargo.toml` |

Two jobs on Rust is deliberate. Windows is where volt is developed; Ubuntu is
where the path handling and the loopback socket tests are most likely to
disagree with it. A collection id such as `C:x.yaml` is a plain filename to
Linux and a drive letter to Windows, and an id written on one goes into a file
that is opened on the other — so `collection::resolve` refuses it on both, and
the matrix is what proves that stays true.

No network is needed: the tests that reach the internet are `#[ignore]`d. The
mock server, WebSocket, SSE and gRPC tests bind real loopback sockets instead.

**There is no `cargo fmt` check, on purpose.** The Rust here is hand-wrapped to
sit beside prose comments that explain *why*, and rustfmt at any width wants to
rewrite several hundred places. Match the surrounding style rather than running
the formatter over a file.

## Releasing

`.github/workflows/release.yml` builds the installers. Push a tag and it does
the rest:

```bash
# Bump all three manifests to the same number first: src-tauri/tauri.conf.json,
# package.json and src-tauri/Cargo.toml.
git tag v0.2.0
git push origin v0.2.0
```

What happens then:

1. **The version is checked** against all three manifests. A `v0.2.0` tag on a
   `0.1.0` manifest stops here, naming the file that disagrees — otherwise the
   release is a download whose version is a lie.
2. **The tests run**, so nothing is published that would not have passed CI.
   The bundle step compiles everything anyway; this is here for the tests,
   which it does not run.
3. **Four bundles are built** — macOS on Apple Silicon and Intel, Linux, and
   Windows — and attached to a **draft** release. Draft, so the notes can be
   written and the artefacts checked before anyone can download them. Publish
   it from the GitHub UI when it looks right.

Linux builds on `ubuntu-22.04` rather than the newest image on purpose: an
AppImage linked against a newer glibc will not start on the distributions
people are actually running.

**Nothing is code-signed.** There are no certificates in this repository, so
Windows SmartScreen and macOS Gatekeeper will warn whoever downloads a build,
and the release notes say so. Signing means adding the secrets Tauri documents
and the matching inputs to the `tauri-action` step; it is a decision left open,
not an oversight.

### The updater

Installed copies of volt check GitHub for a newer release on start and offer it
from Settings. Three things have to line up for that to work.

**0. One installer format on Windows.** `bundle.targets` names `nsis` and
not `msi`. With both built, `latest.json` pointed the updater at the MSI while
people had installed the NSIS `-setup.exe`, and an update would have put a
second, per-machine copy beside the first, asking for administrator rights on
the way. The NSIS installer updates the per-user install in place and asks
for nothing.

**1. The endpoint.** `plugins.updater.endpoints` in `src-tauri/tauri.conf.json`
points at this repository's releases. A fork has to change it to its own, or
its builds will go on offering this project's releases to people running a
different binary. A wrong endpoint fails silently by design, so this is the one
setting that will not tell you it is wrong.

**2. The signing key.** The updater refuses an unsigned release, so the release
workflow needs two repository secrets:

| Secret | What it is |
| --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | the contents of the private key file |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | the password it was generated with |

The matching public key is already in `tauri.conf.json`. It is safe to commit —
that is the half that verifies. **The private key is not in this repository and
must never be**: whoever holds it can sign an update that every installed copy
will accept and install without asking. Keep a backup somewhere you would keep
a password, because losing it means no existing installation can ever be
updated again; they would all have to be reinstalled by hand.

To make a new pair:

```bash
pnpm tauri signer generate -w ~/.tauri/volt.key
```

**3. Publishing.** The workflow creates a **draft** release. A draft is not
served at `/releases/latest/`, so nothing reaches anybody's updater until it is
published from the GitHub UI. That is the point: the artefacts get looked at
before an app on someone else's machine offers to install them.

Only installed builds update themselves — NSIS and MSI on Windows, the `.app`
on macOS, the AppImage on Linux. `.deb` and `.rpm` belong to the system package
manager and are left alone.

## Verifying UI work

Tauri's WebView2 cannot be scripted, but the frontend is a plain SPA: run
`pnpm dev` and drive it with Playwright, replacing the one IPC entry point —
`window.__TAURI_INTERNALS__.invoke` — via `addInitScript`. Everything above
that line is the real store and the real components. Two things to know if you
write such a script: `plugin:store|get` returns a `[value, exists]` tuple,
`plugin:store|load` returns a resource id, and with no `recentCollections`
seeded `restore()` asks `default_collection` and opens whatever
`open_collection` answers — seed `failing: ['default_collection']` to reach the
no-collection state. Playwright's
mouse API does not raise HTML5 drag events, so dispatch `dragstart`/`dragover`/
`drop` yourself with a shared `DataTransfer`, and wait a tick before reading
classes back — Vue flushes the DOM asynchronously. Read the port off the dev
server rather than assuming 3000: if something else already has it, Nuxt takes
3001 without saying much, and every locator then times out against somebody
else.

More stub rules learned the hard way:
- Return a fresh copy from every stubbed invoke (`JSON.parse(JSON.stringify(…))`).
  Real IPC serializes; handing back the same array twice means Vue sees no
  change and the UI looks broken when it is not.
- Answer every command the store calls on load — currently `open_collection`
  and `list_history` — with the right shape. `null` for a list crashes the
  sidebar, which real Rust never returns.
- Dialogs: `ask` arrives as `plugin:dialog|message` and is confirmed by
  returning the `okLabel` string; `open` arrives as `plugin:dialog|open`.
- `page.reload()` re-runs `addInitScript`, which resets `window.__handlers`.
  Anything the script overrode before the reload is silently gone and the stub's
  defaults answer instead — seed the scenario through `launch()` rather than
  reloading.
- Match a label case-insensitively anyway. `.silk` no longer uppercases, but
  a test that survives a change of case is a test that survives a redesign.
- Playwright's `keyboard.press('Shift+/')` arrives as `key: '/'`, not `'?'`.
  Press the character itself (`press('?')`) to test a punctuation shortcut, and
  blur first: a binding without `mod` is ignored while a field has focus.
- A Vue interpolation cannot contain `{{`, so building `{{name}}` inline in a
  template is a compile error the dev server reports but `pnpm typecheck` does
  not. Put it in a helper in the script block, as `RequestPane` and
  `CodeDialog` do.
- Nuxt's component scanner does not always pick up a `.vue` file created while
  the dev server is running: the component renders as an unknown element and
  the log says "Failed to resolve component". Restart `pnpm dev`.
- A heredoc through this tooling collapses a doubled backslash into one, so
  Rust or JS source written that way loses an escape. Where a file needs a
  literal backslash, write it as a unicode escape or splice that line in on
  its own.
- The harness can deliver Tauri events: `listen()` arrives as
  `plugin:event|listen` with the callback itself as `args.handler` (the stub's
  `transformCallback` is the identity), so it is kept in `window.__listeners`
  and `window.__emit(name, payload)` pushes one the way Rust would. That is how
  the stream pane is tested without a socket.
- `getByRole('tab', { name: … })` matches a substring of the accessible name,
  so `Body` also matches the response's `Body json`, and `Inherit` matches the
  request tab's `AUTH inherit`. Scope the locator to the pane
  (`section.request [role="tab"]`) or match on the exact text.
- `UiSegmented` renders `role="radio"`, not `role="tab"`. A test that picks a
  segment has to say so.

## The update, end to end

`tests/e2e/update.mjs` is the updater with nothing stubbed: a debug build that
calls itself an older version is pointed at a local `latest.json` naming a
published installer and its signature, driven to Settings → Install and
restart, and the per-user install in `%LOCALAPPDATA%\Volt` is expected to
change version. It needs the same driver setup as the test below, a
`volt` installed from the NSIS installer of an older release, and a build
made with a config override:

```bash
# override.json: {"version":"0.1.9","plugins":{"updater":{"endpoints":["http://127.0.0.1:8765/latest.json"],"dangerousInsecureTransportProtocol":true}}}
pnpm tauri build --debug --no-bundle --config override.json
EDGEDRIVER=path/to/msedgedriver.exe node tests/e2e/update.mjs
```

Two things it taught: keep the WebDriver session open until the version on
disk changes, because closing it kills the app mid-download; and see the
`markRaw` note in ARCHITECTURE — the Install key was broken from 0.1.0 to
0.2.0 and this test is what found it. Run it before a release that touches
the updater, the store, or the settings dialog.

## The real thing, end to end

The Playwright suites replace the Rust side with a stub, and the Rust tests
never see the UI. One test does neither: `tests/e2e/drive.mjs` drives the built
`volt.exe` over WebDriver, through `tauri-driver`, opening a throwaway
collection, making a request from the sidebar, sending it across the real
network and reading the status back off the screen. Run it when a change
touches the seam between the two — IPC payloads, the URL bar, Send.

```bash
cargo install tauri-driver --locked
# msedgedriver has to match the installed WebView2 (see the version under
# "C:Program Files (x86)MicrosoftEdgeWebViewApplication"):
#   https://msedgedriver.microsoft.com/<version>/edgedriver_win64.zip
pnpm tauri build --debug --no-bundle     # src-tauri/target/debug/volt.exe with the SPA embedded
EDGEDRIVER=path/to/msedgedriver.exe node tests/e2e/drive.mjs
```

The test hands volt a throwaway collection as an argument, which is what
`volt <folder>` does for anyone: `startup_collection` opens it instead of
the last one, so the test never touches a collection of yours. (A WebDriver
passes every argument on as a `--switch`, which is why leading dashes are
dropped before the path is checked.) It needs no package beyond Node itself.

## The command line

`volt-run` runs a collection from a terminal and exits non-zero when a check
fails, which is what makes checks worth writing:

```bash
cargo run --bin volt-run -- ./my-api --env prod --stop-on-failure
```

It is `volt_lib::runner::cli`, the same code the app's runner uses, so a run in
CI and a run in the app cannot drift apart. It is the only `pub mod` in the
library; everything else stays private on purpose.
