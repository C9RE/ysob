//! The lighthouse. Every message, tool call and bottle passes through here.
//! Agents never talk to each other or to the hold directly.

use crate::breaker::Breaker;
use crate::db::{self, Crew, Island};
use crate::lanes::{self, Grant, Lane, Verdict};
use crate::roster::{self, Member};
use crate::detect;
use crate::sandbox;
use crate::worktree;
use anyhow::{anyhow, bail, Result};
use base64::Engine as _;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{broadcast, mpsc, oneshot};

const ASK_TIMEOUT_SECS: u64 = 600;
const BOTTLES_PER_MINUTE: usize = 6;
const SCROLLBACK: usize = 256 * 1024;
/// A terminal engine sends no end-of-turn signal. After this much silence the captain is asked to look.
const PTY_QUIET_SECS: i64 = 120;
/// The newest log entries a pirate is briefed with. Older ones stay in the log, out of the prompt.
const BRIEF_LOG_ENTRIES: usize = 40;
/// The engine's role id for the squad leader. Kept as it is in existing databases; people
/// only ever see "the squad leader".
pub const BOSS: &str = "Captain";

pub fn token() -> String {
    let mut b = [0u8; 16];
    getrandom::getrandom(&mut b).expect("os random");
    b.iter().map(|x| format!("{x:02x}")).collect()
}

struct Pty {
    writer: Box<dyn std::io::Write + Send>,
    master: Box<dyn portable_pty::MasterPty + Send>,
    scrollback: Vec<u8>,
    /// When the terminal last printed or was typed into, in seconds.
    seen: i64,
}

/// What we know about one pirate while the ship is running.
struct Live {
    token: String,
    status: String,
    station: String,
    label: String,
    breaker: Breaker,
    stdin: Option<mpsc::UnboundedSender<String>>,
    kill: Option<oneshot::Sender<()>>,
    /// tool_use_id -> (message row, tool name)
    tools: HashMap<String, (i64, String)>,
    last_cost: f64,
    /// The last thing the pirate said this turn, for when a job stalls.
    last_text: String,
    bottles: Vec<i64>,
    pty: Option<Pty>,
    /// The reply being typed right now, from Claude Code's partial messages. Shown live, then
    /// replaced by the finished message.
    stream: String,
    /// When the live text was last sent, so a fast reply doesn't flood the event channel.
    stream_sent: Option<std::time::Instant>,
    /// A held-back update is on its way.
    stream_pending: bool,
    /// When this turn started (unix seconds), while the agent is busy on it; the UI counts up from it.
    since: Option<i64>,
    /// Tool calls this turn, and how many of them failed or were stopped: the turn's summary line.
    turn_tools: u32,
    turn_fail: u32,
    /// Tokens in the last request, and the model's context window once Claude Code has reported it.
    ctx_tokens: f64,
    ctx_window: f64,
}

impl Live {
    fn new() -> Live {
        Live {
            token: token(), status: "idle".into(), station: "rest".into(), label: "off watch".into(), breaker: Breaker::default(),
            stdin: None, kill: None, tools: HashMap::new(), last_cost: 0.0, last_text: String::new(), bottles: vec![], pty: None, stream: String::new(), stream_sent: None, stream_pending: false,
            since: None, turn_tools: 0, turn_fail: 0, ctx_tokens: 0.0, ctx_window: 0.0,
        }
    }
}

struct Need {
    id: u64,
    crew_id: String,
    kind: &'static str,
    title: String,
    detail: String,
    grant: Option<Grant>,
    tx: Option<oneshot::Sender<String>>,
}

pub struct Hub {
    pub data_dir: PathBuf,
    pub bin: PathBuf,
    pub sock: PathBuf,
    /// What agents get as `YSOB_SOCK`: the socket path, or `tcp:127.0.0.1:<port>`. Set when the door opens.
    pub door: std::sync::OnceLock<String>,
    db: Mutex<Connection>,
    pub events: broadcast::Sender<String>,
    live: Mutex<HashMap<String, Live>>,
    needs: Mutex<Vec<Need>>,
    next_need: AtomicU64,
    /// The last full scan of agent CLIs, API keys and local models (detect.rs). Null until it runs.
    detected: Mutex<Value>,
    /// Who the squad leader may raise (roster.rs): the theme pack's squad, or the built-in one.
    roster: Mutex<Vec<Member>>,
}

fn s<'a>(v: &'a Value, k: &str) -> Result<&'a str> {
    v.get(k).and_then(Value::as_str).filter(|x| !x.trim().is_empty()).ok_or_else(|| anyhow!("Missing {k}."))
}
fn clip(t: &str, n: usize) -> String {
    let t = t.trim();
    if t.chars().count() <= n { t.to_string() } else { format!("{}…", t.chars().take(n).collect::<String>()) }
}

/// Where a pirate stands for a tool, and what the plate under their name says.
pub fn station_for(tool: &str, input: &Value) -> (&'static str, &'static str) {
    match tool {
        "Read" | "Grep" | "Glob" | "LS" | "NotebookRead" => ("chart", "reading"),
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => ("work", "writing code"),
        "WebSearch" | "WebFetch" => ("bow", "researching"),
        "Task" | "Agent" | "TodoWrite" => ("helm", "giving orders"),
        "Bash" => {
            let c = input.get("command").and_then(Value::as_str).unwrap_or("");
            if lanes::is_test_command(c) { ("cannons", "running tests") } else if c.trim().starts_with("ysob ") { ("helm", "giving orders") } else { ("hatch", "building") }
        }
        _ => ("hatch", "working"),
    }
}

fn tool_arg(tool: &str, input: &Value, island: &str) -> String {
    let g = |k: &str| input.get(k).and_then(Value::as_str).unwrap_or("");
    let rel = |p: &str| p.strip_prefix(island).map(|r| r.trim_start_matches('/').to_string()).unwrap_or_else(|| p.to_string());
    match tool {
        "Read" | "Edit" | "Write" | "MultiEdit" | "NotebookEdit" | "LS" => rel(if g("file_path").is_empty() { g("path") } else { g("file_path") }),
        "Bash" => clip(g("command"), 160),
        "Grep" | "Glob" => g("pattern").to_string(),
        "WebSearch" => g("query").to_string(),
        "WebFetch" => g("url").to_string(),
        "Task" | "Agent" => clip(g("description"), 80),
        "TodoWrite" => {
            let all = input["todos"].as_array().map(|a| a.len()).unwrap_or(0);
            let done = input["todos"].as_array().map(|a| a.iter().filter(|t| t["status"] == "completed").count()).unwrap_or(0);
            if all == 0 { String::new() } else { format!("{done} of {all} done") }
        }
        _ => String::new(),
    }
}

/// Pictures pasted into a message, checked and turned into Claude's image blocks: at most
/// five, each a PNG, JPEG, GIF or WebP in base64 of no more than 5 MB (the API's own limit).
fn pictures(v: Option<&Value>) -> Result<Vec<Value>> {
    let Some(list) = v.and_then(Value::as_array) else { return Ok(vec![]) };
    if list.len() > 5 {
        bail!("Five pictures at most in one message.");
    }
    list.iter()
        .map(|p| {
            let media = p["type"].as_str().unwrap_or("");
            if !matches!(media, "image/png" | "image/jpeg" | "image/gif" | "image/webp") {
                bail!("Pictures can be PNG, JPEG, GIF or WebP.");
            }
            let data = p["data"].as_str().unwrap_or("");
            if data.is_empty() || data.len() > 5 * 1024 * 1024 * 4 / 3 {
                bail!("A picture can be 5 MB at most.");
            }
            if !data.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'=')) {
                bail!("That picture didn't come through whole. Paste it again.");
            }
            Ok(json!({"type": "image", "source": {"type": "base64", "media_type": media, "data": data}}))
        })
        .collect()
}

/// The agent's own plan (Claude Code's TodoWrite) as the UI shows it: each item and whether it
/// is pending, in progress or completed. Only what the agent reported; nothing is guessed.
fn todos(input: &Value) -> Value {
    let items = input["todos"].as_array().cloned().unwrap_or_default();
    json!(items
        .iter()
        .take(40)
        .filter_map(|t| {
            let text = t["content"].as_str().filter(|s| !s.trim().is_empty())?;
            Some(json!({"text": clip(text, 200), "st": t["status"].as_str().unwrap_or("pending")}))
        })
        .collect::<Vec<_>>())
}

/// How full the context is, 0 to 1. Until Claude Code has said how big the model's window is,
/// assume the common 200k.
fn ctx_share(tokens: f64, window: f64) -> f64 {
    (tokens / if window > 0.0 { window } else { 200_000.0 }).min(1.0)
}

fn verb(tool: &str) -> &'static str {
    match tool {
        "Read" | "NotebookRead" => "Read",
        "Edit" | "MultiEdit" | "NotebookEdit" => "Edited",
        "Write" => "Wrote",
        "Bash" => "Ran",
        "Grep" | "Glob" => "Searched",
        "WebSearch" => "Searched the web",
        "WebFetch" => "Fetched",
        "LS" => "Listed",
        "Task" | "Agent" => "Sent a hand",
        "TodoWrite" => "Planned",
        _ => "Used",
    }
}

impl Hub {
    pub fn new(data_dir: &Path, bin: &Path) -> Result<Arc<Hub>> {
        std::fs::create_dir_all(data_dir.join("agents"))?;
        let conn = db::open(&data_dir.join("ysob.db"))?;
        for (k, v) in [("ship", "YSOB"), ("budget", "18"), ("breaker", "1"), ("hard_stop", "1"), ("quote_log", "1")] {
            if db::setting(&conn, k).is_none() {
                db::set_setting(&conn, k, v)?;
            }
        }
        // Nobody is at work when the ship comes up. A job that was mid-flight when it stopped goes
        // to the captain rather than sitting in Doing with nobody on it, or being re-sent unasked.
        conn.execute(
            "UPDATE tasks SET col='wait', ask='The engine stopped while ' || COALESCE((SELECT name FROM crew WHERE crew.id=tasks.crew_id),'a squad member')
                || ' was on this. Send it back out when you''re ready.' WHERE col='doing' AND crew_id IS NOT NULL",
            [],
        )?;
        let (tx, _) = broadcast::channel(1024);
        Ok(Arc::new(Hub {
            data_dir: data_dir.canonicalize()?, bin: bin.to_path_buf(), sock: sock_path(&data_dir.canonicalize()?)?, door: std::sync::OnceLock::new(),
            db: Mutex::new(conn), events: tx, live: Mutex::new(HashMap::new()), needs: Mutex::new(vec![]), next_need: AtomicU64::new(1), detected: Mutex::new(Value::Null), roster: Mutex::new(roster::builtin()),
        }))
    }

    /// Take the squad from the theme pack of the app served from `ui` (see roster.rs).
    pub fn load_pack(&self, ui: &Path) {
        *self.roster.lock().unwrap() = roster::load(ui);
    }

    fn db(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.db.lock().unwrap()
    }
    pub fn emit(&self, kind: &str, data: Value) {
        let _ = self.events.send(json!({"type": kind, "data": data}).to_string());
    }
    fn toast(&self, text: String, icon: &str) {
        self.emit("toast", json!({"text": text, "icon": icon}));
    }

    // ---------- views ----------

    /// True when this agent runs on a subscription rather than an API key (see on_claude's init).
    fn on_plan(&self, id: &str) -> bool {
        db::setting(&self.db(), &format!("billing.{id}")).as_deref() == Some("plan")
    }

    fn crew_view(&self, c: &Crew) -> Value {
        let billing = db::setting(&self.db(), &format!("billing.{}", c.id));
        let live = self.live.lock().unwrap();
        let l = live.get(&c.id);
        json!({"id": c.id, "name": c.name, "engine": c.engine, "role": c.role, "island_id": c.island_id, "lane": c.lane, "face": c.face,
            "task": c.task, "cost": c.cost, "ctx": c.ctx, "temp": c.temp,
            "status": l.map(|l| l.status.as_str()).unwrap_or("idle"), "station": l.map(|l| l.station.as_str()).unwrap_or("rest"),
            "label": l.map(|l| l.label.as_str()).unwrap_or("off watch"), "guarded": c.engine == "claude",
            "since": l.and_then(|l| l.since), "billing": billing})
    }
    fn emit_crew(&self, id: &str) {
        let c = db::crew_one(&self.db(), id).ok().flatten();
        if let Some(c) = c {
            self.emit("crew", self.crew_view(&c));
        }
    }
    fn needs_view(&self) -> Value {
        // `always` says in plain words what a standing yes would cover, so nobody grants more than they meant to.
        Value::Array(self.needs.lock().unwrap().iter().map(|n| json!({"id": n.id, "crew_id": n.crew_id, "kind": n.kind, "title": n.title, "detail": n.detail, "always": n.grant.as_ref().map(grant_words)})).collect())
    }
    fn settings_view(&self) -> Value {
        let d = self.db();
        let g = |k: &str| db::setting(&d, k).unwrap_or_default();
        json!({"ship": g("ship"), "budget": g("budget").parse::<f64>().unwrap_or(18.0), "breaker": g("breaker") == "1", "hard_stop": g("hard_stop") == "1", "quote_log": g("quote_log") == "1",
            "sandbox": g("sandbox") != "off", "sandbox_ok": sandbox::available(), "sandbox_why": sandbox::missing_reason(), "version": env!("CARGO_PKG_VERSION")})
    }
    /// Engines for the hire and new-mission pickers: the full scan once it has run, else a quick PATH check.
    fn engines_view(&self) -> Value {
        let d = self.detected.lock().unwrap();
        if d["engines"].is_array() {
            return d["engines"].clone();
        }
        drop(d);
        detect::quick(&self.data_dir, sandbox::available())
    }

    /// Run the full scan off the async runtime and tell the UI when it lands.
    pub fn rescan(self: &Arc<Self>) {
        let hub = self.clone();
        std::thread::spawn(move || {
            let v = detect::scan(&hub.data_dir, sandbox::available());
            *hub.detected.lock().unwrap() = v.clone();
            hub.emit("detect", v);
        });
    }

    pub fn state(&self) -> Result<Value> {
        let (islands, crew, tasks, log, spent) = {
            let d = self.db();
            (db::islands(&d)?, db::crew(&d)?, db::tasks(&d)?, db::log(&d)?, db::spent_today(&d))
        };
        Ok(json!({"settings": self.settings_view(), "islands": islands, "crew": crew.iter().map(|c| self.crew_view(c)).collect::<Vec<_>>(),
            "tasks": tasks, "log": log, "needs": self.needs_view(), "spent": spent, "engines": self.engines_view(), "detect": self.detected.lock().unwrap().clone()}))
    }
    fn emit_tasks(&self) {
        let t = db::tasks(&self.db());
        if let Ok(t) = t {
            self.emit("tasks", json!(t));
        }
    }
    fn emit_log(&self) {
        let t = db::log(&self.db());
        if let Ok(t) = t {
            self.emit("log", json!(t));
        }
    }

    fn set_live(&self, id: &str, status: Option<&str>, station: Option<(&str, &str)>) {
        {
            let mut live = self.live.lock().unwrap();
            let l = live.entry(id.to_string()).or_insert_with(Live::new);
            if let Some(st) = status {
                // A turn runs from leaving rest until coming back to it: asking the commander or
                // being held by the breaker is still the same turn, so its clock keeps going.
                let busy = |s: &str| matches!(s, "working" | "waiting" | "held");
                if busy(st) && !busy(&l.status) {
                    l.since = Some(db::now());
                    l.turn_tools = 0;
                    l.turn_fail = 0;
                } else if !busy(st) {
                    l.since = None;
                }
                l.status = st.into();
            }
            if let Some((st, label)) = station {
                l.station = st.into();
                l.label = label.into();
            }
        }
        self.emit_crew(id);
    }
    fn add_msg(&self, crew_id: &str, body: Value) -> i64 {
        let id = db::add_message(&self.db(), crew_id, &body).unwrap_or(0);
        let mut b = body;
        b["id"] = json!(id);
        b["ts"] = json!(db::now());
        self.emit("msg", json!({"crew_id": crew_id, "msg": b}));
        id
    }

    // ---------- commands from the captain's UI ----------

    pub async fn dispatch(self: &Arc<Self>, cmd: &str, a: Value) -> Result<Value> {
        match cmd {
            "state" => self.state(),
            "messages" => Ok(json!(db::messages(&self.db(), s(&a, "crew_id")?, 400)?)),
            "settings.set" => {
                let key = s(&a, "key")?;
                if !["ship", "budget", "breaker", "hard_stop", "quote_log", "sandbox"].contains(&key) {
                    bail!("Unknown setting.");
                }
                let v = match a.get("value") {
                    Some(Value::Bool(b)) => (if *b { "1" } else { "0" }).to_string(),
                    Some(Value::Number(n)) => n.to_string(),
                    Some(Value::String(t)) if !t.trim().is_empty() => t.trim().to_string(),
                    _ => bail!("Missing value."),
                };
                db::set_setting(&self.db(), key, &v)?;
                self.emit("settings", self.settings_view());
                Ok(json!({}))
            }
            "engines.scan" => {
                self.rescan();
                Ok(json!({}))
            }
            "grants.list" => {
                let d = self.db();
                let mut q = d.prepare("SELECT g.crew_id, g.grant_json, c.name, i.name FROM grants g JOIN crew c ON c.id=g.crew_id LEFT JOIN islands i ON i.id=c.island_id ORDER BY i.name, c.name")?;
                let rows = q
                    .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, Option<String>>(3)?)))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(Value::Array(rows.into_iter().map(|(crew_id, raw, name, mission)| {
                    let what = serde_json::from_str::<Grant>(&raw).map(|g| grant_words(&g)).unwrap_or_else(|_| raw.clone());
                    json!({"crew_id": crew_id, "raw": raw, "name": name, "mission": mission, "what": what})
                }).collect()))
            }
            "grants.revoke" => {
                let n = self.db().execute("DELETE FROM grants WHERE crew_id=?1 AND grant_json=?2", params![s(&a, "crew_id")?, s(&a, "raw")?])?;
                if n == 0 {
                    bail!("That permission was already gone.");
                }
                Ok(json!({}))
            }
            "island.add" => {
                let name = s(&a, "name")?.trim().to_string();
                let path = std::fs::canonicalize(s(&a, "path")?).map_err(|_| anyhow!("That folder doesn't exist."))?;
                if !path.is_dir() {
                    bail!("That path is a file. Pick a folder.");
                }
                if path.starts_with(&self.data_dir) || self.data_dir.starts_with(&path) {
                    bail!("That folder holds the engine's own data. Pick a project folder.");
                }
                let id = token();
                // Windows canonical paths start `\\?\`; people and tools expect the plain form.
                let shown = path.display().to_string();
                let shown = shown.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(shown);
                self.db().execute("INSERT INTO islands(id,name,path,created) VALUES(?1,?2,?3,?4)", params![id, name, shown, db::now()])?;
                self.emit("islands", json!(db::islands(&self.db())?));
                Ok(json!({"id": id}))
            }
            "island.remove" => {
                let id = s(&a, "id")?;
                let n: i64 = self.db().query_row("SELECT COUNT(*) FROM crew WHERE island_id=?1", [id], |r| r.get(0))?;
                if n > 0 {
                    bail!("Take the {n} squad member(s) off this mission first.");
                }
                self.db().execute("DELETE FROM islands WHERE id=?1", [id])?;
                self.emit("islands", json!(db::islands(&self.db())?));
                Ok(json!({}))
            }
            "crew.hire" => {
                let name = s(&a, "name")?.trim().to_string();
                let engine = s(&a, "engine")?.trim().to_string();
                // Only engines the registry knows (white-label ones come from engines.json):
                // the engine name is what gets started.
                if !detect::registry(&self.data_dir).iter().any(|e| e.id == engine || e.bin == engine) {
                    bail!("Unknown engine {engine}. Add it to engines.json to use it.");
                }
                let island_id = s(&a, "island_id")?.to_string();
                let lane = Lane::parse(s(&a, "lane")?).ok_or_else(|| anyhow!("Lane must be READ, WRITE or HOST."))?;
                {
                    let d = self.db();
                    if db::island(&d, &island_id)?.is_none() {
                        bail!("That mission doesn't exist.");
                    }
                    if db::crew_by_name(&d, &island_id, &name)?.is_some() {
                        bail!("There's already a {name} on this mission. Pick another name.");
                    }
                    // Names become a worktree folder and a branch; two names that fold to the same
                    // one ("Bob 1" and "Bob-1") would share a checkout, so the second is refused.
                    let folded = worktree::slug(&name);
                    if let Some(c) = db::crew(&d)?.into_iter().find(|c| c.island_id == island_id && worktree::slug(&c.name) == folded) {
                        bail!("{name} is too close to {} on this mission (they'd share a checkout). Pick another name.", c.name);
                    }
                }
                if !name.chars().all(|c| c.is_alphanumeric() || c == ' ' || c == '-' || c == '\'') || name.len() > 24 {
                    bail!("Names are letters, numbers and spaces, up to 24 characters.");
                }
                let id = token();
                let role = a.get("role").and_then(Value::as_str).unwrap_or("Deckhand");
                let face = a.get("face").and_then(Value::as_str).unwrap_or("temp");
                let temp = a.get("temp").and_then(Value::as_bool).unwrap_or(false);
                self.db().execute(
                    "INSERT INTO crew(id,name,engine,role,island_id,lane,face,temp,created) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                    params![id, name, engine, role, island_id, lane.as_str(), face, temp as i64, db::now()],
                )?;
                self.set_live(&id, Some("idle"), Some(("rest", "off watch")));
                self.toast(format!("{name} joined the squad"), "scroll");
                self.dispatch_queue().await;
                Ok(json!({"id": id}))
            }
            "crew.fire" => {
                let id = s(&a, "id")?.to_string();
                self.stop(&id);
                let leaving = db::crew_one(&self.db(), &id)?;
                if let Some(c) = leaving {
                    if let Some(main) = self.main_of(&c) {
                        worktree::retire(Path::new(&main.path), &c.name);
                    }
                }
                self.drop_needs(&id);
                {
                    let d = self.db();
                    d.execute("DELETE FROM crew WHERE id=?1", [&id])?;
                    d.execute("DELETE FROM messages WHERE crew_id=?1", [&id])?;
                    d.execute("DELETE FROM grants WHERE crew_id=?1", [&id])?;
                    d.execute("UPDATE tasks SET crew_id=NULL WHERE crew_id=?1", [&id])?;
                    d.execute("UPDATE tasks SET want=NULL WHERE want=?1", [&id])?;
                }
                self.live.lock().unwrap().remove(&id);
                let _ = std::fs::remove_dir_all(self.data_dir.join("agents").join(&id));
                self.emit("crew_gone", json!({"id": id}));
                self.emit_tasks();
                self.dispatch_queue().await;
                Ok(json!({}))
            }
            "crew.lane" => {
                let id = s(&a, "id")?;
                let lane = Lane::parse(s(&a, "lane")?).ok_or_else(|| anyhow!("Lane must be READ, WRITE or HOST."))?;
                self.db().execute("UPDATE crew SET lane=?2 WHERE id=?1", params![id, lane.as_str()])?;
                // A running agent keeps the sandbox it started with; stop it so the next start
                // builds one for the new lane.
                self.stop(id);
                self.emit_crew(id);
                Ok(json!({}))
            }
            "crew.say" => {
                let images = pictures(a.get("images"))?;
                let text = a.get("text").and_then(Value::as_str).unwrap_or("");
                if text.trim().is_empty() && images.is_empty() {
                    bail!("Nothing to send.");
                }
                self.say_with(s(&a, "id")?, text, "me", &images).await?;
                Ok(json!({}))
            }
            "crew.stop" => {
                let id = s(&a, "id")?;
                self.stop(id);
                self.set_stream(id, None);
                self.drop_needs(id);
                self.set_live(id, Some("idle"), Some(("rest", "off watch")));
                // Whatever they were on goes back in the queue for someone else.
                self.db().execute("UPDATE tasks SET crew_id=NULL WHERE col='doing' AND crew_id=?1", [id])?;
                self.emit_tasks();
                Ok(json!({}))
            }
            "need.answer" => {
                let id = a.get("id").and_then(Value::as_u64).ok_or_else(|| anyhow!("Missing id."))?;
                self.answer(id, s(&a, "answer")?).await
            }
            "task.add" => {
                let id = self.task_add(s(&a, "title")?, a.get("island_id").and_then(Value::as_str), a.get("crew_id").and_then(Value::as_str))?;
                self.task_edit(id, &a)?;
                Ok(json!({"id": id}))
            }
            "task.edit" => {
                let id = a.get("id").and_then(Value::as_i64).ok_or_else(|| anyhow!("Missing id."))?;
                self.task_edit(id, &a)?;
                // A changed order or a lifted dependency can free a job to go out.
                self.dispatch_queue().await;
                Ok(json!({}))
            }
            "task.answer" => {
                let id = a.get("id").and_then(Value::as_i64).ok_or_else(|| anyhow!("Missing id."))?;
                self.task_answer(id, s(&a, "text")?).await?;
                Ok(json!({}))
            }
            "task.move" => {
                let id = a.get("id").and_then(Value::as_i64).ok_or_else(|| anyhow!("Missing id."))?;
                self.task_move(id, s(&a, "col")?, a.get("proof").and_then(Value::as_str)).await?;
                Ok(json!({}))
            }
            "task.delete" => {
                let id = a.get("id").and_then(Value::as_i64).ok_or_else(|| anyhow!("Missing id."))?;
                self.db().execute("DELETE FROM tasks WHERE id=?1", [id])?;
                self.emit_tasks();
                Ok(json!({}))
            }
            "log.add" => {
                let kind = s(&a, "kind")?;
                if !["rule", "decision", "mistake"].contains(&kind) {
                    bail!("A log entry is a rule, a decision or a mistake.");
                }
                self.db().execute(
                    "INSERT INTO log(kind,title,detail,island_id,strikes,created) VALUES(?1,?2,?3,?4,?5,?6)",
                    params![kind, s(&a, "title")?.trim(), a.get("detail").and_then(Value::as_str).unwrap_or("").trim(), a.get("island_id").and_then(Value::as_str), (kind == "mistake") as i64, db::now()],
                )?;
                self.emit_log();
                Ok(json!({}))
            }
            "log.delete" => {
                let id = a.get("id").and_then(Value::as_i64).ok_or_else(|| anyhow!("Missing id."))?;
                self.db().execute("DELETE FROM log WHERE id=?1", [id])?;
                self.emit_log();
                Ok(json!({}))
            }
            "pty.open" => {
                let id = s(&a, "id")?.to_string();
                self.ensure_pty(&id)?;
                let live = self.live.lock().unwrap();
                let sb = live.get(&id).and_then(|l| l.pty.as_ref()).map(|p| base64::engine::general_purpose::STANDARD.encode(&p.scrollback)).unwrap_or_default();
                Ok(json!({"scrollback": sb}))
            }
            "pty.input" => {
                let id = s(&a, "id")?;
                let data = a.get("data").and_then(Value::as_str).unwrap_or("");
                let mut live = self.live.lock().unwrap();
                if let Some(p) = live.get_mut(id).and_then(|l| l.pty.as_mut()) {
                    p.seen = db::now();
                    p.writer.write_all(data.as_bytes())?;
                    p.writer.flush()?;
                }
                Ok(json!({}))
            }
            "pty.resize" => {
                let id = s(&a, "id")?;
                let cols = a.get("cols").and_then(Value::as_u64).unwrap_or(100).clamp(20, 400) as u16;
                let rows = a.get("rows").and_then(Value::as_u64).unwrap_or(30).clamp(5, 200) as u16;
                let live = self.live.lock().unwrap();
                if let Some(p) = live.get(id).and_then(|l| l.pty.as_ref()) {
                    let _ = p.master.resize(portable_pty::PtySize { rows, cols, pixel_width: 0, pixel_height: 0 });
                }
                Ok(json!({}))
            }
            _ => bail!("Unknown command: {cmd}"),
        }
    }

    // ---------- tasks ----------

    fn task_add(&self, title: &str, island_id: Option<&str>, crew_id: Option<&str>) -> Result<i64> {
        if title.trim().is_empty() {
            bail!("A todo needs a title.");
        }
        let d = self.db();
        d.execute("INSERT INTO tasks(title,col,island_id,crew_id,created) VALUES(?1,'ideas',?2,?3,?4)", params![clip(title, 200), island_id, crew_id, db::now()])?;
        let id = d.last_insert_rowid();
        drop(d);
        self.emit_tasks();
        Ok(id)
    }

    /// Change a card's wording, notes, urgency or what it waits behind. Only the fields given are touched.
    fn task_edit(&self, id: i64, a: &Value) -> Result<()> {
        let d = self.db();
        let exists = |d: &Connection, id: i64| d.query_row("SELECT 1 FROM tasks WHERE id=?1", [id], |_| Ok(())).is_ok();
        if !exists(&d, id) {
            bail!("No such todo.");
        }
        if let Some(t) = a.get("title").and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty()) {
            d.execute("UPDATE tasks SET title=?2 WHERE id=?1", params![id, clip(t, 200)])?;
        }
        if let Some(n) = a.get("note").and_then(Value::as_str) {
            d.execute("UPDATE tasks SET note=?2 WHERE id=?1", params![id, clip(n, 2000)])?;
        }
        if let Some(w) = a.get("want") {
            // Who the job is meant for, or null for anyone free. Must be on the same mission.
            let want = w.as_str().map(str::to_string);
            if let Some(cid) = &want {
                let task_island: Option<String> = d.query_row("SELECT island_id FROM tasks WHERE id=?1", [id], |r| r.get(0)).ok().flatten();
                let crew_island = db::crew_one(&d, cid)?.map(|c| c.island_id);
                if crew_island.is_none() || (task_island.is_some() && crew_island != task_island) {
                    bail!("That squadmate isn't on this todo's mission.");
                }
            }
            d.execute("UPDATE tasks SET want=?2 WHERE id=?1", params![id, want])?;
        }
        if let Some(p) = a.get("pri") {
            let urgent = p.as_bool().unwrap_or(false) || p.as_i64().unwrap_or(0) > 0;
            d.execute("UPDATE tasks SET pri=?2 WHERE id=?1", params![id, urgent as i64])?;
        }
        if let Some(after) = a.get("after") {
            match after.as_i64() {
                None => {
                    d.execute("UPDATE tasks SET after_id=NULL WHERE id=?1", [id])?;
                }
                Some(dep) => {
                    if dep == id || !exists(&d, dep) {
                        bail!("A todo can only wait behind another todo.");
                    }
                    // Walk the chain: a loop would leave every card in it waiting for ever.
                    let (mut at, mut hops) = (Some(dep), 0);
                    while let Some(x) = at {
                        if x == id || hops > 64 {
                            bail!("That would make the todos wait on each other in a circle.");
                        }
                        at = d.query_row("SELECT after_id FROM tasks WHERE id=?1", [x], |r| r.get::<_, Option<i64>>(0)).ok().flatten();
                        hops += 1;
                    }
                    d.execute("UPDATE tasks SET after_id=?2 WHERE id=?1", params![id, dep])?;
                }
            }
        }
        drop(d);
        self.emit_tasks();
        Ok(())
    }

    /// Move a card. Nothing reaches "done" without proof.
    pub async fn task_move(self: &Arc<Self>, id: i64, col: &str, proof: Option<&str>) -> Result<()> {
        self.task_move_by(id, col, proof, None).await
    }

    /// Move a card, naming the squadmate who closed it (from the agents' door). Without one,
    /// a card moved to done is credited to whoever held it, or the commander.
    async fn task_move_by(self: &Arc<Self>, id: i64, col: &str, proof: Option<&str>, closer: Option<(&str, &str)>) -> Result<()> {
        if !["ideas", "doing", "wait", "done"].contains(&col) {
            bail!("Unknown column.");
        }
        let (title, crew_id, old_proof, old_col): (String, Option<String>, Option<String>, String) =
            self.db().query_row("SELECT title,crew_id,proof,col FROM tasks WHERE id=?1", [id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).map_err(|_| anyhow!("No such todo."))?;
        let given = proof.map(str::trim).filter(|p| !p.is_empty()).map(str::to_string);
        if let Some(p) = &given {
            if col == "done" && weak_proof(p) {
                bail!("That isn't proof. Give the test run, check or output that shows it works.");
            }
        }
        // A chest dug back up needs proving again.
        let proof = given.or(if old_col == "done" && col != "done" { None } else { old_proof });
        if col == "done" && proof.is_none() {
            bail!("No proof, no trophy. Add the test run, check or screenshot that proves it.");
        }
        if old_col == "wait" {
            if let Some(c) = &crew_id {
                self.captain_looked(c);
            }
        }
        // A card pulled back to Ideas comes off its pirate. A card sent back into Doing goes out
        // again from the queue, unless its pirate is still at work on it.
        let at_work = crew_id.as_deref().map(|c| self.live.lock().unwrap().get(c).map(|l| l.status == "working").unwrap_or(false)).unwrap_or(false);
        let mut who = if col == "ideas" || (col == "doing" && old_col != "doing" && !at_work) { None } else { crew_id };
        if let (Some((cid, _)), "done") = (closer, col) {
            who = Some(cid.to_string());
        }
        let by = match (&who, col) {
            (Some(c), "done") => db::crew_one(&self.db(), c).ok().flatten().map(|c| c.name),
            (None, "done") => Some("the commander".to_string()),
            _ => None,
        };
        self.db().execute(
            "UPDATE tasks SET col=?2, proof=?3, crew_id=?4, ask=CASE WHEN ?2='wait' THEN ask ELSE NULL END,
             finished=CASE WHEN ?2='done' THEN ?5 ELSE NULL END, by_name=?6 WHERE id=?1",
            params![id, col, proof, who, db::now(), by],
        )?;
        self.emit_tasks();
        if col == "done" && old_col != "done" {
            self.toast(format!("Job done: {}", clip(&title, 60)), "chest");
            let landed = self.land(who.as_deref(), id, &title);
            self.report_to_boss(id, &title, by.as_deref(), proof.as_deref(), landed.as_deref()).await;
        }
        if col == "doing" || col == "done" {
            // Doing puts it in the queue; done can free a job that was waiting behind this one.
            self.dispatch_queue().await;
        }
        Ok(())
    }

    /// A squadmate closed a job: the squad leader on that mission hears it with the proof,
    /// so he never has to poll the board.
    /// A job closed by a squadmate with their own checkout: commit it and merge it into the main
    /// checkout. Returns what happened, in words for the boss.
    fn land(&self, crew_id: Option<&str>, id: i64, title: &str) -> Option<String> {
        let crew = db::crew_one(&self.db(), crew_id?).ok().flatten()?;
        let main = self.main_of(&crew)?;
        let msg = format!("#{id} {}", clip(title, 72));
        Some(match worktree::land(Path::new(&main.path), &crew.name, &msg) {
            Ok(done) => done,
            Err(e) => {
                self.toast(format!("{}'s work for #{id} didn't merge. It's on {}.", crew.name, worktree::branch(&crew.name)), "warn");
                format!("Not merged: {e}. The work is on branch {}. Merge it yourself or ask {} to sort it out.", worktree::branch(&crew.name), crew.name)
            }
        })
    }

    async fn report_to_boss(self: &Arc<Self>, id: i64, title: &str, by: Option<&str>, proof: Option<&str>, landed: Option<&str>) {
        let island: Option<String> = self.db().query_row("SELECT island_id FROM tasks WHERE id=?1", [id], |r| r.get(0)).ok().flatten();
        let Some(island) = island else { return };
        let boss = db::crew(&self.db()).unwrap_or_default().into_iter().find(|c| c.island_id == island && c.role == BOSS);
        let Some(boss) = boss else { return };
        let who = by.unwrap_or("the commander");
        if who == boss.name {
            return;
        }
        let mut text = format!("[Report from {who}] #{id} {} is done. Their proof (check it before you rely on it): {}", clip(title, 120), clip(proof.unwrap_or(""), 400));
        if let Some(l) = landed {
            text.push_str(&format!(" Their checkout: {l}"));
        }
        // A boss caught in the net hears it when he's cut down, not now.
        if self.live.lock().unwrap().get(&boss.id).map(|l| l.breaker.held).unwrap_or(false) {
            self.add_msg(&boss.id, json!({"kind": "bottle", "text": text}));
            return;
        }
        if let Err(e) = self.say(&boss.id, &text, "bottle").await {
            self.add_msg(&boss.id, json!({"kind": "error", "text": format!("Couldn't pass a report to {}: {e}", boss.name)}));
        }
    }

    /// The captain answers a card that was waiting. The answer is kept on the card and goes
    /// back to the pirate who asked, or out with the job to the next free hand.
    pub async fn task_answer(self: &Arc<Self>, id: i64, text: &str) -> Result<()> {
        let (title, crew_id, col): (String, Option<String>, String) =
            self.db().query_row("SELECT title,crew_id,col FROM tasks WHERE id=?1", [id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).map_err(|_| anyhow!("No such todo."))?;
        if col != "wait" {
            bail!("That todo isn't waiting on you.");
        }
        let text = clip(text, 1000);
        if let Some(c) = &crew_id {
            self.captain_looked(c);
        }
        let same = crew_id.filter(|c| {
            let aboard = db::crew_one(&self.db(), c).ok().flatten().is_some();
            let busy: i64 = self.db().query_row("SELECT COUNT(*) FROM tasks WHERE col='doing' AND crew_id=?1", [c], |r| r.get(0)).unwrap_or(0);
            aboard && busy == 0 && self.live.lock().unwrap().get(c).map(|l| l.status == "idle" && !l.breaker.held).unwrap_or(true)
        });
        self.db().execute("UPDATE tasks SET col='doing', ask=NULL, answer=?2, crew_id=?3 WHERE id=?1", params![id, text, same])?;
        self.emit_tasks();
        if let Some(c) = same {
            let job = format!("[Commander's answer on job #{id}: {}] {text}\nCarry on. When it's done, run: ysob todo done {id} --proof \"<the test run or check that proves it>\"", clip(&title, 80));
            if let Err(e) = self.say(&c, &job, "job").await {
                // They can't take it: back in the queue, answer attached.
                let _ = self.db().execute("UPDATE tasks SET crew_id=NULL WHERE id=?1", [id]);
                self.emit_tasks();
                self.toast(format!("Couldn't reach them with the answer on #{id}: {e}"), "bell");
            }
        }
        self.dispatch_queue().await;
        Ok(())
    }

    /// A pirate's turn ended with their job still open. It doesn't sit in Doing looking busy:
    /// it goes to the captain with the reason.
    fn stalled(&self, crew_id: &str, why: &str) {
        let name = db::crew_one(&self.db(), crew_id).ok().flatten().map(|c| c.name).unwrap_or_else(|| "A squad member".into());
        let open: Vec<i64> = self
            .db()
            .prepare("SELECT id FROM tasks WHERE col='doing' AND crew_id=?1")
            .and_then(|mut st| st.query_map([crew_id], |r| r.get(0)).map(|rows| rows.flatten().collect()))
            .unwrap_or_default();
        for id in &open {
            let ask = format!("{name} stopped before finishing this. {}", clip(why, 400));
            let _ = self.db().execute("UPDATE tasks SET col='wait', ask=?2 WHERE id=?1", params![id, ask]);
            self.toast(format!("#{id} needs you: {name} stopped"), "bell");
        }
        if !open.is_empty() {
            self.emit_tasks();
        }
    }

    /// The captain's routing. Every todo in Doing with nobody on it is handed to a
    /// free pirate on its island; if nobody is free it waits, and this runs again
    /// whenever a pirate comes off a job. Returns how many jobs went out.
    pub async fn dispatch_queue(self: &Arc<Self>) -> usize {
        let mut sent = 0;
        loop {
            let pick = {
                let d = self.db();
                // Urgent jobs first, then oldest. A job waiting behind another stays put until that one is buried.
                let queued: Vec<(i64, String, Option<String>, Option<String>)> = d
                    .prepare(
                        "SELECT id,title,island_id,want FROM tasks t WHERE col='doing' AND crew_id IS NULL
                         AND (after_id IS NULL OR NOT EXISTS(SELECT 1 FROM tasks d WHERE d.id=t.after_id AND d.col!='done'))
                         ORDER BY pri DESC, id",
                    )
                    .and_then(|mut st| st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).map(|rows| rows.flatten().collect()))
                    .unwrap_or_default();
                let busy: Vec<String> = d
                    .prepare("SELECT crew_id FROM tasks WHERE col='doing' AND crew_id IS NOT NULL")
                    .and_then(|mut st| st.query_map([], |r| r.get::<_, String>(0)).map(|rows| rows.flatten().collect()))
                    .unwrap_or_default();
                let crew = db::crew(&d).unwrap_or_default();
                let live = self.live.lock().unwrap();
                let free = |c: &Crew| c.role != BOSS && !busy.contains(&c.id) && live.get(&c.id).map(|l| l.status == "idle" && !l.breaker.held).unwrap_or(true);
                // A job that needs hands goes to someone who can write before someone who can only look.
                let rank = |c: &Crew| (c.engine != "claude") as u8 * 2 + (c.lane == "READ") as u8;
                queued.into_iter().find_map(|(id, title, island, want)| {
                    // A job meant for someone waits for them; anyone else's job goes to the best free hand.
                    let mut able: Vec<&Crew> = crew
                        .iter()
                        .filter(|c| free(c) && island.as_deref().map(|i| i == c.island_id).unwrap_or(true) && want.as_deref().map(|w| w == c.id).unwrap_or(true))
                        .collect();
                    able.sort_by_key(|c| rank(c));
                    able.first().map(|c| (id, title, (*c).clone()))
                })
            };
            let Some((id, title, c)) = pick else { return sent };
            let _ = self.db().execute("UPDATE tasks SET crew_id=?2, started=?3 WHERE id=?1 AND crew_id IS NULL", params![id, c.id, db::now()]);
            self.emit_tasks();
            let (note, answer): (String, Option<String>) = self.db().query_row("SELECT note,answer FROM tasks WHERE id=?1", [id], |r| Ok((r.get(0)?, r.get(1)?))).unwrap_or_default();
            let mut job = format!("[Job #{id} from the board] {title}\n");
            if !note.trim().is_empty() {
                job.push_str(&format!("Notes from the commander: {}\n", note.trim()));
            }
            if let Some(a) = answer.filter(|a| !a.trim().is_empty()) {
                job.push_str(&format!("The commander already answered a question on this: {a}\n"));
            }
            if let Some(main) = self.main_of(&c) {
                if let Some(note) = worktree::sync(Path::new(&main.path), &c.name) {
                    job.push_str(&format!("{note}\n"));
                }
            }
            job.push_str(&format!(
                "If you can't finish it without the commander, run: ysob todo ask {id} \"<one clear question>\" and stop.\nWhen it's done, run: ysob todo done {id} --proof \"<the test run or check that proves it>\""
            ));
            match self.say(&c.id, &job, "job").await {
                Ok(()) => {
                    sent += 1;
                    self.toast(format!("{} picked up #{id}: {}", c.name, clip(&title, 50)), "compass");
                }
                Err(e) => {
                    // Couldn't hand it over: put it back in the queue and say why, once.
                    let _ = self.db().execute("UPDATE tasks SET crew_id=NULL WHERE id=?1", [id]);
                    self.emit_tasks();
                    self.toast(format!("Couldn't give #{id} to {}: {e}", c.name), "bell");
                    return sent;
                }
            }
        }
    }

    /// Run the routing from code that isn't async (the agent stream, a terminal thread).
    fn dispatch_soon(self: &Arc<Self>) {
        if let Ok(h) = tokio::runtime::Handle::try_current() {
            let hub = self.clone();
            h.spawn(async move {
                hub.dispatch_queue().await;
            });
        }
    }

    // ---------- talking to a pirate ----------

    pub async fn say(self: &Arc<Self>, crew_id: &str, text: &str, kind: &str) -> Result<()> {
        self.say_with(crew_id, text, kind, &[]).await
    }

    /// Say something, with pictures pasted into the message. Only Claude Code takes pictures:
    /// the other engines run in a terminal, where there is nowhere to put one.
    pub async fn say_with(self: &Arc<Self>, crew_id: &str, text: &str, kind: &str, images: &[Value]) -> Result<()> {
        let crew = db::crew_one(&self.db(), crew_id)?.ok_or_else(|| anyhow!("No such squad member."))?;
        if !images.is_empty() && crew.engine != "claude" {
            bail!("{} runs in a terminal, which can't take pictures. Only Claude Code chats can.", crew.name);
        }
        let island = db::island(&self.db(), &crew.island_id)?.ok_or_else(|| anyhow!("That squad member's mission is gone."))?;
        if self.live.lock().unwrap().get(crew_id).map(|l| l.breaker.held).unwrap_or(false) && kind == "me" {
            bail!("{} is held by the breaker. Let them carry on, or stop them, first.", crew.name);
        }
        let mut shown = json!({"kind": kind, "text": text});
        if !images.is_empty() {
            // The conversation keeps how many pictures went with the message, not the pictures.
            shown["images"] = json!(images.len());
        }
        self.add_msg(crew_id, shown);
        if kind == "me" || kind == "job" {
            self.db().execute("UPDATE crew SET task=?2 WHERE id=?1", params![crew_id, clip(text.lines().next().unwrap_or(""), 120)])?;
        }
        if crew.engine == "claude" {
            self.ensure_claude(&crew, &island).await?;
            let content = if images.is_empty() {
                json!(text)
            } else {
                let mut parts: Vec<Value> = images.to_vec();
                if !text.trim().is_empty() {
                    parts.push(json!({"type": "text", "text": text}));
                }
                json!(parts)
            };
            let line = json!({"type": "user", "message": {"role": "user", "content": content}}).to_string();
            let tx = self.live.lock().unwrap().get(crew_id).and_then(|l| l.stdin.clone()).ok_or_else(|| anyhow!("{} isn't running.", crew.name))?;
            tx.send(line).map_err(|_| anyhow!("{} has gone below. Try again.", crew.name))?;
            self.set_live(crew_id, Some("working"), Some(("chart", "thinking")));
        } else {
            self.ensure_pty(crew_id)?;
            let mut live = self.live.lock().unwrap();
            if let Some(p) = live.get_mut(crew_id).and_then(|l| l.pty.as_mut()) {
                p.seen = db::now();
                p.writer.write_all(text.as_bytes())?;
                p.writer.write_all(b"\r")?;
                p.writer.flush()?;
            }
            drop(live);
            self.set_live(crew_id, Some("working"), Some(("work", "working")));
        }
        Ok(())
    }

    fn brief(&self, crew: &Crew, island: &Island) -> String {
        // Before taking the hold: wants_worktree reads a setting.
        let own = if self.wants_worktree(crew, island) {
            format!(
                "You work in your own checkout of the code, on branch {br}, so nobody else's edits get in your way. When you close a job with proof, the engine commits your work and merges it into the main checkout, and your copy takes the latest before each new job. Don't commit, merge or switch branches yourself.\n",
                br = worktree::branch(&crew.name)
            )
        } else {
            String::new()
        };
        let d = self.db();
        let mates = db::crew(&d).unwrap_or_default().into_iter().filter(|c| c.id != crew.id && c.island_id == crew.island_id).map(|c| format!("{} ({})", c.name, if c.role == BOSS { "squad leader" } else { c.role.as_str() })).collect::<Vec<_>>().join(", ");
        let mut log = String::new();
        // db::log is newest first. The brief carries a page of it, not the whole book.
        let mut briefed = 0;
        for e in db::log(&d).unwrap_or_default() {
            let isl = e["island_id"].as_str();
            if briefed < BRIEF_LOG_ENTRIES && (isl.is_none() || isl == Some(island.id.as_str())) {
                briefed += 1;
                let strikes = e["strikes"].as_i64().unwrap_or(0);
                log.push_str(&format!("- {}: {}{}{}\n", e["kind"].as_str().unwrap_or(""), e["title"].as_str().unwrap_or(""),
                    if e["detail"].as_str().unwrap_or("").is_empty() { String::new() } else { format!(" ({})", e["detail"].as_str().unwrap_or("")) },
                    if strikes > 0 { format!(" [{strikes} strike(s)]") } else { String::new() }));
            }
        }
        let lane = match crew.lane.as_str() {
            "READ" => "LOOK: you may read this mission's folder. Edits and commands radio the commander for you.",
            "WRITE" => "EDIT: you may edit files in this mission's folder and run tests and read-only git. Other commands radio the commander for you.",
            _ => "FULL: you may edit files in this mission's folder and run commands.",
        };
        let roster = self.roster.lock().unwrap().iter().map(|r| format!("{} ({}: {})", r.name, r.job.to_lowercase(), r.does)).collect::<Vec<_>>().join(", ");
        let boss = if crew.role == BOSS {
            format!(
                "You are the squad leader. The commander talks to you and you decide who does what.\n\
                 - Quick or small jobs: do them yourself.\n\
                 - Bigger jobs: split them into todos with `ysob todo add`, one clear outcome each, `--after <id>` where order matters. Your todos go straight out to free squadmates, and each one must be proven done. Add `--to <name>` to give a job to one squadmate, for example the tracker for reading and research.\n\
                 - Direct word with one squadmate: `ysob order <name> \"<message>\"`.\n\
                 - Never wait in a loop or poll the board. When a squadmate closes a job, the engine sends you their report with the proof. End your turn and pick up from the report.\n\
                 - Need more hands: `ysob squad add <name>` brings someone onto this mission. The roster: {roster}. Only add who the job needs.\n\
                 - Keep the commander posted in short, plain updates, and report back when the work is proven done.\n"
            )
        } else {
            String::new()
        };
        format!(
            "You are {name}, {role} in the YSOB squad. Your mission is \"{island}\", which is this folder.\n\
             Your access is {lane}\n\
             {own}\
             Every tool call goes through the YSOB engine. When a job needs something outside your access, just make the tool call: the engine radios the commander and holds your call until they answer. Never ask for permission in chat first.\n\
             If the engine says no, don't try another way round: say what you need in one line and stop.\n\
             If the mission has a CLAUDE.md or AGENTS.md, read it before you start: it holds the project's own conventions.\n\
             The `ysob` command is your radio:\n\
             - `ysob todo add \"<title>\"` adds a todo to the mission board.\n\
             - `ysob todo add \"<title>\" --after <id>` waits until todo <id> is done. `--urgent` puts it at the front. `--note \"<detail>\"` adds what the next person needs to know.\n\
             - `ysob todo done <id> --proof \"<evidence>\"` closes a finished job. Evidence is required: the test run, check or output that proves it. Never claim done without it.\n\
             - `ysob todo ask <id> \"<question>\"` when you can't finish without the commander. Ask one clear question, then stop: the answer comes back to you.\n\
             - `ysob todo list` shows this mission's board. `ysob todo show <id>` shows one todo in full.\n\
             - `ysob order <name> '<message>'` sends word to a squadmate on this mission. Only when you need something from them.\n\
             Quote `ysob` messages and notes with single quotes and leave out backticks and $: inside double quotes the shell runs them, so the engine stops and asks.\n\
             {boss}\
             Squad on this mission when you started: {mates}. People join and leave; run `ysob squad` for who is here now.\n\
             Mission log (rules, decisions and past mistakes). Follow it:\n{log}\
             Be brief and plain. Say what you did and what you found. Never say a job is done unless a test run or check proves it.",
            name = crew.name, role = if crew.role == BOSS { "the squad leader" } else { crew.role.as_str() }, island = island.name, lane = lane, boss = boss, own = own,
            mates = if mates.is_empty() { "none yet".into() } else { mates },
            log = if log.is_empty() { "- (nothing yet)\n".to_string() } else { log },
        )
    }

    /// True when this squadmate gets a checkout of their own: they edit, they aren't the boss
    /// (who works in the main checkout and merges), and the mission is a git repo.
    fn wants_worktree(&self, crew: &Crew, island: &Island) -> bool {
        crew.role != BOSS && crew.lane != "READ" && db::setting(&self.db(), "worktrees").as_deref() != Some("off") && worktree::is_repo(Path::new(&island.path))
    }

    /// The mission as this squadmate sees it: their own checkout when they have one.
    fn workdir(&self, crew: &Crew, island: &Island) -> Island {
        let wt = worktree::path(Path::new(&island.path), &crew.name);
        if self.wants_worktree(crew, island) && wt.join(".git").is_file() {
            return Island { path: wt.display().to_string(), ..island.clone() };
        }
        island.clone()
    }

    /// Like `workdir`, but makes the checkout if it isn't there yet. On failure the squadmate
    /// works in the main checkout and the user is told.
    fn prepare_workdir(&self, crew: &Crew, island: &Island) -> Island {
        if self.wants_worktree(crew, island) {
            if let Err(e) = worktree::ensure(Path::new(&island.path), &crew.name) {
                self.toast(format!("{} shares the main checkout: couldn't make their own ({e}).", crew.name), "warn");
            }
        }
        self.workdir(crew, island)
    }

    /// The main mission, when this squadmate works in a checkout of their own.
    fn main_of(&self, crew: &Crew) -> Option<Island> {
        let island = db::island(&self.db(), &crew.island_id).ok().flatten()?;
        (self.workdir(crew, &island).path != island.path).then_some(island)
    }

    /// The sandbox arguments for this agent, or None when it runs uncaged: the HOST lane, the
    /// cage switched off in settings, or no bubblewrap here (the user is told once per start).
    /// What an agent's process needs in its environment: `ysob` first on PATH, the engine's
    /// door and the agent's own token, and the sign-in variables of its engine only.
    fn agent_env(&self, crew: &Crew, tok: &str) -> Result<Vec<(String, String)>> {
        let mut dirs: Vec<PathBuf> = self.bin.parent().map(|p| vec![p.to_path_buf()]).unwrap_or_default();
        dirs.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
        let door = self.door.get().cloned().unwrap_or_else(|| self.sock.display().to_string());
        let mut env = vec![
            ("PATH".to_string(), std::env::join_paths(dirs)?.to_string_lossy().into_owned()),
            ("YSOB_SOCK".to_string(), door),
            ("YSOB_AGENT".to_string(), crew.id.clone()),
            ("YSOB_TOKEN".to_string(), tok.to_string()),
        ];
        let first = crew.engine.split_whitespace().next().unwrap_or("");
        if let Some(spec) = detect::registry(&self.data_dir).into_iter().find(|e| e.id == first || e.bin == first) {
            for v in spec.auth_env {
                if let Ok(val) = std::env::var(&v) {
                    env.push((v, val));
                }
            }
        }
        if first == "claude" {
            for v in ["ANTHROPIC_BASE_URL", "ANTHROPIC_MODEL", "CLAUDE_CODE_USE_BEDROCK", "CLAUDE_CODE_USE_VERTEX"] {
                if let Ok(val) = std::env::var(v) {
                    env.push((v.to_string(), val));
                }
            }
        }
        Ok(env)
    }

    /// The sandbox for this squadmate, or None when they run uncaged: bwrap's arguments, and the
    /// whole environment to start bwrap with (never on its command line: see sandbox::env).
    #[allow(clippy::type_complexity)]
    fn cage(&self, crew: &Crew, island: &Island, engine: &str, prog: &Path, pty: bool, env: &[(String, String)]) -> Result<Option<(Vec<std::ffi::OsString>, Vec<(std::ffi::OsString, std::ffi::OsString)>)>> {
        let lane = Lane::parse(&crew.lane).unwrap_or(Lane::Read);
        if lane == Lane::Host || db::setting(&self.db(), "sandbox").as_deref() == Some("off") {
            return Ok(None);
        }
        if !sandbox::available() {
            self.toast(format!("{} runs without a sandbox: {}.", crew.name, sandbox::missing_reason()), "warn");
            return Ok(None);
        }
        let home = std::env::var_os("HOME").map(PathBuf::from).ok_or_else(|| anyhow!("No HOME, so the sandbox can't be built."))?;
        let agent_home = self.data_dir.join("agents").join(&crew.id).join("home");
        let programs = [prog.to_path_buf(), self.bin.clone()];
        // A worktree links the main checkout's installed dependencies; the cage shows them read-only.
        let share = self.main_of(crew).map(|m| worktree::shared(Path::new(&m.path))).unwrap_or_default();
        // A worktree agent's git folders come from the engine, never from its own `.git` file.
        let git = self.main_of(crew).map(|m| worktree::gitdirs(Path::new(&m.path), &crew.name));
        let spec = sandbox::Spec {
            lane, engine, mission: Path::new(&island.path), home: &home, agent_home: &agent_home, data_dir: &self.data_dir,
            programs: &programs, sock: (!cfg!(windows)).then_some(self.sock.as_path()), pty, share: &share, git, env,
        };
        Ok(Some((sandbox::args(&spec)?, sandbox::env(&spec))))
    }

    async fn ensure_claude(self: &Arc<Self>, crew: &Crew, island: &Island) -> Result<()> {
        if self.live.lock().unwrap().get(&crew.id).map(|l| l.stdin.is_some()).unwrap_or(false) {
            return Ok(());
        }
        let brief = self.brief(crew, island);
        let island = &self.prepare_workdir(crew, island);
        // The agent's settings live in the hold, which no lane can write to.
        let dir = self.data_dir.join("agents").join(&crew.id);
        std::fs::create_dir_all(&dir)?;
        let hook = hook_command(&self.bin);
        let settings = json!({"hooks": {"PreToolUse": [{"matcher": "*", "hooks": [{"type": "command", "command": hook, "timeout": ASK_TIMEOUT_SECS + 60}]}]}});
        // Passed inline, not as a file: the hold is out of sight inside the cage, and a settings
        // file the agent could reach is one it could rewrite to drop its own hook.
        let settings = settings.to_string();

        let tok = token();
        // Windows won't find `claude.cmd` on its own; resolve it the way a shell would.
        let claude = which("claude").unwrap_or_else(|| PathBuf::from("claude"));
        let env = self.agent_env(crew, &tok)?;
        let caged = self.cage(crew, island, "claude", &claude, false, &env)?;
        let mut cmd = match &caged {
            Some((wrap, cenv)) => {
                let mut c = tokio::process::Command::new("bwrap");
                c.args(wrap).arg(&claude).env_clear().envs(cenv.iter().map(|(k, v)| (k, v)));
                c
            }
            None => tokio::process::Command::new(&claude),
        };
        cmd.arg("-p").args(["--input-format", "stream-json", "--output-format", "stream-json", "--verbose", "--include-partial-messages"])
            .arg("--settings").arg(&settings)
            // No settings from disk. A mission's own .claude/settings.json is the repo's to write:
            // in -p mode it would apply with no trust prompt (its hooks run commands, its env can
            // send the sign-in elsewhere). The guard above comes in through --settings instead,
            // and the brief points the agent at the project's CLAUDE.md / AGENTS.md as reading.
            .args(["--setting-sources", ""])
            .args(["--strict-mcp-config", "--mcp-config", "{\"mcpServers\":{}}"])
            .arg("--append-system-prompt").arg(brief);
        if let Some(sid) = &crew.session_id {
            cmd.args(["--resume", sid]);
        }
        if let Ok(model) = std::env::var("YSOB_CLAUDE_MODEL") {
            cmd.args(["--model", &model]);
        }
        // Uncaged, the variables go on the process; caged, bwrap was given its whole environment.
        if caged.is_none() {
            for (k, v) in &env {
                cmd.env(k, v);
            }
        }
        cmd.current_dir(&island.path)
            .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        let mut child = cmd.spawn().map_err(|e| anyhow!("Couldn't start claude: {e}. Is Claude Code installed and on your PATH?"))?;
        let mut stdin = child.stdin.take().ok_or_else(|| anyhow!("no stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow!("no stdout"))?;
        let stderr = child.stderr.take().ok_or_else(|| anyhow!("no stderr"))?;
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let (kill_tx, kill_rx) = oneshot::channel::<()>();
        {
            let mut live = self.live.lock().unwrap();
            let l = live.entry(crew.id.clone()).or_insert_with(Live::new);
            l.token = tok;
            l.stdin = Some(tx);
            l.kill = Some(kill_tx);
            l.last_cost = 0.0;
            l.tools.clear();
        }
        tokio::spawn(async move {
            while let Some(line) = rx.recv().await {
                if stdin.write_all(line.as_bytes()).await.is_err() || stdin.write_all(b"\n").await.is_err() || stdin.flush().await.is_err() {
                    break;
                }
            }
        });
        let hub = self.clone();
        let id = crew.id.clone();
        let island_path = island.path.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Ok(v) = serde_json::from_str::<Value>(&line) {
                    hub.on_claude(&id, &island_path, v);
                }
            }
        });
        let hub = self.clone();
        let id = crew.id.clone();
        let name = crew.name.clone();
        tokio::spawn(async move {
            let mut err = String::new();
            let mut lines = BufReader::new(stderr).lines();
            let waiter = async {
                while let Ok(Some(l)) = lines.next_line().await {
                    if err.len() < 2000 {
                        err.push_str(&l);
                        err.push('\n');
                    }
                }
                child.wait().await
            };
            tokio::select! {
                status = waiter => {
                    let ok = status.map(|s| s.success()).unwrap_or(false);
                    if !ok && !err.trim().is_empty() {
                        hub.add_msg(&id, json!({"kind": "error", "text": format!("{name} went below: {}", clip(&err, 400))}));
                    }
                }
                _ = kill_rx => {}
            }
            {
                let mut live = hub.live.lock().unwrap();
                if let Some(l) = live.get_mut(&id) {
                    l.stdin = None;
                    l.kill = None;
                    if l.status == "working" {
                        l.status = "idle".into();
                        l.station = "rest".into();
                        l.label = "off watch".into();
                    }
                }
            }
            hub.emit_crew(&id);
        });
        Ok(())
    }

    /// Append to (Some) or clear (None) the reply being typed, and tell the UI.
    fn set_stream(self: &Arc<Self>, id: &str, more: Option<&str>) {
        let text = {
            let mut live = self.live.lock().unwrap();
            let Some(l) = live.get_mut(id) else { return };
            match more {
                Some(t) => {
                    l.stream.push_str(t);
                    // At most 20 updates a second; the finished message always lands in full.
                    let now = std::time::Instant::now();
                    if l.stream_sent.map(|t| now.duration_since(t) < std::time::Duration::from_millis(50)).unwrap_or(false) {
                        // Held back: send whatever is there a moment later, so a pause in the
                        // reply never leaves the last words unsent.
                        if !l.stream_pending {
                            l.stream_pending = true;
                            if let Ok(rt) = tokio::runtime::Handle::try_current() {
                                let (hub, id) = (self.clone(), id.to_string());
                                rt.spawn(async move {
                                    tokio::time::sleep(std::time::Duration::from_millis(60)).await;
                                    let text = {
                                        let mut live = hub.live.lock().unwrap();
                                        let Some(l) = live.get_mut(&id) else { return };
                                        l.stream_pending = false;
                                        if l.stream.is_empty() {
                                            return;
                                        }
                                        l.stream_sent = Some(std::time::Instant::now());
                                        l.stream.clone()
                                    };
                                    hub.emit("stream", json!({"id": id, "text": text}));
                                });
                            }
                        }
                        return;
                    }
                    l.stream_sent = Some(now);
                }
                None if l.stream.is_empty() => return,
                None => {
                    l.stream.clear();
                    l.stream_sent = None;
                }
            }
            l.stream.clone()
        };
        self.emit("stream", json!({"id": id, "text": text}));
    }

    /// One line of Claude Code's stream.
    fn on_claude(self: &Arc<Self>, id: &str, island_path: &str, v: Value) {
        match v["type"].as_str().unwrap_or("") {
            "system" if v["subtype"] == "init" => {
                if let Some(sid) = v["session_id"].as_str() {
                    let _ = self.db().execute("UPDATE crew SET session_id=?2 WHERE id=?1", params![id, sid]);
                }
                // How this agent is paid for. Signed in to a subscription (Claude Pro or Max) it
                // reports no API key, and its dollar figures are only what the turn would cost at
                // API prices: they never count against the budget.
                if let Some(src) = v["apiKeySource"].as_str() {
                    let plan = src == "none";
                    let key = format!("billing.{id}");
                    if db::setting(&self.db(), &key).as_deref() != Some(if plan { "plan" } else { "api" }) {
                        let _ = db::set_setting(&self.db(), &key, if plan { "plan" } else { "api" });
                        self.emit_crew(id);
                    }
                }
            }
            // Partial messages: the reply as it is typed. Text only; tool calls arrive whole.
            "stream_event" => {
                let ev = &v["event"];
                match ev["type"].as_str().unwrap_or("") {
                    "message_start" => self.set_stream(id, None),
                    "content_block_delta" if ev["delta"]["type"] == "text_delta" => {
                        if let Some(t) = ev["delta"]["text"].as_str() {
                            self.set_stream(id, Some(t));
                        }
                    }
                    _ => {}
                }
            }
            "assistant" => {
                // The finished message replaces whatever was streamed.
                self.set_stream(id, None);
                if let Some(u) = v["message"]["usage"].as_object() {
                    let n = |k: &str| u.get(k).and_then(Value::as_f64).unwrap_or(0.0);
                    let tokens = n("input_tokens") + n("cache_read_input_tokens") + n("cache_creation_input_tokens");
                    let window = {
                        let mut live = self.live.lock().unwrap();
                        live.get_mut(id).map(|l| {
                            l.ctx_tokens = tokens;
                            l.ctx_window
                        })
                    };
                    let ctx = ctx_share(tokens, window.unwrap_or(0.0));
                    let _ = self.db().execute("UPDATE crew SET ctx=?2 WHERE id=?1", params![id, ctx]);
                }
                for c in v["message"]["content"].as_array().cloned().unwrap_or_default() {
                    match c["type"].as_str().unwrap_or("") {
                        "text" => {
                            let t = c["text"].as_str().unwrap_or("").trim();
                            if !t.is_empty() {
                                self.add_msg(id, json!({"kind": "text", "text": t}));
                                if let Some(l) = self.live.lock().unwrap().get_mut(id) {
                                    l.last_text = t.to_string();
                                }
                            }
                        }
                        "tool_use" => {
                            let name = c["name"].as_str().unwrap_or("?").to_string();
                            let input = &c["input"];
                            let mut body = json!({"kind": "tool", "verb": verb(&name), "name": name, "arg": tool_arg(&name, input, island_path)});
                            if name == "Edit" {
                                let cut = |k: &str| input[k].as_str().unwrap_or("").lines().take(8).map(|l| clip(l, 140)).collect::<Vec<_>>();
                                body["diff"] = json!({"del": cut("old_string"), "add": cut("new_string")});
                            }
                            if name == "TodoWrite" {
                                body["todos"] = todos(input);
                            }
                            let mid = self.add_msg(id, body);
                            let (st, label) = station_for(&name, input);
                            {
                                let mut live = self.live.lock().unwrap();
                                if let Some(l) = live.get_mut(id) {
                                    l.tools.insert(c["id"].as_str().unwrap_or("").to_string(), (mid, name));
                                    l.turn_tools += 1;
                                    if l.status == "working" {
                                        l.station = st.into();
                                        l.label = label.into();
                                    }
                                }
                            }
                            self.emit_crew(id);
                        }
                        _ => {}
                    }
                }
            }
            "user" => {
                for c in v["message"]["content"].as_array().cloned().unwrap_or_default() {
                    if c["type"] != "tool_result" {
                        continue;
                    }
                    let is_err = c["is_error"].as_bool().unwrap_or(false);
                    let text = match &c["content"] {
                        Value::String(t) => t.clone(),
                        Value::Array(parts) => parts.iter().filter_map(|p| p["text"].as_str()).collect::<Vec<_>>().join("\n"),
                        _ => String::new(),
                    };
                    let from_lighthouse = text.starts_with("PreToolUse:");
                    let entry = self.live.lock().unwrap().get_mut(id).and_then(|l| l.tools.remove(c["tool_use_id"].as_str().unwrap_or("")));
                    let Some((mid, name)) = entry else { continue };
                    if is_err || from_lighthouse {
                        if let Some(l) = self.live.lock().unwrap().get_mut(id) {
                            l.turn_fail += 1;
                        }
                    }
                    let res = if from_lighthouse { text.split_once("hook error: ").map(|x| x.1).unwrap_or(&text).to_string() } else { summarise(&name, &text, is_err) };
                    // Take each lock in its own statement: a guard made inside an
                    // `if let` condition lives for the whole block and would deadlock.
                    let mut all = db::messages(&self.db(), id, 60).unwrap_or_default();
                    if let Some(m) = all.iter_mut().find(|m| m["id"] == json!(mid)) {
                        m["ok"] = json!(!is_err);
                        m["res"] = json!(res);
                        m["blocked"] = json!(from_lighthouse);
                        let _ = db::update_message(&self.db(), mid, m);
                        self.emit("msg_update", json!({"crew_id": id, "msg": m}));
                    }
                    if from_lighthouse {
                        continue;
                    }
                    let tripped = {
                        let on = db::setting(&self.db(), "breaker").as_deref() == Some("1");
                        let mut live = self.live.lock().unwrap();
                        match live.get_mut(id) {
                            Some(l) if is_err && on => l.breaker.fail(&name, &text),
                            Some(l) => {
                                if !is_err {
                                    l.breaker.ok();
                                }
                                false
                            }
                            None => false,
                        }
                    };
                    if tripped {
                        self.hold(id, &name, &text);
                    }
                }
            }
            "result" => {
                self.set_stream(id, None);
                let total = v["total_cost_usd"].as_f64().unwrap_or(0.0);
                let (delta, quiet) = {
                    let mut live = self.live.lock().unwrap();
                    match live.get_mut(id) {
                        Some(l) => {
                            let d = (total - l.last_cost).max(0.0);
                            l.last_cost = total;
                            (d, l.status == "working")
                        }
                        None => (0.0, false),
                    }
                };
                // Only an agent paying by API key spends real money; a subscription's figure is
                // shown as what it would have cost, and kept out of the budget.
                let plan = self.on_plan(id);
                if delta > 0.0 && !plan {
                    let _ = db::add_spend(&self.db(), id, delta);
                    self.emit("spent", json!(db::spent_today(&self.db())));
                }
                let failed = v["is_error"].as_bool().unwrap_or(false);
                if failed {
                    let why = v["result"].as_str().unwrap_or("The turn ended with an error.");
                    self.add_msg(id, json!({"kind": "error", "text": clip(why, 400)}));
                }
                // The model's real context window: a 1M-token model isn't full at 200k.
                let window = v["modelUsage"].as_object().map(|m| m.values().filter_map(|u| u["contextWindow"].as_f64()).fold(0.0, f64::max)).unwrap_or(0.0);
                let (tools, fails, tokens) = {
                    let mut live = self.live.lock().unwrap();
                    match live.get_mut(id) {
                        Some(l) => {
                            if window > 0.0 {
                                l.ctx_window = window;
                            }
                            let out = (l.turn_tools, l.turn_fail, l.ctx_tokens);
                            l.turn_tools = 0;
                            l.turn_fail = 0;
                            out
                        }
                        None => (0, 0, 0.0),
                    }
                };
                if window > 0.0 && tokens > 0.0 {
                    let _ = self.db().execute("UPDATE crew SET ctx=?2 WHERE id=?1", params![id, ctx_share(tokens, window)]);
                }
                // One quiet line under the turn: how long it took, what it did, what it cost.
                if let Some(ms) = v["duration_ms"].as_u64() {
                    self.add_msg(id, json!({"kind": "turn", "ms": ms, "tools": tools, "failed": fails, "cost": delta, "plan": plan}));
                }
                if quiet {
                    // The turn is over. A job still open goes to the captain with the pirate's last word.
                    let last = self.live.lock().unwrap().get_mut(id).map(|l| std::mem::take(&mut l.last_text)).unwrap_or_default();
                    let why = if failed {
                        format!("It failed: {}", v["result"].as_str().unwrap_or("the turn ended with an error."))
                    } else if last.is_empty() {
                        "They said nothing.".to_string()
                    } else {
                        format!("Their last word: \"{last}\"")
                    };
                    self.stalled(id, &why);
                    self.set_live(id, Some("idle"), Some(("rest", "off watch")));
                    self.dispatch_soon();
                } else {
                    self.emit_crew(id);
                }
            }
            _ => {}
        }
    }

    // ---------- the ysob ----------

    fn hold(self: &Arc<Self>, id: &str, tool: &str, error: &str) {
        let (crew, quote) = {
            let d = self.db();
            let crew = db::crew_one(&d, id).ok().flatten();
            let quote = if db::setting(&d, "quote_log").as_deref() == Some("1") { crew.as_ref().and_then(|c| matching_mistake(&d, &c.island_id, error)) } else { None };
            (crew, quote)
        };
        let Some(crew) = crew else { return };
        if let Some((log_id, _)) = &quote {
            let _ = self.db().execute("UPDATE log SET strikes=strikes+1 WHERE id=?1", [log_id]);
            self.emit_log();
        }
        let detail = match &quote {
            Some((_, q)) => format!("Same failure three times: {}. The log says: \"{q}\"", clip(error, 140)),
            None => format!("Same failure three times with {tool}: {}", clip(error, 180)),
        };
        self.needs.lock().unwrap().push(Need {
            id: self.next_need.fetch_add(1, Ordering::SeqCst), crew_id: id.into(), kind: "breaker",
            title: format!("{} is caught in the net", crew.name), detail: detail.clone(), grant: None, tx: None,
        });
        self.add_msg(id, json!({"kind": "breaker", "text": detail, "quote": quote.map(|q| q.1)}));
        self.set_live(id, Some("held"), Some(("brig", "caught in the net")));
        self.emit("needs", self.needs_view());
    }

    fn drop_needs(&self, crew_id: &str) {
        let mut needs = self.needs.lock().unwrap();
        for n in needs.iter_mut().filter(|n| n.crew_id == crew_id) {
            if let Some(tx) = n.tx.take() {
                let _ = tx.send("deny".into());
            }
        }
        needs.retain(|n| n.crew_id != crew_id);
        drop(needs);
        self.emit("needs", self.needs_view());
    }

    async fn answer(self: &Arc<Self>, need_id: u64, answer: &str) -> Result<Value> {
        let need = {
            let mut needs = self.needs.lock().unwrap();
            let i = needs.iter().position(|n| n.id == need_id).ok_or_else(|| anyhow!("That bell was already answered."))?;
            needs.remove(i)
        };
        self.emit("needs", self.needs_view());
        let id = need.crew_id.clone();
        match (need.kind, answer) {
            ("bell", "grant") | ("bell", "always") | ("bell", "deny") => {
                if answer == "always" {
                    if let Some(g) = &need.grant {
                        let _ = self.db().execute("INSERT OR IGNORE INTO grants(crew_id,grant_json) VALUES(?1,?2)", params![id, serde_json::to_string(g)?]);
                    }
                }
                let text = match answer {
                    "grant" => "Granted once.",
                    "always" => "Granted for this squadmate from now on.",
                    _ => "Denied.",
                };
                self.add_msg(&id, json!({"kind": "bell", "text": need.title, "detail": need.detail, "answer": text}));
                if let Some(tx) = need.tx {
                    let _ = tx.send(answer.into());
                }
            }
            ("breaker", "release") | ("breaker", "stop") => {
                {
                    let mut live = self.live.lock().unwrap();
                    if let Some(l) = live.get_mut(&id) {
                        l.breaker.release();
                    }
                }
                self.set_live(&id, Some("idle"), Some(("rest", "off watch")));
                if answer == "release" {
                    self.say(&id, &format!("[Commander] You've been cut down. {} Fix that first, then carry on.", need.detail), "job").await?;
                } else {
                    self.add_msg(&id, json!({"kind": "bell", "text": need.title, "detail": need.detail, "answer": "Job stopped."}));
                }
            }
            _ => bail!("That isn't an answer to this."),
        }
        Ok(json!({}))
    }

    // ---------- hooks: every Claude tool call lands here ----------

    pub async fn hook(self: &Arc<Self>, agent: &str, tok: &str, ev: &Value) -> (bool, String) {
        let known = self.live.lock().unwrap().get(agent).map(|l| l.token == tok && !tok.is_empty()).unwrap_or(false);
        if !known {
            return (false, "The engine doesn't know you.".into());
        }
        if ev["hook_event_name"] != "PreToolUse" {
            return (true, String::new());
        }
        let tool = ev["tool_name"].as_str().unwrap_or("");
        let input = &ev["tool_input"];
        let (crew, island, grants, over_budget) = {
            let d = self.db();
            let Some(crew) = db::crew_one(&d, agent).ok().flatten() else { return (false, "You're not in the squad.".into()) };
            let Some(island) = db::island(&d, &crew.island_id).ok().flatten() else { return (false, "Your mission is gone.".into()) };
            let grants: Vec<Grant> = d
                .prepare("SELECT grant_json FROM grants WHERE crew_id=?1")
                .and_then(|mut s| s.query_map([agent], |r| r.get::<_, String>(0)).map(|rows| rows.flatten().collect::<Vec<_>>()))
                .unwrap_or_default()
                .iter()
                .filter_map(|g| serde_json::from_str(g).ok())
                .collect();
            let budget = db::setting(&d, "budget").and_then(|b| b.parse::<f64>().ok()).unwrap_or(18.0);
            let over = db::setting(&d, "hard_stop").as_deref() == Some("1") && db::spent_today(&d) >= budget;
            (crew, island, grants, over)
        };
        if over_budget {
            return (false, "Today's budget is spent. The whole squad is stood down until the commander raises it.".into());
        }
        if self.live.lock().unwrap().get(agent).map(|l| l.breaker.held).unwrap_or(false) {
            return (false, "The breaker is holding you. Stop and wait for the commander.".into());
        }
        let Some(lane) = Lane::parse(&crew.lane) else { return (false, "Your lane is unreadable.".into()) };
        let island = self.workdir(&crew, &island);
        let ctx = lanes::Ctx { lane, island: Path::new(&island.path), data_dir: &self.data_dir, grants: &grants };
        match lanes::decide(&ctx, tool, input) {
            Verdict::Allow => {
                let (st, label) = station_for(tool, input);
                self.set_live(agent, Some("working"), Some((st, label)));
                (true, String::new())
            }
            Verdict::Deny(why) => (false, why),
            Verdict::Ask { what, grant } => {
                let (tx, rx) = oneshot::channel();
                let need_id = self.next_need.fetch_add(1, Ordering::SeqCst);
                self.needs.lock().unwrap().push(Need {
                    id: need_id, crew_id: agent.into(), kind: "bell", title: format!("{} wants to {}", crew.name, what),
                    detail: format!("{} is on the {} lane on {}.", crew.name, lane_word(&crew.lane), island.name), grant: Some(grant), tx: Some(tx),
                });
                self.set_live(agent, Some("waiting"), Some(("cabin", "needs you")));
                self.emit("needs", self.needs_view());
                self.toast(format!("Bell: {} wants to {}", crew.name, clip(&what, 60)), "bell");
                let ans = tokio::time::timeout(std::time::Duration::from_secs(ASK_TIMEOUT_SECS), rx).await;
                let out = match ans {
                    Ok(Ok(a)) if a == "grant" || a == "always" => (true, String::new()),
                    Ok(_) => (false, "The commander said no. Don't try another way round.".into()),
                    Err(_) => {
                        self.needs.lock().unwrap().retain(|n| n.id != need_id);
                        self.emit("needs", self.needs_view());
                        (false, "No answer from the commander. Stop and say what you need.".to_string())
                    }
                };
                let (st, label) = if out.0 { station_for(tool, input) } else { ("chart", "thinking") };
                self.set_live(agent, Some("working"), Some((st, label)));
                out
            }
        }
    }

    // ---------- the `ysob` command, run by agents ----------

    pub async fn cli(self: &Arc<Self>, agent: &str, tok: &str, args: &[String]) -> Result<String> {
        let known = self.live.lock().unwrap().get(agent).map(|l| l.token == tok && !tok.is_empty()).unwrap_or(false);
        if !known {
            bail!("The engine doesn't know you.");
        }
        let me = db::crew_one(&self.db(), agent)?.ok_or_else(|| anyhow!("You're not in the squad."))?;
        let a: Vec<&str> = args.iter().map(String::as_str).collect();
        match a.as_slice() {
            ["todo", "add", rest @ ..] if !rest.is_empty() => {
                // Words up to the first flag are the title. --note takes everything after it.
                let (mut title, mut edit, mut i) = (vec![], json!({}), 0);
                while i < rest.len() {
                    match rest[i] {
                        "--urgent" => edit["pri"] = json!(1),
                        "--after" => {
                            i += 1;
                            let dep: i64 = rest.get(i).and_then(|x| x.trim_start_matches('#').parse().ok()).ok_or_else(|| anyhow!("--after needs a todo number."))?;
                            edit["after"] = json!(dep);
                        }
                        "--to" => {
                            i += 1;
                            let who = rest.get(i).ok_or_else(|| anyhow!("--to needs a squadmate's name."))?;
                            let mate = db::crew_by_name(&self.db(), &me.island_id, who)?.ok_or_else(|| anyhow!("Nobody called {who} on this mission. Bring them in first with `ysob squad add {who}`."))?;
                            if mate.role == BOSS || mate.id == me.id {
                                bail!("--to gives a job to a squadmate, not to the squad leader or yourself.");
                            }
                            edit["want"] = json!(mate.id);
                        }
                        "--note" => {
                            // The note runs until the next flag, so flags after it still count.
                            let mut words = vec![];
                            while i + 1 < rest.len() && !matches!(rest[i + 1], "--urgent" | "--after" | "--to" | "--note") {
                                i += 1;
                                words.push(rest[i]);
                            }
                            edit["note"] = json!(words.join(" "));
                        }
                        w if w.starts_with("--") => bail!("Unknown option {w}. Options: --to <name>, --after <id>, --urgent, --note \"<detail>\"."),
                        w => title.push(w),
                    }
                    i += 1;
                }
                if let Some(dep) = edit["after"].as_i64() {
                    if self.task_island(dep)? != Some(me.island_id.clone()) {
                        bail!("Todo #{dep} isn't on your mission.");
                    }
                }
                let id = self.task_add(&title.join(" "), Some(&me.island_id), None)?;
                self.task_edit(id, &edit)?;
                if me.role == BOSS {
                    // The squad leader's todos are orders: straight to Doing, out to a free hand.
                    self.task_move(id, "doing", None).await?;
                    return Ok(format!("Added todo #{id}. It goes to the next free squadmate."));
                }
                Ok(format!("Added todo #{id}."))
            }
            ["todo", "list"] => {
                let t = db::tasks(&self.db())?;
                let mine: Vec<String> = t
                    .iter()
                    .filter(|t| t["island_id"].as_str().map(|i| i == me.island_id).unwrap_or(true))
                    .map(|t| {
                        let who = t["crew_id"].as_str().and_then(|c| db::crew_one(&self.db(), c).ok().flatten()).map(|c| format!(" ({})", c.name)).unwrap_or_default();
                        let after = t["after"].as_i64().map(|a| format!(" after #{a}")).unwrap_or_default();
                        format!("#{} [{}]{}{} {}{}", t["id"], t["col"].as_str().unwrap_or(""), if t["pri"].as_i64().unwrap_or(0) > 0 { " urgent" } else { "" }, after, t["title"].as_str().unwrap_or(""), who)
                    })
                    .collect();
                Ok(if mine.is_empty() { "The board is empty.".into() } else { mine.join("\n") })
            }
            ["todo", "show", id] => {
                let id: i64 = id.trim_start_matches('#').parse().map_err(|_| anyhow!("Give the todo number."))?;
                if self.task_island(id)?.map(|i| i != me.island_id).unwrap_or(false) {
                    bail!("That todo belongs to another mission.");
                }
                let t = db::tasks(&self.db())?.into_iter().find(|t| t["id"] == json!(id)).ok_or_else(|| anyhow!("No such todo."))?;
                let line = |k: &str, label: &str| t[k].as_str().filter(|x| !x.is_empty()).map(|x| format!("\n{label}: {x}")).unwrap_or_default();
                Ok(format!("#{id} [{}] {}{}{}{}{}", t["col"].as_str().unwrap_or(""), t["title"].as_str().unwrap_or(""), line("note", "Notes"), line("ask", "Waiting on the commander"), line("answer", "The commander said"), line("proof", "Proof")))
            }
            ["todo", "ask", id, question @ ..] if !question.is_empty() => {
                let id: i64 = id.trim_start_matches('#').parse().map_err(|_| anyhow!("Give the todo number."))?;
                if self.task_island(id)?.map(|i| i != me.island_id).unwrap_or(false) {
                    bail!("That todo belongs to another mission.");
                }
                self.owns(&me, id)?;
                let col: String = self.db().query_row("SELECT col FROM tasks WHERE id=?1", [id], |r| r.get(0)).map_err(|_| anyhow!("No such todo."))?;
                if col == "done" {
                    bail!("#{id} is already done.");
                }
                let ask = format!("{} asks: {}", me.name, clip(&question.join(" "), 600));
                self.db().execute("UPDATE tasks SET col='wait', ask=?2, crew_id=?3 WHERE id=?1", params![id, ask, me.id])?;
                self.emit_tasks();
                self.toast(format!("{} has a question on #{id}", me.name), "bell");
                Ok(format!("Asked the commander about #{id}. Stop here: the answer will come back to you."))
            }
            ["todo", "ask", ..] => bail!("Use: ysob todo ask <id> \"<one clear question>\""),
            ["todo", "done", id, "--proof", proof @ ..] => {
                let id: i64 = id.trim_start_matches('#').parse().map_err(|_| anyhow!("Give the todo number."))?;
                if self.task_island(id)?.map(|i| i != me.island_id).unwrap_or(false) {
                    bail!("That todo belongs to another mission.");
                }
                let col: String = self.db().query_row("SELECT col FROM tasks WHERE id=?1", [id], |r| r.get(0)).map_err(|_| anyhow!("No such todo."))?;
                if col == "done" {
                    bail!("#{id} is already done.");
                }
                // Only the squadmate on the job closes it, or the boss accepting it for them; the
                // work that gets merged is the assignee's either way.
                let assignee = self.owns(&me, id)?;
                let closer = assignee.unwrap_or_else(|| me.clone());
                self.task_move_by(id, "done", Some(&proof.join(" ")), Some((&closer.id, &closer.name))).await?;
                self.db().execute("UPDATE tasks SET crew_id=?2, by_name=?3 WHERE id=?1", params![id, closer.id, closer.name])?;
                self.emit_tasks();
                // A terminal pirate has no end-of-turn signal. Burying its job is how it says it is finished.
                if me.engine != "claude" {
                    let open: i64 = self.db().query_row("SELECT COUNT(*) FROM tasks WHERE col='doing' AND crew_id=?1", [&me.id], |r| r.get(0)).unwrap_or(0);
                    if open == 0 {
                        self.set_live(&me.id, Some("idle"), Some(("rest", "off watch")));
                        self.dispatch_soon();
                    }
                }
                Ok(format!("Done #{id}. Trophy taken."))
            }
            ["todo", "done", ..] => bail!("No proof, no trophy. Use: ysob todo done <id> --proof \"<evidence>\""),
            ["squad", "add", who] => {
                if me.role != BOSS {
                    bail!("Only the squad leader brings people onto a mission.");
                }
                let pick = {
                    let roster = self.roster.lock().unwrap();
                    let names = roster.iter().map(|r| r.name.as_str()).collect::<Vec<_>>().join(", ");
                    roster.iter().find(|r| r.name.eq_ignore_ascii_case(who)).cloned().ok_or_else(|| anyhow!("No one called {who} on the roster. Pick from: {names}."))?
                };
                let (name, job) = (pick.name.as_str(), pick.job.as_str());
                self.dispatch("crew.hire", json!({"name": name, "engine": me.engine, "island_id": me.island_id, "lane": pick.lane, "role": job, "face": pick.slug})).await?;
                Ok(format!("{name} ({job}) joined the squad on this mission. Give them work with `ysob todo add` or `ysob order {name} \"...\"`."))
            }
            ["squad"] | ["crew"] => Ok(db::crew(&self.db())?.iter().filter(|c| c.island_id == me.island_id).map(|c| format!("{} ({}, {})", c.name, c.role, c.lane)).collect::<Vec<_>>().join("\n")),
            ["bottle" | "order", to, rest @ ..] if !rest.is_empty() => {
                let target = db::crew_by_name(&self.db(), &me.island_id, to)?.ok_or_else(|| anyhow!("Nobody called {to} is on this mission."))?;
                if target.id == me.id {
                    bail!("That's you.");
                }
                if self.live.lock().unwrap().get(&target.id).map(|l| l.breaker.held).unwrap_or(false) {
                    bail!("{} is caught in the net. Wait until the commander cuts them down.", target.name);
                }
                {
                    let mut live = self.live.lock().unwrap();
                    let l = live.get_mut(agent).ok_or_else(|| anyhow!("gone"))?;
                    let now = db::now();
                    l.bottles.retain(|t| now - t < 60);
                    if l.bottles.len() >= BOTTLES_PER_MINUTE {
                        bail!("Too many orders this minute. Wait for an answer before sending more.");
                    }
                    l.bottles.push(now);
                }
                // No line of an order can pose as another engine header ("[Order from Captain]").
                let text = rest
                    .join(" ")
                    .lines()
                    .map(|l| if l.trim_start().starts_with('[') { l.replacen('[', "(", 1) } else { l.to_string() })
                    .collect::<Vec<_>>()
                    .join("\n");
                self.add_msg(&me.id, json!({"kind": "bottle_out", "to": target.name, "text": text}));
                self.say(&target.id, &format!("[Order from {}] {}", me.name, text), "bottle").await?;
                Ok(format!("Order passed to {}.", target.name))
            }
            _ => bail!("Usage: ysob todo add|list|show|ask|done, ysob order <name> <message>, ysob squad [add <name>]"),
        }
    }

    /// Checks the caller may act on a todo: it's theirs, nobody has it yet, or they lead the
    /// squad. Nobody takes over a squadmate's job in progress. Returns the assignee when the boss
    /// acts on someone else's job, so their work is what gets merged.
    fn owns(&self, me: &Crew, id: i64) -> Result<Option<Crew>> {
        let holder: Option<String> = self.db().query_row("SELECT crew_id FROM tasks WHERE id=?1", [id], |r| r.get(0)).map_err(|_| anyhow!("No such todo."))?;
        match holder {
            None => Ok(None),
            Some(h) if h == me.id => Ok(None),
            Some(h) if me.role == BOSS => Ok(db::crew_one(&self.db(), &h)?),
            Some(_) => bail!("#{id} is a squadmate's job. Only whoever is on it, or the squad leader, can close it or ask about it."),
        }
    }

    /// The island a todo belongs to. Errors if there is no such todo.
    fn task_island(&self, id: i64) -> Result<Option<String>> {
        self.db().query_row("SELECT island_id FROM tasks WHERE id=?1", [id], |r| r.get(0)).map_err(|_| anyhow!("No such todo."))
    }

    // ---------- other engines run in a terminal ----------

    fn ensure_pty(self: &Arc<Self>, crew_id: &str) -> Result<()> {
        if self.live.lock().unwrap().get(crew_id).map(|l| l.pty.is_some()).unwrap_or(false) {
            return Ok(());
        }
        let crew = db::crew_one(&self.db(), crew_id)?.ok_or_else(|| anyhow!("No such squad member."))?;
        if crew.engine == "claude" {
            bail!("{} talks in chat. The shell is for other engines.", crew.name);
        }
        let island = db::island(&self.db(), &crew.island_id)?.ok_or_else(|| anyhow!("That squad member's mission is gone."))?;
        let island = self.prepare_workdir(&crew, &island);
        let mut parts = crew.engine.split_whitespace();
        let named = parts.next().ok_or_else(|| anyhow!("No engine command."))?;
        // The registry says which program an engine id starts (white-label ids can differ from it).
        let reg = detect::registry(&self.data_dir);
        let prog_owned = reg.iter().find(|e| e.id == named).map(|e| e.bin.clone()).unwrap_or_else(|| named.to_string());
        let prog = prog_owned.as_str();
        // Terminal agents get the radio too: their own token (the one issued at hiring), the
        // door and `ysob` on PATH.
        let tok = self.live.lock().unwrap().get(crew_id).map(|l| l.token.clone()).unwrap_or_else(token);
        let mut env = self.agent_env(&crew, &tok)?;
        env.push(("TERM".to_string(), "xterm-256color".to_string()));
        let pair = portable_pty::native_pty_system().openpty(portable_pty::PtySize { rows: 30, cols: 100, pixel_width: 0, pixel_height: 0 })?;
        // Resolve on PATH ourselves: on Windows the pty starts programs by full name only, and a
        // `.cmd` shim (how npm installs codex, gemini and friends) has to go through cmd.exe.
        let found = which(prog).unwrap_or_else(|| PathBuf::from(prog));
        let shim = found.extension().map(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat")).unwrap_or(false);
        let cage = self.cage(&crew, &island, prog, &found, true, &env)?;
        let mut cmd = if let Some((wrap, cenv)) = &cage {
            let mut c = portable_pty::CommandBuilder::new("bwrap");
            for w in wrap {
                c.arg(w);
            }
            c.arg(&found);
            c.env_clear();
            for (k, v) in cenv {
                c.env(k, v);
            }
            c
        } else if cfg!(windows) && shim {
            let mut c = portable_pty::CommandBuilder::new("cmd.exe");
            c.arg("/d");
            c.arg("/c");
            c.arg(&found);
            c
        } else {
            portable_pty::CommandBuilder::new(&found)
        };
        for p in parts {
            cmd.arg(p);
        }
        cmd.cwd(&island.path);
        if cage.is_none() {
            for (k, v) in &env {
                cmd.env(k, v);
            }
        }
        let mut child = pair.slave.spawn_command(cmd).map_err(|e| anyhow!("Couldn't start {prog}: {e}. Is it installed and on your PATH?"))?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        {
            let mut live = self.live.lock().unwrap();
            let l = live.entry(crew_id.to_string()).or_insert_with(Live::new);
            l.pty = Some(Pty { writer, master: pair.master, scrollback: Vec::new(), seen: db::now() });
            l.token = tok.clone();
        }
        // Watch for the terminal going quiet. The runtime handle lets the watcher hand out the next job.
        let (hub, id, rt) = (self.clone(), crew_id.to_string(), tokio::runtime::Handle::try_current().ok());
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_secs(2));
            if hub.live.lock().unwrap().get(&id).map(|l| l.pty.is_none()).unwrap_or(true) {
                break;
            }
            if hub.pty_quiet_check(&id, PTY_QUIET_SECS) {
                if let Some(rt) = &rt {
                    let hub = hub.clone();
                    rt.spawn(async move {
                        hub.dispatch_queue().await;
                    });
                }
            }
        });
        let hub = self.clone();
        let id = crew_id.to_string();
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match std::io::Read::read(&mut reader, &mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        {
                            let mut live = hub.live.lock().unwrap();
                            if let Some(p) = live.get_mut(&id).and_then(|l| l.pty.as_mut()) {
                                p.seen = db::now();
                                p.scrollback.extend_from_slice(&buf[..n]);
                                if p.scrollback.len() > SCROLLBACK {
                                    let cut = p.scrollback.len() - SCROLLBACK;
                                    p.scrollback.drain(..cut);
                                }
                            }
                        }
                        hub.emit("pty", json!({"id": id, "data": base64::engine::general_purpose::STANDARD.encode(&buf[..n])}));
                    }
                }
            }
            let _ = child.wait();
            if let Some(l) = hub.live.lock().unwrap().get_mut(&id) {
                l.pty = None;
                l.status = "idle".into();
                l.station = "rest".into();
                l.label = "off watch".into();
            }
            hub.emit("pty_end", json!({"id": id}));
            hub.emit_crew(&id);
        });
        Ok(())
    }

    /// A terminal pirate that was working and has printed nothing for `quiet` seconds may have
    /// finished, be waiting at a prompt, be stuck, or be in the middle of a long silent build.
    /// Silence can't tell those apart, so the ship doesn't guess: the job goes to the captain to
    /// look at, and the pirate is NOT freed for new work, because typing a second job into a
    /// terminal that is still busy would corrupt the first. They come free when they bury the job
    /// themselves or the captain has looked. True when the captain was asked.
    #[doc(hidden)]
    pub fn pty_quiet_check(&self, crew_id: &str, quiet: i64) -> bool {
        let stopped = {
            let live = self.live.lock().unwrap();
            live.get(crew_id).map(|l| l.status == "working" && l.pty.as_ref().map(|p| db::now() - p.seen >= quiet).unwrap_or(false)).unwrap_or(false)
        };
        if stopped {
            self.stalled(crew_id, "Their terminal has been quiet for a while. Open the shell: they may be done, waiting at a prompt, or still building.");
            self.set_live(crew_id, Some("waiting"), Some(("cabin", "gone quiet")));
        }
        stopped
    }

    /// The captain has acted on a quiet terminal pirate's card or spoken to them, so they are
    /// taken as ready for orders again. Pirates waiting on a bell are left alone.
    fn captain_looked(&self, crew_id: &str) {
        let quiet = self.live.lock().unwrap().get(crew_id).map(|l| l.pty.is_some() && l.status == "waiting" && l.label == "gone quiet").unwrap_or(false);
        if quiet {
            self.set_live(crew_id, Some("idle"), Some(("rest", "off watch")));
        }
    }

    /// Feed one line of an agent's stream, as if Claude Code had printed it.
    #[doc(hidden)]
    pub fn feed(self: &Arc<Self>, crew_id: &str, island_path: &str, line: Value) {
        self.on_claude(crew_id, island_path, line);
    }

    /// The standing orders a pirate is started with.
    #[doc(hidden)]
    pub fn brief_for(&self, crew_id: &str) -> Result<String> {
        let crew = db::crew_one(&self.db(), crew_id)?.ok_or_else(|| anyhow!("No such squad member."))?;
        let island = db::island(&self.db(), &crew.island_id)?.ok_or_else(|| anyhow!("That squad member's mission is gone."))?;
        Ok(self.brief(&crew, &island))
    }

    /// The token an agent's own process holds. Tests use it to stand in for an agent.
    #[doc(hidden)]
    pub fn agent_token(&self, crew_id: &str) -> Option<String> {
        self.live.lock().unwrap().get(crew_id).map(|l| l.token.clone())
    }

    pub fn stop(&self, crew_id: &str) {
        let mut live = self.live.lock().unwrap();
        if let Some(l) = live.get_mut(crew_id) {
            l.stdin = None;
            if let Some(k) = l.kill.take() {
                let _ = k.send(());
            }
            l.pty = None;
            l.breaker.release();
        }
    }
}

/// A lane in the words agents and people see: look, edit, full.
fn lane_word(lane: &str) -> &'static str {
    match lane {
        "READ" => "look",
        "WRITE" => "edit",
        _ => "full",
    }
}

/// Words that claim a job is done without showing it.
fn weak_proof(p: &str) -> bool {
    let p = p.trim().trim_end_matches(['.', '!']).to_lowercase();
    p.chars().filter(|c| c.is_alphanumeric()).count() < 4
        || ["done", "ok", "okay", "yes", "works", "it works", "tested", "fixed", "finished", "complete", "completed", "none", "proof", "todo", "trust me", "looks good", "lgtm"].contains(&p.as_str())
}

fn summarise(tool: &str, text: &str, is_err: bool) -> String {
    if is_err {
        return clip(text.lines().find(|l| !l.trim().is_empty()).unwrap_or("failed"), 160);
    }
    match tool {
        "Bash" => clip(text.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("done"), 100),
        "Read" => format!("{} lines", text.lines().count()),
        "Grep" | "Glob" => format!("{} hits", text.lines().filter(|l| !l.trim().is_empty()).count()),
        _ => "done".into(),
    }
}

/// The log entry that best matches a failure, by shared words.
fn matching_mistake(d: &Connection, island_id: &str, error: &str) -> Option<(i64, String)> {
    let err = error.to_lowercase();
    let words: Vec<&str> = err.split(|c: char| !c.is_alphanumeric()).filter(|w| w.len() > 3).collect();
    let mut best: Option<(usize, i64, String)> = None;
    for e in db::log(d).ok()? {
        if e["kind"] != "mistake" {
            continue;
        }
        let isl = e["island_id"].as_str();
        if isl.is_some() && isl != Some(island_id) {
            continue;
        }
        let text = format!("{} {}", e["title"].as_str().unwrap_or(""), e["detail"].as_str().unwrap_or(""));
        let low = text.to_lowercase();
        let score = words.iter().filter(|w| low.contains(**w)).count();
        if score > 0 && best.as_ref().map(|b| score > b.0).unwrap_or(true) {
            best = Some((score, e["id"].as_i64().unwrap_or(0), text.trim().to_string()));
        }
    }
    best.map(|b| (b.1, b.2))
}

/// Where the agents' unix socket lives. Normally `<data>/lighthouse.sock`, but a socket path must fit
/// in `sun_path` (104 bytes on macOS, 108 on Linux), and app-data or temp dirs can be deeper than
/// that. Then it goes somewhere short and private: `$XDG_RUNTIME_DIR/ysob-<hash>.sock`, else
/// `/tmp/ysob-<uid>/ysob-<hash>.sock` in a 0700 folder we own. The hash ties it to this data dir.
/// On Windows the door is TCP and this path is only a label.
pub fn sock_path(data_dir: &Path) -> Result<PathBuf> {
    const MAX: usize = 100;
    let home = data_dir.join("lighthouse.sock");
    if cfg!(windows) || home.as_os_str().len() <= MAX {
        return Ok(home);
    }
    // FNV-1a: stable across runs and builds, which std's hasher doesn't promise.
    let hash = data_dir.as_os_str().as_encoded_bytes().iter().fold(0xcbf29ce484222325u64, |h, b| (h ^ *b as u64).wrapping_mul(0x100000001b3));
    let name = format!("ysob-{:012x}.sock", hash & 0xffff_ffff_ffff);
    if let Some(run) = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).filter(|d| d.is_absolute() && d.is_dir()) {
        let p = run.join(&name);
        if p.as_os_str().len() <= MAX {
            return Ok(p);
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
        let uid = std::fs::metadata(data_dir)?.uid();
        // Not temp_dir(): on macOS that is a long per-user path, the very thing we are avoiding.
        let dir = PathBuf::from(format!("/tmp/ysob-{uid}"));
        match std::fs::DirBuilder::new().mode(0o700).create(&dir) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
        // Someone else may have made it first in a shared /tmp. Only use it if it is ours and closed.
        let m = std::fs::symlink_metadata(&dir)?;
        if !m.is_dir() || m.uid() != uid || m.permissions().mode() & 0o077 != 0 {
            bail!("{} is not a private folder of ours, so the lighthouse won't put its socket there.", dir.display());
        }
        Ok(dir.join(name))
    }
    #[cfg(not(unix))]
    return Ok(home);
}

/// A standing permission in plain words, for the list where the user can take it back.
pub fn grant_words(g: &Grant) -> String {
    match g {
        Grant::Write => "Edit files in the mission".into(),
        Grant::Command(c) => {
            if let Some(h) = c.strip_prefix("web:") {
                format!("Fetch pages from {h}")
            } else if let Some(p) = c.strip_prefix("read:") {
                format!("Read {p}")
            } else if let Some(l) = c.strip_prefix("exact:") {
                format!("Run exactly `{l}`")
            } else if let Some(l) = c.strip_prefix("rigging:") {
                format!("Run `{l}` (touches agent settings)")
            } else if let Some(t) = c.strip_prefix("tool:") {
                format!("Use the tool {t}")
            } else {
                format!("Run any `{c} ...` command")
            }
        }
    }
}

/// The PreToolUse hook command Claude Code runs. On unix it is a POSIX shell word. On Windows Claude
/// Code runs hooks through Git Bash or cmd, so the path is double-quoted with forward slashes, which
/// both read the same way.
pub fn hook_command(bin: &Path) -> String {
    let p = bin.display().to_string();
    if cfg!(windows) {
        format!("\"{}\" hook", p.replace('\\', "/"))
    } else {
        format!("'{}' hook", p.replace('\'', "'\\''"))
    }
}

/// Find a program on PATH the way a shell would. On Windows PATH is `;`-separated and an
/// `.exe`/`.cmd` (PATHEXT order) beats an extensionless file, which is usually a Git Bash script.
pub fn which(bin: &str) -> Option<PathBuf> {
    which_in(bin, &std::env::var_os("PATH")?, std::env::var("PATHEXT").ok().as_deref())
}

pub fn which_in(bin: &str, path: &std::ffi::OsStr, pathext: Option<&str>) -> Option<PathBuf> {
    let mut exts: Vec<String> = vec![];
    if cfg!(windows) {
        exts.extend(pathext.unwrap_or(".COM;.EXE;.BAT;.CMD").split(';').filter(|e| !e.is_empty()).map(|e| e.to_ascii_lowercase()));
    }
    exts.push(String::new());
    // Only absolute PATH entries: an empty or relative one would run a program from wherever the
    // engine happens to be.
    std::env::split_paths(path).filter(|d| d.is_absolute()).find_map(|d| exts.iter().map(|e| d.join(format!("{bin}{e}"))).find(|f| f.is_file()))
}

