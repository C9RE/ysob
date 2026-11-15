//! Two doors into the lighthouse:
//! - the agents' door (an owner-only unix socket, or loopback TCP on Windows) that agents' hooks and the `ysob` command use;
//! - a loopback HTTP + WebSocket API for the captain's UI, behind a bearer token.

use crate::hub::Hub;
use anyhow::Result;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path as AxPath, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[derive(Clone)]
struct App {
    hub: Arc<Hub>,
    token: Arc<String>,
    hosts: Arc<Vec<String>>,
}

/// Agents' door. On unix: an owner-only socket file. Elsewhere: a loopback TCP port.
#[cfg(unix)]
pub async fn serve_socket(hub: Arc<Hub>) -> Result<()> {
        let _ = std::fs::remove_file(&hub.sock);
    let listener = tokio::net::UnixListener::bind(&hub.sock)?;
    std::fs::set_permissions(&hub.sock, std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
    let _ = hub.door.set(hub.sock.display().to_string());
    loop {
        let (stream, _) = listener.accept().await?;
        let (r, w) = stream.into_split();
        tokio::spawn(answer(hub.clone(), r, w));
    }
}

#[cfg(not(unix))]
pub async fn serve_socket(hub: Arc<Hub>) -> Result<()> {
    serve_tcp(hub).await
}

/// The agents' door as a loopback TCP port. Anyone on this machine can knock, so every request
/// still has to carry the agent's own token. The address lands in `hub.door` for `YSOB_SOCK`.
pub async fn serve_tcp(hub: Arc<Hub>) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
    let _ = hub.door.set(format!("tcp:{}", listener.local_addr()?));
    loop {
        let (stream, peer) = listener.accept().await?;
        if !peer.ip().is_loopback() {
            continue;
        }
        let (r, w) = stream.into_split();
        tokio::spawn(answer(hub.clone(), r, w));
    }
}

/// One JSON line in, one JSON line out.
async fn answer(hub: Arc<Hub>, r: impl tokio::io::AsyncRead + Unpin, mut w: impl tokio::io::AsyncWrite + Unpin) {
    let mut line = String::new();
    if BufReader::new(r).read_line(&mut line).await.is_err() {
        return;
    }
    let reply = match serde_json::from_str::<Value>(&line) {
        Ok(v) => {
            let agent = v["agent"].as_str().unwrap_or("");
            let tok = v["token"].as_str().unwrap_or("");
            match v["op"].as_str() {
                Some("hook") => {
                    let (allow, reason) = hub.hook(agent, tok, &v["event"]).await;
                    json!({"allow": allow, "reason": reason})
                }
                Some("cli") => {
                    let args: Vec<String> = v["args"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
                    match hub.cli(agent, tok, &args).await {
                        Ok(text) => json!({"ok": true, "text": text}),
                        Err(e) => json!({"ok": false, "text": e.to_string()}),
                    }
                }
                _ => json!({"allow": false, "ok": false, "reason": "Unknown request.", "text": "Unknown request."}),
            }
        }
        Err(_) => json!({"allow": false, "ok": false, "reason": "Unreadable request.", "text": "Unreadable request."}),
    };
    let _ = w.write_all(format!("{reply}\n").as_bytes()).await;
}

fn authed(app: &App, headers: &HeaderMap, q: &HashMap<String, String>) -> bool {
    // A page on another site must never drive the ship, even with a guessed token.
    if let Some(origin) = headers.get(header::ORIGIN).and_then(|o| o.to_str().ok()) {
        let host = origin.split("://").nth(1).unwrap_or("").split(':').next().unwrap_or("");
        if !app.hosts.iter().any(|h| h == host) {
            return false;
        }
    }
    let bearer = headers.get(header::AUTHORIZATION).and_then(|h| h.to_str().ok()).and_then(|h| h.strip_prefix("Bearer "));
    let given = bearer.or_else(|| q.get("token").map(String::as_str)).unwrap_or("");
    // Constant-time compare.
    given.len() == app.token.len() && given.bytes().zip(app.token.bytes()).fold(0u8, |a, (x, y)| a | (x ^ y)) == 0
}

async fn api(State(app): State<App>, AxPath(cmd): AxPath<String>, Query(q): Query<HashMap<String, String>>, headers: HeaderMap, body: Option<Json<Value>>) -> Response {
    if !authed(&app, &headers, &q) {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "Not signed in to the engine."}))).into_response();
    }
    let args = body.map(|b| b.0).unwrap_or(json!({}));
    match app.hub.dispatch(&cmd, args).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

async fn ws(State(app): State<App>, Query(q): Query<HashMap<String, String>>, headers: HeaderMap, up: WebSocketUpgrade) -> Response {
    if !authed(&app, &headers, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    up.on_upgrade(move |sock| pump(sock, app.hub))
}

async fn pump(mut sock: WebSocket, hub: Arc<Hub>) {
    let mut rx = hub.events.subscribe();
    loop {
        tokio::select! {
            ev = rx.recv() => match ev {
                Ok(text) => if sock.send(Message::Text(text)).await.is_err() { break },
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    if sock.send(Message::Text(json!({"type":"resync"}).to_string())).await.is_err() { break }
                }
                Err(_) => break,
            },
            msg = sock.recv() => match msg {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                _ => {}
            }
        }
    }
}

/// Serve the app on a listener the caller has already bound, so the URL it prints is one this
/// engine already owns.
pub async fn serve_http(hub: Arc<Hub>, host: &str, listener: tokio::net::TcpListener, token: String, ui: Option<PathBuf>) -> Result<()> {
    let mut hosts = vec!["127.0.0.1".to_string(), "localhost".to_string(), "tauri.localhost".to_string()];
    if !hosts.contains(&host.to_string()) {
        hosts.push(host.to_string());
    }
    let app = App { hub, token: Arc::new(token), hosts: Arc::new(hosts) };
    // Room for a few pasted images in one message (Claude Code takes them; see Hub::say_with).
    let mut router = Router::new()
        .route("/api/:cmd", post(api).get(api))
        .layer(axum::extract::DefaultBodyLimit::max(32 * 1024 * 1024))
        .route("/ws", get(ws))
        .with_state(app);
    if let Some(dir) = ui {
        let index = dir.join("index.html");
        router = router.fallback_service(tower_http::services::ServeDir::new(dir).fallback(tower_http::services::ServeFile::new(index)));
    }
    axum::serve(listener, router).await?;
    Ok(())
}
