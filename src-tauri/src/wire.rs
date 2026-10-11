//! HTTP/2 as gRPC needs it: frames, not a body.
//!
//! reqwest hands back a response body and nothing else. gRPC needs three things
//! it cannot give: the messages as they arrive rather than at the end, a way to
//! keep sending after the response has started, and the **trailers** — the
//! `grpc-status` that follows a body and says whether the call actually worked.
//! Without trailers a failed streaming call reads as a successful one, which is
//! the worst kind of wrong.
//!
//! So this opens the connection itself, over `h2`, which reqwest already brings
//! into the binary. It is deliberately small: connect, open one stream, send
//! frames, read frames, read trailers. Everything that knows what a gRPC
//! message *means* is in `grpc.rs`; this only moves bytes.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::Engine as _;
use bytes::{Buf as _, Bytes, BytesMut};
use rustls::pki_types::pem::PemObject as _;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use tokio::net::TcpStream;

use crate::error::{Error, Result};

/// Anything a connection can be. `Box<dyn Io>` is an `AsyncRead`/`AsyncWrite`
/// in its own right, which is what lets plaintext, TLS and a tunnel through a
/// proxy share one code path.
pub(crate) trait Io: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Io for T {}

/// A connection that has got as far as the far end, by whatever road.
pub(crate) type Stream = Box<dyn Io>;

/// How a connection is made, as opposed to where it goes: what to trust, what
/// to present, and what to go through.
///
/// It is `http::ExecOptions` with the request's overrides already applied and
/// the paths already resolved against the collection, which is what lets a
/// WebSocket, a gRPC call and a Send be told the same thing in the same words.
/// Before this, each of them took a bare `verify_tls` and nothing else, so a
/// client certificate or a proxy reached HTTP and silently no further.
#[derive(Clone)]
pub struct Transport {
    pub verify_tls: bool,
    /// A PEM of CA certificates trusted on top of the built-in roots.
    pub ca_cert: Option<PathBuf>,
    /// A PEM with a certificate and its key, or a PKCS#12 archive.
    pub client_cert: Option<PathBuf>,
    /// The passphrase of a PKCS#12 archive. Never part of a collection.
    pub client_cert_password: Option<String>,
    /// As in `ExecOptions`: a URL, an empty string for "go direct", or `None`
    /// to do what the environment says.
    pub proxy: Option<String>,
}

impl Transport {
    /// Straight there, checking certificates against the built-in roots.
    pub const fn direct() -> Self {
        Self { verify_tls: true, ca_cert: None, client_cert: None, client_cert_password: None, proxy: None }
    }

    /// The same, and deaf to `HTTPS_PROXY` and its relatives: what an empty
    /// proxy in the settings means. Tests against loopback use it so that the
    /// environment of whoever runs them cannot get between a test and its
    /// own server.
    pub const fn unproxied() -> Self {
        Self {
            verify_tls: true,
            ca_cert: None,
            client_cert: None,
            client_cert_password: None,
            proxy: Some(String::new()),
        }
    }
}

/// Written by hand: a derived one would say `verify_tls: false`, and the safe
/// direction for a default is the one that checks.
impl Default for Transport {
    fn default() -> Self {
        Self::direct()
    }
}

/// An open HTTP/2 connection. Dropping it closes the connection: the task
/// driving it ends as soon as nothing can send on it any more.
pub struct Connection {
    send: h2::client::SendRequest<Bytes>,
    pub authority: String,
    pub scheme: &'static str,
}

/// One request on that connection, in the two halves gRPC needs. They are
/// separate values on purpose: a bidirectional call has a task sending and a
/// task receiving, and one `&mut` over both would not let it.
pub struct Exchange {
    pub status: http::StatusCode,
    pub headers: http::HeaderMap,
    pub tx: Sender,
    pub rx: Receiver,
}

pub struct Sender {
    out: h2::SendStream<Bytes>,
}

pub struct Receiver {
    body: h2::RecvStream,
    /// What has arrived and is not yet a whole message.
    buffer: BytesMut,
    /// The server's trailers, once the body has ended. This is the whole reason
    /// gRPC cannot be done over an ordinary HTTP client.
    pub trailers: Option<http::HeaderMap>,
    done: bool,
}

/// Say which TLS backend rustls should use, once, before anything builds a
/// `ClientConfig`.
///
/// rustls refuses to choose when more than one backend is compiled in, and two
/// are: volt asks for `ring`, and reqwest's `rustls` feature drags in
/// `aws-lc-rs`. Without this, `ClientConfig::builder()` does not return an
/// error — it *panics*, which unwinds the command task, so the promise in the
/// webview never settles and the UI waits forever with no banner. That is how
/// every `grpcs://` call and every `wss://` socket failed.
pub fn ensure_crypto_provider() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // `ring` because volt's own Cargo.toml names it: a reqwest bump cannot
        // take it away.
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// Open a connection to a `http://` or `https://` endpoint.
///
/// gRPC is HTTP/2 only. Over TLS that is negotiated through ALPN; over
/// plaintext there is nothing to negotiate with, so it is assumed — which is
/// what "h2c with prior knowledge" means and what every gRPC server expects.
pub async fn connect(url: &str, transport: &Transport) -> Result<Connection> {
    ensure_crypto_provider();
    let parsed = url::Url::parse(url).map_err(|_| Error::Invalid(format!("`{url}` is not a URL")))?;
    let host = parsed.host_str().ok_or_else(|| Error::Invalid("that URL has no host".into()))?.to_string();
    let tls = match parsed.scheme() {
        "https" | "grpcs" => true,
        "http" | "grpc" => false,
        other => return Err(Error::Invalid(format!("`{other}` is not a scheme gRPC speaks"))),
    };
    let port = parsed.port().unwrap_or(if tls { 443 } else { 80 });
    let authority = match parsed.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.clone(),
    };

    let tcp = dial(&host, port, tls, transport).await?;

    let io: Stream = match tls {
        false => tcp,
        true => {
            let mut config = tls_config(transport)?;
            // Without this the server has no way to know HTTP/2 was wanted.
            config.alpn_protocols = vec![b"h2".to_vec()];
            Box::new(handshake_tls(tcp, &host, config).await?)
        }
    };

    let (send, connection) = h2::client::handshake(io)
        .await
        .map_err(|e| Error::Http(format!("HTTP/2 could not be started: {e}")))?;
    // The connection only makes progress while something polls it.
    tokio::spawn(async move {
        let _ = connection.await;
    });

    Ok(Connection { send, authority, scheme: if tls { "https" } else { "http" } })
}

/// The rustls config volt uses: which roots, whether to check at all, and the
/// certificate to present when the server asks for one.
///
/// Shared, so the WebSocket path cannot quietly verify while the app bar says
/// "TLS off" — which it did, because tungstenite built its own.
pub fn tls_config(transport: &Transport) -> Result<rustls::ClientConfig> {
    ensure_crypto_provider();
    let builder = rustls::ClientConfig::builder();
    let builder = match transport.verify_tls {
        true => {
            let mut roots = rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
            // In addition to the built-in roots, not instead of them: a company
            // CA is for the company's hosts, and the rest of the internet still
            // has to check out.
            if let Some(path) = &transport.ca_cert {
                for certificate in read_roots(path)? {
                    roots.add(certificate).map_err(|e| {
                        Error::Invalid(format!("CA certificate `{}`: {e}", path.display()))
                    })?;
                }
            }
            builder.with_root_certificates(roots)
        }
        // The same escape hatch Send has, and the same warning: it is for a
        // staging box with a self-signed certificate, not for the internet.
        false => builder.dangerous().with_custom_certificate_verifier(Arc::new(AcceptAnything)),
    };
    match &transport.client_cert {
        Some(path) => {
            let identity = read_identity(path, transport.client_cert_password.as_deref())?;
            builder
                .with_client_auth_cert(identity.chain, identity.key)
                .map_err(|e| Error::Invalid(format!("client certificate `{}`: {e}", path.display())))
        }
        None => Ok(builder.with_no_client_auth()),
    }
}

/// The CA certificates in a file: one or more PEM `CERTIFICATE` blocks, or a
/// single certificate in DER, which is what a `.cer` saved from a browser is.
pub fn read_roots(path: &Path) -> Result<Vec<CertificateDer<'static>>> {
    let bytes = std::fs::read(path).map_err(|e| Error::io(path.to_string_lossy().into_owned(), e))?;
    if !looks_like_pem(&bytes) {
        // DER starts with a SEQUENCE. Anything else is not a certificate at
        // all, and saying so here beats a TLS error three layers down.
        if bytes.first() == Some(&0x30) {
            return Ok(vec![CertificateDer::from(bytes)]);
        }
        return Err(Error::Invalid(format!(
            "CA certificate `{}` is not a PEM or DER certificate",
            path.display()
        )));
    }
    let found: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(&bytes)
        .collect::<std::result::Result<_, _>>()
        .map_err(|e| Error::Invalid(format!("CA certificate `{}` is not valid PEM: {e}", path.display())))?;
    if found.is_empty() {
        return Err(Error::Invalid(format!(
            "CA certificate `{}` holds no certificate; it should be one or more `BEGIN CERTIFICATE` blocks",
            path.display()
        )));
    }
    Ok(found)
}

fn looks_like_pem(bytes: &[u8]) -> bool {
    bytes.windows(10).any(|window| window == b"-----BEGIN")
}

/// A client certificate and the key that proves it is ours.
pub struct Identity {
    /// The leaf first, then whatever leads from it to a root.
    pub chain: Vec<CertificateDer<'static>>,
    pub key: PrivateKeyDer<'static>,
}

/// Read a client certificate: a PEM holding the certificate and its key, or a
/// PKCS#12 archive (`.p12`, `.pfx`) opened with `password`.
///
/// Which one it is is decided by what the file holds rather than by its name:
/// a PEM is text with `-----BEGIN` in it and an archive is not.
pub fn read_identity(path: &Path, password: Option<&str>) -> Result<Identity> {
    let shown = path.display();
    let bytes = std::fs::read(path).map_err(|e| Error::io(path.to_string_lossy().into_owned(), e))?;

    if !looks_like_pem(&bytes) {
        let store = p12_keystore::KeyStore::from_pkcs12(
            &bytes,
            password.unwrap_or_default(),
            p12_keystore::Pkcs12ImportPolicy::Relaxed,
        )
        .map_err(|e| {
            Error::Invalid(format!(
                "client certificate `{shown}` could not be opened as PKCS#12 — is the passphrase right? ({e})"
            ))
        })?;
        let (_, chain) = store
            .private_key_chain()
            .ok_or_else(|| Error::Invalid(format!("client certificate `{shown}` holds no private key")))?;
        if chain.certs().is_empty() {
            return Err(Error::Invalid(format!(
                "client certificate `{shown}` holds a key but no certificate for it"
            )));
        }
        return Ok(Identity {
            chain: chain.certs().iter().map(|cert| CertificateDer::from(cert.as_der().to_vec())).collect(),
            key: PrivateKeyDer::Pkcs8(chain.key().as_der().to_vec().into()),
        });
    }

    let chain: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(&bytes)
        .collect::<std::result::Result<_, _>>()
        .map_err(|e| Error::Invalid(format!("client certificate `{shown}` is not valid PEM: {e}")))?;
    if chain.is_empty() {
        return Err(Error::Invalid(format!("client certificate `{shown}` holds no certificate")));
    }
    let key = PrivateKeyDer::from_pem_slice(&bytes).map_err(|_| {
        // rustls cannot open a passphrase-protected PEM key, and saying "no
        // key" about a file that visibly has one sends people the wrong way.
        match bytes.windows(9).any(|window| window == b"ENCRYPTED") {
            true => Error::Invalid(format!(
                "the key in client certificate `{shown}` is passphrase-protected; use a .p12/.pfx file with its passphrase, or a PEM with the key decrypted"
            )),
            false => Error::Invalid(format!(
                "client certificate `{shown}` holds no private key; it should be one PEM with the certificate and its key"
            )),
        }
    })?;
    Ok(Identity { chain, key })
}

impl Identity {
    /// The same identity as PEM text, which is the one form reqwest takes with
    /// rustls. A PKCS#12 archive reaches HTTP this way rather than through a
    /// second TLS stack that could read it directly.
    pub fn pem(&self) -> String {
        let block = |label: &str, der: &[u8]| {
            let encoded = base64::engine::general_purpose::STANDARD.encode(der);
            let mut out = format!("-----BEGIN {label}-----\n");
            for line in encoded.as_bytes().chunks(64) {
                out.push_str(&String::from_utf8_lossy(line));
                out.push('\n');
            }
            out.push_str(&format!("-----END {label}-----\n"));
            out
        };
        let mut out = String::new();
        for certificate in &self.chain {
            out.push_str(&block("CERTIFICATE", certificate.as_ref()));
        }
        let label = match &self.key {
            PrivateKeyDer::Pkcs1(_) => "RSA PRIVATE KEY",
            PrivateKeyDer::Sec1(_) => "EC PRIVATE KEY",
            _ => "PRIVATE KEY",
        };
        out.push_str(&block(label, self.key.secret_der()));
        out
    }
}

async fn handshake_tls(
    tcp: Stream,
    host: &str,
    config: rustls::ClientConfig,
) -> Result<tokio_rustls::client::TlsStream<Stream>> {
    let name = rustls::pki_types::ServerName::try_from(host.trim_matches(['[', ']']).to_string())
        .map_err(|_| Error::Invalid(format!("`{host}` is not a name a certificate can be for")))?;
    tokio_rustls::TlsConnector::from(Arc::new(config))
        .connect(name, tcp)
        .await
        .map_err(|e| Error::Http(format!("TLS failed: {e}")))
}

// ---------------------------------------------------------------------------
// Getting there: direct, or through a proxy
// ---------------------------------------------------------------------------

/// Open a TCP connection to `host:port`, through the proxy when there is one.
///
/// What comes back is a plain byte stream to the far end either way — a tunnel
/// is indistinguishable from a socket once it is open — so TLS and everything
/// above it are the caller's, exactly as they would be without a proxy.
/// `secure` only decides which environment variable is consulted when the
/// settings name no proxy.
pub(crate) async fn dial(host: &str, port: u16, secure: bool, transport: &Transport) -> Result<Stream> {
    let bare = host.trim_matches(['[', ']']);
    let Some(proxy) = proxy_for(transport, bare, secure) else {
        let tcp = TcpStream::connect((bare, port))
            .await
            .map_err(|e| Error::Http(format!("could not connect to {host}:{port}: {e}")))?;
        // Nagle's algorithm holds a small frame back waiting for company, which
        // on a stream of one-message frames is exactly the wrong trade.
        let _ = tcp.set_nodelay(true);
        return Ok(Box::new(tcp));
    };

    let parsed = parse_proxy(&proxy)?;
    let tcp = TcpStream::connect((parsed.host.as_str(), parsed.port)).await.map_err(|e| {
        Error::Http(format!("could not connect to the proxy {}:{}: {e}", parsed.host, parsed.port))
    })?;
    let _ = tcp.set_nodelay(true);

    match parsed.kind {
        ProxyKind::Http => {
            let mut stream: Stream = Box::new(tcp);
            http_connect(&mut stream, host, port, parsed.credentials.as_ref()).await?;
            Ok(stream)
        }
        ProxyKind::Https => {
            // The proxy itself is behind TLS; the tunnel then runs inside that,
            // and the caller's own TLS inside the tunnel.
            let outer = handshake_tls(Box::new(tcp), &parsed.host, tls_config(transport)?).await?;
            let mut stream: Stream = Box::new(outer);
            http_connect(&mut stream, host, port, parsed.credentials.as_ref()).await?;
            Ok(stream)
        }
        ProxyKind::Socks5 => {
            let mut stream: Stream = Box::new(tcp);
            socks5_connect(&mut stream, bare, port, parsed.credentials.as_ref()).await?;
            Ok(stream)
        }
    }
}

/// The proxy a connection to `host` goes through, if any.
///
/// The same three answers the HTTP path gives: a URL in the settings is used,
/// an empty string means go direct whatever the environment says, and nothing
/// said at all leaves it to `HTTPS_PROXY` / `HTTP_PROXY` / `ALL_PROXY` and
/// `NO_PROXY`, which is what reqwest reads for a Send.
fn proxy_for(transport: &Transport, host: &str, secure: bool) -> Option<String> {
    match transport.proxy.as_deref().map(str::trim) {
        Some("") => None,
        Some(url) => Some(url.to_string()),
        None => {
            let read = |name: &str| std::env::var(name).ok().filter(|value| !value.trim().is_empty());
            let names: [&str; 4] = match secure {
                true => ["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"],
                false => ["HTTP_PROXY", "http_proxy", "ALL_PROXY", "all_proxy"],
            };
            let proxy = names.iter().find_map(|name| read(name))?;
            let excluded = read("NO_PROXY").or_else(|| read("no_proxy")).unwrap_or_default();
            (!no_proxy_covers(&excluded, host)).then(|| proxy.trim().to_string())
        }
    }
}

/// Whether a `NO_PROXY` list excludes `host`: `*` is everything, and an entry
/// matches the host itself and anything under it, with or without its dot.
fn no_proxy_covers(list: &str, host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    list.split(',')
        .map(|entry| entry.trim().to_ascii_lowercase())
        .filter(|entry| !entry.is_empty())
        .any(|entry| {
            let entry = entry.trim_start_matches('.');
            entry == "*" || host == entry || host.ends_with(&format!(".{entry}"))
        })
}

#[derive(Debug, PartialEq, Eq)]
enum ProxyKind {
    Http,
    Https,
    Socks5,
}

#[derive(Debug)]
struct ProxyTarget {
    kind: ProxyKind,
    host: String,
    port: u16,
    credentials: Option<(String, String)>,
}

fn parse_proxy(proxy: &str) -> Result<ProxyTarget> {
    // `corp:8080` is how most people write one, and how reqwest reads it too.
    let text = if proxy.contains("://") { proxy.to_string() } else { format!("http://{proxy}") };
    let url = url::Url::parse(&text).map_err(|e| Error::Invalid(format!("proxy `{proxy}`: {e}")))?;
    let (kind, default_port) = match url.scheme() {
        "http" => (ProxyKind::Http, 80),
        "https" => (ProxyKind::Https, 443),
        "socks5" | "socks5h" => (ProxyKind::Socks5, 1080),
        other => {
            return Err(Error::Invalid(format!(
                "proxy `{proxy}`: `{other}` is not a kind of proxy volt can tunnel through — http, https and socks5 are"
            )))
        }
    };
    let host = url
        .host_str()
        .ok_or_else(|| Error::Invalid(format!("proxy `{proxy}` has no host")))?
        .trim_matches(['[', ']'])
        .to_string();
    // The URL keeps a username and password percent-encoded; the proxy wants
    // the characters themselves.
    let decode = |text: &str| percent_decode(text);
    let credentials = (!url.username().is_empty())
        .then(|| (decode(url.username()), decode(url.password().unwrap_or_default())));
    Ok(ProxyTarget { kind, host, port: url.port().unwrap_or(default_port), credentials })
}

/// The longest answer to a CONNECT volt will read. A proxy's reply is a status
/// line and a few headers; one that runs past this is not a proxy.
const MAX_PROXY_REPLY: usize = 16 * 1024;

/// Ask an HTTP proxy for a tunnel, and wait until it says the tunnel is open.
async fn http_connect(
    stream: &mut Stream,
    host: &str,
    port: u16,
    credentials: Option<&(String, String)>,
) -> Result<()> {
    let mut request = format!("CONNECT {host}:{port} HTTP/1.1\r\nHost: {host}:{port}\r\n");
    if let Some((username, password)) = credentials {
        let encoded = base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
        request.push_str(&format!("Proxy-Authorization: Basic {encoded}\r\n"));
    }
    request.push_str("\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|e| Error::Http(format!("the proxy stopped listening: {e}")))?;

    // A byte at a time, on purpose: whatever follows the blank line already
    // belongs to the tunnel, and reading ahead would swallow the start of it.
    let mut reply = Vec::new();
    let mut byte = [0u8; 1];
    while !reply.ends_with(b"\r\n\r\n") {
        if reply.len() >= MAX_PROXY_REPLY {
            return Err(Error::Http("the proxy answered with more than a reply to CONNECT".into()));
        }
        match stream.read(&mut byte).await {
            Ok(0) => return Err(Error::Http("the proxy closed the connection before answering".into())),
            Ok(_) => reply.push(byte[0]),
            Err(e) => return Err(Error::Http(format!("the proxy did not answer: {e}"))),
        }
    }
    let text = String::from_utf8_lossy(&reply);
    let line = text.lines().next().unwrap_or_default();
    let status = line.split_whitespace().nth(1).and_then(|code| code.parse::<u16>().ok());
    match status {
        Some(200..=299) => Ok(()),
        Some(407) => Err(Error::Http(
            "the proxy wants a username and password (407); put them in its URL as http://user:password@host:port"
                .into(),
        )),
        Some(code) => Err(Error::Http(format!("the proxy refused to connect to {host}:{port} ({code})"))),
        None => Err(Error::Http(format!("the proxy answered `{line}`, which is not HTTP"))),
    }
}

/// RFC 1928, the part a client needs: no authentication or a username and
/// password (RFC 1929), and one CONNECT. The name is handed to the proxy to
/// resolve, because the proxy is usually the only thing that can.
async fn socks5_connect(
    stream: &mut Stream,
    host: &str,
    port: u16,
    credentials: Option<&(String, String)>,
) -> Result<()> {
    let gone = |e: std::io::Error| Error::Http(format!("the SOCKS proxy dropped the connection: {e}"));

    let greeting: &[u8] = match credentials {
        Some(_) => &[5, 2, 0, 2],
        None => &[5, 1, 0],
    };
    stream.write_all(greeting).await.map_err(gone)?;
    let mut chosen = [0u8; 2];
    stream.read_exact(&mut chosen).await.map_err(gone)?;
    match (chosen, credentials) {
        ([5, 0], _) => {}
        ([5, 2], Some((username, password))) => {
            // Each is one length byte and then the text, so 255 is the ceiling.
            if username.len() > 255 || password.len() > 255 {
                return Err(Error::Invalid("a SOCKS username or password is at most 255 bytes".into()));
            }
            let mut login = vec![1, username.len() as u8];
            login.extend_from_slice(username.as_bytes());
            login.push(password.len() as u8);
            login.extend_from_slice(password.as_bytes());
            stream.write_all(&login).await.map_err(gone)?;
            let mut answer = [0u8; 2];
            stream.read_exact(&mut answer).await.map_err(gone)?;
            if answer[1] != 0 {
                return Err(Error::Http("the SOCKS proxy refused the username and password".into()));
            }
        }
        ([5, _], _) => {
            return Err(Error::Http("the SOCKS proxy wants a way of signing in volt does not have".into()))
        }
        _ => return Err(Error::Http("that proxy did not answer as SOCKS5".into())),
    }

    let mut request = vec![5, 1, 0];
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(address)) => {
            request.push(1);
            request.extend_from_slice(&address.octets());
        }
        Ok(std::net::IpAddr::V6(address)) => {
            request.push(4);
            request.extend_from_slice(&address.octets());
        }
        Err(_) => {
            if host.len() > 255 {
                return Err(Error::Invalid(format!("`{host}` is too long a name for SOCKS")));
            }
            request.push(3);
            request.push(host.len() as u8);
            request.extend_from_slice(host.as_bytes());
        }
    }
    request.extend_from_slice(&port.to_be_bytes());
    stream.write_all(&request).await.map_err(gone)?;

    let mut head = [0u8; 4];
    stream.read_exact(&mut head).await.map_err(gone)?;
    if head[0] != 5 {
        return Err(Error::Http("that proxy did not answer as SOCKS5".into()));
    }
    if head[1] != 0 {
        let why = match head[1] {
            2 => "the proxy's rules do not allow it",
            3 => "the network is unreachable",
            4 => "the host is unreachable",
            5 => "the connection was refused",
            6 => "it took too long",
            _ => "the proxy gave up",
        };
        return Err(Error::Http(format!("the SOCKS proxy could not connect to {host}:{port}: {why}")));
    }
    // The address the proxy bound, which nothing here needs — but it is on the
    // wire ahead of the tunnel, so it has to be read off.
    let rest = match head[3] {
        1 => 4 + 2,
        4 => 16 + 2,
        3 => {
            let mut length = [0u8; 1];
            stream.read_exact(&mut length).await.map_err(gone)?;
            length[0] as usize + 2
        }
        _ => return Err(Error::Http("the SOCKS proxy answered with an address of no known kind".into())),
    };
    let mut bound = vec![0u8; rest];
    stream.read_exact(&mut bound).await.map_err(gone)?;
    Ok(())
}

/// Used only when the user has turned certificate checking off for the request.
#[derive(Debug)]
struct AcceptAnything;

impl rustls::client::danger::ServerCertVerifier for AcceptAnything {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider().signature_verification_algorithms.supported_schemes()
    }
}

impl Connection {
    /// Start a call: send the headers and hand back the two halves.
    ///
    /// The response is deliberately *not* awaited here. A gRPC server reads the
    /// request message before it answers, so waiting for headers first is a
    /// deadlock: the server waits for a DATA frame that the client will not
    /// send until the server has answered. Send first, then `receive`.
    pub async fn start(
        &mut self,
        path: &str,
        headers: &[(String, String)],
    ) -> Result<(Sender, h2::client::ResponseFuture)> {
        let uri = format!("{}://{}{}", self.scheme, self.authority, path);
        let mut builder = http::Request::builder().method(http::Method::POST).uri(uri);
        for (name, value) in headers {
            builder = builder.header(name.as_str(), value.as_str());
        }
        let request = builder
            .body(())
            .map_err(|e| Error::Invalid(format!("those headers cannot be sent: {e}")))?;

        let ready = futures_util::future::poll_fn(|cx| self.send.poll_ready(cx));
        ready.await.map_err(|e| Error::Http(format!("the connection was not ready: {e}")))?;

        let (response, out) = self
            .send
            .send_request(request, false)
            .map_err(|e| Error::Http(format!("the call could not be started: {e}")))?;

        Ok((Sender { out }, response))
    }
}

/// Wait for the server's headers, once at least one message has gone out.
pub async fn receive(response: h2::client::ResponseFuture, tx: Sender) -> Result<Exchange> {
    let response = response.await.map_err(|e| Error::Http(describe(&e)))?;
    let (parts, body) = response.into_parts();
    Ok(Exchange {
        status: parts.status,
        headers: parts.headers,
        tx,
        rx: Receiver { body, buffer: BytesMut::new(), trailers: None, done: false },
    })
}

/// The largest single gRPC message volt will hold. Well past any reply worth
/// reading in an API client, and far short of what a hostile server can claim.
const MAX_MESSAGE: usize = 16 * 1024 * 1024;

/// The gRPC length-prefixed frame: one flag byte, four length bytes, the
/// message. The flag is compression, which volt does not use and does not
/// unpack.
pub fn frame(message: &[u8]) -> Bytes {
    let mut out = BytesMut::with_capacity(message.len() + 5);
    out.extend_from_slice(&[0]);
    out.extend_from_slice(&(message.len() as u32).to_be_bytes());
    out.extend_from_slice(message);
    out.freeze()
}

impl Sender {
    /// Send one message. `last` closes the request half after it.
    ///
    /// Capacity is waited for rather than assumed: HTTP/2 flow control means a
    /// peer can say "not yet", and a message pushed into a full window is
    /// refused rather than queued.
    pub async fn send(&mut self, message: &[u8], last: bool) -> Result<()> {
        let payload = frame(message);
        self.out.reserve_capacity(payload.len());
        while self.out.capacity() < payload.len() {
            let given = futures_util::future::poll_fn(|cx| self.out.poll_capacity(cx)).await;
            match given {
                Some(Ok(_)) => {}
                Some(Err(e)) => return Err(Error::Http(describe(&e))),
                None => return Err(Error::Http("the server stopped listening".into())),
            }
        }
        self.out
            .send_data(payload, last)
            .map_err(|e| Error::Http(format!("the message could not be sent: {e}")))
    }

    /// Say nothing more. A server-streaming call does this at once.
    pub fn finish(&mut self) -> Result<()> {
        self.out
            .send_data(Bytes::new(), true)
            .map_err(|e| Error::Http(format!("the call could not be closed: {e}")))
    }
}

impl Receiver {
    /// The next whole message, or `None` once the response is over. The
    /// trailers are read as soon as the body ends, so by the time this answers
    /// `None` the status that matters is in `trailers`.
    pub async fn next(&mut self) -> Result<Option<Bytes>> {
        loop {
            if let Some(message) = self.take()? {
                return Ok(Some(message));
            }
            if self.done {
                return Ok(None);
            }
            match self.body.data().await {
                Some(Ok(chunk)) => {
                    // Flow control: what is not released is never sent again.
                    let _ = self.body.flow_control().release_capacity(chunk.len());
                    self.buffer.extend_from_slice(&chunk);
                    // And a server that never sends a length prefix at all must
                    // not be able to grow the buffer for ever either.
                    if self.buffer.len() > MAX_MESSAGE + 5 {
                        return Err(Error::Invalid(
                            "the server sent more than volt will buffer for one message".into(),
                        ));
                    }
                }
                Some(Err(e)) => return Err(Error::Http(describe(&e))),
                None => {
                    self.done = true;
                    self.trailers =
                        self.body.trailers().await.map_err(|e| Error::Http(describe(&e)))?;
                }
            }
        }
    }

    /// One message out of the buffer, if a whole one is there.
    fn take(&mut self) -> Result<Option<Bytes>> {
        if self.buffer.len() < 5 {
            return Ok(None);
        }
        if self.buffer[0] == 1 {
            return Err(Error::Invalid("the reply is compressed, which volt does not unpack".into()));
        }
        let length =
            u32::from_be_bytes([self.buffer[1], self.buffer[2], self.buffer[3], self.buffer[4]])
                as usize;
        // The length is the server's word for it, and the server may be hostile
        // or broken. Without a ceiling it can ask volt to hold 4 GiB while the
        // receiver keeps handing out more flow-control capacity.
        if length > MAX_MESSAGE {
            return Err(Error::Invalid(format!(
                "the server declared a {length}-byte message, larger than volt will read"
            )));
        }
        if self.buffer.len() < 5 + length {
            return Ok(None);
        }
        self.buffer.advance(5);
        Ok(Some(self.buffer.split_to(length).freeze()))
    }
}

/// h2's errors say `stream error received: ...`, which helps nobody.
pub fn describe(error: &h2::Error) -> String {
    if error.is_io() {
        return format!("the connection dropped: {error}");
    }
    match error.reason() {
        Some(h2::Reason::REFUSED_STREAM) => "the server refused the call".into(),
        Some(h2::Reason::CANCEL) => "the call was cancelled".into(),
        Some(reason) => format!("HTTP/2 said {reason}"),
        None => error.to_string(),
    }
}

/// `%41` → `A`. Used for a `grpc-message`, which arrives percent-encoded,
/// and for the username and password in a proxy URL. A `%` that is not
/// followed by two hex digits is left as it is.
pub(crate) fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let hex = bytes.get(at + 1..at + 3).and_then(|pair| std::str::from_utf8(pair).ok());
        match (bytes[at], hex.and_then(|hex| u8::from_str_radix(hex, 16).ok())) {
            (b'%', Some(byte)) => {
                out.push(byte);
                at += 3;
            }
            (byte, _) => {
                out.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A header, by a name that is not case sensitive.
pub fn header(headers: &http::HeaderMap, name: &str) -> Option<String> {
    headers.get(name).and_then(|value| value.to_str().ok()).map(str::to_string)
}

/// What the TLS and proxy tests in several modules share: an authority made
/// for the occasion, a server certificate and a client certificate it signed,
/// and a proxy that really tunnels. Nothing here is a mock of the thing under
/// test — the handshakes are real ones, over loopback.
#[cfg(test)]
pub(crate) mod testing {
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    use super::*;

    /// The passphrase on the PKCS#12 archive.
    pub const PASSPHRASE: &str = "correct horse";

    pub struct Pki {
        pub dir: PathBuf,
        /// The authority's certificate, as PEM.
        pub ca: PathBuf,
        /// The client's certificate and key in one PEM.
        pub client: PathBuf,
        /// The same client identity as a PKCS#12 archive.
        pub client_p12: PathBuf,
        ca_der: CertificateDer<'static>,
        server_chain: Vec<CertificateDer<'static>>,
        server_key: Vec<u8>,
    }

    impl Pki {
        /// `tag` keeps one test's files apart from another's.
        pub fn new(tag: &str) -> Pki {
            ensure_crypto_provider();
            let dir = std::env::temp_dir().join(format!("volt-pki-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();

            let ca_key = rcgen::KeyPair::generate().unwrap();
            let mut ca_params = rcgen::CertificateParams::new(Vec::<String>::new()).unwrap();
            ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
            ca_params.distinguished_name.push(rcgen::DnType::CommonName, "volt test authority");
            ca_params.key_usages = vec![rcgen::KeyUsagePurpose::KeyCertSign, rcgen::KeyUsagePurpose::DigitalSignature];
            let ca_cert = ca_params.self_signed(&ca_key).unwrap();
            let issuer = rcgen::Issuer::new(ca_params, ca_key);

            let server_key = rcgen::KeyPair::generate().unwrap();
            let server = rcgen::CertificateParams::new(vec!["localhost".to_string(), "127.0.0.1".to_string()])
                .unwrap()
                .signed_by(&server_key, &issuer)
                .unwrap();

            let client_key = rcgen::KeyPair::generate().unwrap();
            let mut client_params = rcgen::CertificateParams::new(Vec::<String>::new()).unwrap();
            client_params.distinguished_name.push(rcgen::DnType::CommonName, "volt test client");
            let client = client_params.signed_by(&client_key, &issuer).unwrap();

            let ca = dir.join("ca.pem");
            std::fs::write(&ca, ca_cert.pem()).unwrap();
            let client_pem = dir.join("client.pem");
            std::fs::write(&client_pem, format!("{}{}", client.pem(), client_key.serialize_pem())).unwrap();

            let mut store = p12_keystore::KeyStore::new();
            store.add_entry(
                "client",
                p12_keystore::KeyStoreEntry::PrivateKeyChain(p12_keystore::PrivateKeyChain::new(
                    "client",
                    p12_keystore::PrivateKey::from_der(&client_key.serialize_der()).unwrap(),
                    [p12_keystore::Certificate::from_der(client.der().as_ref()).unwrap()],
                )),
            );
            let client_p12 = dir.join("client.p12");
            std::fs::write(&client_p12, store.writer(PASSPHRASE).write().unwrap()).unwrap();

            Pki {
                dir,
                ca,
                client: client_pem,
                client_p12,
                ca_der: ca_cert.der().clone(),
                server_chain: vec![server.der().clone()],
                server_key: server_key.serialize_der(),
            }
        }

        /// A server's side of TLS. `mutual` refuses a client that presents no
        /// certificate signed by the authority; `alpn` is what it offers.
        pub fn acceptor(&self, mutual: bool, alpn: &[&[u8]]) -> tokio_rustls::TlsAcceptor {
            let key = PrivateKeyDer::Pkcs8(self.server_key.clone().into());
            let builder = rustls::ServerConfig::builder();
            let mut config = match mutual {
                false => builder.with_no_client_auth().with_single_cert(self.server_chain.clone(), key).unwrap(),
                true => {
                    let mut roots = rustls::RootCertStore::empty();
                    roots.add(self.ca_der.clone()).unwrap();
                    let verifier =
                        rustls::server::WebPkiClientVerifier::builder(Arc::new(roots)).build().unwrap();
                    builder
                        .with_client_cert_verifier(verifier)
                        .with_single_cert(self.server_chain.clone(), key)
                        .unwrap()
                }
            };
            config.alpn_protocols = alpn.iter().map(|protocol| protocol.to_vec()).collect();
            tokio_rustls::TlsAcceptor::from(Arc::new(config))
        }

        /// A transport that trusts the authority and goes straight there.
        pub fn trusting(&self) -> Transport {
            Transport { ca_cert: Some(self.ca.clone()), ..Transport::unproxied() }
        }
    }

    impl Drop for Pki {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// An HTTP proxy on loopback that does what a real one does with CONNECT:
    /// opens the connection asked for and copies bytes both ways. Every
    /// request head it was sent is kept, so a test can see what was asked.
    pub async fn http_proxy() -> (u16, Arc<Mutex<Vec<String>>>) {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen: Arc<Mutex<Vec<String>>> = Arc::default();
        let kept = seen.clone();

        tokio::spawn(async move {
            while let Ok((mut client, _)) = listener.accept().await {
                let kept = kept.clone();
                tokio::spawn(async move {
                    let mut head = Vec::new();
                    let mut byte = [0u8; 1];
                    while !head.ends_with(b"\r\n\r\n") {
                        match client.read(&mut byte).await {
                            Ok(1) => head.push(byte[0]),
                            _ => return,
                        }
                    }
                    let head = String::from_utf8_lossy(&head).to_string();
                    let target = head.split_whitespace().nth(1).unwrap_or_default().to_string();
                    kept.lock().unwrap().push(head);
                    let Ok(mut upstream) = tokio::net::TcpStream::connect(target.as_str()).await else {
                        let _ = client.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n").await;
                        return;
                    };
                    if client.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n").await.is_err() {
                        return;
                    }
                    let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
                });
            }
        });
        (port, seen)
    }

    /// Something to tunnel to: whatever arrives is sent straight back.
    pub async fn echo() -> u16 {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let (mut reader, mut writer) = socket.split();
                    let _ = tokio::io::copy(&mut reader, &mut writer).await;
                });
            }
        });
        port
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bug this guards against is a panic, not an error, so a test that
    /// merely builds a config is the whole check: without the installed
    /// provider `builder()` unwinds and takes the command task with it.
    #[test]
    fn a_tls_config_can_be_built_at_all() {
        let config = tls_config(&Transport::direct()).expect("the plain config builds");
        assert!(!config.alpn_protocols.iter().any(|p| p == b"h2"), "ALPN is set by the caller, not here");
        tls_config(&Transport { verify_tls: false, ..Transport::direct() }).expect("and so does the unchecked one");
    }

    #[test]
    fn a_default_transport_checks_certificates() {
        assert!(Transport::default().verify_tls, "the default has to be the direction that is safe");
        assert!(Transport::default().proxy.is_none(), "and it leaves the proxy to the environment");
        assert_eq!(Transport::unproxied().proxy.as_deref(), Some(""));
    }

    #[test]
    fn ca_certificates_are_read_one_or_many_and_a_bad_file_says_why() {
        let pki = testing::Pki::new("roots");
        assert_eq!(read_roots(&pki.ca).unwrap().len(), 1);

        // A bundle is the same blocks one after another, with whatever people
        // leave between them.
        let one = std::fs::read_to_string(&pki.ca).unwrap();
        let bundle = pki.dir.join("bundle.pem");
        std::fs::write(&bundle, format!("# company roots\n{one}\n\n{one}")).unwrap();
        assert_eq!(read_roots(&bundle).unwrap().len(), 2, "every certificate in the file is trusted");

        // DER, which is what a browser saves as `.cer`.
        let der = pki.dir.join("ca.cer");
        std::fs::write(&der, read_roots(&pki.ca).unwrap()[0].as_ref()).unwrap();
        assert_eq!(read_roots(&der).unwrap().len(), 1);

        let missing = read_roots(&pki.dir.join("nope.pem")).unwrap_err();
        assert!(matches!(missing, Error::Io { .. }), "{missing}");
        assert!(missing.to_string().contains("nope.pem"), "the path is in the message: {missing}");

        let junk = pki.dir.join("junk.pem");
        std::fs::write(&junk, "this is not a certificate").unwrap();
        let said = read_roots(&junk).unwrap_err().to_string();
        assert!(said.contains("junk.pem") && said.contains("not a PEM or DER certificate"), "{said}");

        // A PEM, but of the wrong thing: the client's file has a key in it and
        // the test below reads it fine, but a file with only a key has no root.
        let key_only = pki.dir.join("key.pem");
        let client = std::fs::read_to_string(&pki.client).unwrap();
        std::fs::write(&key_only, &client[client.find("-----BEGIN PRIVATE KEY").unwrap()..]).unwrap();
        let said = read_roots(&key_only).unwrap_err().to_string();
        assert!(said.contains("holds no certificate"), "{said}");

        // And the config that would have used it fails as an error, not a panic.
        let broken = Transport { ca_cert: Some(junk), ..Transport::direct() };
        assert!(tls_config(&broken).is_err());
        // With checking off, the roots are not consulted, so they are not read.
        assert!(tls_config(&Transport { verify_tls: false, ..broken }).is_ok());
    }

    #[test]
    fn a_client_certificate_reads_the_same_from_pem_and_from_pkcs12() {
        let pki = testing::Pki::new("identity");

        let pem = read_identity(&pki.client, None).expect("the PEM reads");
        let archive = read_identity(&pki.client_p12, Some(testing::PASSPHRASE)).expect("the archive opens");
        assert_eq!(pem.chain, archive.chain, "one identity, two containers");
        assert_eq!(pem.key.secret_der(), archive.key.secret_der());

        // What reqwest is handed reads back as the same identity.
        let round = pki.dir.join("round.pem");
        std::fs::write(&round, archive.pem()).unwrap();
        let back = read_identity(&round, None).unwrap();
        assert_eq!(back.chain, archive.chain);
        assert_eq!(back.key.secret_der(), archive.key.secret_der());

        let wrong = read_identity(&pki.client_p12, Some("nope")).err().expect("a wrong passphrase is refused");
        assert!(wrong.to_string().contains("passphrase"), "{wrong}");
        assert!(read_identity(&pki.client_p12, None).is_err(), "and so is none at all");

        // The authority's file has a certificate and no key.
        let said = read_identity(&pki.ca, None).err().unwrap().to_string();
        assert!(said.contains("holds no private key"), "{said}");

        // A key rustls cannot open is named for what it is.
        let locked = pki.dir.join("locked.pem");
        let certificate = std::fs::read_to_string(&pki.ca).unwrap();
        std::fs::write(
            &locked,
            format!("{certificate}-----BEGIN ENCRYPTED PRIVATE KEY-----\nAAAA\n-----END ENCRYPTED PRIVATE KEY-----\n"),
        )
        .unwrap();
        let said = read_identity(&locked, None).err().unwrap().to_string();
        assert!(said.contains("passphrase-protected") && said.contains(".p12"), "{said}");

        // Both containers make a config that will present the certificate.
        for (path, password) in [(&pki.client, None), (&pki.client_p12, Some(testing::PASSPHRASE.to_string()))] {
            let transport = Transport {
                client_cert: Some(path.clone()),
                client_cert_password: password,
                ..pki.trusting()
            };
            assert!(tls_config(&transport).unwrap().client_auth_cert_resolver.has_certs());
        }
        assert!(!tls_config(&pki.trusting()).unwrap().client_auth_cert_resolver.has_certs());
    }

    #[test]
    fn a_proxy_is_read_the_way_people_write_one() {
        let plain = parse_proxy("corp.test:8080").unwrap();
        assert_eq!((plain.kind, plain.host.as_str(), plain.port), (ProxyKind::Http, "corp.test", 8080));
        assert!(plain.credentials.is_none());

        let signed_in = parse_proxy("http://ada:p%40ss%20word@corp.test").unwrap();
        assert_eq!(signed_in.port, 80, "the scheme's own port when none is written");
        assert_eq!(signed_in.credentials, Some(("ada".into(), "p@ss word".into())), "decoded, as the proxy wants them");

        assert_eq!(parse_proxy("https://corp.test").unwrap().port, 443);
        let socks = parse_proxy("socks5h://127.0.0.1").unwrap();
        assert_eq!((socks.kind, socks.port), (ProxyKind::Socks5, 1080));

        let said = parse_proxy("ftp://corp.test").unwrap_err().to_string();
        assert!(said.contains("`ftp`") && said.contains("socks5"), "{said}");
    }

    #[test]
    fn an_empty_proxy_means_direct_and_a_named_one_is_used() {
        let named = Transport { proxy: Some(" http://corp.test:8080 ".into()), ..Transport::direct() };
        assert_eq!(proxy_for(&named, "api.test", true).as_deref(), Some("http://corp.test:8080"));
        // Whatever HTTPS_PROXY says in the environment this runs in.
        assert_eq!(proxy_for(&Transport::unproxied(), "api.test", true), None);
        assert_eq!(proxy_for(&Transport::unproxied(), "api.test", false), None);
    }

    #[test]
    fn no_proxy_excludes_a_host_and_what_is_under_it() {
        assert!(no_proxy_covers("localhost, .corp.test", "localhost"));
        assert!(no_proxy_covers("localhost, .corp.test", "api.corp.test"));
        assert!(no_proxy_covers("corp.test", "API.Corp.Test"), "names do not care about case");
        assert!(no_proxy_covers("corp.test", "corp.test"));
        assert!(!no_proxy_covers("corp.test", "notcorp.test"), "a suffix has to start at a dot");
        assert!(no_proxy_covers("*", "anything.test"));
        assert!(!no_proxy_covers("", "anything.test"));
    }

    #[test]
    fn percent_escapes_decode_and_a_stray_percent_survives() {
        assert_eq!(percent_decode("no%20such%20order"), "no such order");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz%41"), "%zzA");
        assert_eq!(percent_decode("ge%C3%A7ti"), "geçti", "bytes are joined before they are read as text");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_tunnel_through_an_http_proxy_carries_the_bytes() {
        let target = testing::echo().await;
        let (proxy, seen) = testing::http_proxy().await;

        let transport =
            Transport { proxy: Some(format!("http://ada:secret@127.0.0.1:{proxy}")), ..Transport::direct() };
        let mut stream = dial("127.0.0.1", target, false, &transport).await.expect("the tunnel opens");
        stream.write_all(b"through the tunnel").await.unwrap();
        let mut back = [0u8; 18];
        stream.read_exact(&mut back).await.unwrap();
        assert_eq!(&back, b"through the tunnel", "nothing of the proxy's reply leaked into the stream");

        let asked = seen.lock().unwrap().join("\n");
        assert!(asked.starts_with(&format!("CONNECT 127.0.0.1:{target} HTTP/1.1")), "{asked}");
        // base64("ada:secret")
        assert!(asked.contains("Proxy-Authorization: Basic YWRhOnNlY3JldA=="), "{asked}");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_proxy_that_refuses_says_so() {
        // The target is a port nothing listens on, so the proxy answers 502.
        let (proxy, _) = testing::http_proxy().await;
        let transport = Transport { proxy: Some(format!("127.0.0.1:{proxy}")), ..Transport::direct() };
        let said = dial("127.0.0.1", 1, false, &transport).await.err().expect("refused").to_string();
        assert!(said.contains("the proxy refused") && said.contains("502"), "{said}");

        // And one that wants signing in says how to do that.
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut discard = [0u8; 1024];
            let _ = socket.read(&mut discard).await;
            let _ = socket.write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\n\r\n").await;
        });
        let transport = Transport { proxy: Some(format!("http://127.0.0.1:{port}")), ..Transport::direct() };
        let said = dial("api.test", 443, true, &transport).await.err().expect("refused").to_string();
        assert!(said.contains("407") && said.contains("user:password@"), "{said}");

        // No proxy listening at all is a different sentence from no server.
        let transport = Transport { proxy: Some("http://127.0.0.1:1".into()), ..Transport::direct() };
        let said = dial("api.test", 443, true, &transport).await.err().expect("refused").to_string();
        assert!(said.contains("could not connect to the proxy"), "{said}");
    }

    /// A SOCKS5 server, the few bytes of it a CONNECT needs, written out so
    /// that the framing checked is the RFC's and not a library's.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_tunnel_through_a_socks_proxy_signs_in_and_carries_the_bytes() {
        let target = testing::echo().await;
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let server = tokio::spawn(async move {
            let (mut client, _) = listener.accept().await.unwrap();
            let mut greeting = [0u8; 4];
            client.read_exact(&mut greeting).await.unwrap();
            assert_eq!(greeting, [5, 2, 0, 2], "version 5, offering none and username/password");
            client.write_all(&[5, 2]).await.unwrap();

            let mut head = [0u8; 2];
            client.read_exact(&mut head).await.unwrap();
            let mut username = vec![0u8; head[1] as usize];
            client.read_exact(&mut username).await.unwrap();
            let mut length = [0u8; 1];
            client.read_exact(&mut length).await.unwrap();
            let mut password = vec![0u8; length[0] as usize];
            client.read_exact(&mut password).await.unwrap();
            client.write_all(&[1, 0]).await.unwrap();

            let mut request = [0u8; 5];
            client.read_exact(&mut request).await.unwrap();
            assert_eq!(request[..4], [5, 1, 0, 3], "a CONNECT, by name");
            let mut name = vec![0u8; request[4] as usize];
            client.read_exact(&mut name).await.unwrap();
            let mut port = [0u8; 2];
            client.read_exact(&mut port).await.unwrap();

            let mut upstream =
                tokio::net::TcpStream::connect(("127.0.0.1", u16::from_be_bytes(port))).await.unwrap();
            client.write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 0]).await.unwrap();
            let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
            (String::from_utf8(username).unwrap(), String::from_utf8(password).unwrap(), String::from_utf8(name).unwrap())
        });

        let transport =
            Transport { proxy: Some(format!("socks5://ada:secret@127.0.0.1:{port}")), ..Transport::direct() };
        let mut stream = dial("localhost", target, false, &transport).await.expect("the tunnel opens");
        stream.write_all(b"hello").await.unwrap();
        let mut back = [0u8; 5];
        stream.read_exact(&mut back).await.unwrap();
        assert_eq!(&back, b"hello");
        drop(stream);

        let (username, password, name) = server.await.unwrap();
        assert_eq!((username.as_str(), password.as_str()), ("ada", "secret"));
        assert_eq!(name, "localhost", "the name goes to the proxy to resolve");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_server_signed_by_our_own_authority_is_trusted_only_when_told_about_it() {
        let pki = testing::Pki::new("wire-tls");
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let acceptor = pki.acceptor(false, &[b"h2"]);
        tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                let acceptor = acceptor.clone();
                tokio::spawn(async move {
                    if let Ok(tls) = acceptor.accept(socket).await {
                        if let Ok(mut connection) = h2::server::handshake(tls).await {
                            while connection.accept().await.is_some() {}
                        }
                    }
                });
            }
        });
        let url = format!("https://localhost:{port}");

        let refused = connect(&url, &Transport::unproxied()).await.err().expect("an unknown authority").to_string();
        assert!(refused.contains("TLS failed"), "{refused}");

        connect(&url, &pki.trusting()).await.expect("the authority's certificate makes it trusted");
        connect(&url, &Transport { verify_tls: false, ..Transport::unproxied() })
            .await
            .expect("and with checking off nothing is asked");
    }
}
