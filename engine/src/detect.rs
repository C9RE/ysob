//! What this machine can run: agent CLIs (installed, version, signed in), the API keys present
//! (names only, never values) and local model servers. The app shows it in Settings and uses it
//! to offer engines when hiring.
//!
//! White-label: `<data>/engines.json` adds CLIs to the built-in registry, as
//! `[{"id": "my-agent", "name": "My Agent", "bin": "my-agent", "site": "https://..."}]`.

use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// One agent CLI the app knows how to run.
#[derive(Debug, Clone, Deserialize)]
pub struct Spec {
    pub id: String,
    pub name: String,
    pub bin: String,
    /// Where to get it, shown when it is missing.
    #[serde(default)]
    pub site: String,
    /// Files under the home folder that mean "signed in" (subscription or OAuth).
    #[serde(default)]
    pub auth_files: Vec<String>,
    /// Environment variables that mean "has an API key".
    #[serde(default)]
    pub auth_env: Vec<String>,
    /// False when the sign-in can live somewhere we can't read (a database, a keychain): then a
    /// missing file means "not checked", not "not signed in".
    #[serde(default = "yes")]
    pub sure: bool,
}

fn yes() -> bool {
    true
}

fn spec(id: &str, name: &str, bin: &str, site: &str, files: &[&str], env: &[&str]) -> Spec {
    Spec {
        id: id.into(),
        name: name.into(),
        bin: bin.into(),
        site: site.into(),
        auth_files: files.iter().map(|s| s.to_string()).collect(),
        auth_env: env.iter().map(|s| s.to_string()).collect(),
        sure: true,
    }
}

fn unsure(mut s: Spec) -> Spec {
    s.sure = false;
    s
}

/// The built-in registry. An engine with no auth files or env listed has a sign-in we can't
/// check from outside; it shows as "not checked" rather than a guess.
pub fn builtin() -> Vec<Spec> {
    vec![
        spec("claude", "Claude Code", "claude", "https://claude.com/claude-code", &[".claude/.credentials.json"], &["ANTHROPIC_API_KEY", "CLAUDE_CODE_OAUTH_TOKEN"]),
        spec("codex", "Codex", "codex", "https://developers.openai.com/codex", &[".codex/auth.json"], &["OPENAI_API_KEY", "CODEX_API_KEY"]),
        spec("gemini", "Gemini CLI", "gemini", "https://github.com/google-gemini/gemini-cli", &[".gemini/oauth_creds.json"], &["GEMINI_API_KEY", "GOOGLE_API_KEY", "GOOGLE_APPLICATION_CREDENTIALS"]),
        unsure(spec("opencode", "OpenCode", "opencode", "https://opencode.ai", &[".local/share/opencode/auth.json"], &["ANTHROPIC_API_KEY", "OPENAI_API_KEY", "OPENROUTER_API_KEY", "GEMINI_API_KEY", "GROQ_API_KEY", "DEEPSEEK_API_KEY", "MISTRAL_API_KEY", "XAI_API_KEY"])),
        spec("cursor-agent", "Cursor", "cursor-agent", "https://cursor.com/cli", &[], &["CURSOR_API_KEY"]),
        spec("aider", "Aider", "aider", "https://aider.chat", &[], &["ANTHROPIC_API_KEY", "OPENAI_API_KEY", "OPENROUTER_API_KEY", "GEMINI_API_KEY", "GROQ_API_KEY", "DEEPSEEK_API_KEY", "MISTRAL_API_KEY", "XAI_API_KEY"]),
        spec("qwen", "Qwen Code", "qwen", "https://github.com/QwenLM/qwen-code", &[".qwen/oauth_creds.json"], &["DASHSCOPE_API_KEY"]),
        spec("amp", "Amp", "amp", "https://ampcode.com", &[], &["AMP_API_KEY"]),
        unsure(spec("goose", "Goose", "goose", "https://block.github.io/goose", &[".config/goose/config.yaml"], &["ANTHROPIC_API_KEY", "OPENAI_API_KEY", "OPENROUTER_API_KEY", "GEMINI_API_KEY", "GROQ_API_KEY", "DEEPSEEK_API_KEY", "MISTRAL_API_KEY", "XAI_API_KEY"])),
        spec("copilot", "Copilot CLI", "copilot", "https://github.com/github/copilot-cli", &[".copilot/config.json"], &["COPILOT_GITHUB_TOKEN", "GH_TOKEN", "GITHUB_TOKEN"]),
        spec("kimi", "Kimi", "kimi", "https://github.com/MoonshotAI/kimi-cli", &[], &["MOONSHOT_API_KEY", "KIMI_API_KEY"]),
        spec("grok", "Grok CLI", "grok", "https://github.com/superagent-ai/grok-cli", &[], &["GROK_API_KEY", "XAI_API_KEY"]),
    ]
}

/// API keys by provider. Only whether a key is set is ever reported.
const KEYS: &[(&str, &[&str])] = &[
    ("Anthropic", &["ANTHROPIC_API_KEY"]),
    ("OpenAI", &["OPENAI_API_KEY"]),
    ("Google Gemini", &["GEMINI_API_KEY", "GOOGLE_API_KEY"]),
    ("OpenRouter", &["OPENROUTER_API_KEY"]),
    ("Groq", &["GROQ_API_KEY"]),
    ("Mistral", &["MISTRAL_API_KEY"]),
    ("xAI", &["XAI_API_KEY"]),
    ("DeepSeek", &["DEEPSEEK_API_KEY"]),
    ("Moonshot", &["MOONSHOT_API_KEY"]),
];

fn set(var: &str) -> bool {
    std::env::var_os(var).map(|v| !v.is_empty()).unwrap_or(false)
}

/// An id or binary name we are willing to run: plain words only, no paths or arguments.
fn plain_word(s: &str) -> bool {
    !s.is_empty() && s.len() <= 40 && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')) && !s.starts_with('.')
}

/// Built-ins plus the white-label extras from `<data>/engines.json`. Bad entries are skipped.
pub fn registry(data_dir: &Path) -> Vec<Spec> {
    let mut all = builtin();
    let Ok(text) = std::fs::read_to_string(data_dir.join("engines.json")) else { return all };
    let Ok(extra) = serde_json::from_str::<Vec<Spec>>(&text) else { return all };
    for mut s in extra {
        if !plain_word(&s.id) || !plain_word(&s.bin) || s.name.trim().is_empty() || all.iter().any(|b| b.id == s.id) {
            continue;
        }
        s.name = s.name.chars().take(40).collect();
        s.auth_files.retain(|f| !f.contains("..") && !Path::new(f).is_absolute());
        s.auth_env.retain(|v| v.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
        if !s.site.starts_with("https://") {
            s.site.clear();
        }
        all.push(s);
    }
    all
}

/// First line of `<bin> --version`, or None if it doesn't answer within the limit.
fn version(path: &Path) -> Option<String> {
    let shim = path.extension().map(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat")).unwrap_or(false);
    let mut cmd = if cfg!(windows) && shim {
        let mut c = std::process::Command::new("cmd");
        c.arg("/d").arg("/c").arg(path);
        c
    } else {
        std::process::Command::new(path)
    };
    // Output goes to a file, not a pipe: a CLI that leaves a background process holding the pipe
    // open can't hang the scan, and only the first 4 KB is read.
    let tmp = std::env::temp_dir().join(format!("ysob-version-{}-{}", std::process::id(), crate::hub::token()));
    let file = std::fs::File::create(&tmp).ok()?;
    let mut child = cmd
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .stdout(file)
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let start = Instant::now();
    let done = loop {
        match child.try_wait() {
            Ok(Some(_)) => break true,
            Ok(None) if start.elapsed() < Duration::from_secs(6) => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
        }
    };
    let mut out = String::new();
    if done {
        if let Ok(f) = std::fs::File::open(&tmp) {
            let _ = f.take(4096).read_to_string(&mut out);
        }
    }
    let _ = std::fs::remove_file(&tmp);
    if !done {
        return None;
    }
    let line = out.lines().map(str::trim).find(|l| !l.is_empty())?;
    // Keep the version-looking part when there is one ("codex-cli 0.153.4" -> "0.153.4").
    let looks = |w: &str| {
        let w = w.strip_prefix('v').unwrap_or(w);
        w.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) && w.contains('.')
    };
    let v = line.split_whitespace().find(|w| looks(w)).unwrap_or(line);
    Some(v.trim_start_matches('v').chars().take(32).collect())
}

/// How an engine is signed in: "account" (its own sign-in file), "api key", "none", or null
/// when we can't tell from outside.
fn sign_in(s: &Spec, home: Option<&Path>) -> Value {
    if let Some(h) = home {
        if s.auth_files.iter().any(|f| h.join(f).is_file()) {
            return json!("account");
        }
    }
    if s.auth_env.iter().any(|v| set(v)) {
        return json!("api key");
    }
    // Gemini CLI says which sign-in it uses; an API-key setup with no key set is "key missing".
    if s.id == "gemini" {
        if let Some(h) = home {
            let kind = std::fs::read_to_string(h.join(".gemini/settings.json"))
                .ok()
                .and_then(|t| serde_json::from_str::<Value>(&t).ok())
                .and_then(|v| v["security"]["auth"]["selectedType"].as_str().or(v["selectedAuthType"].as_str()).map(str::to_string));
            // A key in Gemini's own .env counts only if one of the key variables is really set there.
            let env_key = std::fs::read_to_string(h.join(".gemini/.env"))
                .map(|t| t.lines().any(|l| {
                    let l = l.trim().trim_start_matches("export ").trim();
                    ["GEMINI_API_KEY=", "GOOGLE_API_KEY="].iter().any(|k| l.strip_prefix(k).map(|v| !v.trim().trim_matches(['"', '\'']).is_empty()).unwrap_or(false))
                }))
                .unwrap_or(false);
            if env_key {
                return json!("api key");
            }
            if kind.as_deref().map(|k| k.contains("api-key")).unwrap_or(false) {
                return json!("key missing");
            }
        }
    }
    if (s.auth_files.is_empty() && s.auth_env.is_empty()) || !s.sure {
        return Value::Null;
    }
    // Claude Code on macOS keeps its sign-in in the Keychain, out of sight.
    if cfg!(target_os = "macos") && s.id == "claude" {
        return Value::Null;
    }
    json!("none")
}

/// A local model server's model names, if it answers on this port. Plain HTTP on loopback, no TLS.
fn local_models(port: u16, path: &str, list_key: &str, name_key: &str) -> Option<Vec<String>> {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let mut s = std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(300)).ok()?;
    s.set_read_timeout(Some(Duration::from_millis(500))).ok()?;
    write!(s, "GET {path} HTTP/1.0\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n").ok()?;
    // A hard two-second limit overall: a server that drips bytes can't stall the scan.
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    while Instant::now() < deadline && buf.len() < 2_000_000 {
        match s.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(_) => break,
        }
    }
    let text = String::from_utf8_lossy(&buf);
    let body = text.split_once("\r\n\r\n")?.1;
    let v: Value = serde_json::from_str(body).ok()?;
    Some(v[list_key].as_array()?.iter().filter_map(|m| m[name_key].as_str().map(|n| n.chars().take(60).collect())).take(50).collect())
}

/// A desktop app started from the launcher misses what the user's shell profile sets up (PATH
/// entries for nvm, bun, cargo; exported API keys). Ask the login shell once, with a time limit,
/// and adopt only extra PATH entries and the known key variables that aren't already set.
/// `YSOB_NO_SHELL_ENV=1` turns it off.
#[cfg(unix)]
pub fn adopt_login_env() {
    if std::env::var_os("YSOB_NO_SHELL_ENV").is_some() {
        return;
    }
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    let tmp = std::env::temp_dir().join(format!("ysob-env-{}-{}", std::process::id(), crate::hub::token()));
    // Owner-only and never an existing file: the dump holds the user's whole environment, keys
    // included, for up to 4 s in a shared temp folder.
    let Ok(file) = ({
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp)
    }) else {
        return;
    };
    // The marker separates the dump from anything the profile prints.
    let Ok(mut child) = std::process::Command::new(&shell)
        .args(["-ilc", "printf '\\n__YSOB_ENV__\\0'; env -0"])
        .stdin(std::process::Stdio::null())
        .stdout(file)
        .stderr(std::process::Stdio::null())
        .spawn()
    else {
        return;
    };
    let start = Instant::now();
    let ok = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st.success(),
            Ok(None) if start.elapsed() < Duration::from_secs(4) => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
        }
    };
    let text = std::fs::read(&tmp).unwrap_or_default();
    let _ = std::fs::remove_file(&tmp);
    if !ok {
        return;
    }
    let text = String::from_utf8_lossy(&text);
    let Some((_, dump)) = text.split_once("__YSOB_ENV__\0") else { return };
    let reg = builtin();
    let keys: std::collections::HashSet<&str> = KEYS.iter().flat_map(|(_, v)| v.iter().copied()).chain(reg.iter().flat_map(|s| s.auth_env.iter().map(String::as_str))).collect();
    for kv in dump.split('\0') {
        let Some((k, v)) = kv.split_once('=') else { continue };
        if k == "PATH" {
            let mut have: Vec<PathBuf> = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect();
            for p in std::env::split_paths(v) {
                if p.is_absolute() && !have.contains(&p) {
                    have.push(p);
                }
            }
            if let Ok(joined) = std::env::join_paths(have) {
                std::env::set_var("PATH", joined);
            }
        } else if keys.contains(k) && std::env::var_os(k).is_none() && !v.is_empty() {
            std::env::set_var(k, v);
        }
    }
}

#[cfg(not(unix))]
pub fn adopt_login_env() {}

/// The fast part: which CLIs are on PATH. Safe to call on every request.
pub fn quick(data_dir: &Path, caged: bool) -> Value {
    Value::Array(
        registry(data_dir)
            .iter()
            .map(|s| json!({"id": s.id, "name": s.name, "found": crate::hub::which(&s.bin).is_some(), "chat": s.id == "claude", "caged": caged, "site": s.site}))
            .collect(),
    )
}

/// The full scan: versions (in parallel, each with a time limit), sign-in, keys and local models.
pub fn scan(data_dir: &Path, caged: bool) -> Value {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    let specs = registry(data_dir);
    let jobs: Vec<_> = specs
        .into_iter()
        .map(|s| {
            let home = home.clone();
            std::thread::spawn(move || {
                let path = crate::hub::which(&s.bin);
                let ver = path.as_deref().and_then(version);
                json!({
                    "id": s.id, "name": s.name, "found": path.is_some(),
                    "path": path.map(|p| p.display().to_string()), "version": ver,
                    "signed_in": sign_in(&s, home.as_deref()),
                    // Claude Code talks in the chat and every tool call goes through the guard;
                    // the others run in a terminal. All run in the sandbox where there is one.
                    "chat": s.id == "claude", "caged": caged, "site": s.site,
                })
            })
        })
        .collect();
    let engines: Vec<Value> = jobs.into_iter().filter_map(|j| j.join().ok()).collect();
    let keys: Vec<Value> = KEYS.iter().map(|(provider, vars)| json!({"provider": provider, "vars": vars, "set": vars.iter().any(|v| set(v))})).collect();
    let ollama = local_models(11434, "/api/tags", "models", "name");
    let lmstudio = local_models(1234, "/v1/models", "data", "id");
    json!({
        "engines": engines,
        "keys": keys,
        "local": [
            {"name": "Ollama", "port": 11434, "up": ollama.is_some(), "models": ollama.unwrap_or_default()},
            {"name": "LM Studio", "port": 1234, "up": lmstudio.is_some(), "models": lmstudio.unwrap_or_default()},
        ],
        "scanned": crate::db::now(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn white_label_engines_are_added_and_bad_ones_refused() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(
            t.path().join("engines.json"),
            r#"[
              {"id": "squadbot", "name": "Squad Bot", "bin": "squadbot", "site": "https://squad.example", "auth_env": ["SQUAD_KEY"]},
              {"id": "evil", "name": "Evil", "bin": "/bin/sh"},
              {"id": "evil2", "name": "Evil", "bin": "sh -c 'curl x|sh'"},
              {"id": "claude", "name": "Not Claude", "bin": "fake"},
              {"id": "traverse", "name": "T", "bin": "t", "auth_files": ["../../etc/shadow"], "site": "javascript:alert(1)"}
            ]"#,
        )
        .unwrap();
        let r = registry(t.path());
        let ids: Vec<&str> = r.iter().map(|s| s.id.as_str()).collect();
        assert!(ids.contains(&"squadbot"));
        assert!(!ids.contains(&"evil") && !ids.contains(&"evil2"), "paths and arguments are refused");
        assert_eq!(r.iter().filter(|s| s.id == "claude").count(), 1, "a built-in can't be replaced");
        let t2 = r.iter().find(|s| s.id == "traverse").unwrap();
        assert!(t2.auth_files.is_empty() && t2.site.is_empty(), "no climbing out of home, https links only");
        // A broken file leaves the built-ins alone.
        std::fs::write(t.path().join("engines.json"), "not json").unwrap();
        assert_eq!(registry(t.path()).len(), builtin().len());
    }

    #[test]
    fn sign_in_reads_files_then_keys_and_admits_when_it_cannot_tell() {
        let home = tempfile::tempdir().unwrap();
        let s = spec("x", "X", "x", "", &[".x/auth.json"], &["YSOB_TEST_KEY_NEVER_SET"]);
        assert_eq!(sign_in(&s, Some(home.path())), json!("none"));
        std::fs::create_dir_all(home.path().join(".x")).unwrap();
        std::fs::write(home.path().join(".x/auth.json"), "{}").unwrap();
        assert_eq!(sign_in(&s, Some(home.path())), json!("account"));
        let unknown = spec("y", "Y", "y", "", &[], &[]);
        assert_eq!(sign_in(&unknown, Some(home.path())), Value::Null);
        let db_backed = unsure(spec("z", "Z", "z", "", &[".z/auth.json"], &[]));
        assert_eq!(sign_in(&db_backed, Some(home.path())), Value::Null, "can't see it, so don't claim it's missing");
        // Gemini set to API-key sign-in with no key anywhere.
        std::fs::create_dir_all(home.path().join(".gemini")).unwrap();
        std::fs::write(home.path().join(".gemini/settings.json"), r#"{"security":{"auth":{"selectedType":"gemini-api-key"}}}"#).unwrap();
        let g = builtin().into_iter().find(|s| s.id == "gemini").unwrap();
        if !set("GEMINI_API_KEY") && !set("GOOGLE_API_KEY") {
            assert_eq!(sign_in(&g, Some(home.path())), json!("key missing"));
        }
    }

    #[cfg(unix)]
    #[test]
    fn versions_are_read_and_a_hung_cli_is_cut_off() {
        use std::os::unix::fs::PermissionsExt;
        let t = tempfile::tempdir().unwrap();
        let ok = t.path().join("ok");
        std::fs::write(&ok, "#!/bin/sh\necho 'codex-cli 0.153.4'\n").unwrap();
        let slow = t.path().join("slow");
        std::fs::write(&slow, "#!/bin/sh\nsleep 30\n").unwrap();
        for p in [&ok, &slow] {
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert_eq!(version(&ok).as_deref(), Some("0.153.4"));
        let vee = t.path().join("vee");
        std::fs::write(&vee, "#!/bin/sh\necho 'opencode v2.0.3'\n").unwrap();
        std::fs::set_permissions(&vee, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(version(&vee).as_deref(), Some("2.0.3"));
        let start = Instant::now();
        assert_eq!(version(&slow), None);
        assert!(start.elapsed() < Duration::from_secs(10), "a hung CLI doesn't hang the scan");
        // Review: a CLI that answers but leaves a child holding stdout open.
        let sneaky = t.path().join("sneaky");
        std::fs::write(&sneaky, "#!/bin/sh\necho 'sneaky 1.2.3'\n(sleep 30) &\n").unwrap();
        std::fs::set_permissions(&sneaky, std::fs::Permissions::from_mode(0o755)).unwrap();
        let start = Instant::now();
        assert_eq!(version(&sneaky).as_deref(), Some("1.2.3"));
        assert!(start.elapsed() < Duration::from_secs(3), "a background child held the scan");
    }

    #[test]
    fn local_servers_that_are_not_running_are_reported_down_quickly() {
        let start = Instant::now();
        assert!(local_models(1, "/api/tags", "models", "name").is_none());
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}
