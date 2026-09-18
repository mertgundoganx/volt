//! Sync and sharing, through the collection's own repository.
//!
//! There is no volt cloud to sync with, and building one would undo the point
//! of the app: the collection is files you own. What a team actually shares
//! them through is the repository they are already in, so this is a thin
//! wrapper over the `git` that is already on the machine — status, pull,
//! commit, push — and nothing more clever than that.
//!
//! Two rules it keeps. Everything is scoped to the collection directory, so a
//! collection inside a larger repository never sweeps up someone else's work.
//! And a pull is `--ff-only`: a merge that needs resolving belongs in the
//! user's editor, not behind a button in an API client.

use std::path::Path;
use std::process::Command;

use serde::Serialize;

use crate::collection;
use crate::error::{Error, Result};
use crate::secrets;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// False when git is missing or the collection is not in a repository —
    /// both are fine, they just mean there is nothing to sync.
    pub repository: bool,
    /// Why not, when `repository` is false.
    pub why: Option<String>,
    pub branch: String,
    pub remote: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// Changed or untracked files, relative to the collection.
    pub changed: Vec<String>,
    /// Whether `.gitignore` keeps the secret files out. False is a warning
    /// worth stopping for before anything is pushed.
    pub secrets_ignored: bool,
}

fn git(root: &Path, args: &[&str]) -> Result<std::process::Output> {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| Error::Invalid(format!("git could not be run: {e}")))
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim().to_string()
}

fn failed(output: &std::process::Output) -> Error {
    let message = text(&output.stderr);
    Error::Invalid(if message.is_empty() { text(&output.stdout) } else { message })
}

pub fn status(root: &Path) -> Result<Status> {
    let not_a_repo = |why: &str| Status {
        repository: false,
        why: Some(why.to_string()),
        branch: String::new(),
        remote: None,
        ahead: 0,
        behind: 0,
        changed: Vec::new(),
        secrets_ignored: secrets_ignored(root),
    };

    let inside = match git(root, &["rev-parse", "--is-inside-work-tree"]) {
        Err(_) => return Ok(not_a_repo("git is not installed, or not on the PATH")),
        Ok(output) if !output.status.success() => {
            return Ok(not_a_repo("this collection is not inside a git repository"))
        }
        Ok(output) => output,
    };
    let _ = inside;

    // `-- .` keeps it to the collection, even when the repository is larger.
    let output = git(root, &["status", "--porcelain=v1", "-b", "--", "."])?;
    if !output.status.success() {
        return Err(failed(&output));
    }
    let listing = text(&output.stdout);
    let mut lines = listing.lines();

    let header = lines.next().unwrap_or("").trim_start_matches("## ");
    let (branch, ahead, behind) = read_branch(header);
    let changed: Vec<String> =
        lines.map(|line| line.get(3..).unwrap_or(line).trim().to_string()).filter(|l| !l.is_empty()).collect();

    let remote = git(root, &["remote", "get-url", "origin"])
        .ok()
        .filter(|output| output.status.success())
        .map(|output| text(&output.stdout))
        .filter(|url| !url.is_empty());

    Ok(Status {
        repository: true,
        why: None,
        branch,
        remote,
        ahead,
        behind,
        changed,
        secrets_ignored: secrets_ignored(root),
    })
}

/// `main...origin/main [ahead 2, behind 1]`
fn read_branch(header: &str) -> (String, u32, u32) {
    let (names, tracking) = header.split_once(" [").unwrap_or((header, ""));
    let branch = names.split("...").next().unwrap_or(names).trim().to_string();

    let count = |what: &str| -> u32 {
        tracking
            .split(&[',', ']'][..])
            .find_map(|part| part.trim().strip_prefix(what))
            .and_then(|n| n.trim().parse().ok())
            .unwrap_or(0)
    };
    (branch, count("ahead "), count("behind "))
}

/// Every secrets file in the collection root that `.gitignore` does not cover.
///
/// Derived from what is actually on disk, not from two guessed names: a
/// teammate who follows the README and hand-copies `.env.prod.example` to
/// `.env.prod` never opens the environment editor, so volt never writes the
/// `.env.*` rule — and a check that only probes `.env` and `.env.local` calls
/// that repository safe. The two literals stay as a floor so a collection with
/// no `.gitignore` at all still reads as uncovered.
fn uncovered_secret_files(root: &Path) -> Vec<String> {
    let gitignore = std::fs::read_to_string(root.join(".gitignore")).unwrap_or_default();
    let mut names: Vec<String> = vec![secrets::LEGACY_FILE.to_string(), ".env.local".to_string()];
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_secrets = name == secrets::LEGACY_FILE || name.starts_with(".env.");
            // `.example` templates are the part meant to be committed.
            if is_secrets && !name.ends_with(".example") && !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names.retain(|name| !secrets::is_ignored(&gitignore, name));
    names
}

fn secrets_ignored(root: &Path) -> bool {
    uncovered_secret_files(root).is_empty()
}

/// Fast-forward only. A pull that would need a merge stops here and says so:
/// resolving one belongs in the user's editor.
pub fn pull(root: &Path) -> Result<String> {
    let output = git(root, &["pull", "--ff-only"])?;
    if output.status.success() {
        return Ok(text(&output.stdout));
    }
    let message = text(&output.stderr);
    Err(Error::Invalid(if message.contains("fatal: Not possible to fast-forward") || message.contains("diverged") {
        "the remote has changes that do not fast-forward. Sort it out in your editor, then come back.".into()
    } else {
        message
    }))
}

/// Stage everything under the collection, commit, and push when asked.
///
/// Refuses while the secret files are not ignored: pushing a token is not a
/// mistake anyone should be able to make from a button.
pub fn commit(root: &Path, message: &str, push: bool) -> Result<String> {
    let uncovered = uncovered_secret_files(root);
    if !uncovered.is_empty() {
        return Err(Error::Invalid(format!(
            "`.gitignore` does not cover {}, so this would commit secret values. Save an environment once and volt writes the rule.",
            uncovered.join(", ")
        )));
    }
    let message = message.trim();
    if message.is_empty() {
        return Err(Error::Invalid("a commit needs a message".into()));
    }

    let staged = git(root, &["add", "--", "."])?;
    if !staged.status.success() {
        return Err(failed(&staged));
    }

    let committed = git(root, &["commit", "-m", message, "--", "."])?;
    let mut said = text(&committed.stdout);
    if !committed.status.success() {
        let stderr = text(&committed.stderr);
        // Nothing staged is not a failure, it is just nothing to do.
        if !said.contains("nothing to commit") && !stderr.contains("nothing to commit") {
            return Err(Error::Invalid(if stderr.is_empty() { said } else { stderr }));
        }
        said = "Nothing to commit.".into();
    }

    if push {
        let pushed = git(root, &["push"])?;
        if !pushed.status.success() {
            return Err(failed(&pushed));
        }
        said.push_str("\nPushed.");
    }
    Ok(said)
}

/// Write `.env.<environment>.example` files: the names a teammate has to fill
/// in, with no values. The real files stay gitignored, and the template is the
/// only part that should ever be committed.
pub fn write_env_templates(root: &Path) -> Result<Vec<String>> {
    let collection = collection::load(root)?;
    let mut written = Vec::new();

    for environment in &collection.environments {
        let secrets: Vec<&crate::model::EnvVar> =
            environment.vars.iter().filter(|var| var.secret).collect();
        if secrets.is_empty() {
            continue;
        }

        let stem = collection::slug(&environment.name);
        let name = format!(".env.{stem}.example");
        let mut text = format!(
            "# Secret values for `{}`. Copy this to `.env.{stem}` and fill it in;\n\
             # that file is gitignored and stays on your machine.\n",
            environment.name
        );
        for var in secrets {
            text.push_str(&format!("{}=\"\"\n", var.name));
        }

        let path = root.join(&name);
        std::fs::write(&path, text).map_err(|e| Error::io(path.to_string_lossy().into_owned(), e))?;
        written.push(name);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_branch_line_says_where_it_stands() {
        assert_eq!(read_branch("main...origin/main [ahead 2, behind 1]"), ("main".into(), 2, 1));
        assert_eq!(read_branch("main...origin/main [ahead 3]"), ("main".into(), 3, 0));
        assert_eq!(read_branch("main...origin/main"), ("main".into(), 0, 0));
        assert_eq!(read_branch("No commits yet on main"), ("No commits yet on main".into(), 0, 0));
    }

    #[test]
    fn a_folder_that_is_not_a_repository_is_not_an_error() {
        let root = std::env::temp_dir().join(format!("volt-git-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(&root).unwrap();

        let status = status(&root).unwrap();
        assert!(!status.repository);
        assert!(status.why.is_some(), "it says why, rather than looking broken");
        assert!(!status.secrets_ignored, "and a missing .gitignore is not cover");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn nothing_is_committed_while_the_secret_files_are_unignored() {
        let root = std::env::temp_dir().join(format!("volt-git-unsafe-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(&root).unwrap();

        let refused = commit(&root, "anything", false).unwrap_err();
        assert!(format!("{refused}").contains(".gitignore"), "{refused}");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_template_lists_the_names_to_fill_in_and_none_of_the_values() {
        let root = std::env::temp_dir().join(format!("volt-git-share-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(root.join("environments")).unwrap();
        std::fs::write(root.join("collection.yaml"), "name: Share\nversion: 1\n").unwrap();

        let environment = crate::model::Environment {
            name: "prod".into(),
            vars: vec![
                crate::model::EnvVar { name: "baseUrl".into(), value: "https://x".into(), secret: false },
                crate::model::EnvVar { name: "token".into(), value: "live-value".into(), secret: true },
            ],
        };
        collection::write_environment(&root, &environment, None).unwrap();

        let written = write_env_templates(&root).unwrap();
        assert_eq!(written, [".env.prod.example"]);

        let template = std::fs::read_to_string(root.join(".env.prod.example")).unwrap();
        assert!(template.contains("token=\"\""), "{template}");
        assert!(!template.contains("live-value"), "a template carries names, never values");
        assert!(!template.contains("baseUrl"), "and only the secret ones: the rest are in the YAML");
        std::fs::remove_dir_all(&root).ok();
    }

    /// The shape a teammate lands in: the repo ships a `.gitignore` from before
    /// per-environment files, they hand-copy `.env.prod.example` following the
    /// README, and never open the environment editor. Probing two fixed names
    /// used to call that safe.
    #[test]
    fn a_hand_made_secrets_file_is_named_in_the_refusal() {
        let root = std::env::temp_dir().join(format!("volt-git-uncovered-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(&root).unwrap();

        std::fs::write(root.join(".gitignore"), ".env\n.env.local\n").unwrap();
        std::fs::write(root.join(".env.prod"), "token=\"live\"\n").unwrap();
        std::fs::write(root.join(".env.prod.example"), "token=\n").unwrap();

        assert_eq!(uncovered_secret_files(&root), vec![".env.prod".to_string()]);
        let refusal = commit(&root, "anything", false).unwrap_err().to_string();
        assert!(refusal.contains(".env.prod"), "it says which file: {refusal}");
        assert!(!refusal.contains(".example"), "a template is meant to be committed: {refusal}");

        // Covering it is enough; the refusal is about secrets, not about git.
        std::fs::write(root.join(".gitignore"), ".env\n.env.*\n").unwrap();
        assert!(uncovered_secret_files(&root).is_empty());

        std::fs::remove_dir_all(&root).ok();
    }
}
