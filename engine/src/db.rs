//! The hold: one SQLite file. Only the core ever opens it.

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub fn open(path: &std::path::Path) -> Result<Connection> {
    let c = Connection::open(path)?;
    c.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;
         CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS islands(id TEXT PRIMARY KEY, name TEXT NOT NULL, path TEXT NOT NULL, created INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS crew(id TEXT PRIMARY KEY, name TEXT NOT NULL, engine TEXT NOT NULL, role TEXT NOT NULL, island_id TEXT NOT NULL,
            lane TEXT NOT NULL, face TEXT NOT NULL, task TEXT NOT NULL DEFAULT '', session_id TEXT, cost REAL NOT NULL DEFAULT 0, ctx REAL NOT NULL DEFAULT 0,
            temp INTEGER NOT NULL DEFAULT 0, created INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS messages(id INTEGER PRIMARY KEY AUTOINCREMENT, crew_id TEXT NOT NULL, ts INTEGER NOT NULL, body TEXT NOT NULL);
         CREATE INDEX IF NOT EXISTS messages_crew ON messages(crew_id, id);
         CREATE TABLE IF NOT EXISTS tasks(id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL, col TEXT NOT NULL, island_id TEXT, crew_id TEXT, proof TEXT, created INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS log(id INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL, title TEXT NOT NULL, detail TEXT NOT NULL DEFAULT '', island_id TEXT, strikes INTEGER NOT NULL DEFAULT 0, created INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS grants(crew_id TEXT NOT NULL, grant_json TEXT NOT NULL, UNIQUE(crew_id, grant_json));
         CREATE TABLE IF NOT EXISTS spend(day TEXT NOT NULL, crew_id TEXT NOT NULL, usd REAL NOT NULL DEFAULT 0, PRIMARY KEY(day, crew_id));",
    )?;
    // Columns added after the first build. A column that is already there is not an error.
    for col in ["note TEXT NOT NULL DEFAULT ''", "pri INTEGER NOT NULL DEFAULT 0", "after_id INTEGER", "ask TEXT", "answer TEXT", "started INTEGER", "finished INTEGER", "by_name TEXT", "want TEXT"] {
        let _ = c.execute(&format!("ALTER TABLE tasks ADD COLUMN {col}"), []);
    }
    Ok(c)
}

pub fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

pub fn today() -> String {
    let d = now() / 86400;
    format!("d{d}")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Island {
    pub id: String,
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Crew {
    pub id: String,
    pub name: String,
    pub engine: String,
    pub role: String,
    pub island_id: String,
    pub lane: String,
    pub face: String,
    pub task: String,
    pub session_id: Option<String>,
    pub cost: f64,
    pub ctx: f64,
    pub temp: bool,
}

pub fn setting(c: &Connection, key: &str) -> Option<String> {
    c.query_row("SELECT value FROM settings WHERE key=?1", [key], |r| r.get(0)).optional().ok().flatten()
}
pub fn set_setting(c: &Connection, key: &str, value: &str) -> Result<()> {
    c.execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key, value])?;
    Ok(())
}

pub fn islands(c: &Connection) -> Result<Vec<Island>> {
    let mut s = c.prepare("SELECT id,name,path FROM islands ORDER BY created")?;
    let v = s.query_map([], |r| Ok(Island { id: r.get(0)?, name: r.get(1)?, path: r.get(2)? }))?.collect::<rusqlite::Result<_>>()?;
    Ok(v)
}
pub fn island(c: &Connection, id: &str) -> Result<Option<Island>> {
    Ok(c.query_row("SELECT id,name,path FROM islands WHERE id=?1", [id], |r| Ok(Island { id: r.get(0)?, name: r.get(1)?, path: r.get(2)? })).optional()?)
}

const CREW_COLS: &str = "id,name,engine,role,island_id,lane,face,task,session_id,cost,ctx,temp";
fn crew_row(r: &rusqlite::Row) -> rusqlite::Result<Crew> {
    Ok(Crew {
        id: r.get(0)?, name: r.get(1)?, engine: r.get(2)?, role: r.get(3)?, island_id: r.get(4)?, lane: r.get(5)?, face: r.get(6)?,
        task: r.get(7)?, session_id: r.get(8)?, cost: r.get(9)?, ctx: r.get(10)?, temp: r.get::<_, i64>(11)? != 0,
    })
}
pub fn crew(c: &Connection) -> Result<Vec<Crew>> {
    let mut s = c.prepare(&format!("SELECT {CREW_COLS} FROM crew ORDER BY created"))?;
    let v = s.query_map([], crew_row)?.collect::<rusqlite::Result<_>>()?;
    Ok(v)
}
pub fn crew_one(c: &Connection, id: &str) -> Result<Option<Crew>> {
    Ok(c.query_row(&format!("SELECT {CREW_COLS} FROM crew WHERE id=?1"), [id], crew_row).optional()?)
}
/// Names are unique within a mission (island), so every mission can have its own squad leader.
pub fn crew_by_name(c: &Connection, island_id: &str, name: &str) -> Result<Option<Crew>> {
    Ok(c.query_row(&format!("SELECT {CREW_COLS} FROM crew WHERE island_id=?1 AND lower(name)=lower(?2)"), [island_id, name], crew_row).optional()?)
}

pub fn add_message(c: &Connection, crew_id: &str, body: &Value) -> Result<i64> {
    c.execute("INSERT INTO messages(crew_id,ts,body) VALUES(?1,?2,?3)", params![crew_id, now(), body.to_string()])?;
    Ok(c.last_insert_rowid())
}
pub fn update_message(c: &Connection, id: i64, body: &Value) -> Result<()> {
    c.execute("UPDATE messages SET body=?2 WHERE id=?1", params![id, body.to_string()])?;
    Ok(())
}
pub fn messages(c: &Connection, crew_id: &str, limit: i64) -> Result<Vec<Value>> {
    let mut s = c.prepare("SELECT id,ts,body FROM (SELECT id,ts,body FROM messages WHERE crew_id=?1 ORDER BY id DESC LIMIT ?2) ORDER BY id")?;
    let v = s
        .query_map(params![crew_id, limit], |r| {
            let body: String = r.get(2)?;
            let mut b: Value = serde_json::from_str(&body).unwrap_or(json!({}));
            b["id"] = json!(r.get::<_, i64>(0)?);
            b["ts"] = json!(r.get::<_, i64>(1)?);
            Ok(b)
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(v)
}

pub fn tasks(c: &Connection) -> Result<Vec<Value>> {
    let mut s = c.prepare("SELECT id,title,col,island_id,crew_id,proof,note,pri,after_id,ask,answer,started,finished,by_name,created,want FROM tasks ORDER BY pri DESC, id")?;
    let v = s
        .query_map([], |r| {
            Ok(json!({"id": r.get::<_, i64>(0)?, "title": r.get::<_, String>(1)?, "col": r.get::<_, String>(2)?,
                "island_id": r.get::<_, Option<String>>(3)?, "crew_id": r.get::<_, Option<String>>(4)?, "proof": r.get::<_, Option<String>>(5)?,
                "note": r.get::<_, String>(6)?, "pri": r.get::<_, i64>(7)?, "after": r.get::<_, Option<i64>>(8)?,
                "ask": r.get::<_, Option<String>>(9)?, "answer": r.get::<_, Option<String>>(10)?, "started": r.get::<_, Option<i64>>(11)?,
                "finished": r.get::<_, Option<i64>>(12)?, "by": r.get::<_, Option<String>>(13)?, "created": r.get::<_, i64>(14)?, "want": r.get::<_, Option<String>>(15)?}))
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(v)
}

pub fn log(c: &Connection) -> Result<Vec<Value>> {
    let mut s = c.prepare("SELECT id,kind,title,detail,island_id,strikes FROM log ORDER BY id DESC")?;
    let v = s
        .query_map([], |r| {
            Ok(json!({"id": r.get::<_, i64>(0)?, "kind": r.get::<_, String>(1)?, "title": r.get::<_, String>(2)?, "detail": r.get::<_, String>(3)?,
                "island_id": r.get::<_, Option<String>>(4)?, "strikes": r.get::<_, i64>(5)?}))
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(v)
}

pub fn spent_today(c: &Connection) -> f64 {
    c.query_row("SELECT COALESCE(SUM(usd),0) FROM spend WHERE day=?1", [today()], |r| r.get(0)).unwrap_or(0.0)
}
pub fn add_spend(c: &Connection, crew_id: &str, usd: f64) -> Result<()> {
    c.execute("INSERT INTO spend(day,crew_id,usd) VALUES(?1,?2,?3) ON CONFLICT(day,crew_id) DO UPDATE SET usd=usd+excluded.usd", params![today(), crew_id, usd])?;
    c.execute("UPDATE crew SET cost=cost+?2 WHERE id=?1", params![crew_id, usd])?;
    Ok(())
}
