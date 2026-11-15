//! `ysob serve` runs the engine. `ysob hook` is what Claude Code calls before
//! every tool. Everything else is the crew's link to the ship.

use ysob_engine::{door, hub, server};
use serde_json::{json, Value};
use std::io::Read;
use std::path::{Path, PathBuf};

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn ask(req: Value) -> Option<Value> {
    door::ask(&std::env::var("YSOB_SOCK").ok()?, &req)
}

/// Owner-only on unix. On Windows the per-user profile folder already keeps other users out.
fn owner_only(path: &Path, mode: u32) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
    }
    let _ = (path, mode);
    Ok(())
}

fn who() -> (String, String) {
    (std::env::var("YSOB_AGENT").unwrap_or_default(), std::env::var("YSOB_TOKEN").unwrap_or_default())
}

/// Claude Code's PreToolUse hook. If the lighthouse can't be reached, the answer is no.
fn hook() {
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let (agent, token) = who();
    let reply = serde_json::from_str::<Value>(&input).ok().and_then(|event| ask(json!({"op": "hook", "agent": agent, "token": token, "event": event})));
    let (allow, reason) = match reply {
        Some(r) => (r["allow"].as_bool().unwrap_or(false), r["reason"].as_str().unwrap_or("").to_string()),
        None => (false, "The YSOB engine isn't answering. Nothing passes until it's back.".to_string()),
    };
    let out = if allow {
        json!({"hookSpecificOutput": {"hookEventName": "PreToolUse", "permissionDecision": "allow"}})
    } else {
        json!({"hookSpecificOutput": {"hookEventName": "PreToolUse", "permissionDecision": "deny", "permissionDecisionReason": reason}})
    };
    println!("{out}");
}

fn crew_cli(args: &[String]) -> i32 {
    let (agent, token) = who();
    match ask(json!({"op": "cli", "agent": agent, "token": token, "args": args})) {
        Some(r) => {
            println!("{}", r["text"].as_str().unwrap_or(""));
            if r["ok"].as_bool().unwrap_or(false) { 0 } else { 1 }
        }
        None => {
            eprintln!("The YSOB engine isn't answering. Is the app running?");
            1
        }
    }
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Asking for help never starts a server (or makes a data folder).
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("ysob serve [--port 7799] [--host 127.0.0.1] [--data ~/.ysob] [--ui <folder>] [--exit-with-stdin]
ysob todo add|list|done · ysob order <name> <message> · ysob squad [add <name>]");
        return Ok(());
    }
    match args.first().map(String::as_str) {
        Some("hook") => {
            hook();
            Ok(())
        }
        Some("serve") => {
            let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_else(|_| ".".into());
            let data = PathBuf::from(flag(&args, "--data").unwrap_or_else(|| format!("{home}/.ysob")));
            let host = flag(&args, "--host").unwrap_or_else(|| "127.0.0.1".into());
            let port: u16 = flag(&args, "--port").and_then(|p| p.parse().ok()).unwrap_or(7799);
            let ui = flag(&args, "--ui").map(PathBuf::from);
            std::fs::create_dir_all(&data)?;
            owner_only(&data, 0o700)?;
            let token_path = data.join("token");
            let token = match std::fs::read_to_string(&token_path) {
                Ok(t) if t.trim().len() >= 32 => t.trim().to_string(),
                _ => {
                    let t = hub::token();
                    std::fs::write(&token_path, &t)?;
                    owner_only(&token_path, 0o600)?;
                    t
                }
            };
            let bin = std::env::current_exe()?;
            // Started by the desktop shell: when the shell goes (even if it crashes), stdin closes and the engine follows.
            let (gone_tx, gone_rx) = tokio::sync::oneshot::channel::<()>();
            if args.iter().any(|a| a == "--exit-with-stdin") {
                std::thread::spawn(move || {
                    let _ = std::io::copy(&mut std::io::stdin(), &mut std::io::sink());
                    let _ = gone_tx.send(());
                });
            } else {
                std::mem::forget(gone_tx);
            }
            // Before any threads start: a launcher-started app picks up the shell's PATH and keys.
            ysob_engine::detect::adopt_login_env();
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(async move {
                let hub = hub::Hub::new(&data, &bin)?;
                // The squad's names and jobs are theme words: they come with the app's theme pack.
                if let Some(ui) = &ui {
                    hub.load_pack(ui);
                }
                // What this machine can run: agent CLIs, API keys, local models. In the background.
                hub.rescan();
                // Bind first, then say so: the URL (and the token in it) only ever names a port this
                // engine already holds. `--port 0` takes any free port; the line says which.
                let listener = tokio::net::TcpListener::bind((host.as_str(), port)).await?;
                let port = listener.local_addr()?.port();
                println!("YSOB engine up: http://{host}:{port}/#token={token}");
                let sock = tokio::spawn(server::serve_socket(hub.clone()));
                let http = tokio::spawn(async move { server::serve_http(hub, &host, listener, token, ui).await });
                tokio::select! {
                    r = sock => r??,
                    r = http => r??,
                    _ = tokio::signal::ctrl_c() => {}
                    _ = gone_rx => {}
                }
                Ok::<(), anyhow::Error>(())
            })
        }
        Some("todo") | Some("bottle") | Some("order") | Some("squad") | Some("crew") => std::process::exit(crew_cli(&args)),
        _ => {
            eprintln!("ysob serve [--port 7799] [--host 127.0.0.1] [--data ~/.ysob] [--ui <folder>] [--exit-with-stdin]
ysob todo add|list|done · ysob order <name> <message> · ysob squad [add <name>]");
            std::process::exit(2)
        }
    }
}
