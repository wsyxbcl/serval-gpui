# Browse and pick

States: [`boards/browse.html`](../boards/browse.html). Proposal [W16](decisions.md); Serval commands in
[serval-protocol §5](serval-protocol.md#5-writing-labels).

## 1. Why

Today, after the labellers return a filled table, users run `xmp init` and `xmp update`, then import the whole
folder into digiKam to filter species across deployments and rate the best images. digiKam has to scan every file
under the folder into its database before filtering, and it filters by album rather than across deployments.

Waxbill starts from the table instead: the labels are already in it, so opening needs no scan, filtering is
across all deployments, and only what is on screen is read from disk. It is not a photo library: no database, no
AI, no maps (Biowatch covers those, [W11](decisions.md)). One person, one machine, local or mounted files.

## 2. Data

- **Source**: a tags table (`tags.csv` from observe, or a table filled by labellers), checked and repaired as in
  [data §2](data.md#2-table-check-and-repair), or a folder: Waxbill runs `serval observe -x --progress json` on it
  and opens the result (seconds for a few thousand sidecars on a local disk). With a table open, a 文件夹 filter
  (folders from the paths, with counts) browses by directory without leaving the table.
- **Deployment**: tags tables have no deployment column, so the deployment comes from a path level, chosen with the
  same picker as capture ([questions §2](questions.md#2-path-level-pickers)) and remembered per folder; the
  filter panel shows the current level ("部署：路径第 4 层 · 更改").
- **In memory**: one record per media file (rows of the same path are merged: species list, individuals list,
  rating, datetime, deployment from the path level, sidecar path). 50,000 files take a few tens of MB. No
  database; nothing is persisted except the thumbnail cache and per-folder memory ([settings §3](settings.md)).
- **Truth**: the XMP sidecars. After Waxbill writes, its records hold what Serval reports is on disk. The user's
  table is never modified; "导出表格" writes a new UTF-8 table from the current records.
- **Labels not yet in the files** (a freshly filled table): "写入文件…" runs `serval xmp write --dry-run`, shows
  what would change per field and which files would lose values, then `serval xmp write --create-missing`
  (board ④). Rating and species edits before that are kept in the records and written by the same run.

## 3. Filters and grid

- Filters: species, rating, deployment, folder, date range, individual, and later custom. Rating is a checkbox
  per value (未评分, 1–5) with counts, plus shortcuts 全部 / ≥3 / ≥4 / 5, so any combination works (e.g. unrated
  and 1 star, to review rejects). Counts are faceted (each list counts under the other filters). Filtering 50,000 records takes
  under 50 ms; it runs on every change.
- Order: by time (default), by deployment then time, by file name.
- Grid: tiles show the image, species (and individual), deployment and stars; size adjustable. Keyboard: arrows
  move, Space/Enter opens the viewer, 1–5 rate the focused or selected images, 0 clears, Shift/Ctrl click selects (⌘ on macOS).
- **Edited images that no longer match** stay in place, dimmed with "不符合筛选", until "刷新" or the next filter
  change (board ③). Counts update at once. This keeps the grid from jumping under the cursor while rating.

## 4. Viewer

- The image at screen size, with the file name and dimensions; ← → move through the current filter order;
  the next and previous images are prepared ahead.
- Side panel: rating (1–5, 0 clears), species and individual as tags (✕ removes; "＋ 添加" with suggestions from
  the table's values and, when chosen, the taglist), time, deployment; "在文件夹中显示", "用默认程序打开".
- Videos: a video tile with the duration when known; "用默认程序打开" plays it in the system player. GPUI has no
  video element (its `surface` is macOS-only), so playback stays external. A frame preview for tiles is planned
  for later (§6).

## 5. Writing edits

- Every edit changes the record at once and joins a write queue. The queue is written when the user moves to
  another image, after 2 seconds without edits, and before the screen or window closes, as one
  `serval xmp set --edits - --json --backup first --log-to <session log>` call. Repeated edits of one file
  collapse to the last value, so each file is written once per flush.
- Operations: rating → `set`; the species or individual list edited in the viewer → `set` with `expect` = the
  list shown before the edit; "＋ 添加" on a multi-selection → `add`.
- The save indicator: "✓ 已保存", "正在写入 N 项…", or "写入失败 · 重试" with the reason (e.g. the file was changed
  elsewhere: the record is refreshed with what Serval reports and the edit is shown for the user to redo).
- Undo (Ctrl+Z, ⌘Z on macOS) restores the previous value through the same queue; it works before and after the write.
- Missing sidecars (a table listing media files with no `.xmp` yet) are created by the write (`--create-missing`),
  once per file, without a separate init step.

## 6. Thumbnails

- Made only for tiles on screen plus one screen ahead, in a background pool; tiles show a placeholder until
  ready. Measured on 4608×2592 camera-trap JPEGs (median 1.6 MB, local SSD): full decode 26 ms, decoding
  directly at 1/8 size 12 ms per image; none of the sample cameras embed an EXIF preview. A screen of ~60 tiles
  is therefore well under a second; on an HDD or NAS, reading the files dominates.
- Cached on disk in Waxbill's cache directory, keyed by path + size + modification time, as JPEG (quality 80,
  320 px wide); revisiting or reopening is instant. The cache has a size limit (default 1 GB, about 100,000
  thumbnails; oldest removed first) and can be cleared in Settings.
- Format, measured on the same images (320×180): JPEG 8.7 KB, encode 0.1 ms; lossy WebP 5.5 KB, 3 ms; AVIF
  4.8 KB, 19 ms; lossless WebP 53 KB. JPEG needs no extra dependency and GPUI decodes it natively; lossy WebP
  would add the libwebp C library for ~35% less disk, and GPUI cannot display AVIF.
- Display: GPUI's `img()` loads in the background with a loading placeholder and an image cache; Waxbill feeds
  it its own scaled decodes. Formats GPUI decodes: PNG, JPEG, WebP, GIF, BMP, TIFF.
- Video frames (later): all test videos are H.264 (MP4, MOV, AVI), so a frame needs a real decoder. Options:
  the operating system's thumbnailer (Windows Explorer's, macOS AVFoundation), which covers most users with no
  added weight; or a bundled H.264 decoder (`openh264`, ~1–2 MB) with MP4/AVI parsing for all platforms; on
  Linux, a system `ffmpeg` if present.
- The decoder must support scaled JPEG decoding (to verify when choosing the crate); other formats (PNG) are
  decoded fully and scaled.

## 7. Selection actions

- "复制到…": copies the selected media with their sidecars to a chosen folder: Waxbill writes the selected rows
  to a subset table and runs `serval extract <subset> -f path -v ALL_VALUES` (pairs kept, interrupted copies
  resume, conflicts asked once, [questions §3](questions.md#3-conflicts)).
- "在文件夹中显示" (the first selected file), "用默认程序打开" (one file), "设为 ★" (rates all selected).
- Batch copying by tags without browsing stays in Extract ([data §3](data.md#3-choosing-from-real-values)).
