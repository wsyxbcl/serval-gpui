# Run

States: [`boards/run-states.html`](../boards/run-states.html). Proposal [W06](decisions.md); output location
decided in [W02](decisions.md).

## 1. Run bar

The run bar always answers one of: what will happen, what is happening, what happened.

| State | Dot | Status line | Button |
| --- | --- | --- | --- |
| Not ready | grey | "还不能运行 · <the first missing thing>" | 运行, disabled |
| Ready | grey | "就绪 · <what will be processed>" (e.g. "将读取 13,782 个文件") | 运行 Ctrl+Enter |
| Running | teal | "<phase> · done / total · 剩余约 m:ss" and a progress bar | 暂停 Ctrl+. ("正在暂停…" until Serval has stopped) |
| Needs a decision | amber | "需要你的选择 · 还没有写入任何文件" | 运行, disabled while the dialog is open |
| Finished | green | "完成 · <one line>" | 再次运行 |
| Finished with warnings | amber | "完成，有 N 条警告" | 再次运行 |
| Failed | red | "失败 · <how far it got>" | 重新运行 |
| Paused | grey | "已暂停 · done / total" | 继续 Ctrl+Enter, and 取消 beside it |

- The exact command (with every flag Waxbill will pass) is shown under the status line before, during and after
  the run, and "复制命令" copies it. During a failure the secondary action is "复制日志".
- The remaining-time estimate appears only after 5 seconds of a phase with a known total.
- While running, the form is read-only (dimmed), the screen's sidebar entry shows a spinner, and other screens
  can be visited (their Run is disabled: "另一个命令正在运行"). Leaving the screen does not warn: nothing
  stops; only closing the window asks.
- Once the form says a different command from the last run's, the bar is about the form again (就绪); the last
  run's results stay on the screen until the next run.

## 2. Progress

- From `--progress json` events ([protocol §4](serval-protocol.md#4-progress-and-result-events)). Phase names
  are shown in words: scan 扫描, read 读取, plan 检查目标, copy 正在复制, move 正在移动, write 正在写入,
  analyse 正在分析.
- A phase without a total shows an indeterminate bar and the count so far.

## 3. Results

After a run, a result area appears at the top of the screen, above the form (the form stays editable for the
next run):

- A banner with the outcome as a sentence: what was done, how many, how long. Green (`--success-soft`) for
  finished, amber for warnings, red for failed, neutral (`--info-soft`) for paused.
- Failed and paused banners say the next step in plain words: "请选择其他输出位置后重新运行；已复制的 412 个
  文件会被识别并跳过。" Never a status code or a raw error as the only text; the raw error is in the log.
- Paused always says the run can be continued: Serval's writes are complete-or-absent (temp files and rename),
  and a rerun skips finished work.
- Warnings: the count in the banner title, the list (path, reason) below it, at most 200 rows with "另外 N 条".
- Files: one row per `output` event — name, size or row count, "预览" (CSV: a table view), "打开". A folder row
  opens the folder.
- Extra notices that need an action get their own banner with that action, e.g. unresolved identified species:
  "设置物种并重新运行" ([questions §4](questions.md#4-identified-species)).

The result area is kept until the next run of that screen starts, and is not restored after relaunch.

## 4. Log

- "运行日志" below the results, collapsed when the run succeeded, expanded when it failed.
- Plain text, monospace, warnings and errors coloured; it contains Serval's ordinary output (not the JSON events)
  and the command line at the top. "复制日志" copies it.

## 5. Output location

- Default: a sibling of the input named `<input>_serval_output/<serval_command>/` — e.g. input folder
  `/data/2024_spring` → `/data/2024_spring_serval_output/serval_observe/`; input CSV
  `/data/2024_spring_serval_output/serval_observe/tags.csv` → stays in `/data/2024_spring_serval_output/` (a CSV
  already inside a `*_serval_output` folder keeps that root).
- The field shows the resolved path with "默认"; "更改…" sets a custom one, which is remembered per command.
- Waxbill always passes `-o` and starts Serval with the output folder's parent as working directory, so nothing
  is written next to the app (finding G1).
- `xmp init` gets `-o` too (its table); `rename`, `xmp update`, `xmp sync`, `xmp remove` write only next to the
  data, and their run log goes to the output root.

## 6. Pausing and cancelling

- 暂停 sends an interrupt and, if Serval has not exited after 3 seconds, ends it. The banner then shows
  "已暂停"; nothing needs cleaning up. 继续 runs the same command again, which skips finished work.
- 取消 goes back to 就绪 and keeps what was written. When the run created its output folder and only copies into
  it (extract; align without moving), 取消 first asks, with "同时删除这次运行新建的文件夹" unticked: the folder
  offered is the highest one the run created, so nothing that existed before is removed.
- Closing the window while a command runs asks "停止并退出 / 继续运行".
