//! Connections that stay open: WebSocket, server-sent events, and a GraphQL
//! subscription, which is a WebSocket with a small protocol spoken over it.
//!
//! Everything else in volt is one request and one response. These are not, and
//! the shape that difference takes is: a connection gets an id, lives in a
//! registry, and pushes events at the UI until it is closed. The UI holds no
//! socket and parses no frames — it listens to `volt://stream` and renders.
//!
//! What they share is how they are addressed. The URL, the headers and the
//! auth come from `http::plan`, the same resolution a Send uses, so a
//! `{{baseUrl}}` and an inherited token mean the same thing here as anywhere.

use std::collections::HashMap;
use std::sync::Mutex;

use futures_util::{SinkExt as _, StreamExt as _};
use serde::Serialize;

use crate::error::{Error, Result};
use crate::http::Plan;
use crate::wire::Transport;

pub const CHANNEL: &str = "volt://stream";

/// Where events go. A closure rather than an `AppHandle`, so a test can hold
/// the other end and watch a real socket rather than a mock of one.
pub type Sink = std::sync::Arc<dyn Fn(Event) + Send + Sync>;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub id: String,
    /// `open`, `message`, `sent`, `error`, `closed`.
    pub kind: &'static str,
    pub at: u64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub data: String,
    /// The `event:` field of an SSE message, when it has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Open {
    pub id: String,
    pub kind: &'static str,
    pub url: String,
}

pub struct Handle {
    stop: tokio::sync::oneshot::Sender<()>,
    /// What to say back, for a connection the UI may write to: a WebSocket
    /// can be, server-sent events and a GraphQL subscription cannot.
    outgoing: Option<tokio::sync::mpsc::UnboundedSender<String>>,
}

/// Cloneable, so a connection's own task can take the registry with it and
/// remove itself when it ends. Without that a closed socket would sit in the
/// map forever and `open_ids` would lie.
#[derive(Default, Clone)]
pub struct Streams(std::sync::Arc<Mutex<HashMap<String, Handle>>>);

impl Streams {
    fn keep(&self, id: String, handle: Handle) {
        self.0.lock().expect("streams").insert(id, handle);
    }

    /// Register a connection another module owns. A streaming gRPC call is
    /// the same kind of thing to the UI as a WebSocket — an id, a list of
    /// events, a way to say more and a way to stop — so it goes in the same
    /// registry rather than a second one that has to be kept in step.
    pub fn register(
        &self,
        id: String,
        stop: tokio::sync::oneshot::Sender<()>,
        outgoing: Option<tokio::sync::mpsc::UnboundedSender<String>>,
    ) {
        self.keep(id, Handle { stop, outgoing });
    }

    pub fn send(&self, id: &str, text: String) -> Result<()> {
        let streams = self.0.lock().expect("streams");
        let handle = streams.get(id).ok_or_else(|| Error::Invalid("that connection is closed".into()))?;
        let outgoing = handle
            .outgoing
            .as_ref()
            .ok_or_else(|| Error::Invalid("that connection only carries messages one way".into()))?;
        outgoing.send(text).map_err(|_| Error::Invalid("that connection is closed".into()))
    }

    pub fn close(&self, id: &str) {
        if let Some(handle) = self.0.lock().expect("streams").remove(id) {
            let _ = handle.stop.send(());
        }
    }

    pub fn open_ids(&self) -> Vec<String> {
        self.0.lock().expect("streams").keys().cloned().collect()
    }

    pub fn forget(&self, id: &str) {
        self.0.lock().expect("streams").remove(id);
    }

    pub fn clone_registry(&self) -> Streams {
        Streams(self.0.clone())
    }
}

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

pub fn emit(sink: &Sink, id: &str, kind: &'static str, data: impl Into<String>, name: Option<String>) {
    sink(Event { id: id.to_string(), kind, at: now(), data: data.into(), name });
}

/// `http(s)://` → `ws(s)://`, which is the only difference in how the two are
/// written down. A URL that already says `ws` is left alone.
pub fn websocket_url(url: &str) -> String {
    match url.split_once("://") {
        Some(("http", rest)) => format!("ws://{rest}"),
        Some(("https", rest)) => format!("wss://{rest}"),
        _ => url.to_string(),
    }
}

pub async fn open_websocket(
    sink: Sink,
    streams: &Streams,
    id: String,
    plan: Plan,
    transport: &Transport,
) -> Result<Open> {
    let url = websocket_url(&plan.url);
    let (socket, status) = connect_websocket(&url, &plan_headers(&plan), &[], transport).await?;
    Ok(run_websocket(sink, streams, id, url, socket, status, None))
}

/// A WebSocket once its handshake is done, whatever it runs over.
pub(crate) type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<crate::wire::Stream>>;

/// Open the socket: reach the host (through the proxy, when there is one),
/// do TLS for `wss`, and upgrade. `protocols` is what goes in
/// `Sec-WebSocket-Protocol`, for a protocol spoken on top of the socket.
///
/// The TCP connection is made here rather than left to tungstenite, which
/// would dial the host itself and so could never go through a proxy.
pub(crate) async fn connect_websocket(
    url: &str,
    headers: &[(String, String)],
    protocols: &[&str],
    transport: &Transport,
) -> Result<(Socket, u16)> {
    let mut request = tokio_tungstenite::tungstenite::client::IntoClientRequest::into_client_request(url)
        .map_err(|e| Error::Invalid(format!("that is not a WebSocket URL: {e}")))?;
    for (name, value) in headers {
        let (Ok(name), Ok(value)) = (
            reqwest::header::HeaderName::from_bytes(name.as_bytes()),
            reqwest::header::HeaderValue::from_str(value),
        ) else {
            continue;
        };
        request.headers_mut().insert(name, value);
    }
    if !protocols.is_empty() {
        if let Ok(value) = reqwest::header::HeaderValue::from_str(&protocols.join(", ")) {
            request.headers_mut().insert("sec-websocket-protocol", value);
        }
    }

    let uri = request.uri().clone();
    let secure = uri.scheme_str() == Some("wss");
    let host = uri.host().ok_or_else(|| Error::Invalid("that WebSocket URL has no host".into()))?.to_string();
    let port = uri.port_u16().unwrap_or(if secure { 443 } else { 80 });
    let tcp = crate::wire::dial(&host, port, secure, transport).await?;

    // tungstenite would otherwise build its own rustls config, which both
    // panics without a chosen provider and ignores the TLS setting entirely.
    // Built only for `wss`: a plain socket has no use for a certificate, and
    // a file that cannot be read should not stop one from opening.
    let connector = match secure {
        true => tokio_tungstenite::Connector::Rustls(std::sync::Arc::new(crate::wire::tls_config(transport)?)),
        false => tokio_tungstenite::Connector::Plain,
    };
    let (socket, response) =
        tokio_tungstenite::client_async_tls_with_config(request, tcp, None, Some(connector))
            .await
            .map_err(|e| Error::Http(format!("could not connect: {e}")))?;
    Ok((socket, response.status().as_u16()))
}

/// A protocol spoken over the socket, which stands between its frames and
/// the UI. A plain WebSocket has none and shows every frame as it came.
pub(crate) struct Protocol {
    /// What to make of a text frame from the server.
    read: Box<dyn FnMut(&str) -> Frame + Send>,
    /// What to say before hanging up, when it is the user who ends it.
    farewell: Option<String>,
}

pub(crate) enum Frame {
    /// Show this as a message.
    Message(String),
    /// Part of the protocol's own housekeeping; nothing to show.
    Quiet,
    /// Answer the server with this frame, and show nothing.
    Reply(String),
    /// The conversation is over, and this is why.
    Done(String),
    /// It is over because it failed: show the error, then close.
    Failed(String),
}

/// Register an open socket and run it until one side ends it.
pub(crate) fn run_websocket(
    sink: Sink,
    streams: &Streams,
    id: String,
    url: String,
    socket: Socket,
    status: u16,
    mut protocol: Option<Protocol>,
) -> Open {
    let (mut writer, mut reader) = socket.split();
    let (stop, mut stopped) = tokio::sync::oneshot::channel();
    let (outgoing, mut to_send) = tokio::sync::mpsc::unbounded_channel::<String>();
    // A protocol on top owns what goes down the socket, so the UI gets no way
    // to push raw frames into the middle of it.
    let writable = protocol.is_none();
    // The sending half has to outlive the loop either way: a channel with no
    // sender left reads as "closed", which the loop takes as its cue to end.
    let unused = (!writable).then(|| outgoing.clone());
    streams.keep(id.clone(), Handle { stop, outgoing: writable.then_some(outgoing) });

    emit(&sink, &id, "open", format!("{status} {url}"), None);

    let finished = id.clone();
    let registry = streams.clone_registry();
    tokio::spawn(async move {
        use tokio_tungstenite::tungstenite::Message;
        let _unused = unused;
        // Why it ended, said once at the end rather than inside the loop.
        let mut said = String::new();
        loop {
            tokio::select! {
                _ = &mut stopped => {
                    if let Some(farewell) = protocol.as_mut().and_then(|protocol| protocol.farewell.take()) {
                        let _ = writer.send(Message::Text(farewell.into())).await;
                    }
                    let _ = writer.send(Message::Close(None)).await;
                    break;
                }
                outgoing = to_send.recv() => match outgoing {
                    None => break,
                    Some(text) => {
                        if let Err(error) = writer.send(Message::Text(text.clone().into())).await {
                            emit(&sink, &finished, "error", error.to_string(), None);
                            break;
                        }
                        emit(&sink, &finished, "sent", text, None);
                    }
                },
                incoming = reader.next() => match incoming {
                    None => break,
                    Some(Err(error)) => {
                        emit(&sink, &finished, "error", error.to_string(), None);
                        break;
                    }
                    Some(Ok(message)) => match message {
                        Message::Text(text) => match protocol.as_mut().map(|protocol| (protocol.read)(text.as_str())) {
                            None => emit(&sink, &finished, "message", text.to_string(), None),
                            Some(Frame::Message(data)) => emit(&sink, &finished, "message", data, None),
                            Some(Frame::Quiet) => {}
                            Some(Frame::Reply(frame)) => {
                                if let Err(error) = writer.send(Message::Text(frame.into())).await {
                                    emit(&sink, &finished, "error", error.to_string(), None);
                                    break;
                                }
                            }
                            Some(Frame::Done(why)) => {
                                said = why;
                                let _ = writer.send(Message::Close(None)).await;
                                break;
                            }
                            Some(Frame::Failed(error)) => {
                                emit(&sink, &finished, "error", error, None);
                                said = "the subscription ended with an error".into();
                                let _ = writer.send(Message::Close(None)).await;
                                break;
                            }
                        },
                        Message::Binary(bytes) => {
                            emit(&sink, &finished, "message", format!("[{} binary bytes]", bytes.len()), None)
                        }
                        Message::Close(frame) => {
                            said = frame
                                .map(|frame| format!("{} {}", frame.code, frame.reason))
                                .unwrap_or_else(|| "closed by the server".into());
                            break;
                        }
                        // Ping and pong are answered by tungstenite itself.
                        _ => {}
                    },
                }
            }
        }
        // Whatever ended it, the registry stops holding it before the event goes
        // out — announcing a closed connection the map still has is how a caller
        // that acts on `closed` finds it open.
        registry.forget(&finished);
        emit(&sink, &finished, "closed", said, None);
    });

    Open { id, kind: "websocket", url }
}

// ---------------------------------------------------------------------------
// GraphQL subscriptions
// ---------------------------------------------------------------------------

/// The WebSocket subprotocol of the `graphql-ws` library, which is what
/// current GraphQL servers speak. Its older rival, `graphql-ws` the
/// *protocol* (from `subscriptions-transport-ws`), is retired and not spoken
/// here.
const GRAPHQL_PROTOCOL: &str = "graphql-transport-ws";

/// The id of the one operation a connection carries. A connection can carry
/// many; volt opens one per subscription, so one id is all there ever is.
const OPERATION: &str = "1";

/// Open a GraphQL subscription: connect, introduce ourselves, wait to be
/// acknowledged, subscribe — and from then on every `next` is a message.
///
/// `payload` is the operation exactly as a query would POST it: `query`,
/// `variables`, already resolved. The address and the headers come from the
/// plan like anything else; a bearer token is *also* put in the
/// `connection_init` payload, because a browser cannot set headers on a
/// WebSocket and so that payload is where most servers look for one.
///
/// The introduction happens here, before anything is registered, so that a
/// server that refuses is an error from the command rather than a stream
/// that opens and at once closes.
pub async fn open_graphql_subscription(
    sink: Sink,
    streams: &Streams,
    id: String,
    plan: Plan,
    payload: serde_json::Value,
    transport: &Transport,
    timeout_ms: u64,
) -> Result<Open> {
    use tokio_tungstenite::tungstenite::Message;

    let url = websocket_url(&plan.url);
    // The body's Content-Type belongs to the POST this request would have
    // been; an upgrade has no body to describe.
    let headers: Vec<(String, String)> = plan_headers(&plan)
        .into_iter()
        .filter(|(name, _)| !name.eq_ignore_ascii_case("content-type") && !name.eq_ignore_ascii_case("content-length"))
        .collect();
    let bearer = headers
        .iter()
        .find(|(name, value)| name.eq_ignore_ascii_case("authorization") && value.to_ascii_lowercase().starts_with("bearer "))
        .map(|(_, value)| value.clone());

    let (mut socket, status) = connect_websocket(&url, &headers, &[GRAPHQL_PROTOCOL], transport).await?;

    let init = match bearer {
        Some(token) => serde_json::json!({ "type": "connection_init", "payload": { "Authorization": token } }),
        None => serde_json::json!({ "type": "connection_init" }),
    };
    let gone = |e: tokio_tungstenite::tungstenite::Error| Error::Http(format!("the connection dropped: {e}"));
    socket.send(Message::Text(init.to_string().into())).await.map_err(gone)?;

    // Nothing may be asked until the server has said `connection_ack`.
    let acknowledged = async {
        loop {
            match socket.next().await {
                None => return Err(Error::Http("the server closed the connection before acknowledging it".into())),
                Some(Err(error)) => return Err(gone(error)),
                Some(Ok(Message::Close(frame))) => {
                    // 4401 Unauthorized, 4403 Forbidden and their relatives:
                    // the protocol refuses by closing with a reason.
                    let why = frame.map(|frame| format!("{} {}", frame.code, frame.reason)).unwrap_or_default();
                    return Err(Error::Http(format!("the server refused the subscription: {}", why.trim())));
                }
                Some(Ok(Message::Text(text))) => {
                    let frame: serde_json::Value = serde_json::from_str(text.as_str()).unwrap_or_default();
                    match frame.get("type").and_then(serde_json::Value::as_str) {
                        Some("connection_ack") => return Ok(()),
                        Some("ping") => {
                            socket.send(Message::Text(r#"{"type":"pong"}"#.into())).await.map_err(gone)?;
                        }
                        // The retired protocol's way of saying no.
                        Some("connection_error") => {
                            return Err(Error::Http(format!("the server refused the subscription: {}", frame["payload"])));
                        }
                        _ => {}
                    }
                }
                Some(Ok(_)) => {}
            }
        }
    };
    let patience = std::time::Duration::from_millis(if timeout_ms == 0 { 30_000 } else { timeout_ms });
    match tokio::time::timeout(patience, acknowledged).await {
        Ok(result) => result?,
        Err(_) => {
            return Err(Error::Http(format!(
                "the server did not acknowledge the connection in time — it may not speak `{GRAPHQL_PROTOCOL}`"
            )))
        }
    }

    let subscribe = serde_json::json!({ "id": OPERATION, "type": "subscribe", "payload": payload });
    socket.send(Message::Text(subscribe.to_string().into())).await.map_err(gone)?;

    let protocol = Protocol {
        read: Box::new(graphql_frame),
        farewell: Some(serde_json::json!({ "id": OPERATION, "type": "complete" }).to_string()),
    };
    let open = run_websocket(sink, streams, id, url, socket, status, Some(protocol));
    Ok(Open { kind: "graphql", ..open })
}

/// One frame from the server, once the subscription is running.
fn graphql_frame(text: &str) -> Frame {
    let pretty = |value: &serde_json::Value| serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string());
    let Ok(frame) = serde_json::from_str::<serde_json::Value>(text) else {
        // Not the protocol's, but something was said, and hiding it would
        // make a misbehaving server harder to see.
        return Frame::Message(text.to_string());
    };
    match frame.get("type").and_then(serde_json::Value::as_str) {
        // `{ data, errors? }` — the same thing a query's response is.
        Some("next") => Frame::Message(pretty(&frame["payload"])),
        // Fatal to the operation: the request itself was no good.
        Some("error") => Frame::Failed(pretty(&frame["payload"])),
        Some("complete") => Frame::Done("the subscription completed".into()),
        Some("ping") => Frame::Reply(r#"{"type":"pong"}"#.into()),
        Some("pong") | Some("connection_ack") => Frame::Quiet,
        _ => Frame::Message(text.to_string()),
    }
}

fn plan_headers(plan: &Plan) -> Vec<(String, String)> {
    let mut headers = plan.headers.clone();
    if let Some((username, password)) = &plan.basic {
        use base64::Engine as _;
        let encoded = base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
        headers.push(("authorization".into(), format!("Basic {encoded}")));
    }
    headers
}

/// Server-sent events: a GET that never finishes, parsed a line at a time.
pub async fn open_sse(
    sink: Sink,
    streams: &Streams,
    id: String,
    plan: Plan,
    transport: &Transport,
) -> Result<Open> {
    // No timeout: not finishing is the point. Everything else about the
    // connection is what a Send would use, through the same function.
    let builder = reqwest::Client::builder().user_agent(concat!("volt/", env!("CARGO_PKG_VERSION")));
    let client = crate::http::configure(builder, transport)?.build().map_err(|e| Error::Http(e.to_string()))?;

    let mut request = client.get(&plan.url).header(reqwest::header::ACCEPT, "text/event-stream");
    for (name, value) in plan_headers(&plan) {
        request = request.header(name, value);
    }

    let response = request.send().await.map_err(|e| Error::Http(format!("could not connect: {e}")))?;
    let status = response.status();
    if !status.is_success() {
        return Err(Error::Http(format!("the endpoint answered {}", status.as_u16())));
    }

    let (stop, mut stopped) = tokio::sync::oneshot::channel();
    streams.keep(id.clone(), Handle { stop, outgoing: None });
    emit(&sink, &id, "open", format!("{} {}", status.as_u16(), plan.url), None);

    let finished = id.clone();
    let registry = streams.clone_registry();
    tokio::spawn(async move {
        let mut body = response.bytes_stream();
        let mut buffer = String::new();
        // Bytes that are not yet a whole character. A chunk boundary can fall
        // inside one, and decoding each chunk on its own turned every such
        // character into U+FFFD — reliably, on any stream with accented text.
        let mut pending: Vec<u8> = Vec::new();

        loop {
            tokio::select! {
                _ = &mut stopped => break,
                chunk = body.next() => match chunk {
                    None => break,
                    Some(Err(error)) => {
                        emit(&sink, &finished, "error", error.to_string(), None);
                        break;
                    }
                    Some(Ok(bytes)) => {
                        pending.extend_from_slice(&bytes);
                        match std::str::from_utf8(&pending) {
                            Ok(text) => {
                                buffer.push_str(text);
                                pending.clear();
                            }
                            Err(error) => {
                                let good = error.valid_up_to();
                                buffer.push_str(&String::from_utf8_lossy(&pending[..good]));
                                match error.error_len() {
                                    // Genuinely invalid: drop the byte and carry on.
                                    Some(bad) => {
                                        buffer.push(char::REPLACEMENT_CHARACTER);
                                        pending.drain(..good + bad);
                                    }
                                    // Just incomplete: wait for the rest.
                                    None => {
                                        pending.drain(..good);
                                    }
                                }
                            }
                        }
                        // A blank line ends an event; anything after it is the
                        // start of the next one.
                        while let Some(at) = buffer.find("\n\n").or_else(|| buffer.find("\r\n\r\n")) {
                            let width = if buffer[at..].starts_with("\r\n") { 4 } else { 2 };
                            let block: String = buffer.drain(..at + width).collect();
                            if let Some((name, data)) = parse_event(&block) {
                                emit(&sink, &finished, "message", data, name);
                            }
                        }
                    }
                }
            }
        }
        // Whatever ended it, the registry should not keep holding it.
        registry.forget(&finished);
        emit(&sink, &finished, "closed", "", None);
    });

    Ok(Open { id, kind: "sse", url: plan.url })
}

/// One event block: `event:` names it, `data:` lines join with newlines, and
/// a line starting with `:` is a keep-alive comment.
fn parse_event(block: &str) -> Option<(Option<String>, String)> {
    let mut name = None;
    let mut data: Vec<&str> = Vec::new();

    for line in block.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        let (field, value) = line.split_once(':').unwrap_or((line, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match field {
            "event" => name = Some(value.to_string()),
            "data" => data.push(value),
            _ => {}
        }
    }

    (!data.is_empty()).then(|| (name, data.join("\n")))
}


#[cfg(test)]
mod tests {
    use super::*;

    fn plan(url: &str) -> Plan {
        Plan {
            method: "GET".into(),
            url: url.into(),
            headers: vec![("x-volt".into(), "1".into())],
            basic: None,
            digest: None,
            ntlm: None,
            aws: None,
            oauth1: None,
            api_key_header: None,
            body: crate::http::PlanBody::None,
            missing: Vec::new(),
        }
    }

    fn collector() -> (Sink, std::sync::mpsc::Receiver<Event>) {
        let (sender, receiver) = std::sync::mpsc::channel();
        let sink: Sink = std::sync::Arc::new(move |event| {
            let _ = sender.send(event);
        });
        (sink, receiver)
    }

    fn wait_for(events: &std::sync::mpsc::Receiver<Event>, kind: &str) -> Event {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            if let Ok(event) = events.recv_timeout(std::time::Duration::from_millis(200)) {
                if event.kind == kind {
                    return event;
                }
            }
        }
        panic!("no `{kind}` event arrived");
    }

    /// A real WebSocket server on loopback, echoing what it is told.
    // Multi-threaded on purpose: `wait_for` blocks the test thread, and on the
    // default current-thread runtime the socket tasks would never run.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_websocket_carries_messages_both_ways_until_it_is_closed() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(socket).await.unwrap();
            use tokio_tungstenite::tungstenite::Message;
            ws.send(Message::Text("hello".into())).await.unwrap();
            while let Some(Ok(message)) = ws.next().await {
                if let Message::Text(text) = message {
                    ws.send(Message::Text(format!("echo: {text}").into())).await.unwrap();
                }
            }
        });

        let (sink, events) = collector();
        let streams = Streams::default();
        let open = open_websocket(sink, &streams, "ws-1".into(), plan(&format!("http://127.0.0.1:{port}/")), &Transport::unproxied())
            .await
            .expect("it connects");

        assert_eq!(open.kind, "websocket");
        assert_eq!(open.url, format!("ws://127.0.0.1:{port}/"), "http became ws");
        assert!(wait_for(&events, "open").data.contains("101"), "the handshake status is reported");
        assert_eq!(wait_for(&events, "message").data, "hello");

        streams.send("ws-1", "ping".into()).expect("a websocket goes both ways");
        assert_eq!(wait_for(&events, "sent").data, "ping", "what was sent is echoed back to the UI too");
        assert_eq!(wait_for(&events, "message").data, "echo: ping");

        assert_eq!(streams.open_ids(), ["ws-1"]);
        streams.close("ws-1");
        wait_for(&events, "closed");
    }

    /// And a real SSE endpoint, spelled out by hand so the framing is the
    /// specification's rather than a library's idea of it.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn server_sent_events_arrive_one_block_at_a_time() {
        use tokio::io::AsyncWriteExt as _;

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut discard = [0u8; 1024];
            let _ = tokio::io::AsyncReadExt::read(&mut socket, &mut discard).await;

            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: keep-alive\r\n\r\n")
                .await
                .unwrap();
            socket.write_all(b": keep-alive\n\n").await.unwrap();
            socket.write_all(b"event: tick\ndata: {\"n\":1}\n\n").await.unwrap();
            // Split across writes, to prove the buffer joins them.
            socket.write_all(b"data: half").await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            socket.write_all(b" and half\n\n").await.unwrap();
            // A multi-byte character split across two writes. Decoding each
            // chunk on its own replaced it with U+FFFD every time.
            let split = "data: geçti\n\n".as_bytes();
            socket.write_all(&split[..9]).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            socket.write_all(&split[9..]).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });

        let (sink, events) = collector();
        let streams = Streams::default();
        let open = open_sse(sink, &streams, "sse-1".into(), plan(&format!("http://127.0.0.1:{port}/events")), &Transport::unproxied())
            .await
            .expect("it connects");
        assert_eq!(open.kind, "sse");

        let first = wait_for(&events, "message");
        assert_eq!(first.name.as_deref(), Some("tick"));
        assert_eq!(first.data, "{\"n\":1}");

        let second = wait_for(&events, "message");
        assert_eq!(second.data, "half and half", "a block split across packets is still one event");

        let third = wait_for(&events, "message");
        assert_eq!(third.data, "geçti", "a character split across two chunks survives whole");

        assert!(streams.send("sse-1", "x".into()).is_err(), "server-sent events only go one way");
        streams.close("sse-1");
        wait_for(&events, "closed");
    }
    #[test]
    fn a_url_becomes_the_websocket_one_without_touching_what_already_is() {
        assert_eq!(websocket_url("https://api.test/socket"), "wss://api.test/socket");
        assert_eq!(websocket_url("http://127.0.0.1:9000/x"), "ws://127.0.0.1:9000/x");
        assert_eq!(websocket_url("wss://api.test/socket"), "wss://api.test/socket");
        assert_eq!(websocket_url("ws://api.test/socket"), "ws://api.test/socket");
    }

    #[test]
    fn an_sse_block_is_read_the_way_the_spec_writes_it() {
        assert_eq!(parse_event("data: hello\n\n"), Some((None, "hello".into())));
        assert_eq!(
            parse_event("event: tick\ndata: 1\ndata: 2\n\n"),
            Some((Some("tick".into()), "1\n2".into())),
            "several data lines join with newlines"
        );
        assert_eq!(parse_event(": keep-alive\n\n"), None, "a comment is not an event");
        assert_eq!(parse_event("id: 7\nretry: 100\n\n"), None, "and neither is a block with no data");
        assert_eq!(parse_event("data:tight\n\n"), Some((None, "tight".into())), "the space is optional");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_connection_the_server_ends_stops_being_open() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(socket).await.unwrap();
            use tokio_tungstenite::tungstenite::Message;
            ws.send(Message::Close(None)).await.unwrap();
        });

        let (sink, events) = collector();
        let streams = Streams::default();
        open_websocket(sink, &streams, "ws-gone".into(), plan(&format!("http://127.0.0.1:{port}/")), &Transport::unproxied())
            .await
            .unwrap();

        let closed = wait_for(&events, "closed");
        assert_eq!(closed.data, "closed by the server", "the reason reaches the one close event");
        // Emptied before the event goes out, so this holds the moment a listener
        // hears it — which is when the UI acts on it.
        assert!(streams.open_ids().is_empty(), "{:?}", streams.open_ids());
        assert!(streams.send("ws-gone", "x".into()).is_err());
        assert!(
            events.recv_timeout(std::time::Duration::from_millis(300)).is_err(),
            "one close, not two"
        );
    }

    #[test]
    fn closing_something_that_is_not_open_is_not_an_error() {
        let streams = Streams::default();
        streams.close("nothing");
        assert!(streams.open_ids().is_empty());
        assert!(streams.send("nothing", "x".into()).is_err(), "but sending into it says so");
    }

    /// A WebSocket echo server, behind TLS when given an acceptor.
    async fn echo_server(acceptor: Option<tokio_rustls::TlsAcceptor>) -> u16 {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                let acceptor = acceptor.clone();
                tokio::spawn(async move {
                    use tokio_tungstenite::tungstenite::Message;
                    let io: crate::wire::Stream = match acceptor {
                        None => Box::new(socket),
                        Some(acceptor) => match acceptor.accept(socket).await {
                            Ok(tls) => Box::new(tls),
                            Err(_) => return,
                        },
                    };
                    let Ok(mut ws) = tokio_tungstenite::accept_async(io).await else { return };
                    while let Some(Ok(message)) = ws.next().await {
                        if let Message::Text(text) = message {
                            let _ = ws.send(Message::Text(format!("echo: {text}").into())).await;
                        }
                    }
                });
            }
        });
        port
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_secure_websocket_trusts_the_named_ca_and_presents_the_client_certificate() {
        let pki = crate::wire::testing::Pki::new("wss");
        let port = echo_server(Some(pki.acceptor(true, &[]))).await;
        let url = format!("https://localhost:{port}/");
        let streams = Streams::default();

        // Unknown authority: refused before anything is registered.
        let (sink, _events) = collector();
        let refused = open_websocket(sink, &streams, "wss-0".into(), plan(&url), &Transport::unproxied()).await;
        assert!(refused.is_err());
        assert!(streams.open_ids().is_empty());

        // Trusted, and with the certificate the server insists on — once as a
        // PEM and once as a PKCS#12 archive.
        let identities = [
            (pki.client.clone(), None),
            (pki.client_p12.clone(), Some(crate::wire::testing::PASSPHRASE.to_string())),
        ];
        for (index, (client_cert, client_cert_password)) in identities.into_iter().enumerate() {
            let transport = Transport { client_cert: Some(client_cert), client_cert_password, ..pki.trusting() };
            let (sink, events) = collector();
            let id = format!("wss-{}", index + 1);
            let open = open_websocket(sink, &streams, id.clone(), plan(&url), &transport).await.expect("it connects");
            assert_eq!(open.url, format!("wss://localhost:{port}/"));
            wait_for(&events, "open");
            streams.send(&id, "ping".into()).unwrap();
            assert_eq!(wait_for(&events, "message").data, "echo: ping");
            streams.close(&id);
            wait_for(&events, "closed");
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_websocket_goes_through_the_proxy_when_one_is_set_and_around_it_when_told_to() {
        let port = echo_server(None).await;
        let (proxy, asked) = crate::wire::testing::http_proxy().await;
        let url = format!("http://127.0.0.1:{port}/");
        let streams = Streams::default();

        let through = Transport { proxy: Some(format!("http://127.0.0.1:{proxy}")), ..Transport::direct() };
        let (sink, events) = collector();
        open_websocket(sink, &streams, "ws-proxied".into(), plan(&url), &through).await.expect("it connects");
        streams.send("ws-proxied", "ping".into()).unwrap();
        assert_eq!(wait_for(&events, "message").data, "echo: ping", "the socket works end to end through the tunnel");
        streams.close("ws-proxied");
        wait_for(&events, "closed");
        let seen = asked.lock().unwrap().clone();
        assert_eq!(seen.len(), 1, "{seen:?}");
        assert!(seen[0].starts_with(&format!("CONNECT 127.0.0.1:{port} ")), "{seen:?}");

        // An empty proxy is "go direct": the proxy hears nothing more.
        let (sink, events) = collector();
        open_websocket(sink, &streams, "ws-direct".into(), plan(&url), &Transport::unproxied()).await.unwrap();
        streams.send("ws-direct", "ping".into()).unwrap();
        assert_eq!(wait_for(&events, "message").data, "echo: ping");
        streams.close("ws-direct");
        wait_for(&events, "closed");
        assert_eq!(asked.lock().unwrap().len(), 1, "nothing went near the proxy");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn server_sent_events_use_the_same_certificates_a_send_does() {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

        let pki = crate::wire::testing::Pki::new("sse-tls");
        let acceptor = pki.acceptor(false, &[]);
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                let acceptor = acceptor.clone();
                tokio::spawn(async move {
                    let Ok(mut tls) = acceptor.accept(socket).await else { return };
                    let mut discard = [0u8; 1024];
                    if !matches!(tls.read(&mut discard).await, Ok(read) if read > 0) {
                        return;
                    }
                    let _ = tls.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\ndata: over tls\n\n").await;
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                });
            }
        });
        let url = format!("https://localhost:{port}/events");
        let streams = Streams::default();

        let (sink, _events) = collector();
        assert!(open_sse(sink, &streams, "sse-0".into(), plan(&url), &Transport::unproxied()).await.is_err());

        let (sink, events) = collector();
        open_sse(sink, &streams, "sse-tls".into(), plan(&url), &pki.trusting()).await.expect("it connects");
        assert_eq!(wait_for(&events, "message").data, "over tls");
        streams.close("sse-tls");
        wait_for(&events, "closed");
    }

    /// What a GraphQL server on loopback does with one subscription.
    #[derive(Clone, Copy, PartialEq)]
    enum Server {
        /// Acknowledge, ping, send two results, complete.
        Streams,
        /// Acknowledge, then refuse the operation.
        Refuses,
        /// Acknowledge, send one result, then wait to be told to stop.
        Waits,
        /// Say nothing at all.
        Silent,
        /// Close with the protocol's "unauthorized".
        Forbids,
    }

    /// A WebSocket server speaking `graphql-transport-ws`, by hand, so that
    /// what is checked is the protocol as it is written down. It hands back
    /// what the handshake carried and every frame the client sent.
    // The handshake callback's error type is tungstenite's, and it is large.
    #[allow(clippy::result_large_err)]
    async fn graphql_server(behaviour: Server) -> (u16, tokio::task::JoinHandle<(Vec<(String, String)>, Vec<serde_json::Value>)>) {
        use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
        use tokio_tungstenite::tungstenite::protocol::{frame::coding::CloseCode, CloseFrame};
        use tokio_tungstenite::tungstenite::Message;

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut seen_headers = Vec::new();
            let mut ws = tokio_tungstenite::accept_hdr_async(socket, |request: &Request, mut response: Response| {
                for (name, value) in request.headers() {
                    seen_headers.push((name.to_string(), value.to_str().unwrap_or_default().to_string()));
                }
                // A server that speaks the protocol says so in its answer.
                response.headers_mut().insert("sec-websocket-protocol", "graphql-transport-ws".parse().unwrap());
                Ok(response)
            })
            .await
            .unwrap();

            let mut frames: Vec<serde_json::Value> = Vec::new();
            let say = |value: serde_json::Value| Message::Text(value.to_string().into());
            while let Some(Ok(message)) = ws.next().await {
                let Message::Text(text) = message else { continue };
                let frame: serde_json::Value = serde_json::from_str(text.as_str()).unwrap();
                let kind = frame["type"].as_str().unwrap_or_default().to_string();
                frames.push(frame);
                match (kind.as_str(), behaviour) {
                    ("connection_init", Server::Silent) => {}
                    ("connection_init", Server::Forbids) => {
                        let _ = ws.send(Message::Close(Some(CloseFrame { code: CloseCode::from(4401), reason: "Unauthorized".into() }))).await;
                    }
                    ("connection_init", _) => ws.send(say(serde_json::json!({ "type": "connection_ack" }))).await.unwrap(),
                    ("subscribe", Server::Streams) => {
                        ws.send(say(serde_json::json!({ "type": "ping" }))).await.unwrap();
                        for n in [1, 2] {
                            let next = serde_json::json!({ "id": "1", "type": "next", "payload": { "data": { "tick": n } } });
                            ws.send(say(next)).await.unwrap();
                        }
                    }
                    // The client's answer to the ping: only now is it done.
                    ("pong", Server::Streams) => ws.send(say(serde_json::json!({ "id": "1", "type": "complete" }))).await.unwrap(),
                    ("subscribe", Server::Refuses) => {
                        let error = serde_json::json!({ "id": "1", "type": "error", "payload": [{ "message": "Cannot query field \"tick\"" }] });
                        ws.send(say(error)).await.unwrap();
                    }
                    ("subscribe", Server::Waits) => {
                        let next = serde_json::json!({ "id": "1", "type": "next", "payload": { "data": { "tick": 1 } } });
                        ws.send(say(next)).await.unwrap();
                    }
                    _ => {}
                }
            }
            (seen_headers, frames)
        });
        (port, handle)
    }

    fn subscription(port: u16) -> (Plan, serde_json::Value) {
        let mut plan = plan(&format!("http://127.0.0.1:{port}/graphql"));
        plan.headers.push(("Authorization".into(), "Bearer tok-123".into()));
        plan.headers.push(("Content-Type".into(), "application/json".into()));
        let payload = serde_json::json!({ "query": "subscription { tick }", "variables": { "every": 1 } });
        (plan, payload)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_graphql_subscription_introduces_itself_subscribes_and_reports_each_result() {
        let (port, server) = graphql_server(Server::Streams).await;
        let (plan, payload) = subscription(port);
        let (sink, events) = collector();
        let streams = Streams::default();

        let open = open_graphql_subscription(sink, &streams, "gql-1".into(), plan, payload, &Transport::unproxied(), 5_000)
            .await
            .expect("it is acknowledged");
        assert_eq!(open.kind, "graphql");
        assert_eq!(open.url, format!("ws://127.0.0.1:{port}/graphql"), "the request's http address, as a socket's");
        assert!(wait_for(&events, "open").data.contains("101"));

        let first: serde_json::Value = serde_json::from_str(&wait_for(&events, "message").data).expect("JSON text");
        assert_eq!(first, serde_json::json!({ "data": { "tick": 1 } }), "the payload of `next`, and none of the envelope");
        let second: serde_json::Value = serde_json::from_str(&wait_for(&events, "message").data).unwrap();
        assert_eq!(second["data"]["tick"], 2);

        let closed = wait_for(&events, "closed");
        assert_eq!(closed.data, "the subscription completed");
        assert!(streams.open_ids().is_empty());

        let (headers, frames) = server.await.unwrap();
        let header = |name: &str| headers.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str());
        assert_eq!(header("sec-websocket-protocol"), Some("graphql-transport-ws"));
        assert_eq!(header("authorization"), Some("Bearer tok-123"), "the auth header goes on the upgrade");
        assert_eq!(header("x-volt"), Some("1"), "with the request's other headers");
        assert_eq!(header("content-type"), None, "but not the one that described a body");

        assert_eq!(
            frames[0],
            serde_json::json!({ "type": "connection_init", "payload": { "Authorization": "Bearer tok-123" } }),
            "and in the payload too, where servers written for browsers look"
        );
        assert_eq!(
            frames[1],
            serde_json::json!({ "id": "1", "type": "subscribe",
                                "payload": { "query": "subscription { tick }", "variables": { "every": 1 } } })
        );
        assert_eq!(frames[2], serde_json::json!({ "type": "pong" }), "a ping is answered and never shown");
        assert_eq!(frames.len(), 3);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn an_operation_the_server_refuses_is_an_error_and_then_the_end() {
        let (port, _server) = graphql_server(Server::Refuses).await;
        let (mut plan, payload) = subscription(port);
        plan.headers.clear();
        let (sink, events) = collector();
        let streams = Streams::default();
        open_graphql_subscription(sink, &streams, "gql-2".into(), plan, payload, &Transport::unproxied(), 5_000)
            .await
            .expect("the connection itself was fine");

        let error = wait_for(&events, "error");
        assert!(error.data.contains("Cannot query field"), "{}", error.data);
        assert_eq!(wait_for(&events, "closed").data, "the subscription ended with an error");
        assert!(events.recv_timeout(std::time::Duration::from_millis(300)).is_err(), "one close, not two");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn stopping_a_subscription_tells_the_server_and_nothing_can_be_typed_into_it() {
        let (port, server) = graphql_server(Server::Waits).await;
        let (mut plan, payload) = subscription(port);
        plan.headers.clear();
        let (sink, events) = collector();
        let streams = Streams::default();
        open_graphql_subscription(sink, &streams, "gql-3".into(), plan, payload, &Transport::unproxied(), 5_000)
            .await
            .unwrap();
        wait_for(&events, "message");

        let refused = streams.send("gql-3", "{}".into()).unwrap_err().to_string();
        assert!(refused.contains("one way"), "the protocol owns the socket: {refused}");

        streams.close("gql-3");
        wait_for(&events, "closed");
        let (_, frames) = server.await.unwrap();
        assert_eq!(frames[0], serde_json::json!({ "type": "connection_init" }), "no token, no payload");
        assert_eq!(frames.last().unwrap(), &serde_json::json!({ "id": "1", "type": "complete" }), "it says it is done before it hangs up");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_server_that_never_acknowledges_or_says_no_is_an_error_not_an_open_stream() {
        let streams = Streams::default();

        let (port, _server) = graphql_server(Server::Silent).await;
        let (plan, payload) = subscription(port);
        let (sink, events) = collector();
        let said = open_graphql_subscription(sink, &streams, "gql-4".into(), plan, payload, &Transport::unproxied(), 300)
            .await
            .unwrap_err()
            .to_string();
        assert!(said.contains("did not acknowledge") && said.contains("graphql-transport-ws"), "{said}");
        assert!(streams.open_ids().is_empty(), "nothing was registered");
        assert!(events.recv_timeout(std::time::Duration::from_millis(200)).is_err(), "and nothing was announced");

        let (port, _server) = graphql_server(Server::Forbids).await;
        let (plan, payload) = subscription(port);
        let (sink, _events) = collector();
        let said = open_graphql_subscription(sink, &streams, "gql-5".into(), plan, payload, &Transport::unproxied(), 5_000)
            .await
            .unwrap_err()
            .to_string();
        assert!(said.contains("refused the subscription") && said.contains("4401"), "{said}");
    }

    #[test]
    fn a_frame_is_shown_swallowed_or_answered_by_what_it_is() {
        assert!(matches!(graphql_frame(r#"{"id":"1","type":"next","payload":{"data":{"a":1}}}"#), Frame::Message(text) if text.contains("\"a\": 1")));
        assert!(matches!(graphql_frame(r#"{"type":"ping"}"#), Frame::Reply(reply) if reply == r#"{"type":"pong"}"#));
        assert!(matches!(graphql_frame(r#"{"type":"pong"}"#), Frame::Quiet));
        assert!(matches!(graphql_frame(r#"{"id":"1","type":"complete"}"#), Frame::Done(_)));
        assert!(matches!(graphql_frame(r#"{"id":"1","type":"error","payload":[{"message":"no"}]}"#), Frame::Failed(text) if text.contains("no")));
        assert!(matches!(graphql_frame("not json"), Frame::Message(text) if text == "not json"), "what is not the protocol's is still shown");
    }
}
