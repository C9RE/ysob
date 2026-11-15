//! A checkout of their own for every squadmate who edits, so two writers never share files.
//!
//! The squadmate works in `<mission>/.ysob/worktrees/<name>` on branch `ysob/<name>`. Before each
//! job their copy takes the latest of the main checkout; when they close a job with proof, the
//! engine commits their work and merges it into the main checkout. A merge that doesn't go
//! through cleanly is aborted and left on the branch for the boss to sort out.
//!
//! The engine's own git calls run with hooks switched off: a hook would run code from the
//! repo outside the sandbox (a husky hook running the tests the agent just wrote, say).

use anyhow::{anyhow, bail, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Folders of installed dependencies that a fresh checkout doesn't have. They are linked from
/// the main checkout so tests run without a reinstall.
const SHARED: &[&str] = &["node_modules", ".venv", "venv"];

/// The folder and branch name for a squadmate: lowercase ASCII letters, digits and dashes.
pub fn slug(name: &str) -> String {
    let s: String = name
        .chars()
        .filter_map(|c| match c {
            c if c.is_ascii_alphanumeric() => Some(c.to_ascii_lowercase()),
            ' ' | '-' | '_' => Some('-'),
            _ => None,
        })
        .collect();
    let s = s.trim_matches('-').to_string();
    // A name with nothing usable ("'", all non-ASCII) still gets a stable, non-empty folder.
    if s.is_empty() {
        format!("agent-{}", name.bytes().map(|b| format!("{b:02x}")).collect::<String>().chars().take(16).collect::<String>())
    } else {
        s
    }
}

pub fn branch(name: &str) -> String {
    format!("ysob/{}", slug(name))
}

/// Windows' canonical paths start `\\?\`, which git can't make folders under. Plain form for git.
fn plain(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    s.strip_prefix(r"\\?\").map(PathBuf::from).unwrap_or_else(|| p.to_path_buf())
}

pub fn path(mission: &Path, name: &str) -> PathBuf {
    plain(mission).join(".ysob").join("worktrees").join(slug(name))
}

/// Git settings that would run a program, or reach into another repository, are switched off
/// for every git the engine runs: agent content must never become code outside the cage.
const SAFE: &[&str] = &[
    "core.hooksPath=/dev/null", "commit.gpgsign=false", "core.fsmonitor=false", "core.untrackedCache=false",
    "submodule.recurse=false", "diff.ignoreSubmodules=all", "status.submoduleSummary=false", "protocol.file.allow=never",
    "core.sshCommand=false", "credential.helper=",
];

fn git(dir: &Path) -> Command {
    let mut c = Command::new("git");
    c.arg("-C").arg(plain(dir));
    for kv in SAFE {
        c.args(["-c", kv]);
    }
    c.env("GIT_TERMINAL_PROMPT", "0").env_remove("GIT_DIR").env_remove("GIT_WORK_TREE").env_remove("GIT_COMMON_DIR");
    c
}

/// Where git keeps a squadmate's checkout, worked out here rather than read from the `.git`
/// file in their folder, which they can rewrite: (gitdir, the main repo's `.git`).
pub fn gitdirs(mission: &Path, name: &str) -> (PathBuf, PathBuf) {
    let common = plain(mission).join(".git");
    (common.join("worktrees").join(slug(name)), common)
}

/// Git in a squadmate's checkout, pinned to the gitdir the engine made.
fn git_wt(mission: &Path, name: &str) -> Command {
    let wt = path(mission, name);
    let (gitdir, _) = gitdirs(mission, name);
    let mut c = git(&wt);
    c.env("GIT_DIR", &gitdir).env("GIT_WORK_TREE", &wt);
    c
}

/// Paths an agent's work may not change without the commander looking: agent settings, git
/// metadata and the squad's own folder.
fn rigging(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    let first = p.split('/').next().unwrap_or("");
    matches!(first, ".claude" | ".codex" | ".gemini" | ".ysob" | ".git" | ".vscode" | ".cursor" | ".mcp.json" | ".gitattributes" | ".gitmodules"
        | ".husky" | ".githooks" | ".qwen" | ".goose" | "lefthook.yml" | ".lefthook.yml" | ".pre-commit-config.yaml" | "opencode.json" | ".aider.conf.yml")
        || p.split('/').any(|c| c == ".git" || c == ".claude" || c == ".gitattributes" || c == ".gitmodules")
}

fn out(mut c: Command) -> Result<String> {
    let o = c.output().map_err(|e| anyhow!("git didn't start: {e}"))?;
    if o.status.success() {
        Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
    } else {
        let err = String::from_utf8_lossy(&o.stderr);
        let err = if err.trim().is_empty() { String::from_utf8_lossy(&o.stdout).to_string() } else { err.to_string() };
        bail!("{}", err.trim().lines().take(4).collect::<Vec<_>>().join(" "))
    }
}

/// True when the mission is a git checkout with at least one commit to branch from.
pub fn is_repo(mission: &Path) -> bool {
    // Checked on every tool call, so the cheap test goes first.
    if !mission.join(".git").exists() {
        return false;
    }
    let mut c = git(mission);
    c.args(["rev-parse", "--verify", "-q", "HEAD"]);
    out(c).is_ok()
}

/// Commits made by the engine carry the squadmate's name, and work even where git has no identity.
fn as_who(c: &mut Command, name: &str, mission: &Path) {
    c.env("GIT_AUTHOR_NAME", format!("{name} (YSOB)")).env("GIT_AUTHOR_EMAIL", "ysob@localhost");
    let mut q = git(mission);
    q.args(["config", "user.email"]);
    if out(q).map(|e| e.is_empty()).unwrap_or(true) {
        c.env("GIT_COMMITTER_NAME", "YSOB").env("GIT_COMMITTER_EMAIL", "ysob@localhost");
    }
}

fn exclude(mission: &Path, lines: &[&str]) -> Result<()> {
    let mut c = git(mission);
    c.args(["rev-parse", "--git-common-dir"]);
    let common = PathBuf::from(out(c)?);
    let common = if common.is_absolute() { common } else { mission.join(common) };
    let file = common.join("info").join("exclude");
    let have = std::fs::read_to_string(&file).unwrap_or_default();
    let missing: Vec<&&str> = lines.iter().filter(|l| !have.lines().any(|h| h.trim() == **l)).collect();
    if missing.is_empty() {
        return Ok(());
    }
    std::fs::create_dir_all(file.parent().unwrap_or(&common))?;
    let mut text = have;
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    for l in missing {
        text.push_str(l);
        text.push('\n');
    }
    std::fs::write(&file, text)?;
    Ok(())
}

/// Link a folder. On Windows a symlink needs admin or developer mode, so a junction (which
/// doesn't) is the fallback. A link that can't be made only costs a reinstall in the checkout.
fn link_dir(src: &Path, dst: &Path) -> bool {
    #[cfg(unix)]
    return std::os::unix::fs::symlink(src, dst).is_ok();
    #[cfg(windows)]
    return std::os::windows::fs::symlink_dir(src, dst).is_ok()
        || Command::new("cmd").args(["/d", "/c", "mklink", "/J"]).arg(dst).arg(src).output().map(|o| o.status.success()).unwrap_or(false);
}

/// The squadmate's checkout, made on first use.
pub fn ensure(mission: &Path, name: &str) -> Result<PathBuf> {
    let wt = path(mission, name);
    if wt.join(".git").is_file() {
        return Ok(wt);
    }
    exclude(mission, &["/.ysob/"])?;
    let br = branch(name);
    let mut has = git(mission);
    has.args(["rev-parse", "--verify", "-q", &format!("refs/heads/{br}")]);
    let mut c = git(mission);
    c.args(["worktree", "add", "-q"]);
    if out(has).is_ok() {
        c.arg(&wt).arg(&br);
    } else {
        c.args(["-b", &br]).arg(&wt).arg("HEAD");
    }
    out(c)?;
    let mut linked = vec![];
    for d in SHARED {
        let src = mission.join(d);
        let dst = wt.join(d);
        if real_dir_inside(mission, &src) && dst.symlink_metadata().is_err() && link_dir(&src, &dst) {
            linked.push(format!("/{d}"));
        }
    }
    // A linked folder is a symlink, which `node_modules/` in .gitignore doesn't match.
    exclude(mission, &linked.iter().map(String::as_str).collect::<Vec<_>>())?;
    Ok(wt)
}

/// The folders the sandbox must also show, read-only, for the linked dependencies to work.
pub fn shared(mission: &Path) -> Vec<PathBuf> {
    SHARED.iter().map(|d| mission.join(d)).filter(|p| real_dir_inside(mission, p)).collect()
}

/// A real folder (not a symlink to one) inside the mission.
fn real_dir_inside(mission: &Path, p: &Path) -> bool {
    let Ok(m) = p.symlink_metadata() else { return false };
    let (Ok(root), Ok(full)) = (mission.canonicalize(), p.canonicalize()) else { return false };
    m.is_dir() && !m.file_type().is_symlink() && full.starts_with(root)
}

fn dirty(mut c: Command) -> Result<bool> {
    c.args(["status", "--porcelain", "--ignore-submodules=all"]);
    Ok(!out(c)?.is_empty())
}

/// Bring the main checkout's latest commit into the squadmate's copy. Returns a note when it
/// couldn't, which goes to the squadmate with their next job.
pub fn sync(mission: &Path, name: &str) -> Option<String> {
    let mut c = git(mission);
    c.args(["rev-parse", "HEAD"]);
    let head = out(c).ok()?;
    if dirty(git_wt(mission, name)).unwrap_or(true) {
        return Some("Your copy has work that isn't closed yet, so it didn't take the latest from the main checkout.".into());
    }
    let mut ff = git_wt(mission, name);
    ff.args(["merge", "-q", "--ff-only", &head]);
    if out(ff).is_ok() {
        return None;
    }
    let mut m = git_wt(mission, name);
    m.args(["merge", "-q", "--no-edit", &head]);
    as_who(&mut m, "YSOB", mission);
    match out(m) {
        Ok(_) => None,
        Err(e) => {
            let mut a = git_wt(mission, name);
            a.args(["merge", "--abort"]);
            let _ = out(a);
            Some(format!("Your copy couldn't take the latest from the main checkout ({e}). Work from what you have and say so in your proof."))
        }
    }
}

/// What the branch changes that the commander must see before it is merged: agent settings,
/// git metadata, symlinks or nested repositories. Empty when the change is ordinary files.
fn needs_review(mission: &Path, br: &str) -> Result<Vec<String>> {
    // Commit-to-commit tree diff from the merge base: it shows nested repositories (gitlinks)
    // even though the engine's git otherwise ignores submodules, and never looks inside them.
    let mut b = git(mission);
    b.args(["merge-base", "HEAD", br]);
    let base = out(b)?;
    let mut c = git(mission);
    c.args(["diff-tree", "-r", "-z", "--no-renames", "--no-abbrev", "--ignore-submodules=none", &base, br]);
    let text = out(c)?;
    let mut flagged = vec![];
    let mut parts = text.split('\0');
    while let Some(meta) = parts.next() {
        let Some(path) = parts.next() else { break };
        let fields: Vec<&str> = meta.trim_start_matches(':').split_whitespace().collect();
        let (new_mode, status) = (fields.get(1).copied().unwrap_or(""), fields.get(4).copied().unwrap_or(""));
        let added_link = status != "D" && (new_mode == "120000" || new_mode == "160000");
        if added_link || rigging(path) {
            flagged.push(if added_link { format!("{path} (a {})", if new_mode == "120000" { "symlink" } else { "nested repository" }) } else { path.to_string() });
        }
    }
    Ok(flagged)
}

/// Commit the squadmate's work and merge it into the main checkout. Ok says what happened;
/// Err says why it stayed on the branch.
pub fn land(mission: &Path, name: &str, message: &str) -> Result<String> {
    let br = branch(name);
    if dirty(git_wt(mission, name))? {
        let mut add = git_wt(mission, name);
        add.args(["add", "-A"]);
        out(add)?;
        let mut c = git_wt(mission, name);
        c.args(["commit", "-q", "--no-verify", "-m", message]);
        as_who(&mut c, name, mission);
        out(c)?;
    }
    let mut ahead = git(mission);
    ahead.args(["rev-list", "--count", &format!("HEAD..{br}")]);
    if out(ahead)?.trim() == "0" {
        return Ok("No file changes to merge.".into());
    }
    let flagged = needs_review(mission, &br)?;
    if !flagged.is_empty() {
        bail!("it changes {} which the commander has to look at first", flagged.iter().take(5).cloned().collect::<Vec<_>>().join(", "));
    }
    let mut on = git(mission);
    on.args(["symbolic-ref", "-q", "--short", "HEAD"]);
    let target = out(on).map_err(|_| anyhow!("the main checkout isn't on a branch"))?;
    let mut m = git(mission);
    m.args(["merge", "-q", "--no-ff", "--no-verify", "-m", &format!("Merge {br}: {message}"), &br]);
    as_who(&mut m, name, mission);
    match out(m) {
        Ok(_) => Ok(format!("Merged into {target}.")),
        Err(e) => {
            let mut a = git(mission);
            a.args(["merge", "--abort"]);
            let _ = out(a);
            bail!("{e}")
        }
    }
}

/// Take the checkout away when the squadmate leaves. Unclosed work is committed to their
/// branch first, so nothing is lost.
pub fn retire(mission: &Path, name: &str) {
    let wt = path(mission, name);
    if !wt.exists() {
        return;
    }
    if dirty(git_wt(mission, name)).unwrap_or(false) {
        let mut add = git_wt(mission, name);
        add.args(["add", "-A"]);
        let _ = out(add);
        let mut c = git_wt(mission, name);
        c.args(["commit", "-q", "--no-verify", "-m", &format!("Unfinished work from {name}")]);
        as_who(&mut c, name, mission);
        let _ = out(c);
    }
    let mut r = git(mission);
    r.args(["worktree", "remove", "--force"]).arg(&wt);
    let _ = out(r);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn git_gets_plain_windows_paths() {
        assert_eq!(plain(Path::new(r"\\?\C:\Users\me\shop")), PathBuf::from(r"C:\Users\me\shop"));
        assert_eq!(plain(Path::new(r"\\?\UNC\nas\share\shop")), PathBuf::from(r"\\nas\share\shop"));
        assert_eq!(plain(Path::new("/home/dev/shop")), PathBuf::from("/home/dev/shop"));
    }
}
