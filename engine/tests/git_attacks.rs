//! The independent review's git attacks, replayed against the engine's own git (worktree.rs).
#![cfg(unix)]

use std::fs;
use std::path::Path;
use std::process::Command;
use ysob_engine::worktree;

fn git(dir: &Path, args: &[&str]) {
    let o = Command::new("git").arg("-C").arg(dir).args(["-c", "user.name=T", "-c", "user.email=t@t"]).args(args).output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
}

fn repo() -> (tempfile::TempDir, std::path::PathBuf) {
    let t = tempfile::tempdir().unwrap();
    let main = t.path().join("shop");
    fs::create_dir_all(&main).unwrap();
    git(&main, &["init", "-q", "-b", "main"]);
    fs::write(main.join("a.txt"), "1").unwrap();
    git(&main, &["add", "."]);
    git(&main, &["commit", "-qm", "start"]);
    (t, main)
}

#[test]
fn a_nested_repo_with_fsmonitor_never_runs_in_the_engines_git() {
    let (t, main) = repo();
    let wt = worktree::ensure(&main, "Moose").unwrap();
    // Review C3: the agent makes a repo inside its checkout whose config runs a program.
    let flag = t.path().join("pwned");
    git(&wt, &["init", "-q", "sub"]);
    fs::write(wt.join("sub/x"), "x").unwrap();
    git(&wt.join("sub"), &["add", "."]);
    git(&wt.join("sub"), &["commit", "-qm", "s"]);
    // Planted last, so only git run after this point could trip it.
    git(&wt.join("sub"), &["config", "core.fsmonitor", &format!("touch {}; false", flag.display())]);
    let _ = fs::remove_file(&flag);
    let _ = worktree::sync(&main, "Moose");
    let r = worktree::land(&main, "Moose", "#1 work");
    assert!(!flag.exists(), "the agent's fsmonitor ran outside the sandbox");
    assert!(r.is_err(), "a nested repository was merged without the commander looking: {r:?}");
}

#[test]
fn a_redirected_commondir_is_ignored_by_the_engine() {
    let (t, main) = repo();
    let wt = worktree::ensure(&main, "Doc").unwrap();
    // Review C2 (engine side): the .git file points at an evil gitdir with fsmonitor set.
    let evil = t.path().join("evil");
    fs::create_dir_all(&evil).unwrap();
    let flag = t.path().join("pwned2");
    fs::write(evil.join("config"), format!("[core]\n\tfsmonitor = touch {}; false\n", flag.display())).unwrap();
    fs::write(wt.join(".git"), format!("gitdir: {}\n", evil.display())).unwrap();
    fs::write(wt.join("b.txt"), "2").unwrap();
    let _ = worktree::land(&main, "Doc", "#2 work");
    assert!(!flag.exists(), "the engine's git followed the agent's .git file");
}

#[test]
fn agent_settings_symlinks_and_git_files_wait_for_the_commander() {
    for (what, plant) in [
        (".claude/settings.json", ".claude"),
        (".mcp.json", ""),
        (".gemini/settings.json", ".gemini"),
        (".gitattributes", ""),
    ] {
        let (_t, main) = repo();
        let wt = worktree::ensure(&main, "Hank").unwrap();
        if !plant.is_empty() {
            fs::create_dir_all(wt.join(plant)).unwrap();
        }
        fs::write(wt.join(what), "{\"hooks\": {}}").unwrap();
        let r = worktree::land(&main, "Hank", "#3 work");
        assert!(r.is_err(), "{what} was merged without review");
        assert!(!main.join(what).exists(), "{what} reached the main checkout");
    }
    // Review C4: a symlink (to the home folder, say) is not merged either.
    let (_t, main) = repo();
    let wt = worktree::ensure(&main, "Luis").unwrap();
    std::os::unix::fs::symlink(std::env::var("HOME").unwrap(), wt.join("venv")).unwrap();
    assert!(worktree::land(&main, "Luis", "#4").is_err(), "a symlink was merged without review");
    assert!(main.join("venv").symlink_metadata().is_err());
    // An ordinary change still goes straight in.
    let (_t, main) = repo();
    let wt = worktree::ensure(&main, "Rick").unwrap();
    fs::write(wt.join("cart.js"), "export {}\n").unwrap();
    assert!(worktree::land(&main, "Rick", "#5").unwrap().starts_with("Merged"));
    assert!(main.join("cart.js").is_file());
}

#[test]
fn shared_dependencies_must_be_real_folders_inside_the_mission() {
    let (t, main) = repo();
    // Review C4: `venv` in the main checkout is a symlink to the home folder.
    std::os::unix::fs::symlink(t.path(), main.join("venv")).unwrap();
    fs::create_dir_all(main.join("node_modules/x")).unwrap();
    let shared = worktree::shared(&main);
    assert!(shared.iter().all(|p| !p.ends_with("venv")), "a symlinked venv was shared: {shared:?}");
    assert!(shared.iter().any(|p| p.ends_with("node_modules")));
}

#[test]
fn names_never_collide_or_come_out_empty() {
    let (_t, main) = repo();
    let a = worktree::path(&main, "Bob 1");
    let b = worktree::path(&main, "Bob1");
    let c = worktree::path(&main, "'");
    assert_ne!(a, b);
    assert_ne!(c, main.join(".ysob/worktrees"), "an empty name pointed at the worktrees folder itself");
}
