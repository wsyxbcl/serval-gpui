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
- [x] Add a setup interface so the user can configure the Serval binary path, and use it for launches when set.
- [x] Add initial UI support for `xmp` and its subfunctions
- [x] Add initial UI support for `extract`
- [x] Add initial UI support for `translate`
- [x] Add collapsible panels so configuration sections do not push the output panel offscreen.
- [x] Add corresponding help messages for each function.
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
- [x] Add i18n language support and let the user configure it in Setup.
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
- [x] Prototype a decoupled GUI-assist layer for `capture` that keeps raw PTY input authoritative
- [x] Use the GUI assist layer for other Serval commands that need interaction, such as `serval extract`.
- [x] Evaluate routing progress-heavy non-interactive Serval commands through PTY when needed so their progress bars can render in the GUI
- [x] Support clipboard and other inline shortcuts
- [x] Fix IME composition handling in custom text inputs so Chinese input works on Windows
- [x] Quote preview and copied command arguments so spaces and shell-sensitive values stay copy-pastable
- [x] Add cross-platform app icon assets/resources and attribution notice
- [x] Derive blank output dirs from the input location and show the auto-output hint in the UI

---

## User Work

- Decide whether to support bundling the Serval binary or require an external executable.
- Decide which subcommands beyond the initial set deserve first-class GUI forms.
- Decide how much terminal fidelity is actually needed in the integrated output pane.
- Refine i18n translations for Chinese.

---

## Future Work

- Add more polished forms for all Serval subcommands, e.g. pre-read CSV input to offer more advanced options in `translate` or pre-read species values in CSV for `extract`.
- Replace common prompt-driven flows with GUI-native configuration screens.
- Add presets, recent inputs, and saved workflows.
- Adapt to future `serval` v0.6.6/v0.7.0 (add `serval xmp init --info` and serval viz utils)
- Add a visualization module utilizing Charton + WASM, by starting a serval in native rust to visualize csv
- Add richer output presentation for tables and generated paths.
- Investigate and fix Linux IME composition in custom text inputs so Chinese input works there too.
- Improve packaging and distribution for Linux, Windows, and macOS.
- Explore Ghostty-based terminal components if a richer embedded terminal becomes necessary.
- Consider extracting reusable Serval command metadata from the CLI project if it materially improves maintainability.
