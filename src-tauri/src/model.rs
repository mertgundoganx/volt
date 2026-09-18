//! The on-disk file format.
//!
//! Every struct here maps 1:1 to YAML so that a collection is reviewable in a
//! pull request. Ordering is explicit (`seq`) and lists are used instead of
//! maps so that duplicate headers survive a round-trip and diffs stay stable.

use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

fn default_seq() -> u32 {
    1
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KeyValue {
    pub name: String,
    #[serde(default)]
    pub value: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// One part of a multipart body. A `KeyValue` plus the one thing a form field
/// has that a header does not: it may be a file rather than text.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FormField {
    pub name: String,
    #[serde(default)]
    pub value: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The value is a path to upload, taken from the collection root when it
    /// is relative, rather than the text to send.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub file: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Body {
    #[default]
    None,
    Text { content: String },
    Json { content: String },
    Xml { content: String },
    /// multipart/form-data
    Form { fields: Vec<FormField> },
    /// application/x-www-form-urlencoded
    UrlEncoded { fields: Vec<KeyValue> },
    /// Path relative to the collection root.
    Binary { path: String },
    /// A query and its variables. On the wire it is a JSON POST like any
    /// other; keeping the two apart is what lets the editor be a GraphQL one
    /// and what makes the YAML readable.
    /// A gRPC call: the proto file, which method, and the message as JSON.
    /// It is a body rather than a `method` because the RPC name and the HTTP
    /// method are different things and one field cannot be both.
    #[serde(rename = "grpc")]
    Grpc {
        /// Path to a `.proto`, from the collection root when relative.
        proto: String,
        /// `package.Service/Method`.
        method: String,
        /// The request message, as JSON.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        message: String,
    },
    #[serde(rename = "graphql")]
    GraphQl {
        query: String,
        /// A JSON object, as text. Empty means none.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        variables: String,
    },
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiKeyLocation {
    #[default]
    Header,
    Query,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Auth {
    None,
    /// Use whatever the parent folder / collection defines.
    #[default]
    Inherit,
    Bearer { token: String },
    Basic { username: String, password: String },
    /// A conversation, not a header: the server challenges and `execute`
    /// answers. See `digest.rs`.
    Digest { username: String, password: String },
    /// Three legs down one connection. See `ntlm.rs`.
    Ntlm {
        username: String,
        password: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        domain: String,
    },
    /// Signed over the whole request, so `http::execute` does it after the
    /// plan is built rather than `plan` expanding it into a header.
    #[serde(rename = "awssigv4")]
    AwsSigV4 {
        key_id: String,
        secret: String,
        region: String,
        service: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        session_token: String,
    },
    #[serde(rename = "apikey")]
    ApiKey {
        key: String,
        value: String,
        #[serde(default)]
        location: ApiKeyLocation,
    },
}


/// Per-request overrides for how it goes out. Every field is optional: what a
/// request does not say is taken from the app's settings, so a collection
/// stays portable between machines that time out differently.
///
/// Snake_case, unlike `ExecOptions` — this one is also file format.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow_redirects: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify_tls: Option<bool>,
    /// An empty string means "go direct", overriding a proxy in the settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy: Option<String>,
    /// A PEM with a client certificate and its key, for a server that asks
    /// for one. Kept relative to the collection root when it is inside it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_cert: Option<String>,
}

impl RequestOptions {
    /// Nothing overridden — the request can be written without the key.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Take a value out of the response and keep it, so the next request can use
/// `{{name}}` — logging in once instead of pasting a token about.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capture {
    /// The variable to write, in the active environment.
    pub name: String,
    /// `status`, `header:Location`, `body`, or a path into a JSON body such as
    /// `$.data.token`.
    pub from: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Keep it out of the committed YAML: a captured token usually is one.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub secret: bool,
}

/// One assertion on the response. `from` is the same vocabulary a capture
/// uses; `op` defaults to `is`, which is what most checks are.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Check {
    /// `status`, `time`, `header:Name`, `body`, or `$.data.id`.
    pub from: String,
    #[serde(default)]
    pub op: crate::checks::Op,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub value: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}


/// What kind of thing a request is. Everything but `http` needs a different
/// transport, which is why it is here rather than inferred from the URL.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    Http,
    Websocket,
    Sse,
    Grpc,
}

/// One `.yaml` file under the collection root.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Request {
    pub name: String,
    /// Absent means `http`, so every file written before this existed reads.
    #[serde(default, skip_serializing_if = "Kind::is_http")]
    pub kind: Kind,
    #[serde(default = "default_seq")]
    pub seq: u32,
    pub method: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<KeyValue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<KeyValue>,
    #[serde(default)]
    pub body: Body,
    #[serde(default)]
    pub auth: Auth,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub captures: Vec<Capture>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<Check>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<RequestOptions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
}

/// `collection.yaml` at the collection root.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionMeta {
    pub name: String,
    #[serde(default = "default_seq")]
    pub version: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<KeyValue>,
    #[serde(default)]
    pub auth: Auth,
    /// Defaults that travel with the collection. An environment overrides
    /// them, which is what makes switching environments useful.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vars: Vec<KeyValue>,
}

impl Default for CollectionMeta {
    fn default() -> Self {
        Self {
            name: "Untitled".into(),
            version: 1,
            headers: Vec::new(),
            auth: Auth::None,
            vars: Vec::new(),
        }
    }
}

/// `folder.yaml`, optional, only needed to rename or reorder a directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default = "default_seq")]
    pub seq: u32,
    /// Sent with every request inside, unless one sets the same name itself.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<KeyValue>,
    /// Used by requests inside that are set to inherit. `Inherit` here passes
    /// the question further out, to the folder above or the collection.
    #[serde(default)]
    pub auth: Auth,
    /// Variables for this subtree. Plain values only — a secret belongs in an
    /// environment, which is what keeps it out of the committed file.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vars: Vec<KeyValue>,
}

/// Written by hand rather than derived: `u32::default()` is 0, which would make
/// a folder with no `folder.yaml` sort ahead of everything else and disagree
/// with the serde default used when the file exists but omits `seq`.
impl Default for FolderMeta {
    fn default() -> Self {
        Self { name: None, seq: default_seq(), headers: Vec::new(), auth: Auth::Inherit, vars: Vec::new() }
    }
}

/// One file under `environments/`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Environment {
    pub name: String,
    #[serde(default)]
    pub vars: Vec<EnvVar>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvVar {
    pub name: String,
    #[serde(default)]
    pub value: String,
    /// Secret values are never written to the YAML file. They live in the
    /// gitignored `.env.<environment>` file at the collection root instead.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub secret: bool,
}

impl Kind {
    /// For `skip_serializing_if`: the default never needs writing down.
    pub fn is_http(&self) -> bool {
        matches!(self, Kind::Http)
    }
}
