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

The current direction (see `Design/spec/decisions.md`):
- a light front end for Serval: forms in, files out; it does not reimplement Serval
- GPUI through `gpui-kit` (pinned exactly), with gpui-component's widgets
- Serval runs as a plain process with every answer as a flag and `--progress json` events (W03); no PTY
- show the exact command being run; Serval's ordinary output goes to the run log

## Design comes first

- `Design/` is the first source for how Waxbill looks and behaves (rules in `Design/spec/*.md`, states in
  `Design/boards/*.html`, tokens in `Design/boards/tokens.css`). A change to behaviour or appearance edits the
  spec and the board in the same change as the code; replace outdated rules in place, never keep versioned copies.
- Proposals are tracked in `Design/spec/decisions.md`; do not implement a proposal before it is confirmed.
- After editing a board, render it with `scripts/render-design.sh <board>.html`.
- Pull requests that change the UI include before and after screenshots of the real app.

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

1. You are working on the `master` branch unless the repo state clearly requires otherwise.
2. You may commit whenever it is useful.
3. Commits should be small, coherent, and descriptive.

## Implementation priorities

1. Preserve existing Serval behavior; changes Waxbill needs go into Serval as flags or events, never as prompt scraping.
2. Keep the UI understandable for non-CLI users (plain words, numbers with separators, what happens next).
3. Keep the exact Serval command visible and copyable.
4. Every board state can be opened with `--design-state <name>` and compared with its board.
5. Avoid unnecessary dependencies and avoid speculative overengineering.

## Notes for decision making

- Serval is a separate program the user chooses (W08); Waxbill needs Serval 0.8.0 or later.
- Messages between them are JSON lines with lenient readers (W18).
- Screenshots for review come from an isolated headless KWin session (`Design/README.md`), never the desktop.
- Do not block useful progress on speculative future architecture.

## Agent reminder
Use `serval --help` or `serval <command> --help` (0.8.0 or later) to check options and arguments.

Whenever resuming work, start by reading:
- `AGENTS.md`
- `ROADMAP.md`

Then proceed with the next actionable task.
