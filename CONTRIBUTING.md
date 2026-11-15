# Contributing

Thanks for looking. YSOB is small on purpose: a Rust engine, a Svelte app and a thin desktop shell.
Fixes, new agent CLIs, theme packs and platform work (macOS, a Windows sandbox) are all welcome.

## Set up

You need Rust (stable), [Bun](https://bun.sh) and, on Linux, `bubblewrap` for the sandbox tests.

```bash
(cd ui && bun install && bun run build)
cargo build --release
./target/release/ysob serve --ui ui/dist      # prints the link to open
```

For UI work, run the engine as above and `cd ui && bun run dev` beside it (port 5199, proxied to the engine).
Open the engine's link once to get the token, then use the dev server's address with the same `#token=...`.

## Before you send a change

```bash
cargo test --release                                  # engine: unit, sandbox, git attacks, end to end
cd ui && bunx svelte-check --no-tsconfig && bun test src/lib && bun run build
```

- A failing test means the code is wrong. Don't weaken or delete a test to get past it.
- New behaviour comes with a test. Anything that touches the guard, the sandbox or the engine's own git
  comes with a test that tries to break it (`engine/tests/cage.rs`, `engine/tests/git_attacks.rs`,
  the tests in `engine/src/lanes.rs`).
- Bun only for JavaScript. No new dependency without a reason a reviewer can check: standard library first.
- JavaScript with JSDoc types, Svelte 5 runes, no TypeScript build step in the app.

## How the code is laid out

| Where | What |
|---|---|
| `engine/src/hub.rs` | The hub: state, commands from the app, the agents' `ysob` command, routing jobs |
| `engine/src/lanes.rs` | The guard: what each lane may do without asking |
| `engine/src/sandbox.rs` | The bubblewrap sandbox (Linux) |
| `engine/src/worktree.rs` | A checkout per editing squadmate, and the engine's own hardened git |
| `engine/src/detect.rs` | Which agent CLIs, keys and local models this machine has |
| `engine/src/roster.rs` | The squad the leader can raise, read from the theme pack |
| `ui/src/` | The app. `lib/store.svelte.js` mirrors the engine; `lib/words.js` holds every themed word |
| `desktop/` | The Tauri shell: starts the engine, opens the window, updates, the folder picker |
| `packs/default/` | The default theme pack, original art |

The engine began life with ship words, and some remain in its code and database: an `island` is a mission,
`crew` are squad members, a `bottle` is an order between squadmates, the `lighthouse` is the hub, the `hold`
is the data folder and the `Captain` role is the squad leader. They are internal names; nothing a user reads
uses them. Renaming them means a database migration, so do it in one change or not at all.

## Words

Everything a user reads is plain British English: short sentences, no jargon, no filler, no em dashes.
Themed words (names, the big-moment lines) belong in the theme pack or in `ui/src/lib/words.js`, never
inline in a component. The app and its docs never name the film it nods to.

## Theme packs

A pack is art and sound you have the rights to, plus a `pack.json`. The format is in the README. Packs are
not committed to this repo (the default one, made from the original art here, is the exception).

## Security

Don't open a public issue for a security problem. See [SECURITY.md](SECURITY.md).
