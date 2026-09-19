//! Reading and writing a collection directory.
//!
//! Layout:
//! ```text
//! my-api/
//!   collection.yaml        # name + collection-wide headers/auth
//!   .env.local             # gitignored; secret values for local.yaml
//!   .env.prod              # gitignored; secret values for prod.yaml
//!   environments/
//!     local.yaml
//!     prod.yaml
//!   auth/
//!     folder.yaml          # optional: display name + ordering
//!     login.yaml           # one request
//!   users/
//!     list-users.yaml
//! ```

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
// The only place the YAML backend is named, so swapping it stays a one-line change.
use serde_yaml_ng as yaml;

use crate::error::{Error, Result};
use crate::model::{Auth, Body, Check, CollectionMeta, Environment, EnvVar, FolderMeta, KeyValue, Request};
use crate::secrets;

pub const COLLECTION_FILE: &str = "collection.yaml";
pub const FOLDER_FILE: &str = "folder.yaml";
pub const ENV_DIR: &str = "environments";

/// A sidebar entry. `id` is the collection-relative path, which gives us a
/// stable identifier without maintaining a separate index file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Node {
    Folder {
        id: String,
        name: String,
        seq: u32,
        children: Vec<Node>,
    },
    Request {
        id: String,
        name: String,
        seq: u32,
        method: String,
    },
}

impl Node {
    fn sort_key(&self) -> (u32, String) {
        match self {
            Node::Folder { seq, name, .. } | Node::Request { seq, name, .. } => {
                (*seq, name.to_lowercase())
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Collection {
    pub root: String,
    pub meta: CollectionMeta,
    pub tree: Vec<Node>,
    pub environments: Vec<Environment>,
    /// Files under the root that did not parse. They are left out of the tree
    /// rather than refusing to open the whole collection — one hand-edited
    /// request with a stray tab used to lock the user out of everything — but
    /// they are named, because a request that vanishes without a word is worse.
    #[serde(default)]
    pub problems: Vec<String>,
}

pub fn load(root: &Path) -> Result<Collection> {
    let meta_path = root.join(COLLECTION_FILE);
    let meta: CollectionMeta = read_yaml(&meta_path)?;

    let mut problems = Vec::new();
    let tree = read_dir_nodes(root, root, &mut problems)?;
    Ok(Collection {
        root: root.to_string_lossy().into_owned(),
        meta,
        tree,
        environments: load_environments(root)?,
        problems,
    })
}

/// Create an empty collection on disk, including the gitignore that keeps
/// secret values out of version control.
pub fn init(root: &Path, name: &str) -> Result<Collection> {
    fs::create_dir_all(root.join(ENV_DIR)).map_err(|e| Error::io(path_str(root), e))?;

    let meta = CollectionMeta { name: name.to_string(), ..Default::default() };
    write_yaml(&root.join(COLLECTION_FILE), &meta)?;

    ensure_gitignore(root)?;

    let default_env = Environment { name: "local".into(), vars: Vec::new() };
    write_yaml(&root.join(ENV_DIR).join("local.yaml"), &default_env)?;

    load(root)
}

/// Every new collection gets this, whether created empty or imported.
pub(crate) fn ensure_gitignore(root: &Path) -> Result<()> {
    let gitignore = root.join(".gitignore");
    if !gitignore.exists() {
        fs::write(&gitignore, "# Secret values, never commit these\n.env\n.env.*\n")
            .map_err(|e| Error::io(path_str(&gitignore), e))?;
    }
    Ok(())
}

/// The folder volt opens when it has nothing else to open. Made on first
/// launch under the user's documents, so "New request" works before anyone
/// has decided where their requests should live; a collection kept in a
/// repository is opened over it the moment there is one.
pub const DEFAULT_NAME: &str = "Personal";

/// `<base>/volt/personal`, created as a collection if it is not one yet.
/// Never touches one that already exists — the point is the files inside it.
pub fn ensure_default(base: &Path) -> Result<PathBuf> {
    let root = base.join("volt").join("personal");
    if !root.join(COLLECTION_FILE).exists() {
        fs::create_dir_all(&root).map_err(|e| Error::io(path_str(&root), e))?;
        init(&root, DEFAULT_NAME)?;
        // One request, so the first thing anyone sees is a response rather
        // than an empty tree. Deleting it is a delete like any other; it is
        // never put back.
        let hello = Request {
            name: "Hello, world".into(),
            method: "GET".into(),
            url: "https://jsonplaceholder.typicode.com/todos/1".into(),
            checks: vec![Check {
                from: "status".into(),
                op: crate::checks::Op::Is,
                value: "200".into(),
                enabled: true,
            }],
            docs: Some(
                "Press Send. This request is here so the first thing you see is a response — delete it whenever."
                    .into(),
            ),
            ..Default::default()
        };
        write_request(&root, "hello.yaml", &hello)?;
    }
    Ok(root)
}

/// A request found by what is *in* it, and where.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub id: String,
    pub name: String,
    pub method: String,
    /// "URL", "param limit", "header X-Api-Key", "body", "docs".
    pub found_in: String,
}

/// Every request whose URL, params, headers, body or notes contain `query`,
/// case-insensitively. Names are not searched here — the tree already knows
/// those — and auth values are not either: they are `{{names}}` by design,
/// and a literal there is the one thing not worth making easy to find.
pub fn search(root: &Path, query: &str) -> Result<Vec<SearchHit>> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Ok(Vec::new());
    }
    let mut ids: Vec<String> = Vec::new();
    fn walk(nodes: &[Node], out: &mut Vec<String>) {
        for node in nodes {
            match node {
                Node::Folder { children, .. } => walk(children, out),
                Node::Request { id, .. } => out.push(id.clone()),
            }
        }
    }
    walk(&load(root)?.tree, &mut ids);

    let mut hits = Vec::new();
    for id in ids {
        let Ok(request) = read_request(root, &id) else { continue };
        let mut places: Vec<(String, String)> = vec![("URL".into(), request.url.clone())];
        for p in &request.params {
            places.push((format!("param {}", p.name), format!("{}={}", p.name, p.value)));
        }
        for h in &request.headers {
            places.push((format!("header {}", h.name), format!("{}: {}", h.name, h.value)));
        }
        match &request.body {
            Body::Text { content } | Body::Json { content } | Body::Xml { content } => places.push(("body".into(), content.clone())),
            Body::UrlEncoded { fields } => {
                for f in fields {
                    places.push((format!("field {}", f.name), format!("{}={}", f.name, f.value)));
                }
            }
            Body::Form { fields } => {
                for f in fields {
                    places.push((format!("field {}", f.name), format!("{}={}", f.name, f.value)));
                }
            }
            Body::GraphQl { query, variables } => places.push(("body".into(), format!("{query}\n{variables}"))),
            Body::Grpc { message, method, .. } => places.push(("body".into(), format!("{method}\n{message}"))),
            Body::Binary { path } => places.push(("body".into(), path.clone())),
            Body::None => {}
        }
        if let Some(docs) = &request.docs {
            places.push(("docs".into(), docs.clone()));
        }
        if let Some((found_in, _)) = places.iter().find(|(_, text)| text.to_lowercase().contains(&needle)) {
            hits.push(SearchHit { id, name: request.name.clone(), method: request.method.clone(), found_in: found_in.clone() });
            if hits.len() >= 100 {
                break;
            }
        }
    }
    Ok(hits)
}

/// Copy a request into another collection's root, under its own file name or
/// the first free numbered one. The folders it lived in are not recreated:
/// the other collection has its own shape, and a copy is a starting point.
pub fn copy_request_to(root: &Path, id: &str, target_root: &Path) -> Result<String> {
    let request = read_request(root, id)?;
    if !target_root.join(COLLECTION_FILE).is_file() {
        return Err(Error::Invalid(format!("{} is not a collection", target_root.display())));
    }
    let stem = Path::new(id).file_stem().and_then(|s| s.to_str()).unwrap_or("request").to_string();
    let mut candidate = format!("{stem}.yaml");
    let mut n = 2;
    while target_root.join(&candidate).exists() {
        candidate = format!("{stem}-{n}.yaml");
        n += 1;
    }
    write_request(target_root, &candidate, &request)?;
    Ok(candidate)
}

/// Parse arbitrary YAML (an import source, not the file format) into JSON
/// values. Lives here so this file stays the only one naming the YAML crate.
pub(crate) fn yaml_value(text: &str) -> std::result::Result<serde_json::Value, String> {
    yaml::from_str(text).map_err(|e| e.to_string())
}

/// The three fields the tree actually shows.
///
/// Walking the collection used to deserialize every request in full — headers,
/// body, auth, captures, checks — and every mutation walks it again. On a
/// collection of a couple of thousand requests that is the slowest thing the
/// app does, for three strings. `seq` repeats `Request`'s default on purpose:
/// ARCHITECTURE.md warns that a numeric default which disagrees with its struct is
/// how ordering silently drifts.
#[derive(Deserialize)]
struct TreeEntry {
    #[serde(default)]
    name: String,
    #[serde(default = "tree_seq")]
    seq: u32,
    #[serde(default)]
    method: String,
}

fn tree_seq() -> u32 {
    1
}

fn read_dir_nodes(dir: &Path, root: &Path, problems: &mut Vec<String>) -> Result<Vec<Node>> {
    let entries = fs::read_dir(dir).map_err(|e| Error::io(path_str(dir), e))?;
    let mut nodes = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|e| Error::io(path_str(dir), e))?;
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().into_owned();

        if file_name.starts_with('.') {
            continue;
        }

        if path.is_dir() {
            // `environments/` is loaded separately, not shown in the tree.
            if dir == root && file_name == ENV_DIR {
                continue;
            }
            let folder: FolderMeta = read_yaml_or_default(&path.join(FOLDER_FILE))?;
            nodes.push(Node::Folder {
                id: relative_id(root, &path),
                name: folder.name.unwrap_or(file_name),
                seq: folder.seq,
                children: read_dir_nodes(&path, root, problems)?,
            });
        } else if is_yaml(&path) {
            if file_name == FOLDER_FILE || (dir == root && file_name == COLLECTION_FILE) {
                continue;
            }
            // A file that does not parse is not a node. One hand-edited request
            // with a stray tab in it used to make the whole collection refuse to
            // open, with nothing to click on and no way to find the file.
            let request: TreeEntry = match read_yaml(&path) {
                Ok(request) => request,
                Err(Error::Parse { .. }) => {
                    problems.push(relative_id(root, &path));
                    continue;
                }
                Err(e) => return Err(e),
            };
            nodes.push(Node::Request {
                id: relative_id(root, &path),
                name: request.name,
                seq: request.seq,
                method: request.method.to_uppercase(),
            });
        }
    }

    nodes.sort_by_key(|n| n.sort_key());
    Ok(nodes)
}

pub fn read_request(root: &Path, id: &str) -> Result<Request> {
    read_yaml(&resolve(root, id)?)
}

pub fn write_request(root: &Path, id: &str, request: &Request) -> Result<()> {
    let path = resolve(root, id)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::io(path_str(parent), e))?;
    }
    write_yaml(&path, request)
}

/// Create a folder inside `parent` (`None` for the collection root) and return
/// its id.
///
/// The name the user typed is kept as the display name in `folder.yaml`; the
/// directory itself gets a filesystem-safe version of it, so a name containing
/// `:` or `?` works on every platform and a clash gets a `-2` suffix instead of
/// silently merging into an existing folder.
pub fn create_folder(root: &Path, parent: Option<&str>, name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::Invalid("folder name cannot be empty".into()));
    }

    let parent_dir = match parent {
        Some(parent) => resolve(root, parent)?,
        None => root.to_path_buf(),
    };
    if !parent_dir.is_dir() {
        return Err(Error::Invalid(format!(
            "`{}` is not a folder",
            parent.unwrap_or("the collection root")
        )));
    }

    let base = folder_dir_name(name);
    let taken = |candidate: &str| {
        // At the root, `environments` is reserved even before it exists.
        parent_dir.join(candidate).exists() || (parent.is_none() && candidate == ENV_DIR)
    };
    let mut dir_name = base.clone();
    let mut n = 2;
    while taken(&dir_name) {
        dir_name = format!("{base}-{n}");
        n += 1;
    }

    let path = parent_dir.join(&dir_name);
    fs::create_dir(&path).map_err(|e| Error::io(path_str(&path), e))?;

    // Only write folder.yaml when the directory name does not already say it.
    if dir_name != name {
        let meta = FolderMeta { name: Some(name.to_string()), ..Default::default() };
        write_yaml(&path.join(FOLDER_FILE), &meta)?;
    }

    Ok(relative_id(root, &path))
}

/// Write `collection.yaml`.
pub fn write_meta(root: &Path, meta: &CollectionMeta) -> Result<()> {
    write_yaml(&root.join(COLLECTION_FILE), meta)
}

/// A folder's own settings. Missing `folder.yaml` is not an error: a folder
/// that has nothing to say does not need a file.
pub fn folder_meta(root: &Path, id: &str) -> Result<FolderMeta> {
    read_yaml_or_default(&resolve(root, id)?.join(FOLDER_FILE))
}

/// Write a folder's settings, removing the file when there is nothing left to
/// keep in it.
pub fn write_folder_meta(root: &Path, id: &str, meta: &FolderMeta) -> Result<()> {
    let path = resolve(root, id)?.join(FOLDER_FILE);
    let empty = meta.name.is_none()
        && meta.seq == FolderMeta::default().seq
        && meta.headers.is_empty()
        && meta.vars.is_empty()
        && matches!(meta.auth, Auth::Inherit);

    if empty {
        return match fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(Error::io(path_str(&path), e)),
            _ => Ok(()),
        };
    }
    write_yaml(&path, meta)
}


/// Where a deleted node goes. Inside the collection, so it is on the same
/// volume and a move cannot fail half way; dotted, so the tree skips it.
pub const TRASH_DIR: &str = ".trash";

/// What was deleted, and enough to put it back.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Deleted {
    /// Where it was.
    pub id: String,
    pub name: String,
    pub folder: bool,
    /// Where it is now, relative to the collection.
    pub at: String,
}

/// Delete a request or a folder by moving it into `.trash/`.
///
/// Not `remove_dir_all`: a folder deleted by accident is a day's work, and an
/// undo that only lives until the app closes is worse than none because people
/// rely on it. The files are still there, gitignored, until the user empties
/// the bin or deletes the collection.
pub fn delete_node(root: &Path, id: &str) -> Result<Deleted> {
    let path = reserved_check(root, id)?;
    let folder = path.is_dir();
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| id.to_string());

    let bin = root.join(TRASH_DIR);
    fs::create_dir_all(&bin).map_err(|e| Error::io(path_str(&bin), e))?;
    ensure_trash_ignored(root)?;

    // Stamped, so deleting two things of the same name does not lose the first.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let mut kept = format!("{stamp}-{name}");
    let mut n = 2;
    while bin.join(&kept).exists() {
        kept = format!("{stamp}-{n}-{name}");
        n += 1;
    }

    let to = bin.join(&kept);
    fs::rename(&path, &to).map_err(|e| Error::io(path_str(&path), e))?;

    Ok(Deleted {
        id: id.to_string(),
        name: display_name(root, &to, folder).unwrap_or(name),
        folder,
        at: format!("{TRASH_DIR}/{kept}"),
    })
}

/// Put a deleted node back where it was. Refuses if something has taken its
/// place: quietly overwriting is how an undo destroys the work it was meant
/// to save.
pub fn restore_node(root: &Path, deleted: &Deleted) -> Result<()> {
    let from = resolve(root, &deleted.at)?;
    if !from.exists() {
        return Err(Error::Invalid("that is no longer in the bin".into()));
    }

    let to = resolve(root, &deleted.id)?;
    if to.exists() {
        return Err(Error::Invalid(format!("`{}` is taken again, so this was left in the bin", deleted.id)));
    }
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::io(path_str(parent), e))?;
    }
    fs::rename(&from, &to).map_err(|e| Error::io(path_str(&from), e))
}

/// The name as the user knew it, which is in the file rather than the path.
fn display_name(root: &Path, path: &Path, folder: bool) -> Option<String> {
    let _ = root;
    if folder {
        let meta: FolderMeta = read_yaml_or_default(&path.join(FOLDER_FILE)).ok()?;
        return meta.name;
    }
    let request: Request = read_yaml(path).ok()?;
    Some(request.name)
}

/// The bin is working state, never something to commit.
fn ensure_trash_ignored(root: &Path) -> Result<()> {
    let path = root.join(".gitignore");
    let current = fs::read_to_string(&path).unwrap_or_default();
    if current.lines().any(|line| line.trim() == TRASH_DIR || line.trim() == "/.trash" || line.trim() == ".trash/") {
        return Ok(());
    }

    let mut next = current;
    if !next.is_empty() && !next.ends_with('\n') {
        next.push('\n');
    }
    next.push_str("# Deleted requests, until the bin is emptied\n.trash/\n");
    fs::write(&path, next).map_err(|e| Error::io(path_str(&path), e))
}

/// Everything in the bin, newest first.
pub fn list_trash(root: &Path) -> Result<Vec<Deleted>> {
    let bin = root.join(TRASH_DIR);
    if !bin.is_dir() {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    for entry in fs::read_dir(&bin).map_err(|e| Error::io(path_str(&bin), e))? {
        let entry = entry.map_err(|e| Error::io(path_str(&bin), e))?;
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().into_owned();
        let folder = path.is_dir();

        // `<stamp>-<name>`: the name it had is after the first dash.
        let original = file_name.split_once('-').map(|(_, rest)| rest.to_string()).unwrap_or_else(|| file_name.clone());
        out.push(Deleted {
            // Where it came from is not recorded on disk, so restoring an old
            // one puts it back at the root under its own name.
            id: original.clone(),
            name: display_name(root, &path, folder).unwrap_or(original),
            folder,
            at: format!("{TRASH_DIR}/{file_name}"),
        });
    }

    out.sort_by(|a, b| b.at.cmp(&a.at));
    Ok(out)
}

/// Throw the bin away for good.
pub fn empty_trash(root: &Path) -> Result<()> {
    let bin = root.join(TRASH_DIR);
    match fs::remove_dir_all(&bin) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(Error::io(path_str(&bin), e)),
        _ => Ok(()),
    }
}


/// Change the display name of a request or folder.
///
/// The file keeps its own name, so the node's `id` does not change and git sees
/// a one-line edit rather than a rename. For a folder that means writing
/// `folder.yaml`, which is created if it was not there before.
pub fn rename_node(root: &Path, id: &str, name: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::Invalid("name cannot be empty".into()));
    }

    let path = reserved_check(root, id)?;
    if path.is_dir() {
        let meta_path = path.join(FOLDER_FILE);
        let mut meta: FolderMeta = read_yaml_or_default(&meta_path)?;
        meta.name = Some(name.to_string());
        write_yaml(&meta_path, &meta)
    } else {
        let mut request: Request = read_yaml(&path)?;
        request.name = name.to_string();
        write_yaml(&path, &request)
    }
}

/// Move a request or folder into another folder. `new_parent` is `None` for the
/// collection root. Returns the node's new id.
pub fn move_node(root: &Path, id: &str, new_parent: Option<&str>) -> Result<String> {
    let from = reserved_check(root, id)?;
    if !from.exists() {
        return Err(Error::Invalid(format!("`{id}` no longer exists")));
    }

    // `environments/` is loaded separately and its files are not requests.
    if new_parent == Some(ENV_DIR) {
        return Err(Error::Invalid(format!("`{ENV_DIR}` is not part of the tree")));
    }

    let parent_dir = match new_parent {
        Some(parent) => resolve(root, parent)?,
        None => root.to_path_buf(),
    };
    if !parent_dir.is_dir() {
        return Err(Error::Invalid(format!(
            "`{}` is not a folder",
            new_parent.unwrap_or("the collection root")
        )));
    }

    let file_name = from
        .file_name()
        .ok_or_else(|| Error::Invalid(format!("`{id}` has no file name")))?
        .to_owned();
    let to = parent_dir.join(&file_name);

    if to == from {
        return Ok(id.to_string());
    }
    // Moving a folder under itself would detach the whole subtree.
    if from.is_dir() && to.starts_with(&from) {
        return Err(Error::Invalid("a folder cannot be moved inside itself".into()));
    }
    if to.exists() {
        return Err(Error::Invalid(format!(
            "`{}` already exists in the destination",
            file_name.to_string_lossy()
        )));
    }

    fs::rename(&from, &to).map_err(|e| Error::io(path_str(&to), e))?;

    // `.examples/` mirrors the tree, so the kept responses have to come along.
    // Left behind, they would sit at the old id waiting for whatever request
    // lands there next and be shown as its own.
    let new_id = relative_id(root, &to);
    crate::examples::move_for(root, id, &new_id)?;
    Ok(new_id)
}

/// Number the children of `parent` so they sort in the order supplied.
///
/// Only the files whose number actually changes are rewritten, so dragging one
/// request a couple of rows does not show up as a diff on every sibling.
pub fn reorder(root: &Path, parent: Option<&str>, ordered: &[String]) -> Result<()> {
    for (index, id) in ordered.iter().enumerate() {
        let seq = index as u32 + 1;
        let path = reserved_check(root, id)?;

        // A node can only be ordered among its own siblings.
        if parent_id(id) != parent {
            return Err(Error::Invalid(format!(
                "`{id}` is not in the folder being reordered"
            )));
        }

        if path.is_dir() {
            let meta_path = path.join(FOLDER_FILE);
            // A folder with no folder.yaml already reads as the default seq, so
            // matching that value means there is nothing to write.
            let mut meta: FolderMeta = read_yaml_or_default(&meta_path)?;
            if meta.seq == seq {
                continue;
            }
            meta.seq = seq;
            write_yaml(&meta_path, &meta)?;
        } else {
            let mut request: Request = read_yaml(&path)?;
            if request.seq == seq {
                continue;
            }
            request.seq = seq;
            write_yaml(&path, &request)?;
        }
    }
    Ok(())
}

/// The folder part of an id, or `None` when it sits at the collection root.
fn parent_id(id: &str) -> Option<&str> {
    id.rfind('/').map(|cut| &id[..cut])
}

/// `collection.yaml` and `folder.yaml` describe the tree rather than sitting in
/// it, so they must not be moved or renamed as if they were nodes.
fn reserved_check(root: &Path, id: &str) -> Result<PathBuf> {
    let path = resolve(root, id)?;
    let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned());
    match file_name.as_deref() {
        Some(FOLDER_FILE) | Some(COLLECTION_FILE) => {
            Err(Error::Invalid(format!("`{id}` is not a node in the tree")))
        }
        _ => Ok(path),
    }
}

// ---------------------------------------------------------------------------
// Environments and secrets
// ---------------------------------------------------------------------------

/// Every environment file with its stem (`prod` for `prod.yaml`). The stem, not
/// the name inside, is the environment's identity on disk: it names the file
/// its secrets live in.
fn environment_files(root: &Path) -> Result<Vec<(String, Environment)>> {
    let dir = root.join(ENV_DIR);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|e| Error::io(path_str(&dir), e))? {
        let path = entry.map_err(|e| Error::io(path_str(&dir), e))?.path();
        if !is_yaml(&path) {
            continue;
        }
        let Some(stem) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
            continue;
        };
        files.push((stem, read_yaml::<Environment>(&path)?));
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}

fn load_environments(root: &Path) -> Result<Vec<Environment>> {
    let legacy = secrets::read(&root.join(secrets::LEGACY_FILE), true)?.unwrap_or_default();

    let mut environments = Vec::new();
    for (stem, mut env) in environment_files(root)? {
        let own = secrets::read(&secrets::file_for(root, &stem), false)?.unwrap_or_default();
        // Secrets are blank in the YAML. Take them from this environment's own
        // file, falling back to the shared `.env` of older collections.
        for var in env.vars.iter_mut().filter(|v| v.secret) {
            var.value = own.get(&var.name).or_else(|| legacy.get(&var.name)).unwrap_or_default().to_string();
        }
        environments.push(env);
    }

    environments.sort_by_key(|environment| environment.name.to_lowercase());
    Ok(environments)
}

/// Save an environment: plain values to `environments/<stem>.yaml`, secret
/// values to `.env.<stem>`.
///
/// `previous` is the name it was loaded under, or `None` for a new one. A
/// changed name moves both files instead of leaving the old ones behind, and
/// saving onto another environment's file is refused rather than overwriting it.
pub fn write_environment(root: &Path, env: &Environment, previous: Option<&str>) -> Result<()> {
    if env.name.trim().is_empty() {
        return Err(Error::Invalid("environment name cannot be empty".into()));
    }
    for var in env.vars.iter().filter(|v| v.secret) {
        secrets::validate_name(&var.name)?;
    }

    let dir = root.join(ENV_DIR);
    fs::create_dir_all(&dir).map_err(|e| Error::io(path_str(&dir), e))?;
    let files = environment_files(root)?;

    // Where this environment lives now. Prefer the file its name would slug to,
    // in case an old rename left two files carrying the same name.
    let existing: Option<&(String, Environment)> = previous.and_then(|previous| {
        let matching = || files.iter().filter(move |(_, e)| e.name == previous);
        matching().find(|(stem, _)| *stem == slug(previous)).or_else(|| matching().next())
    });

    // An unchanged name keeps its file, even one named by hand; a new or
    // changed name gets the slug.
    let stem = match existing {
        Some((stem, _)) if previous == Some(env.name.as_str()) => stem.clone(),
        _ => slug(&env.name),
    };
    let moved_from = existing.map(|(old, _)| old.as_str()).filter(|old| *old != stem);

    let taken_by_other = files.iter().any(|(s, _)| *s == stem) && existing.is_none_or(|(s, _)| *s != stem);
    if taken_by_other {
        return Err(Error::Invalid(format!(
            "an environment is already saved as `{ENV_DIR}/{stem}.yaml`; choose a different name"
        )));
    }

    // Older collections share one `.env`. Give every other environment its own
    // copy first, so nothing depends on the shared file once this save is done.
    let skip: Vec<&str> = [Some(stem.as_str()), moved_from].into_iter().flatten().collect();
    let legacy = migrate_legacy_secrets(root, &files, &skip)?;

    // This environment's secrets file, carried across a rename.
    let own_path = secrets::file_for(root, &stem);
    let mut own = secrets::read(&own_path, false)?.unwrap_or_else(|| secrets::DotEnv::for_environment(&stem));
    let old_secrets_path = moved_from.map(|old| secrets::file_for(root, old));
    if let Some(old_path) = &old_secrets_path {
        if let Some(old) = secrets::read(old_path, false)? {
            own.merge_from(&old);
        }
    }

    // Every name this environment declared before or declares now belongs to
    // it. Those no longer secret (or deleted) come out; hand-added keys stay.
    let secret_values: HashMap<&str, &str> =
        env.vars.iter().filter(|v| v.secret).map(|v| (v.name.as_str(), v.value.as_str())).collect();
    let owned = existing
        .into_iter()
        .flat_map(|(_, e)| e.vars.iter().map(|v| v.name.clone()))
        .chain(env.vars.iter().map(|v| v.name.clone()));
    for name in owned {
        if !secret_values.contains_key(name.as_str()) {
            own.remove(&name);
        }
    }
    for var in env.vars.iter().filter(|v| v.secret) {
        own.set(&var.name, &var.value);
    }

    let mut redacted = env.clone();
    for var in redacted.vars.iter_mut().filter(|v| v.secret) {
        var.value.clear();
    }

    // Secrets before YAML: if the YAML write fails, the values are still safe.
    secrets::write(root, &own_path, &own)?;
    write_yaml(&dir.join(format!("{stem}.yaml")), &redacted)?;

    if let Some(old) = moved_from {
        let old_yaml = dir.join(format!("{old}.yaml"));
        fs::remove_file(&old_yaml).map_err(|e| Error::io(path_str(&old_yaml), e))?;
        if let Some(old_path) = old_secrets_path.filter(|p| p.exists()) {
            fs::remove_file(&old_path).map_err(|e| Error::io(path_str(&old_path), e))?;
        }
    }

    // Finally drop from the shared file whatever every environment now holds
    // itself. Keys no environment declares are left alone.
    if let Some(mut legacy) = legacy {
        let declared = files
            .iter()
            .flat_map(|(_, e)| e.vars.iter().filter(|v| v.secret).map(|v| v.name.clone()))
            .chain(secret_values.keys().map(|k| k.to_string()));
        for name in declared {
            legacy.remove(&name);
        }
        let legacy_path = root.join(secrets::LEGACY_FILE);
        // The shared file is rewritten, not deleted, when something hand-added
        // is still in it — so it has to be covered by .gitignore like any other
        // secrets file.
        secrets::ensure_ignored(root, secrets::LEGACY_FILE)?;
        if legacy_is_empty(&legacy) {
            fs::remove_file(&legacy_path).map_err(|e| Error::io(path_str(&legacy_path), e))?;
        } else {
            fs::write(&legacy_path, legacy.render()).map_err(|e| Error::io(path_str(&legacy_path), e))?;
        }
    }

    Ok(())
}

/// Delete an environment: its YAML file and the secrets file beside it.
///
/// Values in the shared legacy `.env` are left alone, since another
/// environment may still be using them.
pub fn delete_environment(root: &Path, name: &str) -> Result<()> {
    let files = environment_files(root)?;
    let matching = || files.iter().filter(|(_, e)| e.name == name);
    let (stem, _) = matching()
        .find(|(stem, _)| *stem == slug(name))
        .or_else(|| matching().next())
        .ok_or_else(|| Error::Invalid(format!("no environment called `{name}`")))?;

    let yaml = root.join(ENV_DIR).join(format!("{stem}.yaml"));
    fs::remove_file(&yaml).map_err(|e| Error::io(path_str(&yaml), e))?;

    let secrets_path = secrets::file_for(root, stem);
    match fs::remove_file(&secrets_path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(Error::io(path_str(&secrets_path), e)),
        _ => Ok(()),
    }
}

/// No keys, and no comment beyond the one the old writer always put at the top.
fn legacy_is_empty(legacy: &secrets::DotEnv) -> bool {
    legacy.keys().is_empty()
        && legacy
            .render()
            .lines()
            .all(|line| line.trim().is_empty() || line.starts_with("# Secret values for this collection"))
}

/// Copy values from the shared `.env` into the per-environment files of every
/// environment except those in `skip`, for secrets they do not have yet.
/// Returns the shared file, if there is one, for the caller to prune.
fn migrate_legacy_secrets(
    root: &Path,
    files: &[(String, Environment)],
    skip: &[&str],
) -> Result<Option<secrets::DotEnv>> {
    let Some(legacy) = secrets::read(&root.join(secrets::LEGACY_FILE), true)? else {
        return Ok(None);
    };

    for (stem, env) in files.iter().filter(|(stem, _)| !skip.contains(&stem.as_str())) {
        let path = secrets::file_for(root, stem);
        let mut own = secrets::read(&path, false)?.unwrap_or_else(|| secrets::DotEnv::for_environment(stem));
        let mut changed = false;
        for var in env.vars.iter().filter(|v| v.secret) {
            if own.get(&var.name).is_none() {
                if let Some(value) = legacy.get(&var.name) {
                    own.set(&var.name, value);
                    changed = true;
                }
            }
        }
        if changed {
            secrets::write(root, &path, &own)?;
        }
    }
    Ok(Some(legacy))
}
// ---------------------------------------------------------------------------
// Scopes
// ---------------------------------------------------------------------------

/// Everything above a request: the collection, then each folder on the way
/// down to it, outermost first.
///
/// Headers, auth and variables all inherit along this chain, and `http::plan`
/// is the only place that resolves it — which is why Send and "Copy as cURL"
/// cannot disagree about what a request inherits.
#[derive(Debug, Clone, Default)]
pub struct Scopes {
    pub collection: CollectionMeta,
    pub folders: Vec<FolderMeta>,
}

impl Scopes {
    /// Outermost first, so a nearer scope can replace a name set further out.
    pub fn headers(&self) -> impl Iterator<Item = &KeyValue> {
        self.collection.headers.iter().chain(self.folders.iter().flat_map(|f| f.headers.iter()))
    }

    /// The nearest scope that says something. A folder set to `inherit` passes
    /// the question outwards rather than answering it.
    pub fn auth(&self) -> &Auth {
        self.folders
            .iter()
            .rev()
            .map(|folder| &folder.auth)
            .find(|auth| !matches!(auth, Auth::Inherit))
            .unwrap_or(&self.collection.auth)
    }

    /// Outermost first, so a nearer scope wins when the map is built.
    pub fn vars(&self) -> impl Iterator<Item = &KeyValue> {
        self.collection.vars.iter().chain(self.folders.iter().flat_map(|f| f.vars.iter()))
    }
}

/// Read the scopes a request sits in. `id` is the request's id; a request that
/// has no file yet (a history replay, say) has no folders, only the collection.
pub fn scopes(root: &Path, id: Option<&str>) -> Result<Scopes> {
    let collection = read_yaml_or_default(&root.join(COLLECTION_FILE))?;
    let Some(id) = id else { return Ok(Scopes { collection, folders: Vec::new() }) };

    let mut folders = Vec::new();
    let mut walked = PathBuf::new();
    // Every segment but the last, which is the request's own file.
    let segments: Vec<&str> = id.split('/').filter(|s| !s.is_empty()).collect();
    for segment in segments.iter().take(segments.len().saturating_sub(1)) {
        walked.push(segment);
        let dir = resolve(root, &walked.to_string_lossy())?;
        folders.push(read_yaml_or_default(&dir.join(FOLDER_FILE))?);
    }

    Ok(Scopes { collection, folders })
}

/// Variables from every scope, weakest first: the ones volt supplies, then the
/// collection, then each folder, then the environment — which is last because
/// switching environments has to be able to override a default.
pub fn scope_context(scopes: &Scopes, env: &[EnvVar]) -> HashMap<String, String> {
    let mut ctx = crate::vars::dynamics();
    ctx.extend(scopes.vars().filter(|v| v.enabled).map(|v| (v.name.clone(), v.value.clone())));
    ctx.extend(env.iter().map(|v| (v.name.clone(), v.value.clone())));
    ctx
}


/// Flatten an environment into the map used for `{{var}}` substitution.
///
/// The variables volt supplies itself go in first, so a collection that
/// defines a name of its own wins over ours rather than the other way round.
/// This is the one place the map is built, which is why `preview`, Send and
/// "Copy as cURL" all resolve to the same values.
pub fn env_context(vars: &[EnvVar]) -> HashMap<String, String> {
    let mut ctx = crate::vars::dynamics();
    ctx.extend(vars.iter().map(|v| (v.name.clone(), v.value.clone())));
    ctx
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

/// Join a collection-relative id onto the root, refusing anything that would
/// escape the collection directory.
fn resolve(root: &Path, id: &str) -> Result<PathBuf> {
    if id.is_empty() {
        return Err(Error::Invalid("empty path".into()));
    }

    let mut path = root.to_path_buf();
    // Both separators, because Windows takes a backslash as one too: splitting
    // on `/` alone let `..\..\x` through as a single "segment".
    for segment in id.split(['/', '\\']) {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(Error::Invalid(format!("unsafe path segment in `{id}`")));
        }
        // A colon is a drive letter or an alternate data stream on Windows, and
        // it is refused here whatever the host is: an id is written into files
        // that a teammate opens on another platform, so `C:x.yaml` must not be
        // a valid id just because the machine that made it was not Windows.
        if segment.contains(':') {
            return Err(Error::Invalid(format!("unsafe path segment in `{id}`")));
        }
        // And each segment has to be one ordinary component, which is what
        // rejects a root and a UNC prefix.
        let mut parts = Path::new(segment).components();
        let one_normal = matches!(parts.next(), Some(std::path::Component::Normal(_)))
            && parts.next().is_none();
        if !one_normal {
            return Err(Error::Invalid(format!("unsafe path segment in `{id}`")));
        }
        path.push(segment);
    }
    Ok(path)
}

fn relative_id(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/")
}

fn is_yaml(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("yaml") | Some("yml")
    )
}

/// A directory name for a folder: accented Latin letters become their plain
/// form first, so "Kullanıcılar" gives `kullanicilar` rather than `kullan-c-lar`.
///
/// Deliberately not folded into `slug`: environment filenames come from `slug`,
/// and changing its output would make an existing environment save to a new
/// file while the old one stays behind as a duplicate.
pub(crate) fn folder_dir_name(name: &str) -> String {
    let mut plain = String::with_capacity(name.len());
    for c in name.chars() {
        match c {
            'ı' | 'İ' | 'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => plain.push('i'),
            'ş' | 'Ş' => plain.push('s'),
            'ğ' | 'Ğ' => plain.push('g'),
            'ü' | 'Ü' | 'ú' | 'ù' | 'û' | 'Ú' | 'Ù' | 'Û' => plain.push('u'),
            'ö' | 'Ö' | 'ó' | 'ò' | 'ô' | 'õ' | 'Ó' | 'Ò' | 'Ô' | 'Õ' => plain.push('o'),
            'ç' | 'Ç' => plain.push('c'),
            'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'Á' | 'À' | 'Â' | 'Ä' | 'Ã' | 'Å' => plain.push('a'),
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => plain.push('e'),
            'ñ' | 'Ñ' => plain.push('n'),
            'ß' => plain.push_str("ss"),
            other => plain.push(other),
        }
    }

    // Windows reserves a handful of names for devices. `CON`, `NUL` and the
    // rest cannot be directories, so a folder called "null" reported success
    // and then was not there. `slug` itself is left alone on purpose: ARCHITECTURE.md
    // notes its output is an environment's identity on disk.
    let out = slug(&plain);
    let reserved = matches!(out.as_str(), "con" | "prn" | "aux" | "nul")
        || (out.len() == 4
            && (out.starts_with("com") || out.starts_with("lpt"))
            && out.as_bytes()[3].is_ascii_digit());
    match reserved {
        true => format!("{out}-folder"),
        false => out,
    }
}

pub fn slug(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();

    while out.contains("--") {
        out = out.replace("--", "-");
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "untitled".into()
    } else {
        trimmed
    }
}

fn path_str(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn read_yaml<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let contents = fs::read_to_string(path).map_err(|e| Error::io(path_str(path), e))?;
    yaml::from_str(&contents).map_err(|e| Error::parse(path_str(path), e))
}

fn read_yaml_or_default<T: serde::de::DeserializeOwned + Default>(path: &Path) -> Result<T> {
    if path.exists() {
        read_yaml(path)
    } else {
        Ok(T::default())
    }
}
/// Parse the collection's own YAML. For files that live in a collection but
/// are not part of the tree, such as saved examples.
pub(crate) fn from_yaml<T: serde::de::DeserializeOwned>(text: &str) -> Result<T> {
    yaml::from_str(text).map_err(|e| Error::Invalid(format!("could not read the YAML: {e}")))
}

pub(crate) fn to_yaml<T: Serialize>(value: &T) -> Result<String> {
    yaml::to_string(value).map_err(|e| Error::Invalid(format!("could not write the YAML: {e}")))
}

/// `resolve`, for the modules that need to turn an id into a path safely.
pub(crate) fn resolve_id(root: &Path, id: &str) -> Result<PathBuf> {
    resolve(root, id)
}


pub(crate) fn write_yaml<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let contents = yaml::to_string(value).map_err(|e| Error::parse(path_str(path), e))?;
    write_atomic(path, &contents)
}

/// Write through a temporary file in the same directory, then rename over the
/// target.
///
/// `fs::write` truncates first: a crash, a full disk or a lost power cable
/// between the truncate and the write leaves the user with an empty or half
/// written request. The rename is atomic on both platforms, and the temp has to
/// be a sibling — one in the system temp directory can be on another volume,
/// where the rename stops being atomic and becomes a copy.
pub(crate) fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return fs::write(path, contents).map_err(|e| Error::io(path_str(path), e));
    };
    // `<name>.tmp`, not `.<name>.tmp`: the dotted form of a secrets file
    // (`..env.prod.tmp`) falls outside the `.env.*` rule, so a crash would
    // leave a credential in a file git is willing to commit. This spelling is
    // covered, and it is not `.yaml`, so the tree ignores it either way.
    let temp = path.with_file_name(format!("{name}.tmp"));

    if let Err(e) = fs::write(&temp, contents) {
        let _ = fs::remove_file(&temp);
        return Err(Error::io(path_str(&temp), e));
    }
    if let Err(e) = fs::rename(&temp, path) {
        let _ = fs::remove_file(&temp);
        return Err(Error::io(path_str(path), e));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/sample-collection")
    }
    fn var(name: &str, value: &str) -> KeyValue {
        KeyValue { name: name.into(), value: value.into(), enabled: value != "no", description: None }
    }


    #[test]
    fn loads_the_example_collection() {
        let collection = load(&sample()).expect("example collection should load");

        assert_eq!(collection.meta.name, "Sample API");
        assert_eq!(collection.environments.len(), 1);
        assert_eq!(collection.environments[0].name, "local");

        // `folder.yaml` decides both the display name and the ordering.
        let names: Vec<&str> = collection
            .tree
            .iter()
            .map(|n| match n {
                Node::Folder { name, .. } | Node::Request { name, .. } => name.as_str(),
            })
            .collect();
        assert_eq!(names, vec!["Auth", "Users"]);
    }

    #[test]
    fn hides_the_environments_directory_from_the_tree() {
        let collection = load(&sample()).unwrap();
        let has_env_folder = collection.tree.iter().any(|n| {
            matches!(n, Node::Folder { name, .. } if name == ENV_DIR)
        });
        assert!(!has_env_folder);
    }

    #[test]
    fn a_request_survives_a_read_write_round_trip() {
        let root = sample();
        let id = "users/list-users.yaml";
        let original = read_request(&root, id).unwrap();

        let tmp = std::env::temp_dir().join(format!("volt-rt-{}", std::process::id()));
        fs::create_dir_all(tmp.join("users")).unwrap();
        write_request(&tmp, id, &original).unwrap();
        let reloaded = read_request(&tmp, id).unwrap();
        fs::remove_dir_all(&tmp).ok();

        assert_eq!(reloaded.name, original.name);
        assert_eq!(reloaded.method, original.method);
        assert_eq!(reloaded.url, original.url);
        assert_eq!(reloaded.params.len(), original.params.len());
    }

    fn plain(name: &str, value: &str) -> EnvVar {
        EnvVar { name: name.into(), value: value.into(), secret: false }
    }

    fn secret(name: &str, value: &str) -> EnvVar {
        EnvVar { name: name.into(), value: value.into(), secret: true }
    }

    fn env(name: &str, vars: Vec<EnvVar>) -> Environment {
        Environment { name: name.into(), vars }
    }

    fn secrets_dir(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("volt-env-{label}-{}", std::process::id()));
        fs::remove_dir_all(&root).ok();
        fs::create_dir_all(root.join(ENV_DIR)).unwrap();
        root
    }

    fn loaded(root: &Path, env_name: &str, var: &str) -> Option<String> {
        load_environments(root)
            .unwrap()
            .into_iter()
            .find(|e| e.name == env_name)
            .and_then(|e| e.vars.into_iter().find(|v| v.name == var))
            .map(|v| v.value)
    }

    fn read(root: &Path, file: &str) -> String {
        fs::read_to_string(root.join(file)).unwrap_or_else(|_| panic!("{file} should exist"))
    }

    #[test]
    fn variables_resolve_from_the_nearest_scope_but_the_environment_has_the_last_word() {
        let scopes = Scopes {
            collection: CollectionMeta {
                vars: vec![var("area", "collection"), var("apiVersion", "v1"), var("off", "no")],
                ..Default::default()
            },
            folders: vec![
                FolderMeta { vars: vec![var("area", "outer")], ..Default::default() },
                FolderMeta { vars: vec![var("area", "inner")], ..Default::default() },
            ],
        };
        let env = vec![EnvVar { name: "apiVersion".into(), value: "v2".into(), secret: false }];

        let ctx = scope_context(&scopes, &env);
        assert_eq!(ctx.get("area").map(String::as_str), Some("inner"), "the nearest folder wins");
        assert_eq!(ctx.get("apiVersion").map(String::as_str), Some("v2"), "switching environment overrides a default");
        assert!(ctx.contains_key("$guid"), "the ones volt supplies are still there");
        assert!(!ctx.contains_key("off"), "a disabled variable is not defined at all");
    }

    #[test]
    fn scopes_are_read_from_the_folders_a_request_sits_in() {
        let inside = scopes(&sample(), Some("auth/login.yaml")).unwrap();
        assert_eq!(inside.collection.name, "Sample API");
        assert_eq!(inside.folders.len(), 1, "one folder above the request");

        let root_level = scopes(&sample(), Some("top.yaml")).unwrap();
        assert!(root_level.folders.is_empty());

        // A request with no file of its own still inherits from the collection.
        let replayed = scopes(&sample(), None).unwrap();
        assert_eq!(replayed.collection.name, "Sample API");
        assert!(replayed.folders.is_empty());
    }

    #[test]
    fn secret_values_stay_out_of_the_environment_yaml() {
        let root = secrets_dir("yaml");
        let prod = env("prod", vec![plain("baseUrl", "https://api.example.com"), secret("token", "s3cr3t")]);
        write_environment(&root, &prod, None).unwrap();

        let yaml = read(&root, "environments/prod.yaml");
        assert!(yaml.contains("https://api.example.com"));
        assert!(!yaml.contains("s3cr3t"), "secret leaked into the YAML: {yaml}");
        assert!(read(&root, ".env.prod").contains("s3cr3t"));
        assert!(secrets::is_ignored(&read(&root, ".gitignore"), ".env.prod"), "a missing .gitignore is created");

        assert_eq!(loaded(&root, "prod", "token").as_deref(), Some("s3cr3t"));
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_same_secret_can_differ_per_environment() {
        let root = secrets_dir("per-env");
        write_environment(&root, &env("dev", vec![secret("token", "dev-token")]), None).unwrap();
        write_environment(&root, &env("prod", vec![secret("token", "prod-token")]), None).unwrap();

        assert_eq!(loaded(&root, "dev", "token").as_deref(), Some("dev-token"));
        assert_eq!(loaded(&root, "prod", "token").as_deref(), Some("prod-token"));

        // Saving one again leaves the other alone.
        write_environment(&root, &env("dev", vec![secret("token", "dev-2")]), Some("dev")).unwrap();
        assert_eq!(loaded(&root, "dev", "token").as_deref(), Some("dev-2"));
        assert_eq!(loaded(&root, "prod", "token").as_deref(), Some("prod-token"));
        fs::remove_dir_all(&root).ok();
    }

    /// A collection from before per-environment files: one shared `.env`.
    fn legacy_collection(label: &str, legacy: &str) -> PathBuf {
        let root = secrets_dir(label);
        fs::write(root.join(".gitignore"), "# Secret values, never commit these\n.env\n").unwrap();
        fs::write(root.join(".env"), legacy).unwrap();
        for name in ["dev", "prod"] {
            let e = env(name, vec![plain("baseUrl", "https://x.test"), secret("token", "")]);
            write_yaml(&root.join(ENV_DIR).join(format!("{name}.yaml")), &e).unwrap();
        }
        root
    }

    #[test]
    fn an_old_shared_env_file_is_still_read() {
        let root = legacy_collection("legacy-read", "# Secret values for this collection. Do not commit.\ntoken=\"shared\"\n");
        assert_eq!(loaded(&root, "dev", "token").as_deref(), Some("shared"));
        assert_eq!(loaded(&root, "prod", "token").as_deref(), Some("shared"));
        // Reading never writes.
        assert!(!root.join(".env.dev").exists());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_first_save_moves_shared_secrets_into_per_environment_files() {
        let root = legacy_collection(
            "legacy-migrate",
            "# Secret values for this collection. Do not commit.\ntoken=\"shared\"\nUNRELATED=\"keep me\"\n",
        );

        // The editor sends back what it loaded, with dev's token changed.
        let dev = env("dev", vec![plain("baseUrl", "https://x.test"), secret("token", "dev-only")]);
        write_environment(&root, &dev, Some("dev")).unwrap();

        assert_eq!(loaded(&root, "dev", "token").as_deref(), Some("dev-only"));
        assert_eq!(loaded(&root, "prod", "token").as_deref(), Some("shared"), "prod kept the shared value");
        assert!(read(&root, ".env.prod").contains("shared"), "…in its own file now");

        // The shared file keeps only what no environment declares.
        let legacy = read(&root, ".env");
        assert!(!legacy.contains("token"), "{legacy}");
        assert!(legacy.contains("UNRELATED=\"keep me\""));

        // The old .gitignore did not cover the new files; now it does.
        let gitignore = read(&root, ".gitignore");
        assert!(secrets::is_ignored(&gitignore, ".env.dev") && secrets::is_ignored(&gitignore, ".env.prod"));
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_fully_migrated_shared_file_is_removed() {
        let root = legacy_collection("legacy-empty", "# Secret values for this collection. Do not commit.\ntoken=\"shared\"\n");
        write_environment(&root, &env("prod", vec![secret("token", "shared")]), Some("prod")).unwrap();
        assert!(!root.join(".env").exists());
        assert_eq!(loaded(&root, "dev", "token").as_deref(), Some("shared"));
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn unsecreting_or_deleting_a_variable_takes_it_out_of_the_secrets_file() {
        let root = secrets_dir("unsecret");
        let first = env("dev", vec![secret("a", "1"), secret("b", "2"), secret("c", "3")]);
        write_environment(&root, &first, None).unwrap();
        // Someone adds a key by hand.
        let path = root.join(".env.dev");
        fs::write(&path, format!("{}# mine\nMANUAL=\"x\"\n", read(&root, ".env.dev"))).unwrap();

        // `a` stops being secret, `b` is deleted, `c` stays.
        let second = env("dev", vec![plain("a", "1"), secret("c", "3")]);
        write_environment(&root, &second, Some("dev")).unwrap();

        let text = read(&root, ".env.dev");
        assert!(!text.contains("a=") && !text.contains("b="), "{text}");
        assert!(text.contains("c=\"3\""));
        assert!(text.contains("# mine\nMANUAL=\"x\""), "hand-added lines survive: {text}");
        assert!(read(&root, "environments/dev.yaml").contains("value: '1'"), "`a` is plain YAML now");

        // With no secrets left and nothing hand-written, the file goes away.
        let root2 = secrets_dir("unsecret-empty");
        write_environment(&root2, &env("dev", vec![secret("a", "1")]), None).unwrap();
        write_environment(&root2, &env("dev", vec![plain("a", "1")]), Some("dev")).unwrap();
        assert!(!root2.join(".env.dev").exists());

        fs::remove_dir_all(&root).ok();
        fs::remove_dir_all(&root2).ok();
    }

    #[test]
    fn renaming_an_environment_moves_its_files_instead_of_duplicating_them() {
        let root = secrets_dir("rename-env");
        write_environment(&root, &env("dev", vec![secret("token", "t")]), None).unwrap();
        fs::write(root.join(".env.dev"), format!("{}MANUAL=\"x\"\n", read(&root, ".env.dev"))).unwrap();

        write_environment(&root, &env("Development", vec![secret("token", "t")]), Some("dev")).unwrap();

        assert!(!root.join("environments/dev.yaml").exists(), "no duplicate left behind");
        assert!(!root.join(".env.dev").exists());
        assert!(root.join("environments/development.yaml").is_file());
        let moved = read(&root, ".env.development");
        assert!(moved.contains("token=\"t\"") && moved.contains("MANUAL=\"x\""), "{moved}");

        let names: Vec<String> = load_environments(&root).unwrap().into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["Development"]);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn an_environment_is_never_saved_over_another() {
        let root = secrets_dir("clash");
        write_environment(&root, &env("dev", vec![secret("token", "dev")]), None).unwrap();
        write_environment(&root, &env("prod", vec![secret("token", "prod")]), None).unwrap();

        // Renaming dev to prod…
        assert!(write_environment(&root, &env("prod", vec![secret("token", "dev")]), Some("dev")).is_err());
        // …or creating a second "PROD".
        assert!(write_environment(&root, &env("PROD", vec![]), None).is_err());

        assert_eq!(loaded(&root, "dev", "token").as_deref(), Some("dev"));
        assert_eq!(loaded(&root, "prod", "token").as_deref(), Some("prod"));
        // Changing only the case of its own name is fine.
        write_environment(&root, &env("Prod", vec![secret("token", "prod")]), Some("prod")).unwrap();
        assert_eq!(loaded(&root, "Prod", "token").as_deref(), Some("prod"));
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_hand_named_environment_file_keeps_its_name() {
        let root = secrets_dir("hand-named");
        write_yaml(&root.join(ENV_DIR).join("production.yaml"), &env("prod", vec![secret("token", "")])).unwrap();
        fs::write(root.join(".env.production"), "token=\"p\"\n").unwrap();
        assert_eq!(loaded(&root, "prod", "token").as_deref(), Some("p"));

        write_environment(&root, &env("prod", vec![secret("token", "p2")]), Some("prod")).unwrap();
        assert!(root.join("environments/production.yaml").is_file());
        assert!(!root.join("environments/prod.yaml").exists());
        assert_eq!(loaded(&root, "prod", "token").as_deref(), Some("p2"));
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn deleting_an_environment_takes_its_secrets_file_with_it() {
        let root = secrets_dir("delete-env");
        write_environment(&root, &env("dev", vec![secret("token", "d")]), None).unwrap();
        write_environment(&root, &env("prod", vec![secret("token", "p")]), None).unwrap();

        delete_environment(&root, "dev").unwrap();
        assert!(!root.join("environments/dev.yaml").exists());
        assert!(!root.join(".env.dev").exists(), "the secret values go too");

        // The other environment is untouched.
        assert_eq!(loaded(&root, "prod", "token").as_deref(), Some("p"));
        let names: Vec<String> = load_environments(&root).unwrap().into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["prod"]);

        // A name that is no longer there says so rather than deleting something else.
        assert!(delete_environment(&root, "dev").is_err());
        assert!(root.join("environments/prod.yaml").is_file());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn deleting_a_hand_named_environment_finds_its_file() {
        let root = secrets_dir("delete-hand-named");
        write_yaml(&root.join(ENV_DIR).join("production.yaml"), &env("prod", vec![secret("token", "")])).unwrap();
        fs::write(root.join(".env.production"), "token=\"p\"\n").unwrap();

        delete_environment(&root, "prod").unwrap();
        assert!(!root.join("environments/production.yaml").exists());
        assert!(!root.join(".env.production").exists());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_secret_name_that_cannot_be_stored_is_refused_before_writing() {
        let root = secrets_dir("bad-name");
        assert!(write_environment(&root, &env("dev", vec![secret("a=b", "x")]), None).is_err());
        assert!(!root.join("environments/dev.yaml").exists());
        assert!(write_environment(&root, &env("  ", vec![]), None).is_err());
        fs::remove_dir_all(&root).ok();
    }

    /// A throwaway collection with two folders and a request, for the
    /// rename/move tests.
    fn scratch(label: &str) -> PathBuf {
        let root = std::env::temp_dir()
            .join(format!("volt-{label}-{}-{:?}", std::process::id(), std::thread::current().id()));
        fs::remove_dir_all(&root).ok();
        init(&root, "Scratch").unwrap();
        fs::create_dir_all(root.join("users")).unwrap();
        fs::create_dir_all(root.join("admin")).unwrap();

        let request = Request {
            name: "List users".into(),
            seq: 4,
            method: "GET".into(),
            url: "{{baseUrl}}/users".into(),
            docs: Some("keep me".into()),
            ..Default::default()
        };
        write_request(&root, "users/list.yaml", &request).unwrap();
        root
    }

    #[test]
    fn renaming_a_request_keeps_its_file_and_its_other_fields() {
        let root = scratch("rename-req");

        rename_node(&root, "users/list.yaml", "  Every user  ").unwrap();

        let after = read_request(&root, "users/list.yaml").unwrap();
        assert_eq!(after.name, "Every user", "the name is trimmed and replaced");
        assert_eq!(after.seq, 4, "unrelated fields survive");
        assert_eq!(after.url, "{{baseUrl}}/users");
        assert_eq!(after.docs.as_deref(), Some("keep me"));

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn renaming_a_folder_writes_folder_yaml_without_moving_the_directory() {
        let root = scratch("rename-dir");

        rename_node(&root, "users", "People").unwrap();

        assert!(root.join("users").is_dir(), "the directory keeps its name");
        let meta: FolderMeta = read_yaml(&root.join("users").join(FOLDER_FILE)).unwrap();
        assert_eq!(meta.name.as_deref(), Some("People"));

        // The tree shows the display name, not the directory name.
        let tree = load(&root).unwrap().tree;
        let names: Vec<&str> = tree
            .iter()
            .map(|n| match n {
                Node::Folder { name, .. } | Node::Request { name, .. } => name.as_str(),
            })
            .collect();
        assert!(names.contains(&"People"), "got {names:?}");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn renaming_rejects_a_blank_name() {
        let root = scratch("rename-blank");
        assert!(rename_node(&root, "users/list.yaml", "   ").is_err());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn moving_a_request_between_folders_and_back_to_the_root() {
        let root = scratch("move-req");

        let id = move_node(&root, "users/list.yaml", Some("admin")).unwrap();
        assert_eq!(id, "admin/list.yaml");
        assert!(root.join("admin/list.yaml").is_file());
        assert!(!root.join("users/list.yaml").exists());
        // The file travels intact.
        assert_eq!(read_request(&root, &id).unwrap().name, "List users");

        let id = move_node(&root, &id, None).unwrap();
        assert_eq!(id, "list.yaml");
        assert!(root.join("list.yaml").is_file());

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn moving_a_node_onto_itself_is_a_no_op() {
        let root = scratch("move-noop");
        let id = move_node(&root, "users/list.yaml", Some("users")).unwrap();
        assert_eq!(id, "users/list.yaml");
        assert!(root.join("users/list.yaml").is_file());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn moving_a_folder_inside_itself_is_refused() {
        let root = scratch("move-cycle");
        fs::create_dir_all(root.join("users/nested")).unwrap();

        // Without this guard the whole subtree would be detached.
        assert!(move_node(&root, "users", Some("users/nested")).is_err());
        assert!(root.join("users/nested").is_dir(), "nothing moved");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_delete_goes_to_the_bin_and_can_be_put_back() {
        let root = scratch("trash");
        let id = "users/list.yaml".to_string();
        let before = fs::read_to_string(root.join(&id)).expect("the request exists");

        let deleted = delete_node(&root, &id).unwrap();
        assert!(!root.join(&id).exists(), "it is gone from where it was");
        assert!(root.join(&deleted.at).is_file(), "and is in the bin: {}", deleted.at);
        assert_eq!(deleted.name, "List users", "named as the user knew it, not as the file is");
        assert!(!deleted.folder);

        // The tree skips dotted names, so the bin is not a folder in the app.
        let tree = load(&root).unwrap().tree;
        assert!(!tree.iter().any(|node| matches!(node, Node::Folder { name, .. } if name.contains("trash"))));

        restore_node(&root, &deleted).unwrap();
        assert_eq!(fs::read_to_string(root.join(&id)).unwrap(), before, "byte for byte");
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_deleted_folder_keeps_everything_inside_it() {
        let root = scratch("trash-folder");
        let deleted = delete_node(&root, "users").unwrap();

        assert!(deleted.folder);
        assert!(root.join(&deleted.at).join("list.yaml").is_file(), "the whole subtree moved");
        assert!(!root.join("users").exists());

        restore_node(&root, &deleted).unwrap();
        assert!(root.join("users/list.yaml").is_file());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn restoring_onto_something_that_took_the_name_is_refused() {
        let root = scratch("trash-clash");
        let deleted = delete_node(&root, "users/list.yaml").unwrap();
        fs::write(root.join("users/list.yaml"), "name: Something else\nmethod: GET\nurl: /x\n").unwrap();

        let refused = restore_node(&root, &deleted).unwrap_err().to_string();
        assert!(refused.contains("taken again"), "{refused}");
        assert!(root.join(&deleted.at).exists(), "and it is still in the bin");
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_bin_is_gitignored_listed_and_emptied_on_purpose() {
        let root = scratch("trash-bin");
        delete_node(&root, "users/list.yaml").unwrap();
        write_request(&root, "users/second.yaml", &Request { name: "Second".into(), ..Default::default() }).unwrap();
        delete_node(&root, "users/second.yaml").unwrap();

        let listed = list_trash(&root).unwrap();
        assert_eq!(listed.len(), 2);

        let ignore = fs::read_to_string(root.join(".gitignore")).unwrap();
        assert!(ignore.contains(".trash/"), "the bin is working state, never committed: {ignore}");

        empty_trash(&root).unwrap();
        assert!(list_trash(&root).unwrap().is_empty());
        assert!(empty_trash(&root).is_ok(), "emptying an empty bin is not an error");
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn moving_onto_an_existing_name_is_refused_rather_than_overwriting() {
        let root = scratch("move-clash");
        let other = Request { name: "Different".into(), ..Default::default() };
        write_request(&root, "admin/list.yaml", &other).unwrap();

        assert!(move_node(&root, "users/list.yaml", Some("admin")).is_err());

        // Both files are untouched.
        assert_eq!(read_request(&root, "admin/list.yaml").unwrap().name, "Different");
        assert_eq!(read_request(&root, "users/list.yaml").unwrap().name, "List users");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_files_that_describe_the_tree_are_not_nodes() {
        let root = scratch("reserved");
        assert!(move_node(&root, COLLECTION_FILE, Some("users")).is_err());
        assert!(rename_node(&root, COLLECTION_FILE, "Nope").is_err());

        write_yaml(&root.join("users").join(FOLDER_FILE), &FolderMeta::default()).unwrap();
        assert!(move_node(&root, "users/folder.yaml", None).is_err());

        assert!(root.join(COLLECTION_FILE).is_file());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_environments_directory_is_not_a_move_target() {
        let root = scratch("move-env");
        assert!(move_node(&root, "users/list.yaml", Some(ENV_DIR)).is_err());
        assert!(root.join("users/list.yaml").is_file());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn move_rejects_traversal_in_either_argument() {
        let root = scratch("move-traversal");
        assert!(move_node(&root, "../escape.yaml", None).is_err());
        assert!(move_node(&root, "users/list.yaml", Some("../..")).is_err());
        fs::remove_dir_all(&root).ok();
    }

    /// The order the sidebar would show, for one folder.
    fn order_in(root: &Path, folder: &str) -> Vec<String> {
        let tree = load(root).unwrap().tree;
        let children = tree
            .iter()
            .find_map(|n| match n {
                Node::Folder { id, children, .. } if id == folder => Some(children),
                _ => None,
            })
            .expect("folder should be in the tree");

        children
            .iter()
            .map(|n| match n {
                Node::Folder { name, .. } | Node::Request { name, .. } => name.clone(),
            })
            .collect()
    }

    #[test]
    fn reordering_changes_the_order_the_tree_comes_back_in() {
        let root = scratch("reorder");
        for (file, name) in [("a.yaml", "Alpha"), ("b.yaml", "Beta"), ("c.yaml", "Gamma")] {
            let request = Request { name: name.into(), seq: 1, ..Default::default() };
            write_request(&root, &format!("users/{file}"), &request).unwrap();
        }
        // All at seq 1, so they fall back to sorting by name.
        assert_eq!(order_in(&root, "users"), ["Alpha", "Beta", "Gamma", "List users"]);

        reorder(
            &root,
            Some("users"),
            &[
                "users/c.yaml".into(),
                "users/a.yaml".into(),
                "users/list.yaml".into(),
                "users/b.yaml".into(),
            ],
        )
        .unwrap();

        assert_eq!(order_in(&root, "users"), ["Gamma", "Alpha", "List users", "Beta"]);
        assert_eq!(read_request(&root, "users/c.yaml").unwrap().seq, 1);
        assert_eq!(read_request(&root, "users/b.yaml").unwrap().seq, 4);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn reordering_only_rewrites_the_files_whose_number_changed() {
        let root = scratch("reorder-churn");
        for (i, file) in ["a.yaml", "b.yaml", "c.yaml"].iter().enumerate() {
            let request = Request { name: file.to_string(), seq: i as u32 + 1, ..Default::default() };
            write_request(&root, &format!("admin/{file}"), &request).unwrap();
        }

        let stamp = |file: &str| {
            fs::metadata(root.join("admin").join(file)).unwrap().modified().unwrap()
        };
        let before = (stamp("a.yaml"), stamp("b.yaml"), stamp("c.yaml"));
        std::thread::sleep(std::time::Duration::from_millis(20));

        // Already in this order: nothing should be touched.
        reorder(
            &root,
            Some("admin"),
            &["admin/a.yaml".into(), "admin/b.yaml".into(), "admin/c.yaml".into()],
        )
        .unwrap();

        assert_eq!(
            (stamp("a.yaml"), stamp("b.yaml"), stamp("c.yaml")),
            before,
            "a no-op reorder must not rewrite any file"
        );

        // Swapping the last two leaves the first alone.
        reorder(
            &root,
            Some("admin"),
            &["admin/a.yaml".into(), "admin/c.yaml".into(), "admin/b.yaml".into()],
        )
        .unwrap();
        assert_eq!(stamp("a.yaml"), before.0, "an untouched sibling stays untouched");
        assert_eq!(read_request(&root, "admin/c.yaml").unwrap().seq, 2);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn reordering_a_folder_writes_its_folder_yaml() {
        let root = scratch("reorder-folders");

        // `users` sorts before `admin` only once it is numbered.
        reorder(&root, None, &["users".into(), "admin".into()]).unwrap();

        let tree = load(&root).unwrap().tree;
        let names: Vec<&str> = tree
            .iter()
            .map(|n| match n {
                Node::Folder { name, .. } | Node::Request { name, .. } => name.as_str(),
            })
            .collect();
        assert_eq!(names, ["users", "admin"]);

        // `users` was already first, so only `admin` needed a file.
        assert!(!root.join("users").join(FOLDER_FILE).exists());
        let meta: FolderMeta = read_yaml(&root.join("admin").join(FOLDER_FILE)).unwrap();
        assert_eq!(meta.seq, 2);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn reordering_refuses_a_node_from_another_folder() {
        let root = scratch("reorder-foreign");
        let err = reorder(
            &root,
            Some("admin"),
            &["users/list.yaml".into()],
        );
        assert!(err.is_err(), "a node from another folder must be rejected");

        // Nothing was renumbered.
        assert_eq!(read_request(&root, "users/list.yaml").unwrap().seq, 4);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_folder_gets_a_safe_directory_and_keeps_the_typed_name() {
        let root = scratch("mkdir-name");

        let id = create_folder(&root, None, "Kullanıcılar: v2?").unwrap();
        assert_eq!(id, "kullanicilar-v2");
        let meta: FolderMeta = read_yaml(&root.join(&id).join(FOLDER_FILE)).unwrap();
        assert_eq!(meta.name.as_deref(), Some("Kullanıcılar: v2?"));

        // A name that is already a clean directory name needs no folder.yaml.
        let plain = create_folder(&root, None, "billing").unwrap();
        assert!(!root.join(&plain).join(FOLDER_FILE).exists());

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_folder_can_be_created_inside_another() {
        let root = scratch("mkdir-nested");
        let id = create_folder(&root, Some("users"), "Admins").unwrap();
        assert_eq!(id, "users/admins");
        assert!(root.join("users/admins").is_dir());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_clashing_folder_name_gets_a_suffix_instead_of_merging() {
        let root = scratch("mkdir-clash");

        // `users` already exists and holds a request.
        let id = create_folder(&root, None, "Users").unwrap();
        assert_eq!(id, "users-2");
        assert!(root.join("users/list.yaml").is_file(), "the original is untouched");

        // `environments` is reserved at the root even when it does not exist yet.
        fs::remove_dir_all(root.join(ENV_DIR)).unwrap();
        assert_eq!(create_folder(&root, None, "Environments").unwrap(), "environments-2");
        // …but is an ordinary name one level down.
        assert_eq!(create_folder(&root, Some("users"), "environments").unwrap(), "users/environments");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn creating_a_folder_rejects_bad_input() {
        let root = scratch("mkdir-bad");
        assert!(create_folder(&root, None, "   ").is_err());
        assert!(create_folder(&root, Some("nope"), "x").is_err());
        assert!(create_folder(&root, Some("../.."), "x").is_err());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn folder_dir_names_do_not_change_environment_slugs() {
        // Environment filenames must stay exactly as they were.
        assert_eq!(slug("Geliştirme"), "geli-tirme");
        assert_eq!(folder_dir_name("Geliştirme"), "gelistirme");
    }

    #[test]
    fn resolve_rejects_traversal() {
        let root = Path::new("/tmp/c");
        assert!(resolve(root, "../../etc/passwd").is_err());
        assert!(resolve(root, "users/../../etc").is_err());
        assert!(resolve(root, "users/list.yaml").is_ok());
    }

    #[test]
    fn slug_makes_a_filename_safe_name() {
        assert_eq!(slug("Local Dev"), "local-dev");
        assert_eq!(slug("  ??  "), "untitled");
        assert_eq!(slug("v1/Users"), "v1-users");
    }

    /// `resolve` is the one gate every id from the UI goes through, so what it
    /// rejects is the whole of the path safety story.
    #[test]
    fn an_id_cannot_leave_the_collection_by_any_spelling() {
        let root = Path::new("C:/demo/api");
        let refused = [
            "..",
            "../outside.yaml",
            "users/../../outside.yaml",
            // Windows takes a backslash as a separator too, so splitting on
            // `/` alone let the whole thing through as one "segment".
            r"..\..\outside.yaml",
            r"users\..\..\outside.yaml",
            // A drive letter, a root and a UNC prefix are not segments. These
            // are refused on every platform, not only Windows: an id is
            // written into files a teammate opens elsewhere.
            "C:",
            "C:/Windows/System32/drivers/etc/hosts",
            "C:x.yaml",
            r"\\server\share\x.yaml",
            "",
            "users//list.yaml",
        ];
        for id in refused {
            assert!(resolve(root, id).is_err(), "`{id}` must be refused");
        }

        // And the ordinary ones still work.
        for id in ["users/list.yaml", "a/b/c/deep.yaml", "with space.yaml", "üst-klasör/x.yaml"] {
            assert!(resolve(root, id).is_ok(), "`{id}` is a normal id");
        }
    }

    /// A truncate-then-write leaves an empty request behind if anything goes
    /// wrong in between; a rename cannot.
    #[test]
    fn a_write_leaves_no_temporary_file_and_no_half_written_one() {
        let root = scratch("atomic");
        let path = root.join("thing.yaml");

        write_atomic(&path, "name: One\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "name: One\n");
        write_atomic(&path, "name: Two\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "name: Two\n", "it replaces rather than appends");

        let left: Vec<String> = fs::read_dir(&root)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(left.is_empty(), "no temporary file survives a successful write: {left:?}");

        // The name it would use is one `.gitignore`'s `.env.*` rule covers, so
        // a crash cannot leave an uncovered secrets file behind.
        assert!(crate::secrets::is_ignored(".env.*\n", ".env.prod.tmp"));

        fs::remove_dir_all(&root).ok();
    }


    /// `.examples/` mirrors the tree by path, so a move that does not carry
    /// them leaves kept responses at the old id — where the next request to
    /// land there would show them as its own.
    #[test]
    fn a_moved_request_takes_its_examples_with_it() {
        let root = scratch("move-examples");
        fs::create_dir_all(root.join(".examples/users")).unwrap();
        fs::write(
            root.join(".examples/users/list.yaml"),
            "- name: Kept\n  at: 0\n  status: 200\n  status_text: OK\n",
        )
        .unwrap();

        let new_id = move_node(&root, "users/list.yaml", Some("admin")).unwrap();
        assert_eq!(new_id, "admin/list.yaml");
        assert!(root.join(".examples/admin/list.yaml").is_file(), "the examples came along");
        assert!(!root.join(".examples/users/list.yaml").exists(), "nothing is left at the old id");
        assert_eq!(crate::examples::list(&root, "admin/list.yaml").unwrap().len(), 1);

        // A request with no examples at all moves just the same.
        write_request(&root, "admin/other.yaml", &Request { name: "Other".into(), ..Default::default() })
            .unwrap();
        assert_eq!(move_node(&root, "admin/other.yaml", Some("users")).unwrap(), "users/other.yaml");

        fs::remove_dir_all(&root).ok();
    }

    /// Windows will not let a directory be called `nul`, so `create_folder`
    /// reported success and the folder was not there.
    #[test]
    fn a_folder_named_after_a_windows_device_still_gets_a_directory() {
        for name in ["nul", "NUL", "Con", "aux", "COM1", "lpt9"] {
            let dir = folder_dir_name(name);
            assert!(dir.ends_with("-folder"), "`{name}` became `{dir}`");
        }
        // Names that merely look like one are untouched.
        for name in ["console", "nullable", "com", "com10", "lptx"] {
            assert!(!folder_dir_name(name).ends_with("-folder"), "`{name}` is not a device");
        }

        let root = scratch("device-folder");
        let id = create_folder(&root, None, "NUL").unwrap();
        assert!(resolve(&root, &id).unwrap().is_dir(), "and the directory actually exists");
        fs::remove_dir_all(&root).ok();
    }

    /// One hand-edited request with a stray tab in it used to make the whole
    /// collection refuse to open, with nothing to click on and no way to find
    /// the file. It is skipped now — and named, because a request that
    /// disappears without a word is its own kind of bad.
    #[test]
    fn a_file_that_does_not_parse_is_named_rather_than_fatal() {
        let root = scratch("broken-yaml");
        fs::write(root.join("users/broken.yaml"), "name: Broken\n\tseq: [unclosed\n").unwrap();

        let loaded = load(&root).expect("the collection still opens");
        assert_eq!(loaded.problems, vec!["users/broken.yaml".to_string()]);

        // And everything else is still there.
        assert!(!loaded.tree.is_empty());
        assert!(read_request(&root, "users/list.yaml").is_ok());

        fs::remove_dir_all(&root).ok();
    }
    #[test]
    fn the_default_collection_is_made_once_and_then_left_alone() {
        let base = std::env::temp_dir().join(format!("volt-default-{}", std::process::id()));
        fs::remove_dir_all(&base).ok();

        let root = ensure_default(&base).unwrap();
        assert_eq!(root, base.join("volt").join("personal"));
        let loaded = load(&root).unwrap();
        assert_eq!(loaded.meta.name, DEFAULT_NAME);
        assert!(root.join(".gitignore").exists(), "secrets are ignored even here");
        assert_eq!(loaded.tree.len(), 1, "it starts with one request to send");
        assert_eq!(read_request(&root, "hello.yaml").unwrap().name, "Hello, world");
        fs::remove_file(root.join("hello.yaml")).unwrap();

        // Something the user made in the meantime.
        let request = Request { name: "Kept".into(), ..Default::default() };
        write_request(&root, "kept.yaml", &request).unwrap();

        let again = ensure_default(&base).unwrap();
        assert_eq!(again, root);
        assert!(root.join("kept.yaml").exists(), "a second launch does not reinitialise it");
        assert!(!root.join("hello.yaml").exists(), "and does not put the sample back");

        fs::remove_dir_all(&base).ok();
    }
    #[test]
    fn search_looks_inside_requests_and_says_where() {
        let root = scratch("search");
        let mut request = Request { name: "Keyed".into(), method: "GET".into(), url: "https://api.test/v2/things".into(), ..Default::default() };
        request.headers.push(KeyValue { name: "X-Api-Key".into(), value: "{{apiKey}}".into(), enabled: true, ..Default::default() });
        request.docs = Some("Needs the Widget scope".into());
        write_request(&root, "users/keyed.yaml", &request).unwrap();
        let mut other = Request { name: "Body".into(), method: "POST".into(), url: "https://api.test/x".into(), ..Default::default() };
        other.body = Body::Json { content: r#"{"kind": "WIDGET"}"#.into() };
        write_request(&root, "body.yaml", &other).unwrap();

        let hits = search(&root, "x-api-key").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "users/keyed.yaml");
        assert_eq!(hits[0].found_in, "header X-Api-Key");

        let hits = search(&root, "widget").unwrap();
        let mut where_found: Vec<String> = hits.iter().map(|h| format!("{}:{}", h.id, h.found_in)).collect();
        where_found.sort();
        assert_eq!(where_found, vec!["body.yaml:body".to_string(), "users/keyed.yaml:docs".to_string()], "case does not matter, and each says where");

        assert!(search(&root, "/v2/").unwrap().iter().any(|h| h.found_in == "URL"));
        assert!(search(&root, "nothing-like-this").unwrap().is_empty());
        assert!(search(&root, "   ").unwrap().is_empty(), "blank finds nothing rather than everything");
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_request_copied_to_another_collection_keeps_its_name_and_finds_a_free_file() {
        let from = scratch("copy-from");
        let to = scratch("copy-to");
        let request = Request { name: "Ping".into(), method: "GET".into(), url: "https://api.test/ping".into(), ..Default::default() };
        write_request(&from, "users/ping.yaml", &request).unwrap();

        assert_eq!(copy_request_to(&from, "users/ping.yaml", &to).unwrap(), "ping.yaml", "lands at the root, folders left behind");
        assert_eq!(read_request(&to, "ping.yaml").unwrap().url, "https://api.test/ping");
        assert_eq!(copy_request_to(&from, "users/ping.yaml", &to).unwrap(), "ping-2.yaml", "a second copy does not overwrite the first");
        assert!(read_request(&from, "users/ping.yaml").is_ok(), "the original is untouched");

        let not_a_collection = std::env::temp_dir().join(format!("volt-not-a-collection-{}", std::process::id()));
        fs::create_dir_all(&not_a_collection).unwrap();
        assert!(copy_request_to(&from, "users/ping.yaml", &not_a_collection).is_err());
        fs::remove_dir_all(&from).ok();
        fs::remove_dir_all(&to).ok();
        fs::remove_dir_all(&not_a_collection).ok();
    }
}
