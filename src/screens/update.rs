//! 元数据更新 Update metadata: `serval xmp update` (board: data.html update-preview). Before Run, the table's
//! changes are listed the way Serval applies them: per file, old → new, with the number of files; rows that give
//! one file two different new values are marked with their row numbers and block Run (Serval would refuse them).

use super::Plan;
use super::form::{Tone, card, field, help_line, segmented};
use crate::i18n::{Language, count, language, t};
use crate::table::Csv;
use crate::theme::TokensExt;
use gpui_kit::component::button::Button;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme, Sizable as _, h_flex, v_flex};
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

pub struct UpdateForm {
    csv: Entity<InputState>,
    mode: usize,
    table: Option<Result<Csv, String>>,
    _subscriptions: Vec<Subscription>,
}

impl UpdateForm {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let csv = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t(
                "选择填好 xmp_update 列的表",
                "Choose a table with the xmp_update column filled",
            ))
        });
        let subscription = cx.subscribe_in(&csv, window, |this, _, event: &InputEvent, _, cx| {
            if matches!(event, InputEvent::Change) {
                this.read(cx);
            }
        });
        UpdateForm {
            csv,
            mode: 0,
            table: None,
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
        cx.notify();
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

    pub fn values(&self, cx: &App) -> Vec<(&'static str, String)> {
        vec![
            ("csv", self.csv.read(cx).value().to_string()),
            ("mode", self.mode.to_string()),
        ]
    }

    pub fn restore(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let settings = cx.default_global::<crate::settings::Settings>().clone();
        self.mode = settings
            .form_value("xmp_update", "mode")
            .and_then(|m| m.parse().ok())
            .filter(|&m| m < MODES.len())
            .unwrap_or(0);
        if let Some(csv) = settings
            .form_value("xmp_update", "csv")
            .filter(|v| !v.is_empty())
        {
            self.csv
                .update(cx, |input, cx| input.set_value(csv.to_string(), window, cx));
            self.read(cx);
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
                    "；{} 行的路径是媒体文件而不是 XMP，元数据更新只修改 XMP：请在标签获取中选择「XMP 旁注文件」生成表格",
                    count(preview.media_rows.len())
                ),
                Language::English => format!(
                    "; {} rows point at media files, not XMP; this changes XMP files only: make the table with 标签获取 reading XMP sidecars",
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
        let input = card(cx).child(
            field(t("修改表", "Changes table"), cx)
                .child(
                    h_flex()
                        .gap(tokens.px("space-2"))
                        .child(div().flex_1().child(Input::new(&self.csv).large().bg(tokens.color("surface"))))
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
                .child(help_line(
                    t(
                        "在 xmp_update 列填新值（时间填 xmp_update_datetime 列）；原值为空的行表示新增。要把整张表写入文件，请用浏览与精选。",
                        "Fill new values in xmp_update (times in xmp_update_datetime); rows with an empty old value add a tag. To write a whole table into the files, use Browse and pick.",
                    ),
                    Tone::Quiet,
                    cx,
                )),
        );
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
            .child(changes)
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
