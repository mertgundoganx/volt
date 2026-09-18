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

use std::sync::Arc;

use bytes::{Buf as _, Bytes, BytesMut};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;

use crate::error::{Error, Result};

/// Anything a connection can be. `Box<dyn Io>` is an `AsyncRead`/`AsyncWrite`
/// in its own right, which is what lets plaintext and TLS share one code path.
trait Io: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Io for T {}

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
pub async fn connect(url: &str, verify_tls: bool) -> Result<Connection> {
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

    let tcp = TcpStream::connect((host.as_str(), port))
        .await
        .map_err(|e| Error::Http(format!("could not connect to {authority}: {e}")))?;
    // Nagle's algorithm holds a small frame back waiting for company, which on
    // a stream of one-message frames is exactly the wrong trade.
    let _ = tcp.set_nodelay(true);

    let io: Box<dyn Io> = match tls {
        false => Box::new(tcp),
        true => Box::new(handshake_tls(tcp, &host, verify_tls).await?),
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

/// The rustls config volt uses, with or without certificate checking.
///
/// Shared, so the WebSocket path cannot quietly verify while the app bar says
/// "TLS off" — which it did, because tungstenite built its own.
pub fn tls_config(verify: bool) -> rustls::ClientConfig {
    ensure_crypto_provider();
    match verify {
        true => {
            let roots = rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
            rustls::ClientConfig::builder().with_root_certificates(roots).with_no_client_auth()
        }
        // The same escape hatch Send has, and the same warning: it is for a
        // staging box with a self-signed certificate, not for the internet.
        false => rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(AcceptAnything))
            .with_no_client_auth(),
    }
}

async fn handshake_tls(
    tcp: TcpStream,
    host: &str,
    verify: bool,
) -> Result<tokio_rustls::client::TlsStream<TcpStream>> {
    let mut config = tls_config(verify);
    // Without this the server has no way to know HTTP/2 was wanted.
    config.alpn_protocols = vec![b"h2".to_vec()];
    let name = rustls::pki_types::ServerName::try_from(host.to_string())
        .map_err(|_| Error::Invalid(format!("`{host}` is not a name a certificate can be for")))?;
    tokio_rustls::TlsConnector::from(Arc::new(config))
        .connect(name, tcp)
        .await
        .map_err(|e| Error::Http(format!("TLS failed: {e}")))
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

/// A header, by a name that is not case sensitive.
pub fn header(headers: &http::HeaderMap, name: &str) -> Option<String> {
    headers.get(name).and_then(|value| value.to_str().ok()).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bug this guards against is a panic, not an error, so a test that
    /// merely builds a config is the whole check: without the installed
    /// provider `builder()` unwinds and takes the command task with it.
    #[test]
    fn a_tls_config_can_be_built_at_all() {
        ensure_crypto_provider();
        let roots = rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
        let config = rustls::ClientConfig::builder().with_root_certificates(roots).with_no_client_auth();
        assert!(!config.alpn_protocols.iter().any(|p| p == b"h2"), "ALPN is set by the caller, not here");
    }
}
