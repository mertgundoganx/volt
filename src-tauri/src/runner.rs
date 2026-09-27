//! Running a folder, or a whole collection, in order.
//!
//! The point is not "send a lot of requests": it is that a run carries its
//! variables forward. A login's capture feeds the next request, which is why
//! the runner is the thing that makes captures and checks worth having.
//!
//! It runs in the app, and from the command line through `volt-run`, over the
//! same code — so what CI reports is what you saw.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use serde::Serialize;

use crate::checks;
use crate::collection::{self, Node};
use crate::cookies;
use crate::error::{Error, Result};
use crate::http::{self, ExecOptions};
use crate::model::{EnvVar, Kind};
use crate::{capture, examples};

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub id: String,
    pub name: String,
    pub method: String,
    pub status: Option<u16>,
    pub duration_ms: Option<u64>,
    pub error: Option<String>,
    pub checks: Vec<checks::Outcome>,
    /// Names captured here, for the report — never the values.
    pub captured: Vec<String>,
    /// A step passes when it got a response and every check on it passed.
    pub ok: bool,
    /// Why the step was not sent: a run speaks HTTP, and a socket, an event
    /// stream or a gRPC call is not a request it can make and judge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skipped: Option<String>,
    /// Which row of the data file this pass used, counting from 1. Absent
    /// when the run had no data file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iteration: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub started: u64,
    pub duration_ms: u64,
    pub steps: Vec<Step>,
    pub passed: usize,
    pub failed: usize,
}

pub struct Context<'a> {
    pub root: &'a Path,
    pub env_vars: &'a [EnvVar],
    pub options: &'a ExecOptions,
    pub cookies: Option<Arc<cookies::Jar>>,
    /// Stop at the first step that fails, the way a script would.
    pub stop_on_failure: bool,
    /// Keep each step's response as an example named after the run.
    pub keep_examples: bool,
    /// Rows from a data file. Each one is a pass over the requests with its
    /// values as variables; none is one pass with none.
    pub data: &'a [Row],
}

/// One row of a data file: column names and this row's values, in order.
pub type Row = Vec<(String, String)>;

/// Read a data file for a run: a CSV with a header line, or a JSON array of
/// objects. Each row is one pass over the requests, its values as variables.
pub fn read_data(path: &Path) -> Result<Vec<Row>> {
    let text = std::fs::read_to_string(path).map_err(|e| Error::io(path.to_string_lossy(), e))?;
    let text = text.trim_start_matches('\u{feff}');
    let is_json = path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
        || text.trim_start().starts_with('[');
    let rows = if is_json { json_rows(text)? } else { csv_rows(text)? };
    if rows.is_empty() {
        return Err(Error::Invalid(format!("`{}` has no rows to run with", path.display())));
    }
    Ok(rows)
}

fn json_rows(text: &str) -> Result<Vec<Row>> {
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| Error::Invalid(format!("the data file is not valid JSON: {e}")))?;
    let Some(items) = value.as_array() else {
        return Err(Error::Invalid("a JSON data file is an array of objects, one per run".into()));
    };
    items
        .iter()
        .map(|item| {
            let Some(object) = item.as_object() else {
                return Err(Error::Invalid("every item in a JSON data file has to be an object".into()));
            };
            Ok(object
                .iter()
                .map(|(key, value)| {
                    let text = match value {
                        serde_json::Value::String(text) => text.clone(),
                        serde_json::Value::Null => String::new(),
                        other => other.to_string(),
                    };
                    (key.clone(), text)
                })
                .collect())
        })
        .collect()
}

/// RFC 4180, which is what spreadsheets write: commas, quoted fields, `""` for
/// a quote inside one, and line breaks inside quotes kept.
fn csv_rows(text: &str) -> Result<Vec<Row>> {
    let mut records: Vec<Vec<String>> = Vec::new();
    let mut record: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    field.push('"');
                    chars.next();
                }
                '"' => quoted = false,
                other => field.push(other),
            }
            continue;
        }
        match c {
            '"' if field.is_empty() => quoted = true,
            ',' => record.push(std::mem::take(&mut field)),
            '\r' => {}
            '\n' => {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
            }
            other => field.push(other),
        }
    }
    if quoted {
        return Err(Error::Invalid("the data file has a quote that is never closed".into()));
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    records.retain(|record| record.iter().any(|field| !field.trim().is_empty()));

    let mut records = records.into_iter();
    let Some(header) = records.next() else { return Ok(Vec::new()) };
    let header: Vec<String> = header.into_iter().map(|name| name.trim().to_string()).collect();
    Ok(records
        .map(|values| {
            header
                .iter()
                .enumerate()
                .filter(|(_, name)| !name.is_empty())
                .map(|(at, name)| (name.clone(), values.get(at).cloned().unwrap_or_default()))
                .collect()
        })
        .collect())
}

/// Every request under `target`, in tree order. `None` runs the collection.
pub fn steps(root: &Path, target: Option<&str>) -> Result<Vec<(String, String, String)>> {
    let collection = collection::load(root)?;
    let mut out = Vec::new();

    fn walk(nodes: &[Node], out: &mut Vec<(String, String, String)>) {
        for node in nodes {
            match node {
                Node::Folder { children, .. } => walk(children, out),
                Node::Request { id, name, method, .. } => {
                    out.push((id.clone(), name.clone(), method.clone()))
                }
            }
        }
    }

    match target {
        None => walk(&collection.tree, &mut out),
        Some(id) => {
            let found = find(&collection.tree, id);
            match found {
                Some(Node::Folder { children, .. }) => walk(children, &mut out),
                Some(node @ Node::Request { .. }) => walk(std::slice::from_ref(node), &mut out),
                None => {
                    return Err(crate::error::Error::Invalid(format!("`{id}` is not in this collection")))
                }
            }
        }
    }
    Ok(out)
}

fn find<'a>(nodes: &'a [Node], id: &str) -> Option<&'a Node> {
    for node in nodes {
        match node {
            Node::Folder { id: folder, children, .. } => {
                if folder == id {
                    return Some(node);
                }
                if let Some(found) = find(children, id) {
                    return Some(found);
                }
            }
            Node::Request { id: request, .. } if request == id => return Some(node),
            _ => {}
        }
    }
    None
}

pub async fn run(ctx: &Context<'_>, target: Option<&str>) -> Result<Run> {
    let started = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let clock = std::time::Instant::now();

    // Captured values are carried forward in memory for the length of the run.
    // They are not written to the environment: a run is a check, not an edit,
    // and a CI job should not leave a token in a file behind it.
    let planned = steps(ctx.root, target)?;
    let passes: Vec<Option<&Row>> = if ctx.data.is_empty() { vec![None] } else { ctx.data.iter().map(Some).collect() };
    let mut steps_out = Vec::new();

    // Each pass starts from the environment and its row, so one row's
    // captures cannot leak into the next and a row can be rerun on its own.
    'passes: for (pass, row) in passes.into_iter().enumerate() {
        let iteration = row.map(|_| pass + 1);
        let mut vars: Vec<EnvVar> = ctx.env_vars.to_vec();
        for (name, value) in row.into_iter().flatten() {
            match vars.iter_mut().find(|var| &var.name == name) {
                Some(existing) => existing.value = value.clone(),
                None => vars.push(EnvVar { name: name.clone(), value: value.clone(), secret: false }),
            }
        }

        for (id, name, method) in planned.iter().cloned() {
            let request = collection::read_request(ctx.root, &id)?;
            if request.kind != Kind::Http {
                let what = match request.kind {
                    Kind::Websocket => "a WebSocket",
                    Kind::Sse => "an event stream",
                    _ => "a gRPC call",
                };
                steps_out.push(Step {
                    id,
                    name,
                    method,
                    status: None,
                    duration_ms: None,
                    error: None,
                    checks: Vec::new(),
                    captured: Vec::new(),
                    ok: true,
                    skipped: Some(format!("skipped: {what} is opened by hand, not run")),
                    iteration,
                });
                continue;
            }
            let scopes = collection::scopes(ctx.root, Some(&id))?;
            let context: HashMap<String, String> = collection::scope_context(&scopes, &vars);

            let result = http::execute(
                &request,
                &http::ExecContext {
                    root: ctx.root,
                    vars: &context,
                    scopes: &scopes,
                    options: ctx.options,
                    cookies: ctx.cookies.clone(),
                },
            )
            .await;

            let mut step = Step {
                id: id.clone(),
                name: name.clone(),
                method,
                status: None,
                duration_ms: None,
                error: None,
                checks: Vec::new(),
                captured: Vec::new(),
                ok: false,
                skipped: None,
                iteration,
            };

            match result {
                Err(error) => step.error = Some(error.to_string()),
                Ok(response) => {
                    step.status = Some(response.status);
                    step.duration_ms = Some(response.duration_ms);
                    step.checks = checks::run(&request.checks, &response);

                    let (captured, _notes) = capture::extract(&request.captures, &response);
                    for value in captured {
                        step.captured.push(value.name.clone());
                        match vars.iter_mut().find(|var| var.name == value.name) {
                            Some(existing) => {
                                existing.value = value.value;
                                existing.secret = existing.secret || value.secret;
                            }
                            None => vars.push(value),
                        }
                    }

                    if ctx.keep_examples {
                        // Best effort: a run that cannot write an example is still
                        // a run, and the failure belongs to the file system.
                        let _ = examples::save(ctx.root, &id, "Last run", &response, &vars);
                    }
                    step.ok = step.checks.iter().all(|check| check.ok);
                }
            }

            let failed = !step.ok;
            // A failed check prints the value it actually got, and `volt-run` writes
            // that to stdout — which in CI is a log. Redacted against the run's own
            // variables, including anything this very step captured.
            let step = crate::history::redacted(&step, &vars);
            steps_out.push(step);
            if failed && ctx.stop_on_failure {
                break 'passes;
            }
        }
    }

    let passed = steps_out.iter().filter(|step| step.ok && step.skipped.is_none()).count();
    let failed = steps_out.iter().filter(|step| !step.ok).count();
    Ok(Run {
        started,
        duration_ms: clock.elapsed().as_millis() as u64,
        failed,
        passed,
        steps: steps_out,
    })
}

/// The run as lines for a terminal. Used by `volt-run`, and the same text the
/// app can copy, so a failure reads the same in both places.
pub fn report(run: &Run) -> String {
    let mut out = String::new();
    let mut pass = None;
    for step in &run.steps {
        if step.iteration.is_some() && step.iteration != pass {
            pass = step.iteration;
            let gap = if out.is_empty() { "" } else { "\n" };
            out.push_str(&format!("{gap}row {}\n", pass.unwrap_or(0)));
        }
        let mark = if step.skipped.is_some() { "skip" } else if step.ok { "ok  " } else { "FAIL" };
        let outcome = match (&step.error, step.status) {
            _ if step.skipped.is_some() => step.skipped.clone().unwrap_or_default(),
            (Some(error), _) => error.clone(),
            (None, Some(status)) => format!("{status} · {} ms", step.duration_ms.unwrap_or(0)),
            _ => "no response".into(),
        };
        out.push_str(&format!("{mark}  {:<40} {outcome}\n", step.name));

        for check in step.checks.iter().filter(|check| !check.ok) {
            let actual = check.actual.clone().unwrap_or_else(|| "nothing".into());
            let detail = check
                .note
                .clone()
                .unwrap_or_else(|| format!("expected {} {}, got {actual}", check.op, check.expected));
            out.push_str(&format!("      {} {detail}\n", check.from));
        }
        if !step.captured.is_empty() {
            out.push_str(&format!("      captured {}\n", step.captured.join(", ")));
        }
    }

    out.push_str(&format!(
        "\n{} passed, {} failed, in {} ms\n",
        run.passed, run.failed, run.duration_ms
    ));
    out
}

/// `volt-run` — the same runner, from a terminal.
///
/// Kept here rather than in the binary so the argument parsing and the exit
/// code are testable, and so CI and the app cannot drift apart.
pub async fn cli(args: Vec<String>) -> std::process::ExitCode {
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        eprintln!("{USAGE}");
        return std::process::ExitCode::SUCCESS;
    }

    let value = |name: &str| -> Option<String> {
        args.iter().position(|arg| arg == name).and_then(|at| args.get(at + 1)).cloned()
    };
    let flag = |name: &str| args.iter().any(|arg| arg == name);

    let Some(path) = value("--collection").or_else(|| args.first().filter(|a| !a.starts_with('-')).cloned())
    else {
        eprintln!("{USAGE}");
        return std::process::ExitCode::from(2);
    };
    let root = std::path::PathBuf::from(path);

    let environment = value("--env");
    let vars = match environment_vars(&root, environment.as_deref()) {
        Ok(vars) => vars,
        Err(error) => {
            eprintln!("volt-run: {error}");
            return std::process::ExitCode::from(2);
        }
    };

    let data = match value("--data").map(|path| read_data(Path::new(&path))).transpose() {
        Ok(rows) => rows.unwrap_or_default(),
        Err(error) => {
            eprintln!("volt-run: {error}");
            return std::process::ExitCode::from(2);
        }
    };

    let options = ExecOptions {
        timeout_ms: value("--timeout").and_then(|t| t.parse().ok()).unwrap_or(30_000),
        verify_tls: !flag("--insecure"),
        ..Default::default()
    };
    let ctx = Context {
        root: &root,
        env_vars: &vars,
        options: &options,
        // One jar for the run, so a login step's cookie reaches the next.
        cookies: Some(Arc::new(cookies::Jar::default())),
        stop_on_failure: flag("--stop-on-failure"),
        keep_examples: false,
        data: &data,
    };

    match run(&ctx, value("--folder").as_deref()).await {
        Err(error) => {
            eprintln!("volt-run: {error}");
            std::process::ExitCode::from(2)
        }
        Ok(finished) => {
            print!("{}", report(&finished));
            if finished.failed == 0 {
                std::process::ExitCode::SUCCESS
            } else {
                std::process::ExitCode::FAILURE
            }
        }
    }
}

const USAGE: &str = "\
volt-run — run a volt collection

    volt-run <collection> [--folder <id>] [--env <name>] [--data <file>]
             [--timeout <ms>] [--insecure] [--stop-on-failure]

--data runs the requests once per row of a CSV (with a header line) or a
JSON array of objects, each row's values available as {{variables}}.

Exits 0 when every check passed, 1 when one did not, 2 when it could not run.
Secret values come from the collection's `.env.<environment>` files, as in the
app; nothing is written back.";

fn environment_vars(root: &Path, wanted: Option<&str>) -> Result<Vec<EnvVar>> {
    let collection = collection::load(root)?;
    let found = match wanted {
        None => collection.environments.into_iter().next(),
        Some(name) => {
            let found = collection.environments.into_iter().find(|env| env.name == name);
            if found.is_none() {
                return Err(crate::error::Error::Invalid(format!("no environment called `{name}`")));
            }
            found
        }
    };
    Ok(found.map(|env| env.vars).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/sample-collection")
    }

    #[test]
    fn a_run_is_every_request_in_tree_order() {
        let all = steps(&sample(), None).unwrap();
        assert!(all.len() >= 3, "{all:?}");
        assert!(all.iter().all(|(id, _, _)| id.ends_with(".yaml")));

        // A folder runs just what is inside it.
        let folder = all.iter().find_map(|(id, _, _)| id.split_once('/').map(|(folder, _)| folder.to_string()));
        if let Some(folder) = folder {
            let inside = steps(&sample(), Some(&folder)).unwrap();
            assert!(!inside.is_empty() && inside.len() < all.len(), "{inside:?}");
            assert!(inside.iter().all(|(id, _, _)| id.starts_with(&format!("{folder}/"))));
        }

        assert!(steps(&sample(), Some("nope")).is_err(), "a target that is not there says so");
    }

    #[test]
    fn the_report_reads_as_a_terminal_would_want_it() {
        let run = Run {
            started: 0,
            duration_ms: 42,
            passed: 1,
            failed: 1,
            steps: vec![
                Step {
                    id: "a.yaml".into(),
                    name: "Login".into(),
                    method: "POST".into(),
                    status: Some(200),
                    duration_ms: Some(12),
                    error: None,
                    checks: vec![],
                    captured: vec!["token".into()],
                    ok: true,
                    skipped: None,
                    iteration: None,
                },
                Step {
                    id: "b.yaml".into(),
                    name: "Orders".into(),
                    method: "GET".into(),
                    status: Some(500),
                    duration_ms: Some(30),
                    error: None,
                    checks: vec![checks::Outcome {
                        ok: false,
                        from: "status".into(),
                        op: "is".into(),
                        expected: "200".into(),
                        actual: Some("500".into()),
                        note: None,
                    }],
                    captured: vec![],
                    ok: false,
                    skipped: None,
                    iteration: None,
                },
            ],
        };

        let text = report(&run);
        assert!(text.contains("ok    Login"), "{text}");
        assert!(text.contains("FAIL  Orders"), "{text}");
        assert!(text.contains("status expected is 200, got 500"), "{text}");
        assert!(text.contains("captured token"), "a capture is named, never its value");
        assert!(text.contains("1 passed, 1 failed"), "{text}");
    }

    #[tokio::test]
    async fn a_socket_in_a_run_is_skipped_not_sent_as_http() {
        let dir = std::env::temp_dir().join(format!("volt-run-skip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("collection.yaml"), "name: Skip\nversion: 1\n").unwrap();
        std::fs::write(dir.join("socket.yaml"), "name: Socket\nkind: websocket\nmethod: GET\nurl: ws://127.0.0.1:1/\n").unwrap();

        let options = ExecOptions::default();
        let ctx = Context { root: &dir, env_vars: &[], options: &options, cookies: None, stop_on_failure: true, keep_examples: false, data: &[] };
        let finished = run(&ctx, None).await.unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(finished.steps.len(), 1);
        let step = &finished.steps[0];
        assert!(step.skipped.as_deref().is_some_and(|why| why.contains("WebSocket")), "{step:?}");
        assert!(step.ok && step.error.is_none() && step.status.is_none());
        assert_eq!((finished.passed, finished.failed), (0, 0), "a skip is neither");
        assert!(report(&finished).contains("skip  Socket"), "{}", report(&finished));
    }

    #[test]
    fn a_data_file_is_a_csv_with_a_header_or_a_json_array() {
        let dir = std::env::temp_dir().join(format!("volt-run-data-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let csv = dir.join("users.csv");
        std::fs::write(&csv, "\u{feff}user,note\r\nada,\"says \"\"hi\"\", then leaves\"\r\n\r\nalan,\"two\nlines\"\nlinus\n").unwrap();
        let rows = read_data(&csv).unwrap();
        assert_eq!(rows.len(), 3, "the blank line is not a row: {rows:?}");
        assert_eq!(rows[0], vec![("user".into(), "ada".into()), ("note".into(), "says \"hi\", then leaves".into())]);
        assert_eq!(rows[1][1].1, "two\nlines", "a line break inside quotes is kept");
        assert_eq!(rows[2][1], ("note".into(), String::new()), "a short row is filled with nothing");

        let json = dir.join("users.json");
        std::fs::write(&json, r#"[{"user":"ada","age":36,"admin":true},{"user":"alan","age":null}]"#).unwrap();
        let rows = read_data(&json).unwrap();
        assert_eq!(rows[0], vec![("admin".into(), "true".into()), ("age".into(), "36".into()), ("user".into(), "ada".into())]);
        assert_eq!(rows[1][0], ("age".into(), String::new()));

        std::fs::write(&json, r#"{"user":"ada"}"#).unwrap();
        assert!(read_data(&json).is_err(), "an object is not a list of rows");
        std::fs::write(&csv, "user\n").unwrap();
        assert!(read_data(&csv).is_err(), "a header alone has nothing to run");
        std::fs::write(&csv, "user\n\"ada\n").unwrap();
        assert!(read_data(&csv).unwrap_err().to_string().contains("quote"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn each_row_is_a_pass_with_its_values_as_variables() {
        let dir = std::env::temp_dir().join(format!("volt-run-rows-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("collection.yaml"), "name: Rows\nversion: 1\n").unwrap();
        // Nothing listens on port 1, so every step fails fast and says its URL.
        std::fs::write(dir.join("get.yaml"), "name: Get\nmethod: GET\nurl: http://127.0.0.1:1/{{user}}\n").unwrap();

        let rows: Vec<Row> = vec![vec![("user".into(), "ada".into())], vec![("user".into(), "alan".into())]];
        let options = ExecOptions { timeout_ms: 5_000, ..Default::default() };
        let env = [EnvVar { name: "user".into(), value: "nobody".into(), secret: false }];
        let ctx = Context { root: &dir, env_vars: &env, options: &options, cookies: None, stop_on_failure: false, keep_examples: false, data: &rows };
        let finished = run(&ctx, None).await.unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(finished.steps.len(), 2, "one step per row");
        assert_eq!(finished.steps.iter().map(|s| s.iteration).collect::<Vec<_>>(), vec![Some(1), Some(2)]);
        let said = |i: usize| finished.steps[i].error.clone().unwrap_or_default();
        assert!(said(0).contains("/ada") && said(1).contains("/alan"), "the row wins over the environment: {} / {}", said(0), said(1));
        let text = report(&finished);
        assert!(text.contains("row 1\n") && text.contains("\nrow 2\n"), "{text}");

        // Stopping at the first failure stops the whole run, not just the pass.
        let ctx = Context { stop_on_failure: true, ..ctx };
        let dir2 = dir.clone();
        std::fs::create_dir_all(&dir2).unwrap();
        std::fs::write(dir2.join("collection.yaml"), "name: Rows\nversion: 1\n").unwrap();
        std::fs::write(dir2.join("get.yaml"), "name: Get\nmethod: GET\nurl: http://127.0.0.1:1/{{user}}\n").unwrap();
        let stopped = run(&ctx, None).await.unwrap();
        let _ = std::fs::remove_dir_all(&dir2);
        assert_eq!(stopped.steps.len(), 1);
    }

    #[test]
    fn the_cli_refuses_without_a_collection_rather_than_guessing() {
        let code = tokio::runtime::Runtime::new().unwrap().block_on(cli(vec!["--folder".into(), "x".into()]));
        assert_eq!(format!("{code:?}"), format!("{:?}", std::process::ExitCode::from(2)));
    }
}
