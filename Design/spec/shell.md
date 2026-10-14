# Shell

States: [`boards/shell.html`](../boards/shell.html). Proposal [W05](decisions.md); sizes are tokens in
[`boards/tokens.css`](../boards/tokens.css).

## 1. Window

- One main window, default `--window-default-w` × `--window-default-h`, minimum `--window-min-w` ×
  `--window-min-h`. Size and position are restored on the next launch.
- Title bar (`--titlebar-h`): the brand mark and "Waxbill" on the left; on the right the Serval status pill and
  the settings button. The pill is the only place Serval's state is shown outside Settings:
  ready (`Serval 0.8.0`, green dot), missing or unsupported (red); "update available" (amber) comes with the
  download server ([settings §1](settings.md#1-delivering-serval)). Clicking it opens Settings.
- No About window: version, licence and source link are at the bottom of Settings.

## 2. Sidebar

Commands grouped by kind of work. Labels are the task, not the CLI name:

| Group | Item | Serval command |
| --- | --- | --- |
| 数据分析 Data analysis | 标签获取 Get tags | `observe` |
| | 独立捕获 Independent captures | `capture` |
| | 查看媒体 View media (was 浏览与精选) | `xmp get`, `xmp set`, `xmp write` ([browse](browse.md)) |
| 元数据 Metadata | XMP 初始化 Initialise XMP | `xmp init` |
| | XMP 更新 Update XMP (was 元数据更新) | `xmp update`, `xmp write` |
| | 元数据同步 Sync metadata | `xmp sync` |
| 文件导出 Export | 媒体提取 Extract media | `extract` |
| | XMP 导出 Export XMP | `xmp copy` |
| 数据整理 Data wrangling | 重命名 Rename | `rename` |
| | 路径对齐 Align paths | `align` |
| 工具 Tools | XMP 删除 Remove XMP | `xmp remove` |
| | 物种名替换 Translate species | `translate` |
| | (later) 数据脱敏 Anonymise data, 添加水印 Watermark, Agent | — |
| (bottom) | 设置 Settings | |

- Order and names from the 2026-10-14 review. The groups scroll on a short window; Settings stays at the bottom.
- Width `--sidebar-w`. The selected item has the `--sidebar-active` fill and a 3px `--accent` marker.
- A running command shows a small spinner next to its item, so the user can look at other screens meanwhile.
  Only one command runs at a time; Run on another screen is disabled with "另一个命令正在运行".
- `tags2img` is not listed (a digiKam helper used rarely); it can be added under Tools if needed.

## 3. Screen anatomy

Every command screen, top to bottom:

1. Title (`--text-xl`) and one sentence saying what the command produces, with a Help link that opens the
   command's help (from `serval <cmd> --help`, localised where available).
2. Result area (only after a run): see [run §3](run.md#3-results).
3. Form in cards: inputs first, then options, then output location, then "高级选项" collapsed.
4. Run bar (sticky, `--runbar-h`): status line, the exact command (one line, monospace, truncated in the
   middle), "复制命令", and the one primary button. See [run §1](run.md#1-run-bar).

Rules:

- One primary button per screen, always in the run bar. Nothing else on the screen uses `--primary` fill.
- Controls say what they are: radio buttons for exclusive choices, checkboxes for independent flags, a
  segmented control for short exclusive sets (≤ 4 options), a text field with a unit for numbers.
- Paths: a field plus "选择…"; folders can be dropped onto the field. A path that does not exist or has the
  wrong type is marked at once (`.input.invalid` and a one-line reason), not at Run.
- Under an input folder or CSV, one quiet line confirms what Waxbill found (e.g. "找到 12,480 张图片、1,302
  个视频", "14,106 行"), read without running Serval.
- Each form keeps everything while Waxbill runs. Across launches it remembers its options (checkboxes, choices,
  intervals) but not paths: data moves, a stale path only gives errors at start, and an empty field shows the
  drop hint. The one exception is 物种名替换's taglist, a reference file that rarely moves. Previews and
  confirmations are not remembered either (2026-10-14).

## 4. Keyboard

- Shortcuts follow the platform: Ctrl on Windows and Linux, ⌘ on macOS, and are shown that way in buttons and
  hints ("运行 Ctrl+Enter" / "运行 ⌘⏎"). Boards show the Windows form, the most common among users.
- Ctrl+Enter runs; Ctrl+. stops a running command. The button and the shortcut do the same thing.
- Ctrl+1…Ctrl+9 select the first nine sidebar items; Ctrl+, opens Settings.
- Tab order follows the form top to bottom, then the run bar.

## 5. Not in the shell

- No embedded terminal. The run log is plain text (see [run §4](run.md#4-log)); Serval's questions are fields or
  a dialog ([questions](questions.md)).
- No help mode toggle: help is a link per screen and a one-line description under fields that need one.
