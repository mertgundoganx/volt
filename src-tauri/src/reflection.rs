//! Asking a server what it serves, when there is no `.proto` to start from.
//!
//! Server reflection is itself a gRPC service, so calling it would normally
//! mean compiling *its* `.proto` — which is circular: volt reaches for
//! reflection precisely because it has no descriptor pool yet. Its messages are
//! four fields deep, so they are written and read here by hand, against the
//! protobuf wire format rather than against a generated type.
//!
//! What comes back is a set of `FileDescriptorProto`s, which is exactly what
//! `protox` produces from a file on disk — so from that point on a reflected
//! service and a compiled one are the same thing to the rest of volt.
//!
//! Reflection is a bidirectional streaming method, which is why this could not
//! be written before `wire.rs`: one request, one response, over and over, on a
//! stream that stays open the whole time.

use std::collections::{BTreeMap, BTreeSet};

use prost::Message as _;
use prost_reflect::prost_types::{FileDescriptorProto, FileDescriptorSet};
use prost_reflect::DescriptorPool;

use crate::error::{Error, Result};
use crate::wire;

/// The v1 service, and the name servers that predate it still answer on.
const PATHS: [&str; 2] = [
    "/grpc.reflection.v1.ServerReflection/ServerReflectionInfo",
    "/grpc.reflection.v1alpha.ServerReflection/ServerReflectionInfo",
];

/// Ask an endpoint for everything it serves, and build a pool from the answer.
pub async fn pool(
    url: &str,
    headers: &[(String, String)],
    timeout_ms: u64,
    verify_tls: bool,
) -> Result<DescriptorPool> {
    let deadline =
        std::time::Duration::from_millis(if timeout_ms == 0 { 30_000 } else { timeout_ms });

    let mut last = String::from("it answered nothing");
    for path in PATHS {
        match tokio::time::timeout(deadline, ask(url, path, headers, verify_tls)).await {
            Ok(Ok(files)) => return build(files),
            Ok(Err(error)) => last = error.to_string(),
            Err(_) => return Err(Error::Http("timed out asking what the server serves".into())),
        }
    }
    // Both names tried, both refused: say which question went unanswered, not
    // just what the socket did.
    Err(Error::Invalid(format!("the endpoint did not answer reflection — {last}")))
}

async fn ask(
    url: &str,
    path: &str,
    headers: &[(String, String)],
    verify_tls: bool,
) -> Result<Vec<FileDescriptorProto>> {
    let mut connection = wire::connect(url, verify_tls).await?;
    // Ask before waiting for the headers: a server reads the request message
    // first, so the other order deadlocks.
    let (mut tx, response) = connection.start(path, headers).await?;
    tx.send(&list_services(), false).await?;

    let exchange = wire::receive(response, tx).await?;
    if !exchange.status.is_success() {
        return Err(Error::Http(format!("the endpoint answered HTTP {}", exchange.status.as_u16())));
    }
    let (mut tx, mut rx) = (exchange.tx, exchange.rx);
    let services = match answer(&mut rx).await? {
        Answer::Services(names) => names,
        Answer::Error(code, message) => return Err(refused(code, &message)),
        _ => return Err(Error::Invalid("the server did not list its services".into())),
    };
    if services.is_empty() {
        return Err(Error::Invalid("the server offers reflection but lists no services".into()));
    }

    let mut files: BTreeMap<String, FileDescriptorProto> = BTreeMap::new();
    let mut asked: BTreeSet<String> = BTreeSet::new();
    for name in &services {
        // Reflection's own service is not something the user called.
        if name.starts_with("grpc.reflection.") {
            continue;
        }
        asked.insert(name.clone());
        tx.send(&containing_symbol(name), false).await?;
        collect(&mut rx, &mut files).await?;
    }

    // A server may answer with one file and leave its imports out. Ask for the
    // ones nothing has supplied until the set closes over itself.
    for _ in 0..8 {
        let missing: Vec<String> = files
            .values()
            .flat_map(|file| file.dependency.iter().cloned())
            .filter(|name| !files.contains_key(name) && !asked.contains(name))
            .collect();
        if missing.is_empty() {
            break;
        }
        for name in missing {
            asked.insert(name.clone());
            tx.send(&by_filename(&name), false).await?;
            collect(&mut rx, &mut files).await?;
        }
    }

    tx.finish()?;
    Ok(files.into_values().collect())
}

fn refused(code: i32, message: &str) -> Error {
    match message.is_empty() {
        true => Error::Invalid(format!("reflection was refused ({code})")),
        false => Error::Invalid(format!("reflection was refused: {message}")),
    }
}

async fn collect(
    rx: &mut wire::Receiver,
    files: &mut BTreeMap<String, FileDescriptorProto>,
) -> Result<()> {
    match answer(rx).await? {
        Answer::Files(raw) => {
            for bytes in raw {
                let file = FileDescriptorProto::decode(bytes.as_slice()).map_err(|e| {
                    Error::Invalid(format!("the server sent a descriptor volt could not read: {e}"))
                })?;
                files.insert(file.name().to_string(), file);
            }
            Ok(())
        }
        // A symbol the server will not talk about is not a reason to give up on
        // the rest of what it serves.
        Answer::Error(_, _) => Ok(()),
        _ => Ok(()),
    }
}

fn build(files: Vec<FileDescriptorProto>) -> Result<DescriptorPool> {
    DescriptorPool::from_file_descriptor_set(FileDescriptorSet { file: files }).map_err(|e| {
        Error::Invalid(format!("what the server described did not fit together: {e}"))
    })
}

// ---------------------------------------------------------------------------
// The four messages, by hand
// ---------------------------------------------------------------------------

/// `ServerReflectionRequest { list_services = 7 }`.
fn list_services() -> Vec<u8> {
    string_field(7, "*")
}

/// `ServerReflectionRequest { file_containing_symbol = 4 }`.
fn containing_symbol(name: &str) -> Vec<u8> {
    string_field(4, name)
}

/// `ServerReflectionRequest { file_by_filename = 3 }`.
fn by_filename(name: &str) -> Vec<u8> {
    string_field(3, name)
}

#[derive(Debug, PartialEq)]
enum Answer {
    Services(Vec<String>),
    Files(Vec<Vec<u8>>),
    Error(i32, String),
    /// Something this does not read, which is not a reason to stop.
    Other,
}

async fn answer(rx: &mut wire::Receiver) -> Result<Answer> {
    let payload = rx
        .next()
        .await?
        .ok_or_else(|| Error::Invalid("the server closed reflection without answering".into()))?;
    Ok(read_response(&payload))
}

/// `ServerReflectionResponse`: field 4 is the descriptors, 6 the service list,
/// 7 a refusal. Everything else is skipped.
fn read_response(bytes: &[u8]) -> Answer {
    for (number, value) in fields(bytes) {
        match (number, value) {
            // FileDescriptorResponse { repeated bytes file_descriptor_proto = 1 }
            (4, Value::Bytes(body)) => {
                let files: Vec<Vec<u8>> = fields(body)
                    .filter_map(|(number, value)| match (number, value) {
                        (1, Value::Bytes(file)) => Some(file.to_vec()),
                        _ => None,
                    })
                    .collect();
                return Answer::Files(files);
            }
            // ListServiceResponse { repeated ServiceResponse service = 1 }
            (6, Value::Bytes(body)) => {
                let names: Vec<String> = fields(body)
                    .filter_map(|(number, value)| match (number, value) {
                        (1, Value::Bytes(service)) => first_string(service),
                        _ => None,
                    })
                    .collect();
                return Answer::Services(names);
            }
            // ErrorResponse { int32 error_code = 1; string error_message = 2 }
            (7, Value::Bytes(body)) => {
                let mut code = 0;
                let mut message = String::new();
                for (number, value) in fields(body) {
                    match (number, value) {
                        (1, Value::Varint(n)) => code = n as i32,
                        (2, Value::Bytes(text)) => {
                            message = String::from_utf8_lossy(text).into_owned()
                        }
                        _ => {}
                    }
                }
                return Answer::Error(code, message);
            }
            _ => {}
        }
    }
    Answer::Other
}

fn first_string(bytes: &[u8]) -> Option<String> {
    fields(bytes).find_map(|(number, value)| match (number, value) {
        (1, Value::Bytes(text)) => Some(String::from_utf8_lossy(text).into_owned()),
        _ => None,
    })
}

// ---------------------------------------------------------------------------
// Just enough protobuf
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq)]
enum Value<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
    Fixed,
}

fn string_field(number: u32, text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() + 8);
    varint((number << 3 | 2) as u64, &mut out);
    varint(text.len() as u64, &mut out);
    out.extend_from_slice(text.as_bytes());
    out
}

fn varint(mut value: u64, out: &mut Vec<u8>) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

/// The fields of a message, in the order they were written. Anything malformed
/// ends the walk rather than guessing.
fn fields(bytes: &[u8]) -> impl Iterator<Item = (u32, Value<'_>)> {
    let mut at = 0usize;
    std::iter::from_fn(move || {
        let (tag, next) = read_varint(bytes, at)?;
        at = next;
        let number = (tag >> 3) as u32;
        match tag & 7 {
            0 => {
                let (value, next) = read_varint(bytes, at)?;
                at = next;
                Some((number, Value::Varint(value)))
            }
            2 => {
                let (length, next) = read_varint(bytes, at)?;
                let end = next.checked_add(length as usize)?;
                if end > bytes.len() {
                    return None;
                }
                at = end;
                Some((number, Value::Bytes(&bytes[next..end])))
            }
            1 => {
                at = at.checked_add(8).filter(|end| *end <= bytes.len())?;
                Some((number, Value::Fixed))
            }
            5 => {
                at = at.checked_add(4).filter(|end| *end <= bytes.len())?;
                Some((number, Value::Fixed))
            }
            _ => None,
        }
    })
}

fn read_varint(bytes: &[u8], mut at: usize) -> Option<(u64, usize)> {
    let mut value = 0u64;
    for shift in 0..10 {
        let byte = *bytes.get(at)?;
        at += 1;
        value |= ((byte & 0x7f) as u64) << (shift * 7);
        if byte & 0x80 == 0 {
            return Some((value, at));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_is_one_string_field_with_the_right_number() {
        // Tag 7<<3|2 = 58, length 1, `*`.
        assert_eq!(list_services(), vec![58, 1, b'*']);
        assert_eq!(containing_symbol("shop.v1.Orders")[0], 4 << 3 | 2);
        assert_eq!(by_filename("shop.proto")[0], 3 << 3 | 2);
    }

    /// Build a `ServerReflectionResponse` the way a server would.
    fn nested(number: u32, body: Vec<u8>) -> Vec<u8> {
        let mut out = Vec::new();
        varint((number << 3 | 2) as u64, &mut out);
        varint(body.len() as u64, &mut out);
        out.extend_from_slice(&body);
        out
    }

    #[test]
    fn a_service_list_reads_back_as_names() {
        let mut list = Vec::new();
        for name in ["shop.v1.Orders", "grpc.reflection.v1.ServerReflection"] {
            list.extend(nested(1, string_field(1, name)));
        }
        assert_eq!(
            read_response(&nested(6, list)),
            Answer::Services(vec![
                "shop.v1.Orders".into(),
                "grpc.reflection.v1.ServerReflection".into()
            ])
        );
    }

    #[test]
    fn descriptors_read_back_as_the_bytes_they_were() {
        let mut body = Vec::new();
        for file in [b"one".as_slice(), b"two".as_slice()] {
            varint(1 << 3 | 2, &mut body);
            varint(file.len() as u64, &mut body);
            body.extend_from_slice(file);
        }
        assert_eq!(
            read_response(&nested(4, body)),
            Answer::Files(vec![b"one".to_vec(), b"two".to_vec()])
        );
    }

    #[test]
    fn a_refusal_carries_its_code_and_its_words() {
        let mut body = Vec::new();
        varint(1 << 3, &mut body);
        varint(12, &mut body);
        body.extend_from_slice(&string_field(2, "not implemented"));
        assert_eq!(read_response(&nested(7, body)), Answer::Error(12, "not implemented".into()));
    }

    #[test]
    fn a_message_that_is_cut_short_ends_the_walk_rather_than_guessing() {
        // A length that runs past the end of the buffer.
        assert_eq!(fields(&[10, 200, 1, 2]).count(), 0);
        // A field this does not read is skipped, and the next one is still found.
        let mut bytes = Vec::new();
        varint(2 << 3, &mut bytes);
        varint(7, &mut bytes);
        bytes.extend_from_slice(&string_field(1, "after"));
        assert_eq!(fields(&bytes).count(), 2);
        assert_eq!(first_string(&bytes).as_deref(), Some("after"));
    }

    /// A server that answers reflection, over a real HTTP/2 socket.
    ///
    /// It replies to each request in turn the way a real one does — one
    /// response per request, on a stream that stays open — which is the part
    /// that could not be tested any other way.
    async fn serve(files: Vec<Vec<u8>>, services: Vec<&'static str>, path_wanted: &'static str) -> u16 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            loop {
                let Ok((socket, _)) = listener.accept().await else { return };
                let files = files.clone();
                let services = services.clone();
                tokio::spawn(async move {
                    let mut connection = h2::server::handshake(socket).await.unwrap();
                    while let Some(Ok((request, mut respond))) = connection.accept().await {
                        let wanted = request.uri().path() == path_wanted;
                        let files = files.clone();
                        let services = services.clone();
                        tokio::spawn(async move {
                            let mut body = request.into_body();
                            // A real reflection server reads the question before
                            // it answers, so the test server does too: a client
                            // that waits for headers first would deadlock here.
                            let first = body.data().await;
                            let response = http::Response::builder()
                                .status(200)
                                .header("content-type", "application/grpc")
                                .body(())
                                .unwrap();
                            let mut out = respond.send_response(response, false).unwrap();
                            let mut buffer = Vec::new();

                            if wanted {
                                // The chunk already read, then the rest.
                                let mut pending = first;
                                loop {
                                    let chunk = match pending.take() {
                                        Some(chunk) => chunk,
                                        None => match body.data().await {
                                            Some(chunk) => chunk,
                                            None => break,
                                        },
                                    };
                                    let Ok(chunk) = chunk else { break };
                                    let _ = body.flow_control().release_capacity(chunk.len());
                                    buffer.extend_from_slice(&chunk);
                                    while buffer.len() >= 5 {
                                        let length = u32::from_be_bytes([
                                            buffer[1], buffer[2], buffer[3], buffer[4],
                                        ]) as usize;
                                        if buffer.len() < 5 + length {
                                            break;
                                        }
                                        let message: Vec<u8> = buffer[5..5 + length].to_vec();
                                        buffer.drain(..5 + length);
                                        let reply = answer_to(&message, &files, &services);
                                        push(&mut out, &reply).await;
                                    }
                                }
                            }

                            let mut trailers = http::HeaderMap::new();
                            let status = if wanted { "0" } else { "12" };
                            trailers.insert("grpc-status", status.parse().unwrap());
                            out.send_trailers(trailers).unwrap();
                        });
                    }
                });
            }
        });
        port
    }

    /// What kind of question was asked, and the answer to it.
    fn answer_to(request: &[u8], files: &[Vec<u8>], services: &[&str]) -> Vec<u8> {
        for (number, value) in fields(request) {
            match (number, value) {
                (7, _) => {
                    let mut list = Vec::new();
                    for name in services {
                        let mut one = Vec::new();
                        varint(1 << 3 | 2, &mut one);
                        let inner = string_field(1, name);
                        varint(inner.len() as u64, &mut one);
                        one.extend_from_slice(&inner);
                        list.extend_from_slice(&one);
                    }
                    return wrap(6, list);
                }
                (3, _) | (4, _) => {
                    let mut body = Vec::new();
                    for file in files {
                        varint(1 << 3 | 2, &mut body);
                        varint(file.len() as u64, &mut body);
                        body.extend_from_slice(file);
                    }
                    return wrap(4, body);
                }
                _ => {}
            }
        }
        wrap(7, Vec::new())
    }

    fn wrap(number: u32, body: Vec<u8>) -> Vec<u8> {
        let mut out = Vec::new();
        varint((number << 3 | 2) as u64, &mut out);
        varint(body.len() as u64, &mut out);
        out.extend_from_slice(&body);
        out
    }

    async fn push(out: &mut h2::SendStream<bytes::Bytes>, message: &[u8]) {
        let payload = crate::wire::frame(message);
        out.reserve_capacity(payload.len());
        while out.capacity() < payload.len() {
            if futures_util::future::poll_fn(|cx| out.poll_capacity(cx)).await.is_none() {
                return;
            }
        }
        out.send_data(payload, false).unwrap();
    }

    fn sample() -> Vec<Vec<u8>> {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../examples/grpc-sample.proto");
        let set = protox::compile([path.as_path()], [path.parent().unwrap()]).unwrap();
        set.file.iter().map(|file| file.encode_to_vec()).collect()
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_server_that_offers_reflection_can_be_asked_what_it_serves() {
        let port = serve(
            sample(),
            vec!["shop.v1.Orders", "grpc.reflection.v1.ServerReflection"],
            PATHS[0],
        )
        .await;

        let built = pool(&format!("http://127.0.0.1:{port}"), &[], 5_000, true)
            .await
            .expect("the server describes itself");

        let service = built.get_service_by_name("shop.v1.Orders").expect("shop.v1.Orders");
        assert!(service.methods().any(|method| method.name() == "GetOrder"));
        assert!(
            service.methods().any(|method| method.name() == "WatchOrders" && method.is_server_streaming()),
            "a reflected service is the same thing as a compiled one"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_server_that_only_speaks_the_older_name_is_still_understood() {
        let port = serve(sample(), vec!["shop.v1.Orders"], PATHS[1]).await;
        let built = pool(&format!("http://127.0.0.1:{port}"), &[], 5_000, true).await.unwrap();
        assert!(built.get_service_by_name("shop.v1.Orders").is_some());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_server_with_no_reflection_says_so_rather_than_hanging() {
        let port = serve(Vec::new(), Vec::new(), "/nothing/here").await;
        let error =
            pool(&format!("http://127.0.0.1:{port}"), &[], 5_000, true).await.unwrap_err().to_string();
        assert!(error.contains("reflection"), "{error}");
    }
}
