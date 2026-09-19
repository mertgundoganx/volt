mod aws;
mod capture;
mod checks;
mod codegen;
mod collection;
mod cookies;
mod curl;
mod digest;
mod docs;
mod error;
mod git;
mod examples;
mod export;
mod history;
mod graphql;
mod grpc;
mod http;
mod import;
mod mock;
mod model;
mod oauth;
mod ntlm;
mod openapi;
mod reflection;
pub mod runner;
mod scripts;
mod secrets;
mod stream;
mod wire;
mod vars;

use std::collections::HashMap;
use std::path::PathBuf;

use error::Result;
use model::{Environment, EnvVar, Request};

#[tauri::command]
fn open_collection(path: String) -> Result<collection::Collection> {
    collection::load(&PathBuf::from(path))
}

#[tauri::command]
fn init_collection(path: String, name: String) -> Result<collection::Collection> {
    collection::init(&PathBuf::from(path), &name)
}

/// The collection volt falls back to on start: a folder of its own under the
/// user's documents, made the first time it is asked for.
#[tauri::command]
fn default_collection(app: tauri::AppHandle) -> Result<String> {
    use tauri::Manager as _;
    let base = app
        .path()
        .document_dir()
        .or_else(|_| app.path().home_dir())
        .map_err(|e| error::Error::Invalid(format!("no folder to keep a collection in: {e}")))?;
    let root = collection::ensure_default(&base)?;
    Ok(root.to_string_lossy().into_owned())
}

/// Convert a Postman or Insomnia export into a new collection directory
/// created inside `into`.
#[tauri::command]
fn import_collection(source: String, into: String) -> Result<import::Outcome> {
    import::import_file(&PathBuf::from(source), &PathBuf::from(into))
}

#[tauri::command]
fn get_request(root: String, id: String) -> Result<Request> {
    collection::read_request(&PathBuf::from(root), &id)
}

#[tauri::command]
fn save_request(root: String, id: String, request: Request) -> Result<()> {
    collection::write_request(&PathBuf::from(root), &id, &request)
}

/// `parent` is `None` for the collection root. Returns the new folder's id.
#[tauri::command]
fn create_folder(root: String, parent: Option<String>, name: String) -> Result<String> {
    collection::create_folder(&PathBuf::from(root), parent.as_deref(), &name)
}

/// Delete a request or folder. It goes to the collection's `.trash/`, so it
/// can be put back; `empty_trash` is what actually removes anything.
#[tauri::command]
fn delete_node(root: String, id: String) -> Result<collection::Deleted> {
    collection::delete_node(&PathBuf::from(root), &id)
}

#[tauri::command]
fn restore_node(root: String, deleted: collection::Deleted) -> Result<()> {
    collection::restore_node(&PathBuf::from(root), &deleted)
}

#[tauri::command]
fn list_trash(root: String) -> Result<Vec<collection::Deleted>> {
    collection::list_trash(&PathBuf::from(root))
}

#[tauri::command]
fn empty_trash(root: String) -> Result<()> {
    collection::empty_trash(&PathBuf::from(root))
}

#[tauri::command]
fn rename_node(root: String, id: String, name: String) -> Result<()> {
    collection::rename_node(&PathBuf::from(root), &id, &name)
}

/// `new_parent` is `None` for the collection root. Returns the node's new id.
#[tauri::command]
fn move_node(root: String, id: String, new_parent: Option<String>) -> Result<String> {
    collection::move_node(&PathBuf::from(root), &id, new_parent.as_deref())
}

/// Number the children of `parent` so they sort in the order supplied.
#[tauri::command]
fn reorder(root: String, parent: Option<String>, ordered: Vec<String>) -> Result<()> {
    collection::reorder(&PathBuf::from(root), parent.as_deref(), &ordered)
}

/// `previous` is the name the environment was loaded under, or `None` for a new
/// one; it is how a rename moves files instead of duplicating them.
#[tauri::command]
fn save_environment(root: String, environment: Environment, previous: Option<String>) -> Result<()> {
    collection::write_environment(&PathBuf::from(root), &environment, previous.as_deref())
}

#[tauri::command]
fn delete_environment(root: String, name: String) -> Result<()> {
    collection::delete_environment(&PathBuf::from(root), &name)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SendOutcome {
    response: http::HttpResponse,
    /// `None` if history could not be written; the send itself still succeeded.
    history: Option<history::Summary>,
    /// Values the request's captures took out of the response, for the UI to
    /// put in the active environment. Rust does not write them itself: which
    /// environment is open is the UI's business.
    captured: Vec<EnvVar>,
    /// A line for every capture that found nothing.
    capture_notes: Vec<String>,
    /// How the request's own checks came out.
    checks: Vec<checks::Outcome>,
}
/// One cookie jar per collection, for as long as the app runs.
///
/// Jars are never written to disk: a session cookie is a credential, and the
/// whole point of a collection is that it can be committed. Closing volt
/// therefore logs you out, which is the safe direction to be wrong in.
#[derive(Default)]
struct Cookies(std::sync::Mutex<HashMap<String, std::sync::Arc<cookies::Jar>>>);

impl Cookies {
    fn jar(&self, root: &str) -> std::sync::Arc<cookies::Jar> {
        let mut jars = self.0.lock().expect("cookie jars");
        jars.entry(root.to_string()).or_default().clone()
    }
}

/// What the collection is currently holding, so it can be looked at rather
/// than guessed at.
#[tauri::command]
fn list_cookies(cookies: tauri::State<'_, Cookies>, root: String) -> Vec<cookies::Cookie> {
    cookies.jar(&root).list()
}

#[tauri::command]
fn delete_cookie(cookies: tauri::State<'_, Cookies>, root: String, name: String, domain: String, path: String) {
    cookies.jar(&root).remove(&name, &domain, &path);
}

/// Forget every cookie held for a collection — the equivalent of signing out.
#[tauri::command]
fn clear_cookies(cookies: tauri::State<'_, Cookies>, root: String) {
    cookies.jar(&root).clear();
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
async fn send_request(
    app: tauri::AppHandle,
    cookies: tauri::State<'_, Cookies>,
    root: String,
    request: Request,
    env_vars: Vec<EnvVar>,
    options: Option<http::ExecOptions>,
    request_id: Option<String>,
    environment: Option<String>,
) -> Result<SendOutcome> {
    let root_path = PathBuf::from(&root);
    // What the request inherits is read from disk rather than passed in, so
    // there is one answer to "what does this request send" and it is the files.
    let scopes = collection::scopes(&root_path, request_id.as_deref())?;
    let context: HashMap<String, String> = collection::scope_context(&scopes, &env_vars);
    let options = options.unwrap_or_default();

    let result = http::execute(
        &request,
        &http::ExecContext {
            root: &root_path,
            vars: &context,
            scopes: &scopes,
            options: &options,
            cookies: Some(cookies.jar(&root)),
        },
    )
    .await;

    // What the request captured counts as a secret for this entry too. On the
    // very first login the token is not in the environment yet, so without this
    // exactly one line in the history file — the one that mattered — held the
    // live credential in the clear, and every send after it looked fine.
    let (captured, capture_notes) = match result.as_ref() {
        Ok(response) => capture::extract(&request.captures, response),
        Err(_) => (Vec::new(), Vec::new()),
    };

    // Failures are history too: "it was refusing connections at 14:02" is
    // exactly the kind of thing worth being able to look back on.
    let mut entry = history::Entry::new(&request, request_id, environment, result.as_ref());
    let mut redact_vars = env_vars.clone();
    redact_vars.extend(captured.iter().cloned());
    entry.redact(&redact_vars);
    let summary = history_dir(&app)
        .and_then(|dir| history::record(&dir, &root, &entry))
        .map(|()| history::Summary::from(&entry))
        .ok();

    result.map(|response| {
        let checks = checks::run(&request.checks, &response);
        SendOutcome { response, history: summary, captured, capture_notes, checks }
    })
}

fn history_dir(app: &tauri::AppHandle) -> Result<PathBuf> {
    use tauri::Manager as _;
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("history"))
        .map_err(|e| error::Error::Invalid(format!("no app data directory: {e}")))
}

#[tauri::command]
fn list_history(app: tauri::AppHandle, root: String) -> Result<Vec<history::Summary>> {
    history::list(&history_dir(&app)?, &root)
}

#[tauri::command]
fn get_history_entry(app: tauri::AppHandle, root: String, id: String) -> Result<history::Entry> {
    history::get(&history_dir(&app)?, &root, &id)
}

#[tauri::command]
fn clear_history(app: tauri::AppHandle, root: String) -> Result<()> {
    history::clear(&history_dir(&app)?, &root)
}

/// Turn a pasted curl command into a request. Credentials it held in plain
/// text come back as new secret variables for the active environment.
#[tauri::command]
fn parse_curl(command: String, env_vars: Vec<EnvVar>) -> Result<curl::Parsed> {
    curl::parse(&command, &env_vars)
}

/// Print a request as a curl command, resolved the way Send resolves it.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
fn export_curl(
    root: String,
    request: Request,
    env_vars: Vec<EnvVar>,
    options: Option<http::ExecOptions>,
    request_id: Option<String>,
    include_secrets: bool,
) -> Result<curl::Rendered> {
    let root_path = PathBuf::from(root);
    let scopes = collection::scopes(&root_path, request_id.as_deref())?;
    curl::export(&root_path, &request, &env_vars, &scopes, &options.unwrap_or_default(), include_secrets)
}

/// Keep a response beside its request, as a file in the collection.
#[tauri::command]
fn save_example(
    root: String,
    id: String,
    name: String,
    response: http::HttpResponse,
    env_vars: Vec<EnvVar>,
) -> Result<Vec<examples::Example>> {
    examples::save(&PathBuf::from(root), &id, &name, &response, &env_vars)
}

#[tauri::command]
fn list_examples(root: String, id: String) -> Result<Vec<examples::Example>> {
    examples::list(&PathBuf::from(root), &id)
}

#[tauri::command]
fn delete_example(root: String, id: String, name: String) -> Result<Vec<examples::Example>> {
    examples::delete(&PathBuf::from(root), &id, &name)
}

#[tauri::command]
fn compare_example(
    root: String,
    id: String,
    name: String,
    response: http::HttpResponse,
) -> Result<examples::Comparison> {
    examples::compare(&PathBuf::from(root), &id, &name, &response)
}

/// Get an OAuth 2.0 token. The UI puts it in the environment; nothing is kept
/// here, because a token that lives in the app is a token nobody can see.
#[tauri::command]
async fn oauth_token(
    config: oauth::Config,
    grant: String,
    refresh_token: Option<String>,
) -> Result<oauth::Token> {
    match grant.as_str() {
        "client_credentials" => oauth::client_credentials(&config).await,
        "authorization_code" => oauth::authorization_code(&config).await,
        "refresh_token" => match refresh_token {
            Some(token) if !token.trim().is_empty() => oauth::refresh(&config, token.trim()).await,
            _ => Err(error::Error::Invalid("there is no refresh token to use yet".into())),
        },
        other => Err(error::Error::Invalid(format!("`{other}` is not a grant volt knows"))),
    }
}
/// What a `.proto` file offers. The path is taken from the collection root
/// when it is relative, like every other path a request names.
#[tauri::command]
fn grpc_services(root: String, proto: String) -> Result<Vec<grpc::Service>> {
    let path = PathBuf::from(root).join(proto.trim());
    Ok(grpc::services(&grpc::compile(&path)?))
}

/// Everything a gRPC call needs, resolved the way a Send resolves a request:
/// the URL, the headers and the inherited auth all come from `http::plan`, so
/// `{{baseUrl}}` and a token above the request mean the same thing here.
struct GrpcPlan {
    proto: String,
    url: String,
    method: String,
    message: String,
    headers: Vec<(String, String)>,
    timeout_ms: u64,
    verify_tls: bool,
    root: PathBuf,
}

fn grpc_plan(
    root: String,
    request: &Request,
    env_vars: &[EnvVar],
    options: Option<http::ExecOptions>,
    request_id: Option<&str>,
) -> Result<GrpcPlan> {
    let model::Body::Grpc { proto, method, message } = request.body.clone() else {
        return Err(error::Error::Invalid("this request is not a gRPC one".into()));
    };

    let root_path = PathBuf::from(root);
    let scopes = collection::scopes(&root_path, request_id)?;
    let context: HashMap<String, String> = collection::scope_context(&scopes, env_vars);
    let options = options.unwrap_or_default().for_request(request);

    // The body `plan` builds is not used: gRPC carries its own.
    let plan = http::plan(
        request,
        &http::PlanContext { root: &root_path, vars: &context, scopes: &scopes, lenient_url: false },
    )?;

    let resolve = |text: &str| vars::interpolate(text, &context).value;
    Ok(GrpcPlan {
        proto: resolve(&proto).trim().to_string(),
        url: plan.url,
        method: resolve(&method),
        message: resolve(&message),
        headers: plan.headers,
        timeout_ms: options.timeout_ms,
        verify_tls: options.verify_tls,
        root: root_path,
    })
}

impl GrpcPlan {
    fn call(&self) -> grpc::Call<'_> {
        grpc::Call {
            url: &self.url,
            method: &self.method,
            message: &self.message,
            headers: &self.headers,
            timeout_ms: self.timeout_ms,
            verify_tls: self.verify_tls,
        }
    }

    /// The `.proto` the request names, from the collection root when the path
    /// is relative — like every other path a request names.
    fn pool(&self) -> Result<prost_reflect::DescriptorPool> {
        grpc::compile(&self.root.join(&self.proto))
    }
}

/// Make a unary gRPC call. Its own command rather than `send_request`, because
/// the transport is not the same one.
#[tauri::command]
async fn send_grpc(
    root: String,
    request: Request,
    env_vars: Vec<EnvVar>,
    options: Option<http::ExecOptions>,
    request_id: Option<String>,
) -> Result<grpc::Reply> {
    let plan = grpc_plan(root, &request, &env_vars, options, request_id.as_deref())?;
    let pool = plan.pool()?;
    grpc::unary(&pool, &plan.call()).await
}

/// Open a streaming gRPC call — client, server or bidirectional. It reaches the
/// UI on the same channel a WebSocket does, because it is the same kind of
/// thing: an id, a list of events, a way to say more and a way to stop.
#[tauri::command]
async fn open_grpc_stream(
    app: tauri::AppHandle,
    streams: tauri::State<'_, stream::Streams>,
    root: String,
    request: Request,
    env_vars: Vec<EnvVar>,
    options: Option<http::ExecOptions>,
    request_id: Option<String>,
) -> Result<stream::Open> {
    let plan = grpc_plan(root, &request, &env_vars, options, request_id.as_deref())?;
    let pool = plan.pool()?;
    let id = format!("grpc-{}", uuid_ish());
    let sink: stream::Sink = std::sync::Arc::new(move |event| {
        use tauri::Emitter as _;
        let _ = app.emit(stream::CHANNEL, event);
    });
    grpc::open_stream(sink, &streams, id, &pool, &plan.call()).await
}

/// Ask the server what it serves, for an endpoint that offers reflection —
/// which is the case where there is no `.proto` on disk to compile.
///
/// What comes back is written nowhere: a service description belongs to the
/// server, and a stale copy committed next to the requests is a lie waiting to
/// be believed.
#[tauri::command]
async fn grpc_reflect(
    root: String,
    request: Request,
    env_vars: Vec<EnvVar>,
    options: Option<http::ExecOptions>,
    request_id: Option<String>,
) -> Result<Vec<grpc::Service>> {
    let plan = grpc_plan(root, &request, &env_vars, options, request_id.as_deref())?;
    let pool =
        reflection::pool(&plan.url, &plan.headers, plan.timeout_ms, plan.verify_tls).await?;
    Ok(grpc::services(&pool))
}
/// Open a connection that stays open. `kind` is `websocket` or `sse`.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
async fn open_stream(
    app: tauri::AppHandle,
    streams: tauri::State<'_, stream::Streams>,
    root: String,
    request: Request,
    env_vars: Vec<EnvVar>,
    options: Option<http::ExecOptions>,
    request_id: Option<String>,
    kind: String,
) -> Result<stream::Open> {
    let root_path = PathBuf::from(root);
    let scopes = collection::scopes(&root_path, request_id.as_deref())?;
    let context: HashMap<String, String> = collection::scope_context(&scopes, &env_vars);
    // `for_request`, so a per-request TLS or timeout override applies to a
    // socket the way it does to a send.
    let options = options.unwrap_or_default().for_request(&request);

    // The same resolution a Send does: URL, headers and inherited auth.
    let plan = http::plan(
        &request,
        &http::PlanContext { root: &root_path, vars: &context, scopes: &scopes, lenient_url: false },
    )?;

    let id = format!("{}-{}", kind, uuid_ish());
    // Events reach the webview on one channel; the stream module knows nothing
    // about Tauri beyond this closure.
    let sink: stream::Sink = std::sync::Arc::new(move |event| {
        use tauri::Emitter as _;
        let _ = app.emit(stream::CHANNEL, event);
    });

    match kind.as_str() {
        "websocket" => stream::open_websocket(sink, &streams, id, plan, options.verify_tls).await,
        "sse" => stream::open_sse(sink, &streams, id, plan, options.verify_tls).await,
        other => Err(error::Error::Invalid(format!("`{other}` is not a kind of stream volt opens"))),
    }
}

#[tauri::command]
fn send_stream(streams: tauri::State<'_, stream::Streams>, id: String, text: String) -> Result<()> {
    streams.send(&id, text)
}

#[tauri::command]
fn close_stream(streams: tauri::State<'_, stream::Streams>, id: String) {
    streams.close(&id);
}

#[tauri::command]
fn open_streams(streams: tauri::State<'_, stream::Streams>) -> Vec<String> {
    streams.open_ids()
}

fn uuid_ish() -> String {
    vars::dynamics().remove("$guid").unwrap_or_else(|| "stream".into())
}

/// Ask a GraphQL endpoint what it can do.
///
/// One introspection query, cached by nobody: a schema belongs to the server,
/// not to the collection, and a stale copy in a file is worse than a fetch.
#[tauri::command]
async fn graphql_schema(
    schemas: tauri::State<'_, Schemas>,
    root: String,
    request: Request,
    env_vars: Vec<EnvVar>,
    options: Option<http::ExecOptions>,
    request_id: Option<String>,
) -> Result<graphql::Schema> {
    const QUERY: &str = "query IntrospectVolt { __schema { queryType { name } mutationType { name } \
        types { kind name description fields(includeDeprecated: false) { name description \
        args { name type { kind name ofType { kind name } } } \
        type { kind name ofType { kind name ofType { kind name } } } } } } }";

    let root_path = PathBuf::from(root);
    let scopes = collection::scopes(&root_path, request_id.as_deref())?;
    let context: HashMap<String, String> = collection::scope_context(&scopes, &env_vars);
    let options = options.unwrap_or_default();

    // The same request the user has open — its URL, its auth, its headers —
    // with the introspection query as the body. Anything else would be asking
    // a different server than the one they are working against.
    let mut probe = request;
    probe.method = "POST".into();
    probe.body = model::Body::GraphQl { query: QUERY.into(), variables: String::new() };
    probe.captures = Vec::new();
    probe.checks = Vec::new();

    let response = http::execute(
        &probe,
        &http::ExecContext {
            root: &root_path,
            vars: &context,
            scopes: &scopes,
            options: &options,
            cookies: None,
        },
    )
    .await?;

    if response.status >= 400 {
        return Err(error::Error::Invalid(format!(
            "the endpoint answered {} to introspection",
            response.status
        )));
    }
    let parsed: serde_json::Value = serde_json::from_str(&response.body)
        .map_err(|_| error::Error::Invalid("the endpoint did not answer introspection with JSON".into()))?;

    if let Some(errors) = parsed.get("errors").filter(|errors| !errors.is_null()) {
        return Err(error::Error::Invalid(format!("introspection was refused: {errors}")));
    }
    let raw = parsed
        .get("data")
        .and_then(|data| data.get("__schema"))
        .ok_or_else(|| error::Error::Invalid("the endpoint answered without a schema".into()))?;

    // Kept in memory against the request that fetched it, so the editor can
    // complete a field name without asking the server again.
    let schema = graphql::read(raw);
    schemas.put(request_id.as_deref().unwrap_or_default(), schema.clone());
    Ok(schema)
}

/// The schemas introspected this session, one per request, for as long as the
/// app runs.
///
/// Deliberately not a file. A schema belongs to the server, and a stale copy
/// committed next to the requests is a lie waiting to be believed — so it is
/// held in memory, where the worst that happens is one more introspection.
#[derive(Default)]
struct Schemas(std::sync::Mutex<HashMap<String, graphql::Schema>>);

impl Schemas {
    fn get(&self, key: &str) -> Option<graphql::Schema> {
        self.0.lock().expect("schemas").get(key).cloned()
    }

    fn put(&self, key: &str, schema: graphql::Schema) {
        self.0.lock().expect("schemas").insert(key.to_string(), schema);
    }
}

/// Which fields in a query the endpoint does not have. Nothing is said until a
/// schema has been fetched for that request, and nothing is said about a part
/// of the query the walk could not follow.
#[tauri::command]
fn graphql_check(
    schemas: tauri::State<'_, Schemas>,
    request_id: Option<String>,
    query: String,
) -> Vec<graphql::Problem> {
    match schemas.get(request_id.as_deref().unwrap_or_default()) {
        Some(schema) => graphql::check(&schema, &query),
        None => Vec::new(),
    }
}

/// The fields that belong where the cursor is.
#[tauri::command]
fn graphql_complete(
    schemas: tauri::State<'_, Schemas>,
    request_id: Option<String>,
    query: String,
    at: usize,
) -> Vec<graphql::Completion> {
    match schemas.get(request_id.as_deref().unwrap_or_default()) {
        Some(schema) => graphql::complete(&schema, &query, at),
        None => Vec::new(),
    }
}


/// Run a folder or the whole collection, in order.
#[tauri::command]
async fn run_collection(
    cookies: tauri::State<'_, Cookies>,
    root: String,
    target: Option<String>,
    env_vars: Vec<EnvVar>,
    options: Option<http::ExecOptions>,
    stop_on_failure: bool,
) -> Result<runner::Run> {
    let root_path = PathBuf::from(&root);
    let options = options.unwrap_or_default();
    runner::run(
        &runner::Context {
            root: &root_path,
            env_vars: &env_vars,
            options: &options,
            cookies: Some(cookies.jar(&root)),
            stop_on_failure,
            keep_examples: false,
        },
        target.as_deref(),
    )
    .await
}


/// Where the collection stands against its repository, if it is in one.
#[tauri::command]
fn sync_status(root: String) -> Result<git::Status> {
    git::status(&PathBuf::from(root))
}

#[tauri::command]
fn sync_pull(root: String) -> Result<String> {
    git::pull(&PathBuf::from(root))
}

#[tauri::command]
fn sync_commit(root: String, message: String, push: bool) -> Result<String> {
    git::commit(&PathBuf::from(root), &message, push)
}

/// Write the `.env.<environment>.example` templates a teammate fills in.
#[tauri::command]
fn write_env_templates(root: String) -> Result<Vec<String>> {
    git::write_env_templates(&PathBuf::from(root))
}


/// Write the collection's documentation as one self-contained HTML file.
#[tauri::command]
fn export_docs(root: String) -> Result<String> {
    let root_path = PathBuf::from(root);
    let collection = collection::load(&root_path)?;
    docs::html(&root_path, &collection)
}

/// Serve this collection's saved examples on loopback.
#[tauri::command]
async fn start_mock(mock: tauri::State<'_, mock::Server>, root: String, port: u16) -> Result<mock::Running> {
    mock::start(&mock, PathBuf::from(root), port).await
}

#[tauri::command]
fn stop_mock(mock: tauri::State<'_, mock::Server>) {
    mock.stop();
}

#[tauri::command]
fn mock_status(mock: tauri::State<'_, mock::Server>) -> Option<mock::Running> {
    mock.running()
}


/// Write the collection out as a Postman v2.1 export.
#[tauri::command]
fn export_postman(root: String) -> Result<export::Exported> {
    let root_path = PathBuf::from(root);
    let collection = collection::load(&root_path)?;
    export::postman(&root_path, &collection)
}

/// A folder's own headers, auth and variables.
#[tauri::command]
fn get_folder(root: String, id: String) -> Result<model::FolderMeta> {
    collection::folder_meta(&PathBuf::from(root), &id)
}

#[tauri::command]
fn save_folder(root: String, id: String, folder: model::FolderMeta) -> Result<()> {
    collection::write_folder_meta(&PathBuf::from(root), &id, &folder)
}

/// The collection's own headers, auth and variables.
#[tauri::command]
fn save_collection_meta(root: String, meta: model::CollectionMeta) -> Result<collection::Collection> {
    let root_path = PathBuf::from(root);
    collection::write_meta(&root_path, &meta)?;
    collection::load(&root_path)
}


/// Print a request as code, resolved the way Send resolves it.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
fn generate_code(
    root: String,
    request: Request,
    env_vars: Vec<EnvVar>,
    options: Option<http::ExecOptions>,
    request_id: Option<String>,
    language: codegen::Language,
    include_secrets: bool,
) -> Result<codegen::Generated> {
    let root_path = PathBuf::from(root);
    let scopes = collection::scopes(&root_path, request_id.as_deref())?;
    codegen::generate(
        &root_path,
        &request,
        &env_vars,
        &scopes,
        &options.unwrap_or_default(),
        language,
        include_secrets,
    )
}

/// The languages the UI offers, so the list lives in one place.
#[tauri::command]
fn code_languages() -> Vec<(codegen::Language, &'static str)> {
    codegen::Language::ALL.iter().map(|language| (*language, language.label())).collect()
}

/// Write a response body where the user picked. It arrives as text, or as
/// base64 when it did not decode as UTF-8, which is how it crossed IPC in the
/// first place.
#[tauri::command]
fn save_body(path: String, body: String, base64_encoded: bool) -> Result<()> {
    use base64::Engine as _;

    let bytes = if base64_encoded {
        base64::engine::general_purpose::STANDARD
            .decode(body)
            .map_err(|e| error::Error::Invalid(format!("the response body could not be decoded: {e}")))?
    } else {
        body.into_bytes()
    };
    std::fs::write(&path, bytes).map_err(|e| error::Error::io(path, e))
}


/// Preview what a string resolves to, so the UI can show the real URL under
/// the input while the user types.
#[tauri::command]
fn preview(input: String, env_vars: Vec<EnvVar>) -> vars::Interpolated {
    vars::interpolate(&input, &collection::env_context(&env_vars))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(Cookies::default())
        .manage(Schemas::default())
        .manage(mock::Server::default())
        .manage(stream::Streams::default())
        .invoke_handler(tauri::generate_handler![
            open_collection,
            init_collection,
            default_collection,
            import_collection,
            get_request,
            save_request,
            create_folder,
            delete_node,
            restore_node,
            list_trash,
            empty_trash,
            rename_node,
            move_node,
            reorder,
            save_environment,
            delete_environment,
            clear_cookies,
            list_cookies,
            delete_cookie,
            save_body,
            send_request,
            list_history,
            get_history_entry,
            clear_history,
            parse_curl,
            export_curl,
            generate_code,
            code_languages,
            export_postman,
            export_docs,
            grpc_services,
            send_grpc,
            open_grpc_stream,
            grpc_reflect,
            open_stream,
            send_stream,
            close_stream,
            open_streams,
            graphql_schema,
            graphql_check,
            graphql_complete,
            oauth_token,
            run_collection,
            sync_status,
            sync_pull,
            sync_commit,
            write_env_templates,
            start_mock,
            stop_mock,
            mock_status,
            save_example,
            list_examples,
            delete_example,
            compare_example,
            get_folder,
            save_folder,
            save_collection_meta,
            preview,
        ])
        .run(tauri::generate_context!())
        .expect("error while running volt");
}
