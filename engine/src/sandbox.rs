//! The cage: every agent below the HOST lane runs inside bubblewrap (Linux).
//!
//! The lane guard reads tool calls, but an agent that may edit files and run tests can write
//! any code into a test. So the guard alone can't contain it. The cage is what the agent's
//! process can actually see:
//!
//! - the system (`/usr`, `/etc`, ...) read-only,
//! - its own mission folder (read-write in EDIT, read-only in LOOK),
//! - a private home of its own, holding only the engine's sign-in file and toolchains,
//! - nothing else: not the real home folder, keys, other projects, `/srv`, the engine's data,
//!   other processes, the ssh agent or the user's session bus.
//!
//! The files that steer agents (`.claude`, `.mcp.json`, git hooks and config, `.codex`,
//! `.gemini`) stay read-only even inside a writable mission.

use crate::lanes::Lane;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub struct Spec<'a> {
    pub lane: Lane,
    /// The engine command's first word: `claude`, `codex`, `gemini`...
    pub engine: &'a str,
    /// The folder the agent works in, as the user gave it.
    pub mission: &'a Path,
    /// The real home folder, hidden from the agent.
    pub home: &'a Path,
    /// The agent's own home, mounted where the real one was.
    pub agent_home: &'a Path,
    pub data_dir: &'a Path,
    /// Programs the agent must be able to run: the engine binary and `ysob` itself.
    pub programs: &'a [PathBuf],
    /// The engine's socket, when it is a file.
    pub sock: Option<&'a Path>,
    /// True for terminal engines: they keep the controlling terminal.
    pub pty: bool,
    /// Extra folders shown read-only (a worktree's linked dependencies).
    pub share: &'a [PathBuf],
    /// A worktree agent's git folders, worked out by the engine (never read from the
    /// agent-writable `.git` file): its gitdir and the main repo's `.git`. Shown read-only.
    pub git: Option<(PathBuf, PathBuf)>,
    /// Environment the agent gets, on top of the plain allowlist (PATH, YSOB_*, the engine's key).
    pub env: &'a [(String, String)],
}

/// Toolchains under the home folder that agents may run but never change.
const TOOLCHAINS: &[&str] = &[
    ".local/bin", ".local/share/claude", ".bun/bin", ".bun/install/cache", ".cargo/bin", ".cargo/registry", ".cargo/git", ".rustup",
    ".nvm", ".deno/bin", "go/bin", "go/pkg/mod", ".pyenv", ".volta", ".local/share/pnpm", ".npm-global",
];

/// Variables passed through from the engine's environment; everything else is cleared, so API
/// keys and tokens in the user's shell never reach an agent's code.
const ENV_PASS: &[&str] = &[
    "LANG", "LANGUAGE", "LC_ALL", "LC_CTYPE", "LC_MESSAGES", "TERM", "COLORTERM", "TZ", "USER", "LOGNAME", "SHELL",
    "HTTP_PROXY", "HTTPS_PROXY", "NO_PROXY", "http_proxy", "https_proxy", "no_proxy", "SSL_CERT_FILE", "SSL_CERT_DIR", "NODE_EXTRA_CA_CERTS",
];

/// Roots that hold other people's or the user's private things. Each becomes an empty folder.
/// `/run` goes too: it holds sockets like docker's (root on the host) and screen sessions.
const HIDDEN: &[&str] = &["/home", "/Users", "/srv", "/mnt", "/media", "/root", "/run", "/tmp", "/var/tmp"];

/// What is brought back from a hidden root because programs need it: DNS on systemd machines.
const KEEP: &[&str] = &["/run/systemd/resolve"];

/// Files and folders in a mission that steer agents. Folders are made if missing (an empty folder
/// changes nothing), so an agent can't create them; files are only protected when they exist,
/// and the merge refuses changes to any of them (worktree.rs).
const RIGGING_DIRS: &[&str] = &[".claude", ".codex", ".gemini"];
// Also protected only when present (never made, so a mission isn't littered with them): hook
// managers that run on the user's own next commit, outside any cage, and other engines' configs.
const RIGGING_FILES: &[&str] = &[".mcp.json", ".husky", ".githooks", ".qwen", ".goose", "lefthook.yml", ".lefthook.yml", ".pre-commit-config.yaml", "opencode.json", ".aider.conf.yml"];

/// Sign-in files each engine needs from the real home, bound in so a refreshed token is shared:
/// the registry's own list for that engine (OpenCode, Qwen, Copilot, Goose, or one declared in
/// engines.json), plus a few extras the built-ins need beside it.
pub fn sign_in(engine: &str, data_dir: &Path) -> Vec<String> {
    let extra: &[&str] = match engine {
        "gemini" => &[".gemini/google_accounts.json"],
        _ => &[],
    };
    let mut files: Vec<String> = crate::detect::registry(data_dir)
        .into_iter()
        .find(|e| e.id == engine || e.bin == engine)
        .map(|e| e.auth_files)
        .unwrap_or_default();
    files.extend(extra.iter().map(|s| s.to_string()));
    // Never anything outside the home or in the places keys live, whatever a config says.
    files.retain(|f| !f.contains("..") && !Path::new(f).is_absolute() && ![".ssh", ".gnupg", ".aws", ".kube", ".docker"].iter().any(|d| f.starts_with(d)));
    files.dedup();
    files
}

/// Settings each engine needs to start, copied once into the agent's home (never bound:
/// the agent may change its copy, which only it reads).
pub fn starter(engine: &str) -> &'static [&'static str] {
    match engine {
        "codex" => &[".codex/config.toml"],
        "gemini" => &[".gemini/settings.json", ".gemini/installation_id"],
        _ => &[],
    }
}

fn canon(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

fn push(a: &mut Vec<OsString>, xs: &[&std::ffi::OsStr]) {
    a.extend(xs.iter().map(|x| x.to_os_string()));
}

/// The bubblewrap arguments, ending in `--`: the caller appends the program and its arguments.
/// Creates the mount points it needs inside the agent's home and the mission.
pub fn args(s: &Spec) -> std::io::Result<Vec<OsString>> {
    let mut a: Vec<OsString> = Vec::new();
    let o = |s: &str| OsString::from(s);
    push(&mut a, &[&o("--ro-bind"), &o("/"), &o("/")]);
    let home = s.home.to_path_buf();
    let home_real = canon(&home);
    let mut hidden: Vec<PathBuf> = HIDDEN.iter().map(PathBuf::from).filter(|p| p.is_dir()).collect();
    for p in [home.clone(), home_real.clone(), canon(s.data_dir)] {
        if !hidden.iter().any(|h| p.starts_with(h)) {
            hidden.push(p);
        }
    }
    for h in &hidden {
        push(&mut a, &[&o("--tmpfs"), h.as_os_str()]);
    }
    for k in KEEP {
        if Path::new(k).is_dir() {
            push(&mut a, &[&o("--ro-bind"), &o(k), &o(k)]);
        }
    }

    // The agent's own home, then the read-only toolchains and the engine's sign-in.
    std::fs::create_dir_all(s.agent_home)?;
    push(&mut a, &[&o("--bind"), s.agent_home.as_os_str(), home.as_os_str()]);
    for rel in TOOLCHAINS {
        let src = home.join(rel);
        if src.exists() {
            push(&mut a, &[&o("--ro-bind"), canon(&src).as_os_str(), home.join(rel).as_os_str()]);
        }
    }
    // The agent can plant symlinks in its own home; the engine never writes through one.
    let ah = canon(s.agent_home);
    let safe = |dst: &Path| dst.symlink_metadata().map(|m| !m.file_type().is_symlink()).unwrap_or(true) && dst.parent().map(|p| canon(p).starts_with(&ah)).unwrap_or(false);
    for rel in starter(s.engine) {
        let (src, dst) = (home.join(rel), s.agent_home.join(rel));
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if src.is_file() && !dst.exists() && safe(&dst) {
            std::fs::create_dir_all(dst.parent().unwrap_or(s.agent_home))?;
            std::fs::copy(&src, &dst)?;
        }
    }
    for rel in sign_in(s.engine, s.data_dir) {
        let rel = rel.as_str();
        let src = home.join(rel);
        if src.is_file() {
            let dst = s.agent_home.join(rel);
            std::fs::create_dir_all(dst.parent().unwrap_or(s.agent_home))?;
            if !safe(&dst) {
                continue;
            }
            if !dst.exists() {
                std::fs::write(&dst, b"")?;
            }
            push(&mut a, &[&o("--bind"), canon(&src).as_os_str(), home.join(rel).as_os_str()]);
        }
    }
    // The program files themselves, not their folders (a folder in the home would bring its
    // neighbours back into view).
    for p in s.programs {
        let f = canon(p);
        if f.is_file() {
            push(&mut a, &[&o("--ro-bind"), f.as_os_str(), f.as_os_str()]);
        }
    }

    // The mission, at its real path and at the path the user gave (often through a symlink).
    let real = canon(s.mission);
    let bind = if s.lane == Lane::Read { "--ro-bind" } else { "--bind" };
    push(&mut a, &[&o(bind), real.as_os_str(), real.as_os_str()]);
    if s.mission != real {
        push(&mut a, &[&o(bind), real.as_os_str(), s.mission.as_os_str()]);
    }
    // Read-only overlays go on every path the mission is mounted at, or the second mount
    // would leave them writable.
    let mounts: Vec<&Path> = if s.mission != real { vec![real.as_path(), s.mission] } else { vec![real.as_path()] };
    let ro = |rel: &str, a: &mut Vec<OsString>| {
        for m in &mounts {
            let p = m.join(rel);
            if p.symlink_metadata().is_ok() {
                a.extend([o("--ro-bind"), real.join(rel).into_os_string(), p.into_os_string()]);
            }
        }
    };
    // Git data is read-only for every agent: no hooks, config, commondir or modules an agent
    // could plant for the engine's or the user's own git to run later. Agents read with git;
    // the engine does all writing (worktree.rs).
    ro(".git", &mut a);
    if let Some((gitdir, common)) = &s.git {
        for g in [common, gitdir] {
            let g = canon(g);
            if g.is_dir() {
                a.extend([o("--ro-bind"), g.clone().into_os_string(), g.into_os_string()]);
            }
        }
    }
    // An agent in the main checkout can't reach into the squad's own checkouts.
    if real.join(".ysob").is_dir() {
        ro(".ysob", &mut a);
    }
    if s.lane != Lane::Read {
        for rel in RIGGING_DIRS {
            let p = real.join(rel);
            if p.symlink_metadata().is_err() {
                std::fs::create_dir_all(&p)?;
            }
            ro(rel, &mut a);
        }
        for rel in RIGGING_FILES {
            ro(rel, &mut a);
        }
    }
    for p in s.share {
        let p = canon(p);
        push(&mut a, &[&o("--ro-bind"), p.as_os_str(), p.as_os_str()]);
    }
    if let Some(sock) = s.sock.filter(|p| p.exists()) {
        push(&mut a, &[&o("--bind"), sock.as_os_str(), sock.as_os_str()]);
    }

    push(&mut a, &[&o("--dev"), &o("/dev"), &o("--proc"), &o("/proc"), &o("--unshare-pid"), &o("--unshare-ipc"), &o("--die-with-parent")]);
    if !s.pty {
        // No controlling terminal, so nothing inside can push keystrokes into ours.
        push(&mut a, &[&o("--new-session")]);
    }
    // The environment is not in these arguments: a command line is readable by every local
    // user (/proc/<pid>/cmdline), and the environment carries API keys. bwrap is started with a
    // cleared environment holding only `env(s)`, and passes it on.
    push(&mut a, &[&o("--chdir"), s.mission.as_os_str(), &o("--")]);
    Ok(a)
}

/// The whole environment for a caged program: the allowlist from the user's shell, then what
/// the engine passes (PATH, YSOB_*, sign-in keys), then HOME. Set it on the `bwrap` process after
/// clearing that process's own environment; nothing else from the user's shell gets in.
pub fn env(s: &Spec) -> Vec<(OsString, OsString)> {
    let mut e: Vec<(OsString, OsString)> = ENV_PASS.iter().filter_map(|v| std::env::var_os(v).map(|val| (OsString::from(v), val))).collect();
    e.extend(s.env.iter().map(|(k, v)| (OsString::from(k), OsString::from(v))));
    e.push((OsString::from("HOME"), s.home.as_os_str().to_owned()));
    e
}

/// True when bubblewrap is installed and allowed to make its namespaces here. Checked once.
pub fn available() -> bool {
    static OK: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *OK.get_or_init(|| {
        cfg!(target_os = "linux")
            && std::process::Command::new("bwrap")
                .args(["--ro-bind", "/", "/", "--dev", "/dev", "--proc", "/proc", "--unshare-pid", "true"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
    })
}

/// Why there is no cage on this machine, in words for the user.
pub fn missing_reason() -> &'static str {
    if cfg!(target_os = "linux") {
        "bubblewrap isn't installed or isn't allowed to run (Ubuntu 24.04: add an AppArmor profile for /usr/bin/bwrap)"
    } else {
        "agent sandboxing is Linux only for now"
    }
}
