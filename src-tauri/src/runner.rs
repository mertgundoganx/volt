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
use crate::error::Result;
use crate::http::{self, ExecOptions};
use crate::model::EnvVar;
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
    let mut vars: Vec<EnvVar> = ctx.env_vars.to_vec();
    let mut steps_out = Vec::new();

    for (id, name, method) in steps(ctx.root, target)? {
        let request = collection::read_request(ctx.root, &id)?;
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
            break;
        }
    }

    let passed = steps_out.iter().filter(|step| step.ok).count();
    Ok(Run {
        started,
        duration_ms: clock.elapsed().as_millis() as u64,
        failed: steps_out.len() - passed,
        passed,
        steps: steps_out,
    })
}

/// The run as lines for a terminal. Used by `volt-run`, and the same text the
/// app can copy, so a failure reads the same in both places.
pub fn report(run: &Run) -> String {
    let mut out = String::new();
    for step in &run.steps {
        let mark = if step.ok { "ok  " } else { "FAIL" };
        let outcome = match (&step.error, step.status) {
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

    volt-run <collection> [--folder <id>] [--env <name>]
             [--timeout <ms>] [--insecure] [--stop-on-failure]

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

    #[test]
    fn the_cli_refuses_without_a_collection_rather_than_guessing() {
        let code = tokio::runtime::Runtime::new().unwrap().block_on(cli(vec!["--folder".into(), "x".into()]));
        assert_eq!(format!("{code:?}"), format!("{:?}", std::process::ExitCode::from(2)));
    }
}
