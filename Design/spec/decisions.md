# Decisions

Numbered proposals and their status. Confirmed items are implemented through the topic specs; proposals are
not implemented before they are confirmed. When an item is confirmed, its topic spec and board are updated in
place and the status changes here.

| No. | Topic | Status | Where |
| --- | --- | --- | --- |
| W01 | Move to `gpui-kit` 0.7 (gpui-pre + gpui-component), pinned to an exact version | Confirmed 2026-10-08 | [appearance](appearance.md) |
| W02 | Default output next to the input, not inside it | Confirmed 2026-10-08 | [run](run.md#output-location) |
| W03 | Waxbill passes every answer as a flag; Serval prompts only remain for the terminal; no two-way protocol | Confirmed 2026-10-08 | [serval-protocol](serval-protocol.md), [questions](questions.md) |
| W04 | Redesign, with this directory as the source | Confirmed 2026-10-08 | this directory |
| W05 | Sidebar groups 数据整理 / 数据分析 / 工具 (names in shell §2); one task per screen | Confirmed 2026-10-09 | [shell](shell.md) |
| W06 | Sticky run bar with one button; results first, log collapsed | Confirmed 2026-10-09 | [run](run.md) |
| W07 | Path levels picked on a sample path (also in browse); conflicts as a dialog after Serval stops with the list | Confirmed 2026-10-09 (capture timeline removed 2026-10-12) | [questions](questions.md) |
| W08 | Serval chosen by the user (provided by the project); not bundled, not linked; no GitHub access. Download and update later, from the project's own server | Confirmed 2026-10-08 | [settings](settings.md) |
| W09 | Palette and type from the brand (paper, teal ink, coral bill), light and dark | Confirmed 2026-10-09 | [appearance](appearance.md) |
| W10 | Settings as a screen in the main window instead of a separate window; how help looks is decided later | Confirmed 2026-10-09 | [settings](settings.md) |
| W11 | Waxbill is the GUI for Serval's wrangling, preparing data towards CamtrapDP; CamtrapDP import/export and xlsx support come in Serval 0.9; Biowatch stays the tool for maps, analysis and AI | Confirmed 2026-10-09 | Serval 0.9 (not yet specified) |
| W12 | Read the input before the run: choices from real values, table check and repair, previews of what the run will do | Confirmed 2026-10-08 (direction); details proposed | [data](data.md) |
| W13 | Identified species asked after the observe scan, from the real species, only when needed; remembered per folder | Confirmed 2026-10-08 | [questions §4](questions.md#4-identified-species) |
| W15 | Writing labels: CLI gains only `xmp write` (overwrite from CSV, all fields in one pass, empty cells unchanged); per-file `xmp get`/`xmp set` exist only for Waxbill (hidden, JSON) | Confirmed 2026-10-09 | [serval-protocol §5](serval-protocol.md#5-writing-labels) |
| W16 | Browse and pick: Waxbill chooses (species, rating from the table), other tools show: Caracal in the browser views and edits, a hard-linked review folder for Explorer, `xmp write` with a preview writes an edited table back. The native grid and viewer were built and removed (no video in GPUI; Waxbill stays small) | Revised 2026-10-10 | [browse](browse.md) |
| W17 | Details: output `<input>_serval_output/`; table repair as a copy; Waxbill requires Serval ≥ 0.8.0; no `--select` (Waxbill writes a subset table) | Confirmed 2026-10-09 (thumbnail format proposed) | [browse](browse.md), [data](data.md), [run](run.md) |
| W18 | Serval ↔ Waxbill messages stay JSON lines (not CBOR or bincode), with shared serde types (`protocol.rs`); edit input refuses unknown fields, output readers ignore them | Confirmed 2026-10-10 | [serval-protocol §5.2](serval-protocol.md#52-for-waxbill-xmp-get-xmp-set) |
| W19 | Waxbill as a local bridge for the hosted Caracal: it serves the chosen table and its media on 127.0.0.1 with a token, and opens Caracal pointed at it, so nothing is picked by hand; optionally edits come back live through `serval xmp set` | Proposed; feasible: on 2026-10-13 Edge (Windows, default settings) showed the local-network prompt for caracal.hinature.cn, and after 允许 the request reached 127.0.0.1:8799 (refused only because nothing listened). Scope v1: read-only (table + media), no write-back | [browse §2](browse.md#2-the-screen) |
| W20 | Explorer's own search over labels embedded in the photos: after `xmp sync`, Explorer reads `dc:subject` as tags and `xmp:Rating` as rating, so `标记:雪豹 分级:5 星` finds across deployments | Works (Windows test 2026-10-12); opt-in only: it changes the original files, and edits made there return only through observe + `xmp write` | [browse](browse.md) |
| W14 | Further GUI features: next steps, result tables, quick look, correction loop, project memory | For discussion | [data §5](data.md#5-further-gui-features-for-discussion) |

## Common direction

- Waxbill is a light front end for Serval: forms in, files out. It does not reimplement Serval.
- Every screen says what will happen, what is happening, or what happened, and what to do next.
- The exact Serval command stays visible and copyable; Waxbill never does anything the command line cannot.
- Plain words over technical detail; the raw log is always one click away.

## W03 Flags instead of prompts: confirmed

Everything Serval asks while running falls into three kinds (inventory in
[serval-protocol §1](serval-protocol.md#1-what-serval-asks-today)): plain parameters, path-level choices, and
decisions that depend on what the run finds. All of them can be answered before the run or by a rerun, so
Waxbill runs Serval as a plain process with a complete set of flags: no PTY, no reading prompt text. The CLI
keeps prompting when a flag is missing, so typing users see no change.

Cost: Serval needs the flags, a progress output and a structured result (one Serval release). Conflicts cost
a second planning pass (no copying) when they occur.

## W18 Message format: confirmed

JSON lines over CBOR and bincode, measured on 50,000 `xmp get` records (release build): JSON 10.6 MB, decode
23 ms; CBOR 8.8 MB, 37 ms; bincode 1.3 7.8 MB, 11 ms; postcard 5.8 MB, 9 ms. Reading those sidecars takes
seconds (local) to minutes (NAS), so the format is under 1% either way; the bulk data (opening 50,000 records)
goes through tags.csv, not the pipe.

What decided it:

- Versions differ (W08: the user chooses Serval). Formats without field names break on that: with one field
  added, bincode decoded wrong values silently (field added in the middle) or aborted on a 7 EB allocation (added
  at the end of a record in a stream). JSON and CBOR ignore the unknown field. bincode is also unmaintained
  (3.0.0 is a `compile_error!`).
- stderr is shared with panics and plain error lines; JSON lines skip what is not JSON, a binary stream cannot
  resynchronise.
- Every command Waxbill runs can be run and read by a person (copied command, logs, bug reports).

Type safety comes from shared serde types instead: `FileEdit`, `FileState`, `Labels` in Serval's
`src/protocol.rs`, used as they are by Waxbill. Edits refuse unknown fields (an older Serval never ignores an
edit); results are read leniently (a newer Serval may add fields).

Would be revisited for: a long-lived Serval process, binary payloads (thumbnails, frames), or Serval and
Waxbill shipped in lockstep.

