# Working from the user's data

States: [`boards/data.html`](../boards/data.html). Decision [W12](decisions.md).

This is what makes Waxbill more than a form for the command line: it reads the user's input before the run,
builds choices from the real values, checks for problems, and shows what the run will do. Serval stays the only
thing that writes data; Waxbill only reads (and writes repaired copies on request).

## 1. Reading inputs

- CSV inputs (extract, capture, translate, xmp update, xmp sync --csv) are read when chosen, in the background,
  with the `csv` and `encoding_rs` crates (no Polars, to keep Waxbill small). Budget: 100,000 rows in under a
  second on a local disk; larger files show "正在读取…" with a count.
- Folder inputs (observe, align, xmp init/sync/copy/remove, rename) are walked with the same ignore rules as
  Serval, counting images, videos and XMP files ("找到 12,480 张图片、1,302 个视频 · 92% 有 XMP"). On a NAS this
  can take a while; the count updates as it goes and never blocks the form.
- A changed file on disk (modified time) is read again when the screen is shown.

## 2. Table check and repair

Every CSV is checked when chosen. Problems that would make Serval fail block Run, with the reason; others are
warnings. One action, "生成修复后的副本", writes `<name>.utf8.csv` next to the original (never overwriting it)
and points the field at the copy.

Built (2026-10-12, `src/repair.rs`): encoding (GBK, UTF-16, tab-separated) and the time columns `datetime`,
`datetime_original`, `xmp_update_datetime`, under every table field (capture, extract, browse, update,
translate). The check is a warning with the repair button; an unreadable table already blocks Run. The other
rows are not built yet.

| Problem (seen in real files) | Detection | Repair |
| --- | --- | --- |
| Encoding is not UTF-8 (Excel's "CSV" on Chinese Windows saves GBK; "Unicode text" is UTF-16) | BOM; strict UTF-8 decode; else GB18030 decode succeeds | Re-encode to UTF-8 with BOM (Serval reads it; Excel opens it correctly) |
| Times reformatted by Excel/WPS (`2023/10/5 06:09`, `2023-10-5 06:09`) | Only where Serval is strict: 整表写入 with 时间 (`xmp write` takes year-month-day with `-`, a space or `T`, and seconds; padding optional). Elsewhere Serval reads times with Polars, which takes every Excel/WPS form tested (2026-10-15: slashes, no seconds, no padding, day first, a mixed column), so nothing is checked. | Rewrite to `yyyy-MM-dd HH:mm:ss` only where nothing is lost (`2023/1/5 06:11:00`). Times without seconds are reported with how to fix them in the spreadsheet, never filled with `00` (the seconds are lost data); times not in year-month-day order (day first) are reported, never guessed |
| Required column missing or renamed | per command (e.g. capture: `path`, `datetime`/`datetime_original`, `species`; update: `path`, `xmp_update`, the tag column) | none; the banner names the column |
| IDs that look like numbers (`1` where `001` was likely) | `individual`/ID columns all digits with differing widths, or all short | warning only; leading zeros cannot be restored |
| Paths that do not exist (data moved to another drive or computer, e.g. `G:\…` → `/mnt/nas/…`) | the first 200 paths checked; most missing but a common prefix replacement makes them exist | "替换路径前缀" with the detected old and new prefix, then the repaired copy |
| `.xlsx` chosen instead of CSV | file type | none yet: "请另存为 CSV"; xlsx support comes with Serval 0.9 (W11) |

## 3. Choosing from real values

- **Extract**: species (and individuals, events, custom values when that tab is chosen) as a searchable list with
  counts and bars, sorted by count; rating as a segmented control; deployments and a date range from the data.
  Counts are faceted: each list shows counts under the other choices. Choices on species, individual, rating,
  event and custom become Serval's filter expression (shown in the command); deployments or a date range add a
  subset table: Waxbill writes the chosen rows to `selection_<time>.csv` in the output folder (kept as a record
  of what was extracted) and runs `serval extract selection_<time>.csv -f path -v ALL_VALUES`. No Serval change is
  needed. With `--rename`, names are built from the rows in the subset, i.e. the matched tags. The advanced filter text stays available for users who want it.
- **Capture**: the deployment level picker counts distinct deployments at each level; the screen lists the
  deployments found with their record counts and date ranges, so a wrong level is obvious.
- **Translate**: from/to are chosen from the taglist's header; before the run, the species in the table that the
  taglist cannot translate are listed (missing vs. present with an empty translation) and block Run.
- **XMP update**: a preview groups the `xmp_update` rows per change (old → new, file count), marks conflicting
  rows with their row numbers and blocks Run until they are fixed.

## 4. Showing what the run will do

- Before Run: the number of files and records, the total size and the free space on the output disk (extract,
  align); an example output path for the chosen options; for align, the dry-run plan as a table (from → to).
- Samples: up to six thumbnails of matching media (extract), decoded and scaled in the background, cached;
  clicking one opens a simple viewer (image, file name, tags from the table). Videos show their first frame only
  if a decoder is available, otherwise a video tile.
- Sizes are summed in the background from file metadata; on a NAS this shows "正在计算大小…" until done.

## 5. Further GUI features (for discussion)

Ranked by value for the work Serval supports, cheapest first:

1. **Next steps**: after a run, the result offers the natural next command with the output prefilled (observe →
   extract / capture / XMP update; init → open the table in Caracal; capture → open counts).
2. **Result tables**: a sortable, filterable table view of any produced CSV, with column statistics; species stats
   and capture counts as simple charts (gpui-component `chart`).
3. **Quick look**: now part of [browse](browse.md) (grid, viewer, rating and species edits).
4. **Correction loop**: largely covered by [browse](browse.md) (species and rating edited per image and written in
   the background). A table editor for the `xmp_update` columns remains an option for bulk corrections.
5. **Project memory**: Waxbill remembers per data folder the last inputs, the identified species, the deployment
   level and the taglist, so a returning user starts where they left off.
