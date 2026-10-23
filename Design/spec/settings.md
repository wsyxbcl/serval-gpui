# Settings and Serval delivery

States: [`boards/settings.html`](../boards/settings.html); first run in [`boards/shell.html`](../boards/shell.html)
③. Proposals [W08](decisions.md), [W10](decisions.md).

## 1. Delivering Serval

Waxbill stays small (about 19 MB stripped today) and does not bundle or link Serval (Serval is about 66 MB
stripped, mostly Polars, and its XMP library is a C++ build).

Now (W08 revised 2026-10-13):

- **Shipped together, still two programs.** The release archive holds `waxbill.exe` and `serval.exe` side by
  side (with their READMEs and licences); Serval is not built into Waxbill. Unpacking the archive is the whole
  installation.
- Waxbill uses, in this order: the program chosen in Settings (or `--serval`) when it works; else `serval.exe`
  next to `waxbill.exe`; else `serval` on PATH: the first that works. One that has gone, is too old or does not
  start never hides a working one after it; when none works, the first one found says why. Only an explicit choice is saved, so unpacking a newer archive brings its Serval
  into use.
- With none of these, the card "选择 Serval 程序" appears in the content area, and Run is disabled with
  "需要 Serval".
- Waxbill checks the chosen program with `serval --version` and remembers it. An unsupported version shows a red
  banner with the required version and "选择其他程序…" ("请向项目组获取新版本").
- Waxbill does not download or update Serval and does not contact GitHub: users often cannot reach it.

Later (needs the project's own download server; until then, a new version is a new archive):

- Download on first run and an update check, from the project's server: platform archive plus a SHA-256 file,
  unpacked into Waxbill's data directory, verified with the checksum and `serval --version`, the previous version
  kept until the new one passes. Updates never install while a command runs.

## 2. Settings screen

Settings is a screen in the main window (sidebar bottom, Ctrl+,), not a separate window. Changes apply at once and
are saved; there is no Save button.

- SERVAL: version and state (dot), the program's path, compatibility, "更换程序…".
- 界面: 语言 (中文 / English, segmented), 外观 (跟随系统 / 浅色 / 深色).
- CARACAL: "允许 Caracal 从本机读取所选表格和媒体" (`caracal_bridge`, on by default; [caracal-bridge](caracal-bridge.md)).
- 关于: Waxbill version, licence, source link, 打开日志文件夹, icon attribution. `waxbill.log` beside settings.conf
  records start-up, the Serval found, every run and how it ended, review folders and Caracal bridge problems
  (UTC times, about 1 MB, then waxbill.old.log), for bug reports.

## 3. Stored settings

The existing config file (`settings.conf` in the platform config directory) keeps its format and gains keys:
`appearance`, per-folder memory (identified species, deployment level, taglist), and per-command last values.
