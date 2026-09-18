//! gRPC, without code generation.
//!
//! The usual way to call gRPC is to compile a `.proto` at build time and get
//! typed stubs. A client cannot do that: the proto belongs to the user and
//! arrives at runtime. So volt does what `grpcurl` does — compile the file in
//! memory, build the message from JSON, and send the bytes.
//!
//! The transport is `wire.rs` — HTTP/2 directly — rather than reqwest or
//! tonic. reqwest hands back a body, and gRPC needs three things that are not
//! in one: the messages as they arrive, a request half that stays open, and the
//! trailers. tonic would supply all three and bring a stack volt has otherwise
//! avoided, for what is, per message, five bytes of framing.
//!
//! So: unary calls read the `grpc-status` wherever it comes, in the headers of
//! a trailers-only failure or after a body; client, server and bidirectional
//! streaming all work, through the same registry and the same events a
//! WebSocket uses; and `reflection.rs` can ask a server what it serves when
//! there is no `.proto` to compile.

use std::path::Path;
use std::time::Duration;

use prost::Message as _;
use prost_reflect::{DescriptorPool, DynamicMessage, MethodDescriptor, SerializeOptions};
use serde::Serialize;

use crate::error::{Error, Result};
use crate::model::KeyValue;
use crate::stream;
use crate::wire;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Method {
    pub name: String,
    /// `package.Service/Method`, which is also the path it is called on.
    pub full_name: String,
    pub input: String,
    pub output: String,
    pub client_streaming: bool,
    pub server_streaming: bool,
    /// A skeleton of the input message as JSON, to start from.
    pub example: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Service {
    pub name: String,
    pub methods: Vec<Method>,
}

/// Compile a `.proto`, resolving imports next to it.
pub fn compile(path: &Path) -> Result<DescriptorPool> {
    let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let files = protox::compile([path], [parent])
        .map_err(|e| Error::Invalid(format!("the proto file did not compile: {e}")))?;
    DescriptorPool::from_file_descriptor_set(files)
        .map_err(|e| Error::Invalid(format!("the proto file did not load: {e}")))
}

pub fn services(pool: &DescriptorPool) -> Vec<Service> {
    pool.services()
        .map(|service| Service {
            name: service.full_name().to_string(),
            methods: service
                .methods()
                .map(|method| Method {
                    name: method.name().to_string(),
                    full_name: format!("{}/{}", service.full_name(), method.name()),
                    input: method.input().full_name().to_string(),
                    output: method.output().full_name().to_string(),
                    client_streaming: method.is_client_streaming(),
                    server_streaming: method.is_server_streaming(),
                    example: skeleton(&method),
                })
                .collect(),
        })
        .collect()
}

/// An empty message of the right shape, so the editor starts with the field
/// names rather than a blank page.
fn skeleton(method: &MethodDescriptor) -> String {
    let message = DynamicMessage::new(method.input());
    let mut buffer = Vec::new();
    let mut serializer = serde_json::Serializer::pretty(&mut buffer);
    let options = SerializeOptions::new().skip_default_fields(false).stringify_64_bit_integers(false);
    match message.serialize_with_options(&mut serializer, &options) {
        Ok(()) => String::from_utf8(buffer).unwrap_or_else(|_| "{}".into()),
        Err(_) => "{}".into(),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reply {
    /// The gRPC status code: 0 is OK.
    pub status: i32,
    pub status_text: String,
    pub message: Option<String>,
    /// The response message as JSON, when there was one.
    pub body: String,
    pub duration_ms: u64,
    pub headers: Vec<KeyValue>,
    /// What came after the body. A gRPC call says whether it actually worked
    /// here, not in the status line, which is why volt speaks HTTP/2 itself.
    pub trailers: Vec<KeyValue>,
}

pub struct Call<'a> {
    pub url: &'a str,
    /// `package.Service/Method`.
    pub method: &'a str,
    /// The request message, as JSON.
    pub message: &'a str,
    pub headers: &'a [(String, String)],
    pub timeout_ms: u64,
    pub verify_tls: bool,
}

/// Look the method up and encode the message — everything that can be got
/// wrong before a socket is opened.
fn prepare(pool: &DescriptorPool, call: &Call<'_>) -> Result<(MethodDescriptor, String, Vec<u8>)> {
    let (service_name, method_name) = call
        .method
        .split_once('/')
        .ok_or_else(|| Error::Invalid("a gRPC method is written `package.Service/Method`".into()))?;

    let service = pool
        .get_service_by_name(service_name)
        .ok_or_else(|| Error::Invalid(format!("`{service_name}` is not in this proto file")))?;
    let method = service
        .methods()
        .find(|method| method.name() == method_name)
        .ok_or_else(|| Error::Invalid(format!("`{service_name}` has no method `{method_name}`")))?;

    let encoded = encode(&method, call.message)?;
    Ok((method, format!("/{service_name}/{method_name}"), encoded))
}

/// JSON in, protobuf out.
fn encode(method: &MethodDescriptor, message: &str) -> Result<Vec<u8>> {
    let text = if message.trim().is_empty() { "{}" } else { message.trim() };
    let mut deserializer = serde_json::Deserializer::from_str(text);
    let request = DynamicMessage::deserialize(method.input(), &mut deserializer).map_err(|e| {
        Error::Invalid(format!("the request message does not fit `{}`: {e}", method.input().full_name()))
    })?;
    Ok(request.encode_to_vec())
}

/// Protobuf in, JSON out.
fn decode(method: &MethodDescriptor, payload: &[u8]) -> Result<String> {
    let reply = DynamicMessage::decode(method.output(), payload).map_err(|e| {
        Error::Invalid(format!("the reply did not decode as `{}`: {e}", method.output().full_name()))
    })?;
    let mut buffer = Vec::new();
    let mut serializer = serde_json::Serializer::pretty(&mut buffer);
    let options = SerializeOptions::new().skip_default_fields(false);
    reply
        .serialize_with_options(&mut serializer, &options)
        .map_err(|e| Error::Invalid(format!("the reply did not render as JSON: {e}")))?;
    Ok(String::from_utf8(buffer).unwrap_or_default())
}

/// The headers every gRPC call carries, then the user's own on top.
fn headers_for(call: &Call<'_>) -> Vec<(String, String)> {
    let mut out = vec![
        ("content-type".to_string(), "application/grpc+proto".to_string()),
        // Without `te: trailers` a server is allowed not to send any.
        ("te".to_string(), "trailers".to_string()),
        ("grpc-accept-encoding".to_string(), "identity".to_string()),
        ("user-agent".to_string(), concat!("volt-grpc/", env!("CARGO_PKG_VERSION")).to_string()),
        ("grpc-timeout".to_string(), format!("{}m", call.timeout_ms.max(1))),
    ];
    out.extend(call.headers.iter().cloned());
    out
}

fn key_values(map: &http::HeaderMap) -> Vec<KeyValue> {
    map.iter()
        .map(|(name, value)| KeyValue {
            name: name.to_string(),
            value: value.to_str().unwrap_or("").to_string(),
            enabled: true,
            description: None,
        })
        .collect()
}

/// The status, from the trailers if they came and from the headers if the
/// server answered with trailers only — which is how gRPC reports most
/// failures, and the one case an ordinary HTTP client gets right.
fn status_of(headers: &http::HeaderMap, trailers: Option<&http::HeaderMap>) -> (i32, Option<String>) {
    let read = |map: &http::HeaderMap| {
        let code = wire::header(map, "grpc-status").and_then(|value| value.parse::<i32>().ok());
        let message = wire::header(map, "grpc-message").map(|text| unescape(&text));
        code.map(|code| (code, message))
    };
    trailers.and_then(read).or_else(|| read(headers)).unwrap_or((0, None))
}

/// `grpc-message` is percent-encoded, and a message full of `%20` helps nobody.
fn unescape(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let hex = (at + 2 < bytes.len()).then(|| &text[at + 1..at + 3]);
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

/// One request, one reply.
pub async fn unary(pool: &DescriptorPool, call: &Call<'_>) -> Result<Reply> {
    let (method, path, encoded) = prepare(pool, call)?;
    if method.is_client_streaming() || method.is_server_streaming() {
        return Err(Error::Invalid(format!(
            "`{}` is a streaming call — open it as a stream rather than sending it",
            method.name()
        )));
    }

    let started = std::time::Instant::now();
    let work = async {
        let mut connection = wire::connect(call.url, call.verify_tls).await?;
        // The message goes out before the headers are waited for: a gRPC server
        // reads the request before it answers.
        let (mut tx, response) = connection.start(&path, &headers_for(call)).await?;
        tx.send(&encoded, true).await?;
        let mut exchange = wire::receive(response, tx).await?;

        let first = exchange.rx.next().await?;
        // Drain, so the trailers are read: they carry the status.
        while exchange.rx.next().await?.is_some() {}
        Ok::<_, Error>((exchange, first))
    };

    let (exchange, first) = match tokio::time::timeout(deadline(call.timeout_ms), work).await {
        Ok(result) => result?,
        Err(_) => return Err(Error::Http("timed out".into())),
    };

    if !exchange.status.is_success() {
        return Err(Error::Http(format!("the endpoint answered HTTP {}", exchange.status.as_u16())));
    }
    let (status, message) = status_of(&exchange.headers, exchange.rx.trailers.as_ref());
    let body = match first {
        Some(payload) => decode(&method, &payload)?,
        None => String::new(),
    };

    Ok(Reply {
        status,
        status_text: status_text(status).into(),
        message,
        body,
        duration_ms: started.elapsed().as_millis() as u64,
        headers: key_values(&exchange.headers),
        trailers: exchange.rx.trailers.as_ref().map(key_values).unwrap_or_default(),
    })
}

fn deadline(timeout_ms: u64) -> Duration {
    Duration::from_millis(if timeout_ms == 0 { 60_000 } else { timeout_ms })
}

/// A call that is not one message each way: client, server or bidirectional
/// streaming.
///
/// It is the same kind of thing to the UI as a WebSocket — an id, a list of
/// events, a way to say more, a way to stop — so it goes through the same
/// registry and the same channel rather than a second one that would have to
/// be kept in step. Sending an empty message means "nothing more from me",
/// which is how a client-streaming call asks for its reply.
pub async fn open_stream(
    sink: stream::Sink,
    streams: &stream::Streams,
    id: String,
    pool: &DescriptorPool,
    call: &Call<'_>,
) -> Result<stream::Open> {
    let (method, path, encoded) = prepare(pool, call)?;
    if !method.is_client_streaming() && !method.is_server_streaming() {
        return Err(Error::Invalid(format!("`{}` is a unary call — send it", method.name())));
    }

    let mut connection = wire::connect(call.url, call.verify_tls).await?;

    // Send first, then wait for the headers: a gRPC server reads the request
    // message before it answers, so the other order deadlocks. The first
    // message is the one in the editor; a client-streaming call keeps its half
    // open for the ones that follow.
    let (mut tx, response) = connection.start(&path, &headers_for(call)).await?;
    let client_streaming = method.is_client_streaming();
    tx.send(&encoded, !client_streaming).await?;

    // A server that never answers must not hang the command with no way out.
    let exchange = match tokio::time::timeout(deadline(call.timeout_ms), wire::receive(response, tx)).await {
        Ok(result) => result?,
        Err(_) => return Err(Error::Http("the server did not answer in time".into())),
    };
    if !exchange.status.is_success() {
        return Err(Error::Http(format!("the endpoint answered HTTP {}", exchange.status.as_u16())));
    }

    let url = format!("{} {}", call.url.trim_end_matches('/'), call.method);
    let (stop, mut stopped) = tokio::sync::oneshot::channel::<()>();
    let (outgoing, mut incoming) = tokio::sync::mpsc::unbounded_channel::<String>();
    streams.register(id.clone(), stop, Some(outgoing));

    let registry = streams.clone_registry();
    let wire::Exchange { headers, mut tx, mut rx, .. } = exchange;
    let method = method.clone();
    let id_for_task = id.clone();

    stream::emit(&sink, &id, "sent", call.message.trim().to_string(), None);

    tokio::spawn(async move {
        let mut half_closed = !client_streaming;
        // Every exit path ends with exactly one `closed`, so the pane cannot be
        // left saying "connected" after the stream has gone.
        let mut closing = String::from("the connection ended");
        loop {
            tokio::select! {
                _ = &mut stopped => break,
                Some(text) = incoming.recv(), if !half_closed => {
                    if text.trim().is_empty() {
                        half_closed = true;
                        if let Err(error) = tx.finish() {
                            stream::emit(&sink, &id_for_task, "error", error.to_string(), None);
                            break;
                        }
                        stream::emit(&sink, &id_for_task, "sent", "(nothing more)", None);
                        continue;
                    }
                    let sent = match encode(&method, &text) {
                        Ok(bytes) => tx.send(&bytes, false).await,
                        Err(error) => Err(error),
                    };
                    match sent {
                        Ok(()) => stream::emit(&sink, &id_for_task, "sent", text, None),
                        // A message that does not fit is the user's to fix; it
                        // is not a reason to drop the connection.
                        Err(error) => stream::emit(&sink, &id_for_task, "error", error.to_string(), None),
                    }
                }
                next = rx.next() => match next {
                    Ok(Some(payload)) => match decode(&method, &payload) {
                        Ok(json) => stream::emit(&sink, &id_for_task, "message", json, None),
                        Err(error) => stream::emit(&sink, &id_for_task, "error", error.to_string(), None),
                    },
                    Ok(None) => {
                        let (status, message) = status_of(&headers, rx.trailers.as_ref());
                        let said = match message {
                            Some(text) if !text.is_empty() => format!("{} — {text}", status_text(status)),
                            _ => status_text(status).to_string(),
                        };
                        closing = said;
                        break;
                    }
                    Err(error) => {
                        stream::emit(&sink, &id_for_task, "error", error.to_string(), None);
                        break;
                    }
                },
            }
        }
        // The registry stops holding it before the event goes out, or a caller
        // that acts on `closed` can still find it open.
        registry.forget(&id_for_task);
        stream::emit(&sink, &id_for_task, "closed", closing, None);
    });

    Ok(stream::Open { id, kind: "grpc", url })
}

pub fn status_text(code: i32) -> &'static str {
    match code {
        0 => "OK",
        1 => "CANCELLED",
        2 => "UNKNOWN",
        3 => "INVALID_ARGUMENT",
        4 => "DEADLINE_EXCEEDED",
        5 => "NOT_FOUND",
        6 => "ALREADY_EXISTS",
        7 => "PERMISSION_DENIED",
        8 => "RESOURCE_EXHAUSTED",
        9 => "FAILED_PRECONDITION",
        10 => "ABORTED",
        11 => "OUT_OF_RANGE",
        12 => "UNIMPLEMENTED",
        13 => "INTERNAL",
        14 => "UNAVAILABLE",
        15 => "DATA_LOSS",
        16 => "UNAUTHENTICATED",
        _ => "UNKNOWN",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool() -> DescriptorPool {
        compile(&std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/grpc-sample.proto"))
            .expect("the sample proto compiles")
    }

    #[test]
    fn a_proto_file_becomes_services_and_methods() {
        let found = services(&pool());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "shop.v1.Orders");

        let get = found[0].methods.iter().find(|method| method.name == "GetOrder").expect("GetOrder");
        assert_eq!(get.full_name, "shop.v1.Orders/GetOrder", "which is also the path it is called on");
        assert_eq!(get.input, "shop.v1.GetOrderRequest");
        assert_eq!(get.output, "shop.v1.Order");
        assert!(!get.server_streaming);

        let watch = found[0].methods.iter().find(|method| method.name == "WatchOrders").unwrap();
        assert!(watch.server_streaming, "streaming is recognised, so it can be refused rather than hang");
    }

    #[test]
    fn the_editor_starts_from_the_shape_of_the_message() {
        let found = services(&pool());
        let get = found[0].methods.iter().find(|method| method.name == "GetOrder").unwrap();
        let example: serde_json::Value = serde_json::from_str(&get.example).expect("valid JSON");
        assert_eq!(example["id"], "", "the field names are there, empty");
    }

    #[tokio::test]
    async fn a_streaming_call_is_refused_rather_than_half_attempted() {
        let pool = pool();
        let error = unary(
            &pool,
            &Call {
                url: "http://127.0.0.1:1",
                method: "shop.v1.Orders/WatchOrders",
                message: "{}",
                headers: &[],
                timeout_ms: 1_000,
                verify_tls: true,
            },
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(error.contains("streaming"), "{error}");
    }

    #[tokio::test]
    async fn a_message_that_does_not_fit_the_proto_says_which_field() {
        let pool = pool();
        let error = unary(
            &pool,
            &Call {
                url: "http://127.0.0.1:1",
                method: "shop.v1.Orders/GetOrder",
                message: r#"{"nope": 1}"#,
                headers: &[],
                timeout_ms: 1_000,
                verify_tls: true,
            },
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(error.contains("GetOrderRequest"), "{error}");
    }

    #[tokio::test]
    async fn an_unknown_service_or_method_is_caught_before_anything_is_sent() {
        let pool = pool();
        let call = |method: &'static str| {
            let pool = pool.clone();
            async move {
                unary(
                    &pool,
                    &Call {
                        url: "http://127.0.0.1:1",
                        method,
                        message: "{}",
                        headers: &[],
                        timeout_ms: 1_000,
                        verify_tls: true,
                    },
                )
                .await
                .unwrap_err()
                .to_string()
            }
        };

        assert!(call("shop.v1.Nope/GetOrder").await.contains("not in this proto"));
        assert!(call("shop.v1.Orders/Nope").await.contains("no method"));
        assert!(call("GetOrder").await.contains("package.Service/Method"));
    }

    #[test]
    fn status_codes_read_as_words() {
        assert_eq!(status_text(0), "OK");
        assert_eq!(status_text(5), "NOT_FOUND");
        assert_eq!(status_text(16), "UNAUTHENTICATED");
        assert_eq!(status_text(99), "UNKNOWN");
    }
    // -----------------------------------------------------------------------
    // The whole thing, over a real HTTP/2 socket
    // -----------------------------------------------------------------------

    /// What the test server answers a call with.
    #[derive(Clone, Default)]
    struct Script {
        /// Messages to frame and send back, already encoded.
        messages: Vec<Vec<u8>>,
        status: i32,
        message: &'static str,
        /// Answer each request message as it arrives, the way a bidirectional
        /// call does, rather than after the request is over.
        echo: bool,
    }

    fn order(pool: &DescriptorPool, id: &str, total: i32) -> Vec<u8> {
        let mut message = DynamicMessage::new(pool.get_message_by_name("shop.v1.Order").unwrap());
        message.set_field_by_name("id", prost_reflect::Value::String(id.into()));
        message.set_field_by_name("total", prost_reflect::Value::I32(total));
        message.encode_to_vec()
    }

    /// A gRPC server on loopback: real frames, real trailers, no mocking of the
    /// thing under test.
    async fn serve(script: Script) -> u16 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut connection = h2::server::handshake(socket).await.unwrap();
            while let Some(Ok((request, mut respond))) = connection.accept().await {
                let script = script.clone();
                tokio::spawn(async move {
                    let mut body = request.into_body();

                    // A real gRPC server reads the request message BEFORE it
                    // answers. Waiting here is what makes a client that awaits
                    // the response headers before sending deadlock — which is
                    // exactly the bug this shape is here to catch.
                    let first = body.data().await;

                    let response = http::Response::builder()
                        .status(200)
                        .header("content-type", "application/grpc")
                        .body(())
                        .unwrap();
                    let mut out = respond.send_response(response, false).unwrap();

                    let mut seen = 0usize;
                    let take = |chunk: &bytes::Bytes| chunk.len() > 5;
                    if let Some(Ok(chunk)) = first {
                        let _ = body.flow_control().release_capacity(chunk.len());
                        if script.echo && take(&chunk) {
                            if let Some(reply) = script.messages.first() {
                                push(&mut out, reply).await;
                            }
                            seen += 1;
                        }
                    }
                    while let Some(Ok(chunk)) = body.data().await {
                        let _ = body.flow_control().release_capacity(chunk.len());
                        if script.echo && take(&chunk) {
                            let reply = script.messages.get(seen).or(script.messages.first());
                            if let Some(reply) = reply {
                                push(&mut out, reply).await;
                            }
                            seen += 1;
                        }
                    }
                    if !script.echo {
                        for message in &script.messages {
                            push(&mut out, message).await;
                        }
                    }

                    let mut trailers = http::HeaderMap::new();
                    trailers.insert("grpc-status", script.status.to_string().parse().unwrap());
                    if !script.message.is_empty() {
                        trailers.insert("grpc-message", script.message.parse().unwrap());
                    }
                    out.send_trailers(trailers).unwrap();
                });
            }
        });
        port
    }

    async fn push(out: &mut h2::SendStream<bytes::Bytes>, message: &[u8]) {
        let payload = wire::frame(message);
        out.reserve_capacity(payload.len());
        while out.capacity() < payload.len() {
            if futures_util::future::poll_fn(|cx| out.poll_capacity(cx)).await.is_none() {
                return;
            }
        }
        out.send_data(payload, false).unwrap();
    }

    fn call_to<'a>(url: &'a str, method: &'a str, message: &'a str) -> Call<'a> {
        Call { url, method, message, headers: &[], timeout_ms: 5_000, verify_tls: true }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_status_that_follows_the_body_is_read() {
        let pool = pool();
        let port = serve(Script {
            messages: vec![order(&pool, "A-1", 42)],
            status: 5,
            message: "no such order",
            ..Default::default()
        })
        .await;

        let url = format!("http://127.0.0.1:{port}");
        let reply = unary(&pool, &call_to(&url, "shop.v1.Orders/GetOrder", r#"{"id":"A-1"}"#))
            .await
            .expect("the call goes through");

        // The point of speaking HTTP/2 directly: this status arrived *after*
        // the body, where an ordinary HTTP client never sees it.
        assert_eq!(reply.status, 5);
        assert_eq!(reply.status_text, "NOT_FOUND");
        assert_eq!(reply.message.as_deref(), Some("no such order"));
        assert!(reply.trailers.iter().any(|t| t.name == "grpc-status"));

        let body: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
        assert_eq!(body["id"], "A-1");
        assert_eq!(body["total"], 42);
    }

    /// A sink that keeps every event, so a test can watch a real stream.
    fn recorder() -> (stream::Sink, std::sync::Arc<std::sync::Mutex<Vec<stream::Event>>>) {
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let kept = seen.clone();
        let sink: stream::Sink =
            std::sync::Arc::new(move |event| kept.lock().unwrap().push(event));
        (sink, seen)
    }

    async fn wait_for(
        seen: &std::sync::Arc<std::sync::Mutex<Vec<stream::Event>>>,
        kind: &str,
    ) -> Vec<stream::Event> {
        for _ in 0..200 {
            let events = seen.lock().unwrap().clone();
            if events.iter().any(|event| event.kind == kind) {
                return events;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("no {kind} event: {:?}", seen.lock().unwrap());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_server_streaming_call_reports_every_message_and_then_the_status() {
        let pool = pool();
        let port = serve(Script {
            messages: vec![order(&pool, "A-1", 1), order(&pool, "A-2", 2), order(&pool, "A-3", 3)],
            ..Default::default()
        })
        .await;

        let (sink, seen) = recorder();
        let streams = stream::Streams::default();
        let url = format!("http://127.0.0.1:{port}");
        let open = open_stream(
            sink,
            &streams,
            "watch".into(),
            &pool,
            &call_to(&url, "shop.v1.Orders/WatchOrders", "{}"),
        )
        .await
        .expect("the stream opens");
        assert_eq!(open.kind, "grpc");

        let events = wait_for(&seen, "closed").await;
        let messages: Vec<&stream::Event> =
            events.iter().filter(|event| event.kind == "message").collect();
        assert_eq!(messages.len(), 3, "{events:?}");
        assert!(messages[2].data.contains("A-3"));
        assert_eq!(events.last().unwrap().data, "OK");
        assert!(streams.open_ids().is_empty(), "a stream that ended is not still open");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_client_streaming_call_keeps_sending_until_it_says_it_is_done() {
        let pool = pool();
        let port =
            serve(Script { messages: vec![order(&pool, "A-9", 3)], ..Default::default() }).await;

        let (sink, seen) = recorder();
        let streams = stream::Streams::default();
        let url = format!("http://127.0.0.1:{port}");
        open_stream(
            sink,
            &streams,
            "place".into(),
            &pool,
            &call_to(&url, "shop.v1.Orders/PlaceOrders", r#"{"id":"A-1"}"#),
        )
        .await
        .expect("the stream opens");

        streams.send("place", r#"{"id":"A-2"}"#.into()).expect("more can be said");
        // An empty message means "nothing more from me", which is how a
        // client-streaming call asks for its reply.
        streams.send("place", String::new()).expect("and the half close goes through");

        let events = wait_for(&seen, "closed").await;
        let sent: Vec<&stream::Event> = events.iter().filter(|event| event.kind == "sent").collect();
        assert_eq!(sent.len(), 3, "the first message, the second, and the half close: {events:?}");
        let messages: Vec<&stream::Event> =
            events.iter().filter(|event| event.kind == "message").collect();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].data.contains("A-9"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_bidirectional_call_answers_each_message_as_it_arrives() {
        let pool = pool();
        let port = serve(Script {
            messages: vec![order(&pool, "R-1", 1), order(&pool, "R-2", 2)],
            echo: true,
            ..Default::default()
        })
        .await;

        let (sink, seen) = recorder();
        let streams = stream::Streams::default();
        let url = format!("http://127.0.0.1:{port}");
        open_stream(
            sink,
            &streams,
            "chat".into(),
            &pool,
            &call_to(&url, "shop.v1.Orders/Chat", r#"{"id":"one"}"#),
        )
        .await
        .expect("the stream opens");

        streams.send("chat", r#"{"id":"two"}"#.into()).unwrap();
        streams.send("chat", String::new()).unwrap();

        let events = wait_for(&seen, "closed").await;
        let messages: Vec<&stream::Event> =
            events.iter().filter(|event| event.kind == "message").collect();
        assert_eq!(messages.len(), 2, "one reply per message, while the stream was open: {events:?}");
        assert!(messages[0].data.contains("R-1"));
        assert!(messages[1].data.contains("R-2"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_unary_call_will_not_be_opened_as_a_stream_or_the_other_way_round() {
        let pool = pool();
        let (sink, _) = recorder();
        let streams = stream::Streams::default();
        let error = open_stream(
            sink,
            &streams,
            "no".into(),
            &pool,
            &call_to("http://127.0.0.1:1", "shop.v1.Orders/GetOrder", "{}"),
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(error.contains("unary call"), "{error}");
    }
}
