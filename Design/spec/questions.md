# Questions

States: [`boards/questions.html`](../boards/questions.html) (identified species ③); conflicts in
[`boards/run-states.html`](../boards/run-states.html) ③. Proposal [W07](decisions.md); the flags are in
[serval-protocol](serval-protocol.md).

Everything Serval used to ask while running is a field in the form, filled before Run, except two questions that
depend on what the run finds: the conflict choice (asked once, after Serval has checked every target and before
it has written anything) and the identified species (asked after an observe scan, only if needed).

## 1. Plain parameters (capture)

- 独立性间隔: a number field with the unit "分钟", default 30, integer ≥ 1. Values above one week (10,080) show a
  quiet note, as the CLI does.
- Below it, a timeline from the user's own data makes the rule visible: one busy day of one deployment and species
  (the one with the most records), records as ticks, each independent capture as a band starting at its first
  record, and one line of result ("按 30 分钟：这一天 9 条记录 → 3 次独立捕获"). It updates as the interval or the
  measure-from choice changes, computed in Waxbill from the table.
- 间隔从哪条记录算起: two radio buttons with one line each — 上一条记录 (default, "每条记录都重新计时"),
  上一条独立记录 ("只在已计为独立的记录处重新计时").
- 分析对象: segmented 物种 / 个体.
- The other capture options (事件编号, 排除默认标签, Camtrap DP input) are checkboxes; the excluded tags are
  listed in the help line.

## 2. Path level pickers

Used for the deployment level (capture, observe debug table) and for the folders extract keeps.

- Waxbill reads one sample path itself: the first data row of the CSV, or the first media file under the folder.
  It is shown as segments (`.pathbar`): every folder is a clickable segment, the file is shown but not
  selectable.
- Deployment level: one segment is selected (`.seg.on`); Serval's detection rule preselects it and marks it
  "自动识别". Below: the chosen name and how many distinct deployments that level gives across the input
  ("表中共有 42 个部署"). If detection finds nothing, no segment is preselected and Run says "还不能运行 ·
  请在路径中选择部署所在的一层".
- Kept folders (extract): the user picks the top folder to keep; it and every folder down to the file are
  highlighted (`.seg.on`, `.seg.kept`). A leading "只要文件" segment means keep none. Below, an example output
  path updates immediately, including the rename pattern if that option is on.
- Paths in the input have different depths: the picker shows the sample and notes "路径深度不一致，按样例的层级
  计算" (the same limitation the CLI has); the level count is still shown so a wrong choice is visible.

## 3. Conflicts

When targets already exist with different content, Serval stops before writing and Waxbill shows a dialog:

- Title: "N 个目标已存在，内容不同". Body: how many can be copied directly ("其余 1,463 个可以直接复制"),
  how many are already done (only when not zero), and "还没有写入任何文件".
- A list of up to the first 200 conflicts (relative target, reason in words: 文件不同, XMP 不同, 旁边有无关的
  XMP), "… 另外 N 个".
- Three options as cards with one line each, skip preselected:
  跳过它们 ("保留已有的文件，只复制其余的"), 替换它们 ("用这次的文件覆盖已有的"), 另存为新名称 ("保留两份，
  新的命名为 IMG_0042_1.JPG（XMP 同名跟随）").
- Buttons: 取消 (nothing is written; the run bar returns to Ready) and 继续 (reruns with `--on-existing`).
- The choice is not remembered: the next conflict asks again, because it depends on what is in the folder.

## 4. Identified species

Observe's input is a folder; which images have several species and individual IDs is only known after the scan,
which is the run itself. So there is no field for it in the form.

- When Serval reports unresolved images (the `unresolved` event with the species involved, their image counts
  and the individual IDs seen), Waxbill shows a dialog after the run (board ③): the count, why it matters in one
  sentence, and the real species from those images as checkboxes (species with individual IDs preselected).
- "重新运行" reruns observe with `--id-species` for the checked species; the button shows the last run's duration
  because the scan is repeated. "先不处理" keeps the result (every individual paired with every species in those
  images) and the result banner stays ([run §3](run.md#3-results)).
- The choice is remembered per input folder and passed on the next run of that folder, so the question is asked
  at most once per folder.
- This case is rare (individually identified species seldom appear together), so most users never see it.
