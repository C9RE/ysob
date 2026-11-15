# Security

YSOB runs AI agents that run code. Its job is to keep that code where you put it, so security reports are
taken seriously and fixed first.

## Reporting a problem

Use GitHub's private vulnerability reporting on this repository (Security tab, "Report a vulnerability").
Please don't open a public issue. Say what you did, what happened and what you expected; a short script or
the exact command an agent ran is the most useful thing you can send.

## What counts

- An agent in the look or edit lane reading or changing anything outside its mission folder, or reaching the
  engine's data folder, another mission, your home folder or your keys (Linux, sandbox on).
- A tool call that should have asked you and didn't, or an "Always" that covers more than its card said.
- Agent output that runs script in the app, or reaches the desktop shell's commands.
- The engine's own git running anything an agent wrote (hooks, config, filters).
- An update being installed that wasn't signed with the release key.

## What is already known

These are limits, written down in the README, not bugs:

- Windows and macOS have no sandbox yet. There the guard checks Claude Code's tool calls, and nothing
  contains an agent's own code.
- Engines other than Claude Code run in a terminal: they are sandboxed on Linux, but their tool calls are
  not checked one by one.
- Agents need the network to reach their model, so code in the sandbox can reach the internet and loopback
  services, and can send the mission's own files out.
- A full-access squadmate runs outside the sandbox by design.
- The Windows installer is not yet code-signed, so Windows warns before it runs. Updates are signed and
  checked by the app itself.
