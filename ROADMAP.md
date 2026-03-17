# ROADMAP

This file is the operational task tracker for the `serval-gpui` project.

Rules:
- Keep exactly three sections: Agent Work, User Work, and Future Work.
- Only add or update items in Agent Work for tracking implementation progress.
- When Agent Work is fully complete, pick the next item from User Work and execute it.
- When User Work is exhausted, continue with Future Work.
- Keep this file current as implementation progresses.

---

## Agent Work

### Completed
- [x] Created initial project specification and working plan.
- [x] Defined v1 direction: thin GUI wrapper over the existing `serval` CLI.
- [x] Decided to support both plain subprocess execution and PTY-backed interactive execution.
- [x] Identified GPUI as the GUI framework for the first implementation.
- [x] Established that Serval should remain unchanged for v1 unless refactoring is clearly needed.

### Pending
- [x] Create the initial repository structure for `serval-gpui`.
- [x] Create a minimal runnable GPUI application.
- [x] Add a basic app layout with a command selection area and an output pane.
- [x] Define internal command metadata structures for Serval subcommands.
- [x] Implement command preview generation from structured UI input.
- [x] Implement subprocess execution for non-interactive commands.
- [x] Stream stdout and stderr into the output pane.
- [x] Implement file and directory picking.
- [x] Add initial UI support for `observe`.
- [x] Add PTY-backed execution support for interactive commands.
- [x] Verify that `serval capture ...` can prompt and accept user input inside the app after Run is clicked.
- [x] Add initial UI support for `capture`.
- [x] Add copy-command and copy-log actions.
- [x] Add open input dir / open output dir actions.
- [x] A setup interface: user can config location of serval binary, if set, gui should be using that to start serval
- [x] Add initial UI support for `xmp` and its subfunctions
- [x] Add initial UI support for `extract`
- [x] Add initial UI support for `translate`
- [x] Add a collaspe design to some panel, as config panel take different space, which may push the output panel to invisible
- [x] Add corresponding help message to each functions
- [x] Improve help UX: load full `serval ... --help` content and show it on Help hover
- [x] Extend hover help to option toggles/chips (observe/capture/xmp update/extract)
- [x] Render help as overlay instead of in-panel block to avoid hover layout shifts
- [x] Refine help interactions: command help via click toggle, option help via hover only
- [x] Move help text to manually configured local source (`src/help_texts.rs`) for future i18n
- [x] Add top-level Helper Mode toggle and gate all help overlays behind mode state
- [x] Add i18n foundation (`src/i18n.rs`) with key-based text lookup and language enum
- [x] Add language selector in Setup and wire core header/panel/action labels to i18n keys
- [x] Make help text source language-aware (`help_texts::text_for_key(language, key)`)
- [x] Localize additional labels/prompts/messages and command buttons using i18n keys
- [x] Add a i18n support for languages and let user config in setup
- [x] Add language support for simplified Chinese
- [x] Add a GitHub Actions workflow to build a Windows release binary and upload it as an artifact
- [x] Fix Windows PTY interactive prompt/input forwarding for `serval capture`
- [x] Rename the Cargo package to `maze-serval-gpui` for publishing
- [x] Fix `cargo publish --dry-run` by using a single `portable-pty` dependency source plus local patch override
- [x] Add basic Cargo publish metadata: description, repository, and Apache-2.0 license
- [x] Fix the Windows stack overflow triggered by browsing for the `capture` CSV path
- [x] Add cancellation support for running processes: probably a button that do something like ctrl-c to the terminal (or other better implement)
- [x] Implement run state management: idle, running, success, failed, cancelled.
- [x] Preserve carriage-return progress updates in the integrated output pane so PTY-rendered progress bars remain visible
- [x] Route `serval observe` through PTY so its progress bars can render in the GUI
- [ ] Evaluate routing progress-heavy non-interactive Serval commands through PTY when needed so their progress bars can render in the GUI

---

## User Work

- Decide whether to support bundling the Serval binary or require an external executable.
- Decide which subcommands beyond the initial set deserve first-class GUI forms.
- Decide how much terminal fidelity is actually needed in the integrated output pane.

---

## Future Work

- Add more polished forms for all Serval subcommands. e.g. pre-read csv from input to give more advanced opiton in translate
- Replace common prompt-driven flows with GUI-native configuration screens.
- Add presets, recent inputs, and saved workflows.
- Add richer output presentation for tables and generated paths.
- Improve packaging and distribution for Linux, Windows, and macOS.
- Explore Ghostty-based terminal components if a richer embedded terminal becomes necessary.
- Consider extracting reusable Serval command metadata from the CLI project if it materially improves maintainability.
