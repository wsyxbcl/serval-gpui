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
| 数据整理 Data wrangling | 重命名 Rename | `rename` |
| | 路径对齐 Align paths | `align` |
| | XMP 初始化 Initialise XMP | `xmp init` |
| 数据分析 Data processing | 浏览与精选 Browse and pick | `xmp get`, `xmp set`, `xmp write` ([browse](browse.md)) |
| | 标签获取 Get tags | `observe` |
| | 独立捕获 Independent captures | `capture` |
| | 物种名替换 Translate species | `translate` |
| | 元数据更新 Update metadata | `xmp update`, `xmp write` |
| | 元数据同步 Sync metadata | `xmp sync` |
| 工具 Tools | 媒体提取 Extract media | `extract` |
| | XMP 导出 Export XMP | `xmp copy` |
| | XMP 删除 Remove XMP | `xmp remove` |
| | (later) 数据脱敏 Anonymise data, Agent | — |
| (bottom) | 设置 Settings | |

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
- Each form remembers its last values per command (restored on next launch).

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
