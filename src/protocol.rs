//! T3 v0.0.45 uses Effect RPC JSON envelopes, not JSON-RPC 2.0.
use eframe::egui;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    net::{TcpStream, ToSocketAddrs},
    sync::mpsc,
    time::{Duration, Instant},
};
use tungstenite::{Message, WebSocket, client::IntoClientRequest, stream::MaybeTlsStream};
use zeroize::Zeroizing;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Route {
    Config,
    Shell,
    Thread(String),
    Command(String),
    /// A one-shot request, identified by a caller-chosen key.
    Rpc(String),
    Vcs(String),
}

pub enum Action {
    Select(Option<String>),
    Dispatch(Value),
    /// One-shot RPC; the reply arrives as `Route::Rpc(key)`.
    Request {
        key: String,
        method: &'static str,
        payload: Value,
    },
    /// Follows the VCS status of a working directory; `None` stops watching.
    WatchVcs(Option<String>),
}

pub enum Event {
    Data(Route, Value),
    Failed(Route, String),
    Disconnected(String),
}

pub struct Connection {
    pub actions: mpsc::Sender<Action>,
    pub events: mpsc::Receiver<Event>,
}

type Socket = WebSocket<MaybeTlsStream<TcpStream>>;

impl Connection {
    pub fn open(
        server: &str,
        token: Zeroizing<String>,
        ctx: egui::Context,
    ) -> Result<Self, String> {
        let url = socket_url(server)?;
        let (actions, inbox) = mpsc::channel();
        let (outbox, events) = mpsc::sync_channel(128);
        std::thread::Builder::new()
            .name("t3-connection".into())
            .spawn(move || {
                let emit = |event| {
                    let sent = outbox.send(event).is_ok();
                    ctx.request_repaint();
                    sent
                };
                if let Err(error) = run(url, token, inbox, &emit) {
                    emit(Event::Disconnected(error));
                }
            })
            .map_err(|_| "Could not start the connection worker")?;
        Ok(Self { actions, events })
    }
}

pub fn socket_url(server: &str) -> Result<url::Url, String> {
    let mut url = url::Url::parse(server.trim()).map_err(|_| "Enter a valid server URL")?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(
            "Enter the server address only; use the authentication field for credentials".into(),
        );
    }
    let scheme = match url.scheme() {
        "http" | "ws" => "ws",
        "https" | "wss" => "wss",
        _ => return Err("The server URL must use HTTP(S) or WS(S)".into()),
    };
    if scheme == "ws" && !matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")) {
        return Err("Use HTTPS for a server outside this computer".into());
    }
    url.set_scheme(scheme).map_err(|_| "Invalid URL scheme")?;
    if !matches!(url.path(), "" | "/" | "/ws") {
        return Err("Use the server origin without a path".into());
    }
    url.set_path("/ws");
    url.set_query(Some("orchestrationProtocol=1"));
    Ok(url)
}

pub fn request(id: u64, method: &str, payload: Value) -> Value {
    json!({"_tag":"Request","id":id.to_string(),"tag":method,"payload":payload,"headers":[]})
}

pub fn frames(text: &str) -> Result<Vec<Value>, String> {
    let value: Value = serde_json::from_str(text).map_err(|_| "The server sent invalid JSON")?;
    Ok(match value {
        Value::Array(values) => values,
        value => vec![value],
    })
}

fn send(socket: &mut Socket, value: Value) -> Result<(), String> {
    socket
        .send(Message::Text(value.to_string().into()))
        .map_err(|_| "Connection lost while writing to the server".into())
}

fn connect(url: &url::Url, token: &str) -> Result<Socket, String> {
    let host = url
        .host_str()
        .ok_or("Missing server host")?
        .trim_matches(['[', ']']);
    let port = url.port_or_known_default().ok_or("Missing server port")?;
    let addresses = (host, port)
        .to_socket_addrs()
        .map_err(|_| "Could not resolve the server address")?;
    let mut tcp = None;
    for address in addresses {
        if let Ok(stream) = TcpStream::connect_timeout(&address, Duration::from_secs(5)) {
            tcp = Some(stream);
            break;
        }
    }
    let tcp = tcp.ok_or("Could not reach the server")?;
    tcp.set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|_| "Could not configure the connection")?;
    tcp.set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|_| "Could not configure the connection")?;
    let mut request = url
        .as_str()
        .into_client_request()
        .map_err(|_| "Invalid server address")?;
    if !token.trim().is_empty() {
        let mut value = format!("Bearer {}", token.trim())
            .parse::<tungstenite::http::HeaderValue>()
            .map_err(|_| "Invalid authentication credential")?;
        value.set_sensitive(true);
        request.headers_mut().insert("authorization", value);
    }
    let (socket, _) = tungstenite::client_tls_with_config(request, tcp, None, None).map_err(|error| match error {
        tungstenite::handshake::HandshakeError::Failure(tungstenite::Error::Http(response)) => format!("Server refused the connection (HTTP {}). Check authentication and server compatibility.",response.status().as_u16()),
        _ => "WebSocket connection failed. Check the server address and TLS certificate.".into(),
    })?;
    let tcp = match socket.get_ref() {
        MaybeTlsStream::Plain(tcp) => tcp,
        MaybeTlsStream::Rustls(tls) => tls.get_ref(),
        _ => return Err("Unsupported TLS transport".into()),
    };
    tcp.set_read_timeout(Some(Duration::from_millis(100)))
        .map_err(|_| "Could not configure the connection")?;
    Ok(socket)
}

fn run(
    url: url::Url,
    token: Zeroizing<String>,
    inbox: mpsc::Receiver<Action>,
    emit: &impl Fn(Event) -> bool,
) -> Result<(), String> {
    let mut socket = connect(&url, &token)?;
    drop(token);
    let mut next_id = 1_u64;
    let mut pending: BTreeMap<u64, (Route, Option<Instant>)> = BTreeMap::new();
    let mut subscribe =
        |socket: &mut Socket, pending: &mut BTreeMap<_, _>, route, method, payload, stream| {
            let id = next_id;
            next_id += 1;
            send(socket, request(id, method, payload))?;
            pending.insert(
                id,
                (route, if stream { None } else { Some(Instant::now()) }),
            );
            Ok::<_, String>(id)
        };
    subscribe(
        &mut socket,
        &mut pending,
        Route::Config,
        "subscribeServerConfig",
        json!({"environmentThemes":true}),
        true,
    )?;
    let mut negotiated = false;
    let opened = Instant::now();
    let mut heartbeat = Instant::now();
    loop {
        loop {
            match inbox.try_recv() {
                Ok(action) if negotiated => match action {
                    Action::Select(thread_id) => {
                        let obsolete: Vec<_> = pending
                            .iter()
                            .filter(|(_, (route, _))| matches!(route, Route::Thread(_)))
                            .map(|(id, _)| *id)
                            .collect();
                        for id in obsolete {
                            send(
                                &mut socket,
                                json!({"_tag":"Interrupt","requestId":id.to_string()}),
                            )?;
                            pending.remove(&id);
                        }
                        if let Some(id) = thread_id {
                            subscribe(
                                &mut socket,
                                &mut pending,
                                Route::Thread(id.clone()),
                                "orchestration.subscribeThread",
                                json!({"threadId":id,"reasoningMessages":true}),
                                true,
                            )?;
                        }
                    }
                    Action::Dispatch(command) => {
                        let id = command["commandId"]
                            .as_str()
                            .ok_or("Missing command id")?
                            .to_owned();
                        subscribe(
                            &mut socket,
                            &mut pending,
                            Route::Command(id),
                            "orchestration.dispatchCommand",
                            command,
                            false,
                        )?;
                    }
                    Action::Request {
                        key,
                        method,
                        payload,
                    } => {
                        subscribe(
                            &mut socket,
                            &mut pending,
                            Route::Rpc(key),
                            method,
                            payload,
                            false,
                        )?;
                    }
                    Action::WatchVcs(cwd) => {
                        let obsolete: Vec<_> = pending
                            .iter()
                            .filter(|(_, (route, _))| matches!(route, Route::Vcs(_)))
                            .map(|(id, _)| *id)
                            .collect();
                        for id in obsolete {
                            send(
                                &mut socket,
                                json!({"_tag":"Interrupt","requestId":id.to_string()}),
                            )?;
                            pending.remove(&id);
                        }
                        if let Some(cwd) = cwd {
                            subscribe(
                                &mut socket,
                                &mut pending,
                                Route::Vcs(cwd.clone()),
                                "subscribeVcsStatus",
                                json!({"cwd":cwd}),
                                true,
                            )?;
                        }
                    }
                },
                Ok(_) => {
                    return Err("Wait for server synchronization before sending commands".into());
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    let _ = socket.close(None);
                    return Ok(());
                }
            }
        }
        let expired: Vec<_> = pending
            .iter()
            .filter(|(_, (_, at))| at.is_some_and(|at| at.elapsed() > Duration::from_secs(30)))
            .map(|(id, _)| *id)
            .collect();
        for id in expired {
            if let Some((route, _)) = pending.remove(&id) {
                emit(Event::Failed(route,"No receipt received. The operation may have been accepted; check the conversation before retrying.".into()));
                send(
                    &mut socket,
                    json!({"_tag":"Interrupt","requestId":id.to_string()}),
                )?;
            }
        }
        if !negotiated && opened.elapsed() > Duration::from_secs(15) {
            return Err("The server did not complete the handshake".into());
        }
        if heartbeat.elapsed() > Duration::from_secs(15) {
            send(&mut socket, json!({"_tag":"Ping"}))?;
            heartbeat = Instant::now();
        }
        match socket.read() {
            Ok(Message::Text(text)) => {
                for frame in frames(&text)? {
                    if matches!(
                        frame["_tag"].as_str(),
                        Some("Defect" | "ClientProtocolError")
                    ) {
                        return Err("The server rejected this client's RPC protocol".into());
                    }
                    let id = frame["requestId"]
                        .as_str()
                        .and_then(|s| s.parse().ok())
                        .or_else(|| frame["requestId"].as_u64());
                    let Some(id) = id else {
                        continue;
                    };
                    if frame["_tag"] == "Chunk" {
                        send(
                            &mut socket,
                            json!({"_tag":"Ack","requestId":frame["requestId"]}),
                        )?;
                    }
                    let Some((route, _)) = pending.get(&id).cloned() else {
                        continue;
                    };
                    match frame["_tag"].as_str() {
                        Some("Chunk") => {
                            let values =
                                frame["values"].as_array().ok_or("Invalid stream chunk")?;
                            for value in values {
                                if route == Route::Config
                                    && value["type"] == "snapshot"
                                    && !negotiated
                                {
                                    let version = value["config"]["environment"]["orchestrationProtocolVersion"].as_u64().unwrap_or(1);
                                    if version != 1 {
                                        return Err(format!(
                                            "Unsupported orchestration protocol {version}; this client requires protocol 1"
                                        ));
                                    }
                                    subscribe(
                                        &mut socket,
                                        &mut pending,
                                        Route::Shell,
                                        "orchestration.subscribeShell",
                                        json!({}),
                                        true,
                                    )?;
                                    negotiated = true;
                                }
                                if !emit(Event::Data(route.clone(), value.clone())) {
                                    return Ok(());
                                }
                            }
                        }
                        Some("Exit") => {
                            pending.remove(&id);
                            if frame["exit"]["_tag"] == "Success" {
                                if matches!(route, Route::Vcs(_)) {
                                    continue;
                                }
                                if !matches!(route, Route::Command(_) | Route::Rpc(_)) {
                                    return Err("A required server subscription ended. Reconnect to synchronize.".into());
                                }
                                if !emit(Event::Data(route, frame["exit"]["value"].clone())) {
                                    return Ok(());
                                }
                            } else {
                                if matches!(route, Route::Config | Route::Shell) {
                                    return Err("Server synchronization failed. Check permissions and server compatibility.".into());
                                }
                                if !emit(Event::Failed(route, remote_error(&frame["exit"]))) {
                                    return Ok(());
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            Ok(Message::Close(_)) => return Err("The server closed the connection".into()),
            Ok(_) => {}
            Err(tungstenite::Error::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => {
                return Err(
                    "Connection lost. Reconnect to reload authoritative server state.".into(),
                );
            }
        }
    }
}

fn remote_error(exit: &Value) -> String {
    exit["cause"]
        .as_array()
        .and_then(|causes| {
            causes
                .iter()
                .find_map(|cause| cause["error"]["message"].as_str())
        })
        .unwrap_or("The server rejected the request")
        .to_owned()
}
