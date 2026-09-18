//! Connections that stay open: WebSocket and server-sent events.
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
    /// WebSocket only: what to say back.
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
            .ok_or_else(|| Error::Invalid("server-sent events only go one way".into()))?;
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
    verify_tls: bool,
) -> Result<Open> {
    let url = websocket_url(&plan.url);

    let mut request = tokio_tungstenite::tungstenite::client::IntoClientRequest::into_client_request(url.as_str())
        .map_err(|e| Error::Invalid(format!("that is not a WebSocket URL: {e}")))?;
    for (name, value) in plan_headers(&plan) {
        let (Ok(name), Ok(value)) = (
            reqwest::header::HeaderName::from_bytes(name.as_bytes()),
            reqwest::header::HeaderValue::from_str(&value),
        ) else {
            continue;
        };
        request.headers_mut().insert(name, value);
    }

    // tungstenite would otherwise build its own rustls config, which both
    // panics without a chosen provider and ignores the TLS setting entirely.
    let connector = tokio_tungstenite::Connector::Rustls(std::sync::Arc::new(
        crate::wire::tls_config(verify_tls),
    ));
    let (socket, response) =
        tokio_tungstenite::connect_async_tls_with_config(request, None, false, Some(connector))
            .await
            .map_err(|e| Error::Http(format!("could not connect: {e}")))?;

    let (mut writer, mut reader) = socket.split();
    let (stop, mut stopped) = tokio::sync::oneshot::channel();
    let (outgoing, mut to_send) = tokio::sync::mpsc::unbounded_channel::<String>();
    streams.keep(id.clone(), Handle { stop, outgoing: Some(outgoing) });

    emit(&sink, &id, "open", format!("{} {}", response.status().as_u16(), url), None);

    let finished = id.clone();
    let registry = streams.clone_registry();
    tokio::spawn(async move {
        use tokio_tungstenite::tungstenite::Message;
        // Why it ended, said once at the end rather than inside the loop.
        let mut said = String::new();
        loop {
            tokio::select! {
                _ = &mut stopped => {
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
                        Message::Text(text) => emit(&sink, &finished, "message", text.to_string(), None),
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

    Ok(Open { id, kind: "websocket", url })
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
pub async fn open_sse(sink: Sink, streams: &Streams, id: String, plan: Plan, verify_tls: bool) -> Result<Open> {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(!verify_tls)
        .user_agent(concat!("volt/", env!("CARGO_PKG_VERSION")))
        // No timeout: not finishing is the point.
        .build()
        .map_err(|e| Error::Http(e.to_string()))?;

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
        let open = open_websocket(sink, &streams, "ws-1".into(), plan(&format!("http://127.0.0.1:{port}/")), true)
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
        let open = open_sse(sink, &streams, "sse-1".into(), plan(&format!("http://127.0.0.1:{port}/events")), true)
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
        open_websocket(sink, &streams, "ws-gone".into(), plan(&format!("http://127.0.0.1:{port}/")), true)
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
}
