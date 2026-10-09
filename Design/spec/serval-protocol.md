# Serval protocol

How Waxbill runs Serval: as a plain process with a complete set of flags, reading one-way progress and result
events. Decided in [W03](decisions.md#w03-flags-instead-of-prompts-confirmed). The flags below do not exist yet;
they are the Serval-side work, planned for Serval 0.8.0.

## 1. What Serval asks today

Every place the current Serval stops for input (branch `fix/review-2026-09`):

| # | Command | Question | Depends on | New flag |
| --- | --- | --- | --- | --- |
| 1 | `capture` | minimum time gap | nothing | `--min-gap <MINUTES>` |
| 2 | `capture` | measure from last record or last independent record | nothing | `--measure-from <last-record\|last-independent>` |
| 3 | `capture` | analyse by species or individual | nothing | `--by <species\|individual>` |
| 4 | `capture` | path level of the deployment | input paths (before the run) | `--deployment-level <N>` |
| 5 | `observe --debug` | path level of the deployment | input paths (before the run) | `--deployment-level <N>` |
| 6 | `extract` | top folder to keep | input paths (before the run) | `--keep-dirs <N>` |
| 7 | `observe` | individually identified species | what the scan finds | `--id-species <NAME>` (repeatable) |
| 8 | `extract`, `align`, `xmp copy` | skip / replace / new name for differing targets | what the plan finds | `--on-existing <skip\|replace\|rename>` |
| 9 | `serval-check` | save the list of untagged files | nothing | `--save-untagged` (not used by Waxbill) |

Kinds: 1–3 and 9 are plain parameters; 4–6 depend only on the input paths, which exist before the run; 7–8 are
the only true runtime decisions.

## 2. Flags

- With a flag, Serval never asks. Without it, Serval asks as today when stdin is a terminal. Without a terminal,
  Serval uses the default where one exists (1–3: `30`, `last-record`, `species`; 4–5: its own detected level)
  and otherwise stops with an error that names the flag.
- `--id-species` (7): the given species receive the individuals in images with several species and individuals
  (exactly one of them present). Images still ambiguous fall back to pairing every individual with every
  species, as today, and are reported (`unresolved` event); the run does not stop, because rescanning is the
  expensive part.
- `--on-existing` (8): without the flag and without a terminal, Serval stops after planning, before writing
  anything, reports the conflicts (`needs` event) and exits with code 3. Waxbill asks the user once and runs
  again with the flag; only the planning step is repeated. `extract --skip-existing` stays as an alias of
  `--on-existing skip`.
- Flags are ordinary, documented options. Typing users never need them, because Serval still asks.

## 3. Path levels

Both sides number levels the same way; Waxbill implements the rule and tests it against the same vectors as
Serval.

- **Deployment level** (`--deployment-level N`): normalise `\` to `/`, split on `/`; component 0 is the root
  part (empty for `/data/…`, `C:` for `C:\data\…`); the last component (the file) is never a level. `N` is the
  index of the deployment folder. Example: `/data/2024_spring/col_a/dep01_col_a/100MEDIA/IMG_0001.JPG` →
  `dep01_col_a` is level 4. This is Serval's current rule (`get_path_levels`, `deployment_from_path_expr`).
- **Kept folders** (`--keep-dirs N`): the number of folders kept above the file, `0` = file only. Same example:
  keeping `dep01_col_a/100MEDIA` is `--keep-dirs 2`. This is how `extract` builds paths today.
- Waxbill reads the sample path itself (first data row of the CSV, or the first media file in a folder) and
  preselects Serval's guess: it runs the same detection (`detect_deployment_path_index`) on the paths it reads.

## 4. Progress and result events

`--progress json` makes Serval write events to stderr, one JSON object per line, each with a `"serval"` key so
they can be told apart from ordinary messages. Progress bars are not drawn in this mode.

```json
{"serval":"hello","protocol":1,"version":"0.8.0","command":"extract"}
{"serval":"progress","phase":"plan","done":800,"total":1500}
{"serval":"progress","phase":"copy","done":1204,"total":1500}
{"serval":"warning","message":"Missing XMP file","path":"/data/…/MOV_0012.MP4"}
{"serval":"needs","kind":"conflicts","count":37,"ready":1463,"items":[{"target":"…/IMG_0042.JPG","reason":"sidecar-differs"}]}
{"serval":"unresolved","kind":"id-species","count":6,"species":[{"name":"雪豹","images":6,"individuals":["SL01","SL03"]},{"name":"岩羊","images":5}],"paths":["…"]}
{"serval":"output","kind":"csv","path":"…/tags_2024_spring_media_20261008.csv","rows":14106}
{"serval":"summary","written":1463,"already":0,"skipped":37,"replaced":0,"failed":0}
```

- `phase` names: `scan`, `read`, `plan`, `copy`, `move`, `write`, `analyse`. `total` is omitted when unknown
  (Waxbill shows an indeterminate bar with `done`).
- Progress is throttled to about 10 events per second.
- `needs.items` lists at most 200 entries; `count` is the full number. Reasons: `file-differs`,
  `sidecar-differs`, `unrelated-sidecar`.
- `output` is emitted for every file or folder a run produces (`csv`, `folder`, `log`); it replaces parsing
  "Saved to …".
- Exit codes: `0` success (possibly with warnings), `1` failed (some or all files), `2` usage error, `3` stopped
  for a decision, nothing written.

## 5. Writing labels

Two audiences. The command-line workflow stays observe → CSV → edit → `xmp update`, with one addition; per-file
editing exists only for Waxbill.

### 5.1 For users: `xmp write`

The missing case today: write the labels in a CSV into the sidecars as they are, especially into files that have
no labels yet. Today that takes one `xmp update` per tag type plus `xmp update --datetime`.

```sh
serval xmp write tags_filled.csv                          # species, individual, rating, datetime in one pass
serval xmp write tags_filled.csv --fields species,rating  # only these fields
```

- Reads the normal tags.csv columns (`species`, `individual`, `rating`, `datetime`); labellers fill these
  directly.
- Overwrites: for every file in the table, each written field becomes exactly the table's value. Species and
  individuals are collected from all the file's rows (observe lists one row per species/individual, so the rows
  of a file are its full lists); rating and datetime must agree across the file's rows, otherwise the file is a
  conflict naming the rows. A field only touches its own entries (`Species|…` and their digiKam / `dc:subject`
  copies); other tags and metadata stay.
- An empty cell leaves that field unchanged; `xmp write` never clears a field.
- Same safety as `xmp update`: every file is checked before any is written, files already showing the result
  are skipped (a rerun finishes an interrupted run), one backup per file per run; `--create-missing` creates a
  missing sidecar from the media file as `xmp init` does.
- The summary lists what changed per field and how many files lost values that were not in the table
  ("12 个文件移除了表中没有的物种").
- `--dry-run` reads the files and reports what would change (per field, and the files that would lose values),
  writing nothing; Waxbill uses it for the preview before writing a filled table.

`xmp update` stays as it is: change old → new where the file currently has old (corrections, with the old-value
check). `xmp update --datetime` keeps working.

### 5.2 For Waxbill: `xmp get`, `xmp set`

Hidden from `--help`; JSON in and out.

```sh
serval xmp get --json <FILE>...
serval xmp set --edits - --json --backup first --log-to <FILE> [--create-missing]
```

- `xmp get` prints one object per file: species, individuals, rating, datetime, the sidecar's modification time.
- `xmp set` reads edits as JSON lines on stdin. Each edit names one field and one operation:

  | `op` | Lists (species, individual) | Single values (rating, datetime) |
  | --- | --- | --- |
  | `set` | the list becomes exactly `values` | overwrite |
  | `add` | append missing `values` | — |
  | `remove` | remove `values` if present | — |
  | `replace` | `old` → `new`; conflict if `old` is absent | — |

  ```json
  {"path":"…/IMG_0042.JPG.xmp","field":"rating","op":"set","value":4}
  {"path":"…/IMG_0042.JPG.xmp","field":"species","op":"set","values":["Red fox"],"expect":["Fox","Deer"]}
  ```

  `expect` (optional) is what Waxbill showed; a file that no longer matches is reported as a conflict and left
  untouched. Edits are grouped per file and written once.
- Output: one result per file — `written`, `unchanged`, `conflict` or `error`, with the values now on disk — so
  Waxbill's table always ends up matching the files.
- `--backup first` keeps only each file's original (no new backup when one already exists); the default for all
  commands stays one backup per write. `--log-to` appends to one session log instead of creating a log per run.

Waxbill's mapping: pressing 1–5 → `set` rating; editing an image's species → `set` with `expect`; "添加物种" →
`add`; writing a filled CSV → `xmp write` after a per-field preview; correcting old → new in a table → `xmp update`.

## 6. Compatibility

- Waxbill reads `serval --version` at startup and when the program changes; it requires Serval ≥ 0.8.0 (the
  first release with these flags) and protocol 1 (from the `hello` event).
- A newer protocol than Waxbill knows: Waxbill shows "update Waxbill" and still allows copying the command.

## 7. Serval-side work

- Add the flags in §1, keeping prompts as the terminal fallback. The prompts live in about eight places; they
  move behind one small interface (ask / progress / report) with a terminal implementation (today's rustyline
  prompts and indicatif bars) and a JSON implementation (§4). This is the "separate interaction from logic"
  step of the Serval review's phase 6.
- Add `--progress json`, the events and the exit codes.
- Add `xmp write` (§5.1) and the hidden `xmp get` / `xmp set` (§5.2), reusing the existing per-file write path
  (`apply_tag_ops`, `apply_rating_ops`, `write_xmp_with_backup`), plus `--backup first` and `--log-to`.
- Tests: the level-numbering vectors (§3) shared with Waxbill; one end-to-end test per command that runs without
  a terminal and checks no prompt is reached.
