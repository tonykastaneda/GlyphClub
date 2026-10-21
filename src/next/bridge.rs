//! The local connection the GlyphClub Illustrator plug-in (see `plugin/`)
//! uses to ask for missing fonts — the same protocol and `bridge.json`
//! session file as `src/illustrator_bridge.rs`, which serves the old front
//! end. It listens on a random loopback port and writes that port plus a
//! fresh token to `bridge.json` (mode 0600), so only this user's processes
//! can find and use it.
//!
//! ```text
//! {"token":"…","method":"ping","pluginVersion":"0.2.0"}
//!   -> {"ok":true,"app":"GlyphClub","version":"0.1.2","latestPluginVersion":"0.2.0"}
//! {"token":"…","method":"activate","fonts":["Depot-Regular", …]}
//!   -> {"ok":true,"activated":[…],"alreadyActive":[…],"notFound":[…],"failed":[…]}
//! {"token":"…","method":"openIntegrations"}   (shows Settings ▸ Integrations)
//! {"token":"…","method":"show"}               (a second GlyphClub launch)
//! ```
//!
//! Requests that need the app are sent over a channel the gpui side drains
//! on its main thread; `activate` waits for the reply.

use std::hash::{BuildHasher, Hasher};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{json, Value};

const MAX_REQUEST_BYTES: u64 = 256 * 1024;

#[derive(Default)]
pub struct ActivateOutcome {
    pub activated: Vec<String>,
    pub already_active: Vec<String>,
    pub not_found: Vec<String>,
    pub failed: Vec<String>,
}

pub enum BridgeEvent {
    PluginVersionSeen(String),
    OpenIntegrations,
    Show,
    Activate {
        fonts: Vec<String>,
        reply: mpsc::Sender<ActivateOutcome>,
    },
}

pub fn session_path() -> PathBuf {
    let base = dirs::data_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("glyphclub");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("bridge.json")
}

/// Starts listening on a background thread. Best-effort: if the port or the
/// session file can't be set up, GlyphClub still runs normally; the plug-in
/// just reports that it can't reach the app.
pub fn start(events: mpsc::Sender<BridgeEvent>) {
    let Ok(listener) = TcpListener::bind(("127.0.0.1", 0)) else {
        return;
    };
    let Ok(addr) = listener.local_addr() else {
        return;
    };
    let token = new_token();
    if write_session(addr.port(), &token).is_err() {
        return;
    }
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let events = events.clone();
            let token = token.clone();
            std::thread::spawn(move || {
                let _ = serve(stream, &token, &events);
            });
        }
    });
}

/// If another GlyphClub is already running with its bridge up, asks it to
/// show its window and returns true; `main` then exits instead of starting
/// a second copy.
pub fn show_running_instance() -> bool {
    let Ok(raw) = std::fs::read(session_path()) else {
        return false;
    };
    let Ok(session) = serde_json::from_slice::<Value>(&raw) else {
        return false;
    };
    let (Some(port), Some(token)) = (
        session.get("port").and_then(Value::as_u64),
        session.get("token").and_then(Value::as_str),
    ) else {
        return false;
    };
    let Ok(stream) = TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port as u16)),
        Duration::from_millis(500),
    ) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let request = json!({ "token": token, "method": "show" }).to_string() + "\n";
    if (&stream).write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut reply = String::new();
    let _ = BufReader::new(&stream).read_line(&mut reply);
    serde_json::from_str::<Value>(&reply)
        .ok()
        .and_then(|r| r.get("ok").and_then(Value::as_bool))
        .unwrap_or(false)
}

/// Removes `bridge.json` on quit, so the plug-in knows straight away that
/// GlyphClub isn't running rather than timing out on a dead port.
pub fn stop() {
    let _ = std::fs::remove_file(session_path());
}

fn write_session(port: u16, token: &str) -> std::io::Result<()> {
    let body = json!({
        "port": port,
        "token": token,
        "pid": std::process::id(),
        "version": env!("CARGO_PKG_VERSION"),
    })
    .to_string();
    let path = session_path();
    let tmp = path.with_extension("json.tmp");
    let _ = std::fs::remove_file(&tmp);
    {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&tmp)?;
        file.write_all(body.as_bytes())?;
    }
    std::fs::rename(&tmp, &path)
}

/// 256 bits from std's per-process random hash keys (seeded from the OS
/// RNG), each round mixing in the previous output.
fn new_token() -> String {
    let mut token = String::with_capacity(64);
    let mut previous = 0u64;
    for round in 0..4u64 {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u64(round);
        hasher.write_u64(previous);
        hasher.write_u32(std::process::id());
        if let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            hasher.write_u128(now.as_nanos());
        }
        previous = hasher.finish();
        token.push_str(&format!("{previous:016x}"));
    }
    token
}

fn serve(stream: TcpStream, token: &str, events: &mpsc::Sender<BridgeEvent>) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    let mut line = String::new();
    BufReader::new((&stream).take(MAX_REQUEST_BYTES)).read_line(&mut line)?;
    let reply = handle(&line, token, events);
    let mut out = &stream;
    out.write_all(reply.to_string().as_bytes())?;
    out.write_all(b"\n")?;
    out.flush()
}

fn handle(line: &str, token: &str, events: &mpsc::Sender<BridgeEvent>) -> Value {
    let Ok(request) = serde_json::from_str::<Value>(line) else {
        return error("bad request");
    };
    let given = request.get("token").and_then(Value::as_str).unwrap_or("");
    if !constant_time_eq(given.as_bytes(), token.as_bytes()) {
        return error("bad token");
    }
    if let Some(version) = request.get("pluginVersion").and_then(Value::as_str) {
        let _ = events.send(BridgeEvent::PluginVersionSeen(version.to_string()));
    }
    let mut reply = handle_method(&request, events);
    if let (Some(object), Some(latest)) = (reply.as_object_mut(), latest_plugin_version()) {
        object.insert("latestPluginVersion".to_string(), Value::String(latest));
    }
    reply
}

pub fn latest_plugin_version() -> Option<String> {
    glyphclub::integrations::bundled_plugin()
        .and_then(|bundle| glyphclub::integrations::plugin_version(&bundle))
        .filter(|v| !v.is_empty())
}

fn handle_method(request: &Value, events: &mpsc::Sender<BridgeEvent>) -> Value {
    match request.get("method").and_then(Value::as_str) {
        Some("ping") => json!({
            "ok": true,
            "app": "GlyphClub",
            "version": env!("CARGO_PKG_VERSION"),
        }),
        Some("openIntegrations") => {
            let _ = events.send(BridgeEvent::OpenIntegrations);
            json!({ "ok": true })
        }
        Some("show") => {
            let _ = events.send(BridgeEvent::Show);
            json!({ "ok": true })
        }
        Some("activate") => {
            let fonts: Vec<String> = request
                .get("fonts")
                .and_then(Value::as_array)
                .map(|names| names.iter().filter_map(Value::as_str).map(str::to_string).collect())
                .unwrap_or_default();
            let (reply, receive) = mpsc::channel();
            if events.send(BridgeEvent::Activate { fonts, reply }).is_err() {
                return error("GlyphClub is quitting");
            }
            match receive.recv_timeout(Duration::from_secs(20)) {
                Ok(outcome) => json!({
                    "ok": true,
                    "activated": outcome.activated,
                    "alreadyActive": outcome.already_active,
                    "notFound": outcome.not_found,
                    "failed": outcome.failed,
                }),
                Err(_) => error("GlyphClub didn't answer in time"),
            }
        }
        _ => error("unknown method"),
    }
}

fn error(message: &str) -> Value {
    json!({ "ok": false, "error": message })
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
