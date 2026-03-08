# serval-gpui specification

## Project summary

`serval-gpui` is a native desktop GUI for the existing Rust CLI tool `serval`.

The goal is to make Serval usable for people who are not comfortable with command-line tools, shell syntax, or manual path handling, while preserving the current Serval CLI as the execution engine.

The initial version should focus on being a thin, reliable GUI wrapper over the current binary instead of rewriting Serval internals.

## Background

The current `serval` CLI already provides useful workflows for preparing data for Maze and Trapper, but several users are blocked by:

- unfamiliarity with command-line usage
- inability to work with raw file system paths
- confusion around flags, options, and interactive prompts
- lack of a visible, guided workflow for common tasks

At the same time, Serval already contains working business logic, so the GUI should reuse that value rather than replacing it.

## Product goals

### Primary goals

1. Provide a guided GUI for major Serval commands.
2. Keep the existing `serval` CLI implementation usable and unchanged for v1.
3. Support Linux, Windows, and macOS.
4. Let users select files and folders through native UI instead of typing paths manually.
5. Show the exact Serval command being run for transparency and debugging.
6. Show live execution output and progress in an integrated pane.
7. Preserve support for interactive Serval commands by running them in a PTY-backed session when necessary.

### Secondary goals

1. Let advanced users run raw `serval ...` commands from within the app.
2. Gradually replace interactive CLI prompts with first-class GUI forms where beneficial.
3. Keep the codebase clean enough to evolve from a thin wrapper to a richer product.

### Non-goals for v1

1. Rewriting Serval business logic.
2. Requiring a refactor of the existing `serval` crate before the GUI becomes usable.
3. Building a general-purpose shell application.
4. Adding arbitrary file management features unrelated to Serval.
5. Embedding a fully featured terminal emulator unless it is needed to preserve current interactive behavior.

## Users

### Primary users

- Serval users who are not comfortable with CLI workflows
- Maze and Trapper related users who need guided data preparation tools
- users who do not understand shell quoting, relative vs absolute paths, or terminal conventions

### Secondary users

- existing power users who know Serval CLI and want a faster or more discoverable UI
- developers who want a transparent wrapper that still shows the underlying command

## Core UX principles

1. **Guide first, expose complexity second.**
   - Use file pickers, checkboxes, dropdowns, tabs, and descriptive labels.
2. **Never hide what is actually happening.**
   - Always show the exact generated Serval command.
3. **Preserve current capability.**
   - If a command works in CLI today, the GUI should not remove that power.
4. **Prefer thin integration first.**
   - Use the existing Serval binary as the engine for v1.
5. **Be friendly to beginners without insulting advanced users.**
   - Provide guided mode and raw command mode.

## Technical decisions

### 1. GUI framework

Use **GPUI** as the primary GUI framework.

Rationale:
- Rust-native
- modern UI model
- aligned with the user's interest and prior experimentation
- suitable for a desktop-native app on Linux, Windows, and macOS

Constraint:
- cross-platform QA, especially on Windows, should be treated as real engineering work rather than assumed to be free

### 2. Execution model

For v1, `serval-gpui` should launch the existing `serval` binary as a child process.

Preferred behavior:
- build argument vectors directly, not through a shell
- display a shell-like preview string for the user
- capture stdout and stderr for non-interactive commands
- use a PTY-backed session for interactive commands

Rationale:
- avoids refactoring the current Serval implementation
- minimizes breakage risk
- preserves existing behavior

### 3. Interactive command support

Some Serval commands, such as `capture`, currently prompt for additional input during execution.

Therefore, the GUI must support two execution paths:

1. **Plain subprocess mode** for non-interactive commands
2. **PTY session mode** for commands that require terminal-style user input

In PTY mode, the integrated pane may accept user keystrokes after execution starts so the user can continue interacting with Serval prompts.

### 4. Integrated output pane

The app should include an integrated output area that serves two closely related purposes:

1. read-only log and output view for normal runs
2. terminal-like interaction surface for PTY-backed runs when Serval prompts for input

This pane does not need to be a full general-purpose shell in v1.

### 5. Optional raw command mode

The app should eventually support an advanced mode where users can type a raw `serval ...` command and run it within the same execution/output framework.

This is primarily for advanced users and debugging.

### 6. Terminal embedding direction

For v1, a full embedded terminal emulator is not required.

However, future work may build on:
- PTY-backed execution infrastructure
- a GPUI-native terminal view
- `libghostty-vt` or related terminal-emulation components if a richer embedded terminal becomes useful

This direction is especially relevant because the user has prior work with a `gpui-ghostty` prototype.

## Initial feature set

### Application shell

- main window
- command tabs or command list
- structured input controls for each supported subcommand
- run button
- cancel button
- command preview area
- output/log pane

### Command configuration UX

For each supported Serval subcommand, provide GUI controls such as:
- file picker
- directory picker
- text field
- dropdown
- radio group
- checkbox
- optional advanced section

The GUI should translate those values into a Serval argv list.

### Output and process UX

- live stdout and stderr view
- visible command preview
- status indicator: idle, running, success, failed, cancelled
- exit code display when relevant
- copy command
- copy logs
- save logs
- rerun with same parameters

### Interactive command UX

For commands like `capture`, the output pane should allow the user to continue the prompt-and-response flow after clicking Run.

A later refinement may replace some of those prompts with GUI-native forms.

## Proposed subcommand rollout

The first milestone should not try to cover every subcommand equally.

### Suggested early coverage

1. `observe`
2. `align`
3. `translate`
4. `capture`
5. `xmp` (only the most common workflows first)

### Later coverage

- `rename`
- `extract`
- `tags2img`
- less common or more complex `xmp` actions

## Architecture outline

### High-level components

1. **App shell**
   - window management
   - global state
   - routing between tabs/views

2. **Command definitions**
   - metadata for each subcommand
   - parameter schema
   - label/help text
   - command preview generation

3. **Execution manager**
   - subprocess launching
   - PTY launching when needed
   - output streaming
   - cancellation
   - status tracking

4. **Output pane**
   - log rendering
   - optional PTY input handoff
   - copy/save actions

5. **Platform integration**
   - native file/folder pickers
   - path normalization
   - platform-specific binary discovery

### Important implementation note

The project should be structured so that UI and process execution are cleanly separated, even if the Serval CLI itself is not refactored yet.

## Risks and constraints

1. **Cross-platform process behavior**
   - PTY behavior differs across Linux, macOS, and Windows
2. **Binary discovery**
   - the GUI must reliably locate the Serval executable in dev and packaged builds
3. **Interactive prompt handling**
   - prompt-driven flows may require more terminal fidelity than plain logs
4. **Windows maturity of GPUI ecosystem**
   - this must be tested early
5. **Scope creep**
   - avoid turning v1 into a full shell or IDE

## Milestones

### Milestone 0: project bootstrap

- create repo structure
- add AGENTS.md and ROADMAP.md
- define architecture and coding conventions
- set up build, lint, and run flow

### Milestone 1: minimal runnable shell

- open a GPUI window
- show placeholder navigation
- show a basic output pane
- wire a test subprocess run

### Milestone 2: command wrapper foundation

- model Serval commands and parameters
- generate command preview strings
- implement file and folder selection
- run non-interactive commands and stream output

### Milestone 3: interactive execution

- add PTY-backed execution path
- support continuing terminal-style input after clicking Run
- verify `serval capture` works end-to-end

### Milestone 4: first usable workflows

- implement polished UI for a small set of priority commands
- validate on Linux first
- then test Windows and macOS

### Milestone 5: packaging and refinement

- improve error handling
- add nicer onboarding/help text
- package distributable builds
- document known limitations

## Definition of success

`serval-gpui` is successful when:

1. a non-technical user can complete common Serval tasks without manually writing paths or flags
2. the app transparently shows the underlying Serval command
3. interactive commands like `capture` still work
4. the codebase is stable enough to expand command coverage later
5. the project runs on Linux, Windows, and macOS with acceptable quality

## Future evolution

Potential later directions:
- replace common interactive prompts with native GUI forms
- support saved presets
- support recent files/projects
- bundle or manage the Serval binary more cleanly
- improve the integrated terminal pane using Ghostty-based components if worthwhile
- consider deeper integration with Serval internals only after the wrapper approach has proven value
