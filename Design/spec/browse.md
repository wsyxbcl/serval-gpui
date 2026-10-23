# Browse and pick

Proposal [W16](decisions.md) (revised 2026-10-10); later steps [W19](decisions.md), [W20](decisions.md). Board:
to be drawn for the hand-off screen (the grid board was removed with the grid).

## 1. Why, and what changed

The case: someone has tagged media (or tagged XMP, or only a filled tags table from others), wants to review a
part of it quickly ("all 5-star snow leopards"), fix or add labels ("find the other 5-star snow leopards"),
perhaps copy the files out, and pass the result on as a tags table (CamtrapDP later).

A native grid and viewer were built in Waxbill and removed: decoding was slow in debug builds and acceptable in
release, but GPUI cannot play video, and a media browser keeps growing, against Waxbill being a small, fast tool
on top of Serval. Waxbill now **chooses**; other tools **show**:

- **Caracal** (hosted, in the browser) views and edits; it already reads tables in WebAssembly, and the browser
  decodes photos and plays MP4/MOV. Its review panel is Caracal's work.
- **The file manager** shows a chosen set as a folder (Windows first: Explorer thumbnails, Photos, the system
  player).
- **Serval** writes: an edited table goes back into the XMP files with `xmp write`.

The table is the working document; the XMP files are updated deliberately, with a preview.

## 2. The screen

- **Table**: a tags.csv from 标签获取, a labellers' table, or one exported from Caracal. One quiet line: rows,
  files, species, date range. The table is remembered.
- **Choice** (挑选, its own card): species as checkboxes with counts (faceted under the rating), rating as a
  segmented control (全部 / ≥ 3 / ≥ 4 / 5, as in 媒体提取). The line below says how many files (and videos) are
  chosen. It feeds both the review folder and Caracal; 应用表格到文件 takes the whole table.
- **Three ways out**, one card each, no run bar:
  1. **在资源管理器中查看所选** — §3.
  2. **在 Caracal 中查看和编辑** — opens Caracal at `#review&table=<the table's full path>`, on the choice: its
     rows (every row of each chosen file, in table order, the header as read, UTF-8 with a BOM) are saved as
     `serval_review/<date>_<species>.csv` (`_2`, … rather than overwrite) and that table is opened, so a large
     table loads only what the user wants to look at. With nothing chosen, the whole table. A page cannot open
     a file from its path; the path is a hint: Caracal finds the table inside a folder the user granted before,
     or names the file to pick. The fragment never leaves the browser; nothing is copied to the clipboard, and
     Caracal is not bundled. The user reviews, edits and exports tags.csv. (W19 would load both without
     picking.) The same link opens observe's tags table in `#species` and xmp init's table in `#time`, from the
     result rows.
  3. **应用表格到文件** — opens 元数据更新 in 整表写入 with this table (§4).

## 3. Review folder (Explorer)

- The chosen media and their sidecars are **hard-linked** into `<data>_serval_output/serval_review/<date>_<species>/`
  and the folder is opened. Names are `<species>__<individuals>__<deployment>__<file name>`, the same as Caracal's
  browser copies and `serval extract --rename` (individuals left out when empty, `untagged` without species); a
  name used twice gets `_1`, `_2`. (Aligned with Caracal 2026-10-23.)
- Hard links take no space; Explorer, Photos and the player treat them as the real files, and copying out of the
  folder copies the real files. Edits Serval makes later replace the sidecar, so a review folder is a snapshot.
- Once the folder is made, a warning says the files are the originals: rotating or editing there changes them;
  deleting removes only the link. (Wording to be refined before release.)
- Hard links need the same drive. Over SMB it depends on the server: a Synology share worked in the 2026-10-12
  test; many other network shares do not allow them (symbolic links need admin rights
  on Windows; Photos does not step through `.lnk` shortcuts). exFAT and FAT32 (most external disks and cards) have
  no hard links at all. Then nothing is made; the message says why — the disk's file system (Windows:
  `GetVolumeInformationW`; Linux: the mount table), or a network location (`GetDriveTypeW`, a `\\server` path),
  or else the system's error — and offers Caracal or "用媒体提取复制", which opens 媒体提取 with the same species
  and rating, which copies.

## 4. Writing a table back (元数据更新 · 整表写入)

- `serval xmp write --dry-run` runs in the background as soon as the table or the options change; its summary
  fills a table: files to change per field (物种, 个体, 评分, 时间), of which lose values that are not in the
  table, the distinct total, how many already match, how many sidecars will be created. Files with conflicting
  or invalid cells are listed and block Run.
- Options: the fields to write (species, individual and rating by default; time is off because spreadsheet
  programs rewrite times when saving), and "没有 XMP 的媒体文件，从媒体文件创建" (`--create-missing`,
  on by default). Run is `serval xmp write` with the same options.
