//! XMP 更新 Update XMP: `serval xmp update` (board: data.html update-preview). Before Run, the table's
//! changes are listed the way Serval applies them: per file, old → new, with the number of files; rows that give
//! one file two different new values are marked with their row numbers and block Run (Serval would refuse them).

use super::Plan;
use super::form::{Tone, accept_drop, card, field, help_line, segmented};
use crate::i18n::{Language, count, language, t};
use crate::repair::TableCheck;
use crate::table::Csv;
use crate::theme::TokensExt;
use gpui_kit::component::button::Button;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme, Sizable as _, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The fields `xmp update` can change: the tag column and its update column, and the `--tag-type` / `--datetime`
/// argument.
const MODES: [(&str, &str, &str); 4] = [
    ("species", "xmp_update", "species"),
    ("individual", "xmp_update", "individual"),
    ("rating", "xmp_update", "rating"),
    ("datetime", "xmp_update_datetime", "datetime"),
];

/// One kind of change in the preview: old → new and how many files get it.
#[derive(Debug, PartialEq)]
pub struct Change {
    pub old: String,
    pub new: String,
    pub files: usize,
    /// Rows that conflict with another row of the same file.
    pub conflict_rows: Vec<usize>,
}

#[derive(Debug, Default, PartialEq)]
pub struct Preview {
    pub changes: Vec<Change>,
    pub files: usize,
    /// Rows whose new value cannot be written (a datetime not in `yyyy-MM-dd HH:mm:ss`).
    pub invalid_rows: Vec<usize>,
    pub conflict_files: usize,
    /// Rows whose path is a media file: `xmp update` changes only XMP files and skips them.
    pub media_rows: Vec<usize>,
}

/// Group the update rows as Serval applies them (`group_update_rows`, `apply_tag_ops`, `single_target`): per file;
/// for tags, two rows replacing the same old value with different new values conflict; for datetime (one value per
/// file), any two different new values conflict.
pub fn preview(csv: &Csv, tag_column: &str, update_column: &str, single_value: bool) -> Preview {
    let (path, tag, update) = (
        csv.column("path"),
        csv.column(tag_column),
        csv.column(update_column),
    );
    let mut per_file: BTreeMap<&str, Vec<(usize, &str, &str)>> = BTreeMap::new();
    let mut preview = Preview::default();
    for (i, row) in csv.rows.iter().enumerate() {
        let new = csv.cell(row, update);
        if new.is_empty() {
            continue;
        }
        let line = i + 2;
        let file = csv.cell(row, path);
        if !file.to_ascii_lowercase().ends_with(".xmp") {
            preview.media_rows.push(line);
            continue;
        }
        if single_value && !is_datetime(new) {
            preview.invalid_rows.push(line);
            continue;
        }
        per_file
            .entry(csv.cell(row, path))
            .or_default()
            .push((line, csv.cell(row, tag), new));
    }
    // (old, new) → the files that get it, and the rows of it that conflict.
    type Group<'a> = (BTreeSet<&'a str>, Vec<usize>);
    let mut groups: BTreeMap<(String, String), Group> = BTreeMap::new();
    for (file, ops) in &per_file {
        let mut conflicting: Vec<usize> = Vec::new();
        for (i, (line, old, new)) in ops.iter().enumerate() {
            let clash = ops
                .iter()
                .take(i)
                .find(|(_, o, n)| n != new && (single_value || (o == old && !old.is_empty())));
            if let Some((first, _, _)) = clash {
                conflicting.extend([*first, *line]);
            }
        }
        conflicting.sort_unstable();
        conflicting.dedup();
        if !conflicting.is_empty() {
            preview.conflict_files += 1;
        }
        for (line, old, new) in ops {
            let old = if single_value {
                String::new()
            } else {
                old.to_string()
            };
            let entry = groups.entry((old, new.to_string())).or_default();
            entry.0.insert(file);
            if conflicting.contains(line) {
                entry.1.push(*line);
            }
        }
    }
    preview.files = per_file.len();
    preview.changes = groups
        .into_iter()
        .map(|((old, new), (files, conflict_rows))| Change {
            old,
            new,
            files: files.len(),
            conflict_rows,
        })
        .collect();
    preview.changes.sort_by_key(|c| std::cmp::Reverse(c.files));
    preview
}

fn is_datetime(text: &str) -> bool {
    let b = text.as_bytes();
    b.len() == 19
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            10 => *c == b' ',
            13 | 16 => *c == b':',
            _ => c.is_ascii_digit(),
        })
}

/// The write preview: `serval xmp write --dry-run` on the table (board browse.html browse-write-table).
enum WriteCheck {
    Idle,
    Checking,
    Done {
        summary: serde_json::Map<String, serde_json::Value>,
        problems: Vec<String>,
    },
    Failed(String),
}

const FIELDS: [(&str, &str, &str); 4] = [
    ("species", "物种", "Species"),
    ("individual", "个体", "Individual"),
    ("rating", "评分", "Rating"),
    ("datetime", "时间", "Time"),
];

pub struct UpdateForm {
    csv: Entity<InputState>,
    csv_check: Entity<TableCheck>,
    mode: usize,
    table: Option<Result<Csv, String>>,
    /// Write the whole table as it is (`xmp write`) instead of the xmp_update column.
    write: bool,
    fields: [bool; 4],
    create_missing: bool,
    check: WriteCheck,
    check_generation: usize,
    /// Its command is running: the inputs are disabled (the app also blocks the mouse).
    pub locked: bool,
    _subscriptions: Vec<Subscription>,
}

/// The table field's hint: 按 xmp_update 列修改 needs that column; 整表写入 takes a tags table as it is.
fn table_hint(write: bool) -> &'static str {
    if write {
        t(
            "选择要写入的标签表（例如从 Caracal 导出的）",
            "Choose the tags table to write (for example one exported from Caracal)",
        )
    } else {
        t(
            "选择填好 xmp_update 列的表",
            "Choose a table with the xmp_update column filled",
        )
    }
}

impl UpdateForm {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let csv = cx.new(|cx| InputState::new(window, cx).placeholder(table_hint(false)));
        let subscription = cx.subscribe_in(&csv, window, |this, _, event: &InputEvent, _, cx| {
            if matches!(event, InputEvent::Change) {
                this.read(cx);
            }
        });
        UpdateForm {
            csv_check: cx.new(|cx| TableCheck::new(&csv, cx)),
            csv,
            mode: 0,
            table: None,
            write: false,
            // Time is off by default: spreadsheet programs rewrite times (WPS saves 2026-1-5 08:03), and the table's
            // times rarely need to go back into the files.
            fields: [true, true, true, false],
            create_missing: true,
            check: WriteCheck::Idle,
            check_generation: 0,
            locked: false,
            _subscriptions: vec![subscription],
        }
    }

    fn csv_path(&self, cx: &App) -> Option<PathBuf> {
        let value = self.csv.read(cx).value();
        let value = value.trim();
        (!value.is_empty()).then(|| PathBuf::from(value))
    }

    fn read(&mut self, cx: &mut Context<Self>) {
        self.table = self
            .csv_path(cx)
            .filter(|p| p.is_file())
            .map(|p| Csv::read(&p));
        self.recheck(cx);
        cx.notify();
    }

    /// The way of writing; the table field's hint follows it.
    fn set_write(&mut self, write: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.write = write;
        self.csv.update(cx, |input, cx| {
            input.set_placeholder(table_hint(write), window, cx)
        });
    }

    /// Open a table in the write mode (from 查看媒体's 应用表格到文件).
    pub fn apply_table(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        self.set_write(true, window, cx);
        self.csv
            .update(cx, |input, cx| input.set_value(path, window, cx));
        self.read(cx);
    }

    fn write_args(&self, csv: &Path, dry_run: bool) -> Vec<String> {
        let mut args = vec!["xmp".to_string(), "write".into()];
        if self.fields.iter().any(|f| !f) {
            let fields: Vec<&str> = FIELDS
                .iter()
                .zip(self.fields)
                .filter(|(_, on)| *on)
                .map(|((name, ..), _)| *name)
                .collect();
            args.extend(["--fields".into(), fields.join(",")]);
        }
        if self.create_missing {
            args.push("--create-missing".into());
        }
        if dry_run {
            args.push("--dry-run".into());
        }
        args.extend([
            "--progress".into(),
            "json".into(),
            csv.to_string_lossy().into(),
        ]);
        args
    }

    /// Run the dry run again a moment after the table or the options change.
    pub fn recheck(&mut self, cx: &mut Context<Self>) {
        // Times matter only when the whole table is written with 时间: there Serval takes yyyy-MM-dd HH:mm:ss only.
        let times = self.write && self.fields[3];
        self.csv_check
            .update(cx, |check, cx| check.set_check_times(times, cx));
        self.check_generation += 1;
        let generation = self.check_generation;
        let program = cx
            .try_global::<crate::serval::ServalProgram>()
            .and_then(|p| p.0.clone());
        let csv = self.csv_path(cx).filter(|p| p.is_file());
        let (Some(program), Some(csv), true, true) =
            (program, csv, self.write, self.fields.iter().any(|f| *f))
        else {
            self.check = WriteCheck::Idle;
            return;
        };
        // A table Waxbill cannot read (not UTF-8) fails in Serval too; say why plainly.
        if let Some(Err(err)) = &self.table {
            self.check = WriteCheck::Failed(format!(
                "{} {err}",
                t("表格无法读取：", "cannot read the table:")
            ));
            return;
        }
        self.check = WriteCheck::Checking;
        let args = self.write_args(&csv, true);
        let cwd = csv
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(std::env::temp_dir);
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(300))
                .await;
            if this
                .update(cx, |this, _| this.check_generation != generation)
                .unwrap_or(true)
            {
                return;
            }
            let check = cx
                .background_executor()
                .spawn(async move { dry_run(&program, &args, &cwd) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.check_generation == generation {
                    this.check = check;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn preview(&self) -> Option<Preview> {
        let Some(Ok(csv)) = &self.table else {
            return None;
        };
        let (tag, update, _) = MODES[self.mode];
        let tag = if tag == "datetime" && csv.column("datetime").is_none() {
            "datetime_original"
        } else {
            tag
        };
        Some(preview(csv, tag, update, self.mode == 3))
    }

    fn missing_column(&self) -> Option<&'static str> {
        let Some(Ok(csv)) = &self.table else {
            return None;
        };
        let (tag, update, _) = MODES[self.mode];
        ["path", update, tag].into_iter().find(|c| {
            csv.column(c).is_none()
                && !(*c == "datetime" && csv.column("datetime_original").is_some())
        })
    }

    pub fn plan(&self, cx: &App) -> Plan {
        if self.write {
            return self.write_plan(cx);
        }
        let csv = self.csv_path(cx);
        let mut args = vec!["xmp".to_string(), "update".into()];
        match MODES[self.mode] {
            (_, _, "datetime") => args.push("--datetime".into()),
            (_, _, kind) => args.extend(["-t".into(), kind.into()]),
        }
        args.extend(["--progress".into(), "json".into()]);
        args.extend(csv.as_ref().map(|p| p.to_string_lossy().into_owned()));
        let preview = self.preview();
        let ready = match (&csv, &self.table) {
            (None, _) => Err(t("选择修改表", "choose the table").to_string()),
            (Some(_), None) => Err(t("找不到这个表格", "the table does not exist").to_string()),
            (_, Some(Err(err))) => Err(format!("{} {err}", t("表格无法读取：", "cannot read:"))),
            _ => match (self.missing_column(), preview) {
                (Some(column), _) => Err(match language() {
                    Language::Chinese => format!("表中没有 {column} 列"),
                    Language::English => format!("the table has no {column} column"),
                }),
                (None, Some(p)) if !p.media_rows.is_empty() => Err(match language() {
                    Language::Chinese => {
                        format!("{} 行的路径不是 XMP 文件", count(p.media_rows.len()))
                    }
                    Language::English => {
                        format!("{} rows are not XMP paths", count(p.media_rows.len()))
                    }
                }),
                (None, Some(p)) if !p.invalid_rows.is_empty() => Err(match language() {
                    Language::Chinese => format!("{} 行时间格式不对", p.invalid_rows.len()),
                    Language::English => {
                        format!("{} rows have a malformed time", p.invalid_rows.len())
                    }
                }),
                (None, Some(p)) if p.conflict_files > 0 => Err(match language() {
                    Language::Chinese => format!("{} 个冲突", p.conflict_files),
                    Language::English => format!("{} conflicts", p.conflict_files),
                }),
                (None, Some(p)) if p.files == 0 => {
                    Err(t("表中没有要修改的行", "no rows to change").to_string())
                }
                (None, Some(p)) => Ok(match language() {
                    Language::Chinese => format!("将修改 {} 个 XMP 文件", count(p.files)),
                    Language::English => format!("{} XMP files will change", count(p.files)),
                }),
                (None, None) => Err(String::new()),
            },
        };
        let cwd = csv
            .as_ref()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .filter(|p| p.is_dir())
            .unwrap_or_else(std::env::temp_dir);
        Plan { args, cwd, ready }
    }

    fn write_plan(&self, cx: &App) -> Plan {
        let csv = self.csv_path(cx);
        let args = match &csv {
            Some(csv) => self.write_args(csv, false),
            None => vec!["xmp".into(), "write".into()],
        };
        let changed = |summary: &serde_json::Map<String, serde_json::Value>| {
            summary.get("changed").and_then(|v| v.as_u64()).unwrap_or(0)
        };
        let ready = match (&csv, &self.check) {
            (None, _) => Err(t("选择要写入的表", "choose the table to write").to_string()),
            (Some(p), _) if !p.is_file() => {
                Err(t("找不到这个表格", "the table does not exist").to_string())
            }
            _ if !self.fields.iter().any(|f| *f) => {
                Err(t("至少选择一个字段", "choose at least one field").to_string())
            }
            (_, WriteCheck::Idle) => {
                Err(t("需要 Serval 才能检查", "Serval is needed to check").to_string())
            }
            (_, WriteCheck::Checking) => Err(t("正在检查…", "checking…").to_string()),
            (_, WriteCheck::Failed(err)) => Err(err.clone()),
            (_, WriteCheck::Done { problems, .. }) if !problems.is_empty() => {
                Err(match language() {
                    Language::Chinese => format!("{} 个文件有问题", count(problems.len())),
                    Language::English => format!("{} files have problems", count(problems.len())),
                })
            }
            (_, WriteCheck::Done { summary, .. }) if changed(summary) == 0 => {
                Err(t("文件已与表格一致", "the files already match the table").to_string())
            }
            (_, WriteCheck::Done { summary, .. }) => Ok(match language() {
                Language::Chinese => format!("将写入 {} 个文件", count(changed(summary) as usize)),
                Language::English => {
                    format!("{} files will be written", count(changed(summary) as usize))
                }
            }),
        };
        let cwd = csv
            .as_ref()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .filter(|p| p.is_dir())
            .unwrap_or_else(std::env::temp_dir);
        Plan { args, cwd, ready }
    }

    pub fn values(&self, _cx: &App) -> Vec<(&'static str, String)> {
        let flag = |b: bool| if b { "1" } else { "0" }.to_string();
        vec![
            ("mode", self.mode.to_string()),
            ("write", flag(self.write)),
            (
                "fields",
                self.fields
                    .iter()
                    .map(|f| if *f { '1' } else { '0' })
                    .collect(),
            ),
            ("create_missing", flag(self.create_missing)),
        ]
    }

    pub fn restore(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let settings = cx.default_global::<crate::settings::Settings>().clone();
        self.mode = settings
            .form_value("xmp_update", "mode")
            .and_then(|m| m.parse().ok())
            .filter(|&m| m < MODES.len())
            .unwrap_or(0);
        self.set_write(
            settings.form_value("xmp_update", "write") == Some("1"),
            window,
            cx,
        );
        self.create_missing = settings.form_value("xmp_update", "create_missing") != Some("0");
        if let Some(fields) = settings
            .form_value("xmp_update", "fields")
            .filter(|f| f.len() == 4)
        {
            for (i, c) in fields.chars().enumerate() {
                self.fields[i] = c == '1';
            }
        }
    }

    fn render_preview(&self, cx: &App) -> Option<Div> {
        let preview = self.preview()?;
        if self.missing_column().is_some() {
            return None;
        }
        let tokens = cx.tokens();
        let muted = cx.theme().muted_foreground;
        let row = |old: String, new: String, files: String, note: Option<String>, header: bool| {
            h_flex()
                .gap(tokens.px("space-3"))
                .px(tokens.px("space-3"))
                .py(px(5.))
                .when(header, |r| {
                    r.text_color(muted).text_size(tokens.px("text-sm"))
                })
                .child(div().w(relative(0.35)).truncate().child(old))
                .child(div().w(relative(0.35)).truncate().child(new))
                .child(div().flex_1().text_right().child(files))
                .when_some(note, |r, note| {
                    r.child(
                        div()
                            .text_color(tokens.color("danger"))
                            .text_size(tokens.px("text-sm"))
                            .child(note),
                    )
                })
        };
        let mut table = v_flex()
            .rounded(tokens.px("radius-sm"))
            .border_1()
            .border_color(cx.theme().border)
            .bg(tokens.color("surface"))
            .child(row(
                t("原值", "Old").into(),
                t("新值", "New").into(),
                t("文件数", "Files").into(),
                None,
                true,
            ));
        for change in preview.changes.iter().take(100) {
            let old = if self.mode == 3 {
                t("（原时间）", "(current time)").to_string()
            } else if change.old.is_empty() {
                t("（新增）", "(added)").to_string()
            } else {
                change.old.clone()
            };
            let note = (!change.conflict_rows.is_empty()).then(|| match language() {
                Language::Chinese => format!("冲突：第 {} 行", join_rows(&change.conflict_rows)),
                Language::English => format!("conflict: rows {}", join_rows(&change.conflict_rows)),
            });
            table = table.child(
                div()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(row(
                        old,
                        change.new.clone(),
                        count(change.files),
                        note,
                        false,
                    )),
            );
        }
        let mut summary = match language() {
            Language::Chinese => format!("将修改 {} 个 XMP 文件", count(preview.files)),
            Language::English => format!("{} XMP files will change", count(preview.files)),
        };
        if preview.conflict_files > 0 {
            summary.push_str(&match language() {
                Language::Chinese => format!(
                    "；{} 个文件的修改互相冲突，需要先在表里改好",
                    preview.conflict_files
                ),
                Language::English => format!(
                    "; the changes to {} files conflict and need fixing in the table first",
                    preview.conflict_files
                ),
            });
        }
        if !preview.media_rows.is_empty() {
            summary.push_str(&match language() {
                Language::Chinese => format!(
                    "；{} 行的路径是媒体文件而不是 XMP，XMP 更新只修改 XMP：请在标签获取中选择「XMP sidecar」生成表格",
                    count(preview.media_rows.len())
                ),
                Language::English => format!(
                    "; {} rows point at media files, not XMP; this changes XMP files only: make the table with Get tags reading XMP sidecars",
                    count(preview.media_rows.len())
                ),
            });
        }
        if !preview.invalid_rows.is_empty() {
            summary.push_str(&match language() {
                Language::Chinese => format!(
                    "；第 {} 行的时间不是 yyyy-MM-dd HH:mm:ss",
                    join_rows(&preview.invalid_rows)
                ),
                Language::English => format!(
                    "; rows {} are not yyyy-MM-dd HH:mm:ss",
                    join_rows(&preview.invalid_rows)
                ),
            });
        }
        summary.push('。');
        let blocked = preview.conflict_files > 0
            || !preview.invalid_rows.is_empty()
            || !preview.media_rows.is_empty();
        Some(
            v_flex()
                .gap(tokens.px("space-2"))
                .child(table)
                .child(help_line(
                    summary,
                    if blocked { Tone::Danger } else { Tone::Quiet },
                    cx,
                )),
        )
    }
}

fn join_rows(rows: &[usize]) -> String {
    let mut text = rows
        .iter()
        .take(10)
        .map(|r| r.to_string())
        .collect::<Vec<_>>()
        .join(t("、", ", "));
    if rows.len() > 10 {
        text.push_str(" …");
    }
    text
}

impl Render for UpdateForm {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.tokens().clone();
        let csv_path = self.csv_path(cx);
        let input = card(cx).child(accept_drop(
            field(t("修改表", "Changes table"), cx)
                .child(
                    h_flex()
                        .gap(tokens.px("space-2"))
                        .child(div().flex_1().child(Input::new(&self.csv).large().disabled(self.locked).bg(tokens.color("surface"))))
                        .child(Button::new("choose-csv").label(t("选择…", "Choose…")).outline().large().on_click(cx.listener(
                            |this, _, window, cx| {
                                let paths = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: false, prompt: None });
                                let input = this.csv.clone();
                                cx.spawn_in(window, async move |_, cx| {
                                    if let Ok(Ok(Some(paths))) = paths.await
                                        && let Some(path) = paths.into_iter().next()
                                    {
                                        let _ = input.update_in(cx, |input, window, cx| {
                                            input.set_value(path.to_string_lossy().to_string(), window, cx);
                                            cx.emit(InputEvent::Change);
                                        });
                                    }
                                })
                                .detach();
                            },
                        ))),
                )
                .child(self.csv_check.clone())
                .child(help_line(
                    if self.write {
                        t(
                            "标注好的表或从 Caracal 导出的表：表中的标签写入 XMP，空白单元格不改动文件。",
                            "A table filled in by labellers, or exported from Caracal: its labels are written into the XMP as they are; empty cells leave the file unchanged.",
                        )
                    } else {
                        t(
                            "在 xmp_update 列填新值（时间填 xmp_update_datetime 列）；原值为空的行表示新增。",
                            "Fill new values in xmp_update (times in xmp_update_datetime); rows with an empty old value add a tag.",
                        )
                    },
                    Tone::Quiet,
                    cx,
                )),
            &self.csv,
            cx,
        ));
        let way = {
            let this = cx.entity().downgrade();
            card(cx).child(
                field(t("写入方式", "How"), cx).child(h_flex().child(segmented(
                    "update-way",
                    &[
                        t("按 xmp_update 列修改", "Change by the xmp_update column"),
                        t("整表写入", "Write the whole table"),
                    ],
                    self.write as usize,
                    move |index, window, cx| {
                        let _ = this.update(cx, |this, cx| {
                            this.set_write(index == 1, window, cx);
                            this.recheck(cx);
                            cx.notify();
                        });
                    },
                ))),
            )
        };
        if self.write {
            return v_flex()
                .gap(tokens.px("space-4"))
                .child(input)
                .child(way)
                .child(self.render_write(cx));
        }
        let this = cx.entity().downgrade();
        let changes = card(cx)
            .child(
                field(t("修改内容", "What to change"), cx).child(h_flex().child(segmented(
                    "update-mode",
                    &[
                        t("物种", "Species"),
                        t("个体", "Individual"),
                        t("评分", "Rating"),
                        t("时间", "Time"),
                    ],
                    self.mode,
                    move |index, _, cx| {
                        let _ = this.update(cx, |this, cx| {
                            this.mode = index;
                            cx.notify();
                        });
                    },
                ))),
            )
            .when_some(self.missing_column(), |card, column| {
                card.child(help_line(
                    match language() {
                        Language::Chinese => format!("表中没有 {column} 列。"),
                        Language::English => format!("The table has no {column} column."),
                    },
                    Tone::Danger,
                    cx,
                ))
            })
            .children(self.render_preview(cx))
            .when_some(csv_path.filter(|p| p.is_file()), |card, path| {
                card.child(
                    h_flex().child(
                        Button::new("open-table")
                            .outline()
                            .compact()
                            .label(t("打开表格", "Open the table"))
                            .on_click(move |_, _, cx| cx.open_with_system(&path)),
                    ),
                )
            });
        v_flex()
            .gap(tokens.px("space-4"))
            .child(input)
            .child(way)
            .child(changes)
    }
}

impl UpdateForm {
    fn render_write(&self, cx: &mut Context<Self>) -> Div {
        let tokens = cx.tokens().clone();
        let muted = cx.theme().muted_foreground;
        let fields = h_flex()
            .gap(tokens.px("space-5"))
            .children(FIELDS.iter().enumerate().map(|(i, (_, zh, en))| {
                Checkbox::new(("write-field", i))
                    .label(t(zh, en))
                    .checked(self.fields[i])
                    .on_click(cx.listener(move |this, on: &bool, _, cx| {
                        this.fields[i] = *on;
                        this.recheck(cx);
                        cx.notify();
                    }))
            }));
        let create = Checkbox::new("create-missing")
            .label(t(
                "没有 XMP 的媒体文件，从媒体文件创建",
                "Create missing XMP files from the media files",
            ))
            .checked(self.create_missing)
            .on_click(cx.listener(|this, on: &bool, _, cx| {
                this.create_missing = *on;
                this.recheck(cx);
                cx.notify();
            }));
        let number = |summary: &serde_json::Map<String, serde_json::Value>, key: &str| {
            summary.get(key).and_then(|v| v.as_u64()).unwrap_or(0) as usize
        };
        let preview: Div = match &self.check {
            WriteCheck::Idle => help_line(
                t(
                    "选择表格后自动检查，不会写入。",
                    "Checked automatically once a table is chosen (--dry-run, nothing written).",
                ),
                Tone::Quiet,
                cx,
            ),
            WriteCheck::Checking => help_line(
                t("正在检查每个文件…", "Checking every file…"),
                Tone::Quiet,
                cx,
            ),
            WriteCheck::Failed(err) => help_line(err.clone(), Tone::Danger, cx),
            WriteCheck::Done { summary, problems } => {
                let row = |label: String, value: usize, strong: bool| {
                    h_flex()
                        .justify_between()
                        .px(tokens.px("space-3"))
                        .py(px(5.))
                        .border_t_1()
                        .border_color(cx.theme().border)
                        .when(strong, |r| r.font_semibold())
                        .child(label)
                        .child(count(value))
                };
                let fields = summary
                    .get("fields")
                    .and_then(|f| f.as_object())
                    .cloned()
                    .unwrap_or_default();
                let mut table = v_flex()
                    .rounded(tokens.px("radius-sm"))
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(tokens.color("surface"))
                    .child(
                        h_flex()
                            .justify_between()
                            .px(tokens.px("space-3"))
                            .py(px(5.))
                            .text_size(tokens.px("text-sm"))
                            .text_color(muted)
                            .child(t("字段", "Field"))
                            .child(t("将修改的文件", "Files to change")),
                    );
                for (i, (key, zh, en)) in FIELDS.iter().enumerate() {
                    if self.fields[i] {
                        table = table.child(row(
                            t(zh, en).to_string(),
                            fields.get(*key).and_then(|v| v.as_u64()).unwrap_or(0) as usize,
                            false,
                        ));
                    }
                }
                table = table
                    .child(row(
                        t(
                            "其中会移除表中没有的值",
                            "of which lose values not in the table",
                        )
                        .to_string(),
                        number(summary, "removed_values"),
                        false,
                    ))
                    .child(row(
                        t("共计（不重复）", "In all (distinct)").to_string(),
                        number(summary, "changed"),
                        true,
                    ));
                let mut notes = Vec::new();
                notes.push(match language() {
                    Language::Chinese => format!(
                        "已检查 {} 个文件，{} 个已与表格一致。表格中的值覆盖文件中的值，空白单元格不改动；每个文件保留一份原始备份。",
                        count(number(summary, "changed") + number(summary, "already")),
                        count(number(summary, "already"))
                    ),
                    Language::English => format!(
                        "Checked {} files; {} already match. The table's values replace the files' values; empty cells change nothing. Each file keeps one backup of its original.",
                        count(number(summary, "changed") + number(summary, "already")),
                        count(number(summary, "already"))
                    ),
                });
                if number(summary, "created_sidecars") > 0 {
                    notes.push(match language() {
                        Language::Chinese => format!("还有 {} 个文件没有 XMP sidecar，将从媒体文件创建。", count(number(summary, "created_sidecars"))),
                        Language::English => format!("{} files have no XMP sidecar yet; they are created from the media files.", count(number(summary, "created_sidecars"))),
                    });
                }
                v_flex()
                    .gap(tokens.px("space-2"))
                    .child(table)
                    .children(notes.into_iter().map(|n| help_line(n, Tone::Quiet, cx)))
                    .when(!problems.is_empty(), |col| {
                        col.child(help_line(
                            t(
                                "这些文件需要先在表里改好：",
                                "Fix these in the table first:",
                            ),
                            Tone::Danger,
                            cx,
                        ))
                        .children(
                            problems
                                .iter()
                                .take(50)
                                .map(|p| help_line(p.clone(), Tone::Danger, cx)),
                        )
                    })
            }
        };
        card(cx)
            .child(
                field(t("写入的字段", "Fields to write"), cx)
                    .child(fields)
                    .when(!self.fields[3], |f| {
                        f.child(help_line(
                            t(
                                "时间默认不写入：表格软件保存时常会改动时间格式。",
                                "Time is not written by default: spreadsheet programs often change its format when saving.",
                            ),
                            Tone::Quiet,
                            cx,
                        ))
                    })
                    .child(create),
            )
            .child(field(t("将要做的修改", "What will change"), cx).child(preview))
    }
}

/// `serval xmp write --dry-run`: the summary, and each file's problem (conflicting or invalid cells, a missing
/// sidecar) as "path (rows …): message".
fn dry_run(program: &Path, args: &[String], cwd: &Path) -> WriteCheck {
    use crate::runner::Event;
    let output = match crate::runner::background_command(program)
        .args(args)
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .output()
    {
        Ok(output) => output,
        Err(err) => return WriteCheck::Failed(format!("{}: {err}", program.display())),
    };
    let mut summary = None;
    let mut problems = Vec::new();
    let mut failure = None;
    for line in String::from_utf8_lossy(&output.stderr).lines() {
        match serde_json::from_str::<Event>(line) {
            Ok(Event::Summary(map)) => summary = Some(map),
            Ok(Event::Error {
                message,
                path: Some(path),
            }) => problems.push(format!("{path}: {message}")),
            Ok(Event::Error {
                message,
                path: None,
            }) => failure = Some(message),
            _ => {}
        }
    }
    match (summary, problems.is_empty()) {
        (Some(summary), _) => WriteCheck::Done { summary, problems },
        (None, false) => WriteCheck::Done {
            summary: Default::default(),
            problems,
        },
        (None, true) => WriteCheck::Failed(
            failure.unwrap_or_else(|| t("检查失败", "the check failed").to_string()),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::preview;
    use crate::table::Csv;

    #[test]
    fn preview_groups_per_file_and_marks_conflicts_like_serval() {
        let csv = Csv {
            header: ["path", "species", "xmp_update"].map(String::from).to_vec(),
            rows: [
                ["a.xmp", "Fox", "Red fox"],
                ["a.xmp", "Fox", "Red fox"], // two individuals of one species: same change, no conflict
                ["b.xmp", "Fox", "Red fox"],
                ["b.xmp", "", "Deer"],
                ["c.xmp", "Fox", "Red fox"],
                ["c.xmp", "Fox", "Vulpes"], // rows 6 and 7 disagree for file c
                ["d.xmp", "Wolf", ""],
                ["e.jpg", "Fox", "Red fox"], // a media path: skipped by Serval,
            ]
            .iter()
            .map(|r| r.map(String::from).to_vec())
            .collect(),
        };
        let p = preview(&csv, "species", "xmp_update", false);
        assert_eq!((p.files, p.conflict_files), (3, 1));
        let fox = p.changes.iter().find(|c| c.new == "Red fox").unwrap();
        assert_eq!((fox.files, fox.conflict_rows.clone()), (3, vec![6]));
        assert!(
            p.changes
                .iter()
                .any(|c| c.old.is_empty() && c.new == "Deer" && c.files == 1)
        );
        assert_eq!(p.media_rows, vec![9]);
    }
}
