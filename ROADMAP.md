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
- [x] Rename the shipped binary and user-facing app identity to Waxbill
- [x] Build the Windows app as a GUI subsystem executable so it does not open an empty console window
- [x] Keep the UI usable on smaller devices with a page-level scrollbar and wrapped top-level control rows.
- [x] Show serval version at the top, and guide the user to Setup when the executable is only coming from PATH or not configured.
- [x] Refine the Active Binary status line so version/setup guidance stays on one line, and only prompt Setup when Serval is missing or its version cannot be read.
- [x] Adapt to Serval v0.6.7 by removing stale `observe` options and adding `serval xmp init --info` support.
- [x] Add a GUI assist layer for `serval xmp init --info`.
- [x] Improve interactive input contrast by making the text black.
- [x] Remove the command panel collapse toggle.
- [x] Add preset `from` and `to` options for `serval translate`: `tag`, `tagCN`, `mazeNameCN`, and `mazeScientificName`.
- [x] Show the Waxbill lockup image in the top-left header area.
- [x] Refine Chinese status copy so header messaging explicitly refers to Serval.
- [x] Give the Setup window a compact default size instead of opening it at the full app size.
- [x] Persist Setup preferences across launches using cross-platform config storage.
- [x] Add an About dialog in the header with centered version, source link, attribution, and copyright footer details.
- [x] Sync the built-in help text with the current Serval CLI, complete bilingual help coverage, and normalize the Chinese help/i18n formatting.
- [x] Refine Chinese command copy by separating extract-specific labels from XMP update labels, tightening extract filter wording, and replacing raw translate preset column names with user-facing labels.
- [x] Default first-launch language to Chinese and make the key Setup labels/actions bilingual in the Chinese UI.
- [x] Give the command help overlay its own scroll context so long help text does not scroll the whole page.
- [x] Investigate Windows startup/save UI stalls and move setup persistence plus Serval version detection off the UI thread.
- [x] Suppress transient Windows console flashes for background `serval` and `taskkill` subprocesses.
- [x] Embed the Waxbill lockup image into the binary so packaged Windows builds always show the header logo.
- [x] Add a GitHub Actions workflow to build macOS artifacts for both Intel and Apple Silicon.
- [x] Pin the macOS font-stack dependencies so the manual macOS workflow resolves a compatible `core-text` and `core-graphics` pair.
- [x] Update the macOS x86 workflow runner label to a currently supported Intel macOS image.
- [x] Keep the PTY output pane fixed-size with internal scrolling, and add a clear-log action.
- [x] Keep the interaction helper bounded with internal scrolling when prompts/options are long.
- [x] Fix nested output/helper scrolling so wheel events update the internal panes instead of the page.
- [x] Add visible scrollbar thumbs for the page and PTY output panes.
- [x] Add mouse drag/click support to the visible page, PTY output, and interaction helper scrollbars.
- [x] Stop showing the interaction helper for free-text prompts that already use the PTY input field.
- [x] Auto-scroll the page when a clickable interaction helper appears.
- [x] Clean up clippy warnings and dead code across the codebase (derived Defaults, dead program tuple in command builders, platform-gated pid fields).
- [x] Harden process/output handling: stop the output pump spinning when a worker thread dies, cap terminal scrollback and CSI cursor-down moves, bound the interaction-helper scan buffer, report PTY write failures, fix POSIX quoting of backslash/`!` args, and guard positional args starting with `-` behind `--`.
- [x] Make the output pane scale to long runs: per-line render cache with lazy per-frame rebuild instead of re-rendering the whole buffer on every PTY chunk, and sticky auto-scroll that stops following the tail while the user has scrolled up.
- [x] Refactor `RootView::render` to extract shared `chip`/`chip_base`/`input_row`/`browse_button`/`muted_label` helpers and centralize the chip color palette, cutting `main.rs` from ~5150 to ~4030 lines with no behavior change.

---

## User Work

- Decide whether to support bundling the Serval binary or require an external executable.
- Decide which subcommands beyond the initial set deserve first-class GUI forms.
- Decide how much terminal fidelity is actually needed in the integrated output pane.
- Refine i18n translations for Chinese.

---

## Future Work

- Add more polished forms for all Serval subcommands, e.g. pre-read CSV input to offer more advanced options in `translate` or pre-read species values in CSV for `extract`.
- Revisit adaptive sizing/scaling for very small windows after the core workflows settle.
- Replace common prompt-driven flows with GUI-native configuration screens.
- Add presets, recent inputs, and saved workflows.
- Add a visualization module utilizing Charton + WASM, by starting a serval in native rust to visualize csv
- Add richer output presentation for tables and generated paths.
- Investigate and fix Linux IME composition in custom text inputs so Chinese input works there too.
- Improve packaging and distribution for Linux, Windows, and macOS.
- Explore Ghostty-based terminal components if a richer embedded terminal becomes necessary.
- Consider extracting reusable Serval command metadata from the CLI project if it materially improves maintainability.
