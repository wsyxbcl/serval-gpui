# AGENTS

This file defines the operating rules for coding agents working on `serval-gpui`.

## Mandatory startup behavior

Every time the agent is loaded or resumed:
1. Read `AGENTS.md`.
2. Read `ROADMAP.md`.

Bootstrap prompt:

> Read AGENTS.md and ROADMAP.md, then proceed.

## Project intent

`serval-gpui` is a native desktop GUI for the existing Rust CLI tool `serval`.

The current preferred direction is:
- keep the existing Serval CLI working as-is for v1
- build a thin GUI wrapper over it
- use GPUI for the GUI
- use normal subprocess execution for non-interactive commands
- use PTY-backed execution for interactive commands such as `capture`
- show the exact command being run and stream output in an integrated pane

## Primary operating rules

1. Maintain `ROADMAP.md` continuously until the planned work is complete.
2. In `ROADMAP.md`, keep exactly three sections:
   - Agent Work
   - User Work
   - Future Work
3. Only add tasks to Agent Work in order to maintain understanding of implementation progress.
4. Agent Work should contain both completed and pending tasks.
5. When all Agent Work is completed, select the next task from User Work and do it.
6. When User Work is exhausted, continue with Future Work.
7. Make reasonable implementation decisions based on the codebase and current documents.
8. Refactor as needed to keep the code understandable and clean.

## Git behavior

1. You are working on the `main` branch unless the repo state clearly requires otherwise.
2. You may commit whenever it is useful.
3. Commits should be small, coherent, and descriptive.

## Implementation priorities

1. Preserve existing Serval behavior where possible.
2. Prefer a thin-wrapper architecture before deeper integration.
3. Keep the UI understandable for non-CLI users.
4. Keep the exact Serval command visible for transparency.
5. Support interactive Serval sessions through PTY when needed.
6. Avoid unnecessary dependencies and avoid speculative overengineering.

## Expected initial milestones

The agent should generally move in this order unless the codebase suggests a better sequence:

1. Ensure project scaffolding exists.
2. Make a minimal GPUI app run.
3. Add a basic layout with controls plus output pane.
4. Implement subprocess execution for non-interactive commands.
5. Add command preview generation.
6. Add file and directory input flows.
7. Add PTY support for interactive commands.
8. Improve UX and platform behavior.

## Notes for decision making

- For v1, the GUI does not need to be a full terminal emulator.
- For v1, we only support two main serval functions: serval observe and serval capture
- However, some commands are interactive, so PTY support is required.
- If richer terminal rendering becomes necessary, the project may later explore Ghostty-based components or prior `gpui-ghostty` work.
- Do not block useful progress on speculative future architecture.

## Agent reminder
serval is available in current environment, so use serval --help or serval <command> --help to check the options/arguments

Whenever resuming work, start by reading:
- `AGENTS.md`
- `ROADMAP.md`

Then proceed with the next actionable task.
