//! 物种名替换 Translate species: `serval translate` (board: data.html translate-coverage). The columns come from the
//! taglist's header, and before Run every species in the table is checked against the taglist the way Serval
//! checks it, so a missing translation is fixed before the run instead of failing it.

use super::Plan;
use super::form::{Tone, accept_drop, card, field, help_line, output_field};
use crate::i18n::{Language, count, language, t};
use crate::output;
use crate::repair::TableCheck;
use crate::table::Csv;
use crate::theme::TokensExt;
use gpui_kit::component::button::Button;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

/// How the table's species meet the taglist (Serval's rules in `tags_csv_translate`).
#[derive(Debug, Default, PartialEq)]
pub struct Coverage {
    pub species: usize,
    /// Species not in the taglist's `from` column, with their row counts.
    pub missing: Vec<(String, usize)>,
    /// Species whose `to` value is empty.
    pub untranslated: Vec<(String, usize)>,
    /// `from` values listed more than once in the taglist.
    pub repeated: Vec<String>,
}

impl Coverage {
    pub fn ok(&self) -> bool {
        self.missing.is_empty() && self.untranslated.is_empty() && self.repeated.is_empty()
    }
}

pub fn coverage(table: &Csv, taglist: &Csv, from: &str, to: &str) -> Coverage {
    let species_column = table.column("species");
    let mut species: BTreeMap<&str, usize> = BTreeMap::new();
    for row in &table.rows {
        let name = table.cell(row, species_column);
        if !name.is_empty() {
            *species.entry(name).or_default() += 1;
        }
    }
    let (from_column, to_column) = (taglist.column(from), taglist.column(to));
    let mut translations: HashMap<&str, &str> = HashMap::new();
    let mut repeated = Vec::new();
    for row in &taglist.rows {
        let key = taglist.cell(row, from_column);
        if key.is_empty() {
            continue;
        }
        if translations
            .insert(key, taglist.cell(row, to_column))
            .is_some()
            && !repeated.contains(&key.to_string())
        {
            repeated.push(key.to_string());
        }
    }
    let mut coverage = Coverage {
        species: species.len(),
        repeated,
        ..Default::default()
    };
    for (name, rows) in species {
        match translations.get(name) {
            None => coverage.missing.push((name.to_string(), rows)),
            Some(&"") => coverage.untranslated.push((name.to_string(), rows)),
            Some(_) => {}
        }
    }
    coverage
}

enum Files {
    Nothing,
    Read(Box<Csv>, Box<Csv>),
    Failed(String),
}

pub struct TranslateForm {
    table: Entity<InputState>,
    taglist: Entity<InputState>,
    checks: [Entity<TableCheck>; 2],
    files: Files,
    from: Option<String>,
    to: Option<String>,
    custom_output: Option<PathBuf>,
    /// Its command is running: the inputs are disabled (the app also blocks the mouse).
    pub locked: bool,
    _subscriptions: Vec<Subscription>,
}

impl TranslateForm {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let table = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t("选择或拖入 tags.csv", "Choose or drop tags.csv"))
        });
        let taglist = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t(
                "选择标签表（例如 taglist_maze.csv）",
                "Choose the taglist (e.g. taglist_maze.csv)",
            ))
        });
        let subscriptions = [&table, &taglist]
            .into_iter()
            .map(|input| {
                cx.subscribe_in(input, window, |this, _, event: &InputEvent, _, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.check(cx);
                    }
                })
            })
            .collect();
        TranslateForm {
            checks: [
                cx.new(|cx| TableCheck::new(&table, cx)),
                cx.new(|cx| TableCheck::new(&taglist, cx)),
            ],
            table,
            taglist,
            files: Files::Nothing,
            from: None,
            to: None,
            custom_output: None,
            locked: false,
            _subscriptions: subscriptions,
        }
    }

    fn path(input: &Entity<InputState>, cx: &App) -> Option<PathBuf> {
        let value = input.read(cx).value();
        let value = value.trim();
        (!value.is_empty()).then(|| PathBuf::from(value))
    }

    /// Read both files (small: a tags table and a taglist) and pick default columns.
    fn check(&mut self, cx: &mut Context<Self>) {
        let (Some(table), Some(taglist)) =
            (Self::path(&self.table, cx), Self::path(&self.taglist, cx))
        else {
            self.files = Files::Nothing;
            cx.notify();
            return;
        };
        if !table.is_file() || !taglist.is_file() {
            self.files = Files::Nothing;
            cx.notify();
            return;
        }
        self.files = match (Csv::read(&table), Csv::read(&taglist)) {
            (Ok(table), Ok(taglist)) => {
                let has = |name: &Option<String>| {
                    name.as_ref().is_some_and(|n| taglist.header.contains(n))
                };
                if !has(&self.from) {
                    self.from = ["tag"]
                        .iter()
                        .map(|s| s.to_string())
                        .find(|s| taglist.header.contains(s))
                        .or_else(|| taglist.header.first().cloned());
                }
                if !has(&self.to) {
                    self.to = ["tagCN"]
                        .iter()
                        .map(|s| s.to_string())
                        .find(|s| taglist.header.contains(s))
                        .or_else(|| taglist.header.get(1).cloned());
                }
                Files::Read(Box::new(table), Box::new(taglist))
            }
            (Err(err), _) | (_, Err(err)) => Files::Failed(err),
        };
        cx.notify();
    }

    fn coverage(&self) -> Option<Coverage> {
        let Files::Read(table, taglist) = &self.files else {
            return None;
        };
        Some(coverage(
            table,
            taglist,
            self.from.as_deref()?,
            self.to.as_deref()?,
        ))
    }

    fn output_root(&self, table: &Path) -> PathBuf {
        self.custom_output
            .clone()
            .unwrap_or_else(|| output::default_root(table))
    }

    pub fn plan(&self, cx: &App) -> Plan {
        let table = Self::path(&self.table, cx);
        let taglist = Self::path(&self.taglist, cx);
        let mut args = vec!["translate".to_string()];
        if let Some(taglist) = &taglist {
            args.extend(["-t".into(), taglist.to_string_lossy().into()]);
        }
        if let (Some(from), Some(to)) = (&self.from, &self.to) {
            args.extend(["--from".into(), from.clone(), "--to".into(), to.clone()]);
        }
        args.extend(["--progress".into(), "json".into()]);
        if let Some(table) = &table {
            let dir = output::command_dir(&self.output_root(table), "translate");
            args.extend([
                "-o".into(),
                dir.to_string_lossy().into(),
                table.to_string_lossy().into(),
            ]);
        }
        let coverage = self.coverage();
        let ready = match (&table, &taglist, &self.files) {
            (None, _, _) => Err(t("选择要替换的表", "choose the table to translate").to_string()),
            (_, None, _) => Err(t("选择标签表", "choose the taglist").to_string()),
            (_, _, Files::Failed(err)) => {
                Err(format!("{} {err}", t("表格无法读取：", "cannot read:")))
            }
            (_, _, Files::Nothing) => Err(t("表格不存在", "a table does not exist").to_string()),
            _ if self.from == self.to => {
                Err(t("选择两个不同的列", "choose two different columns").to_string())
            }
            _ => match coverage {
                Some(c) if !c.ok() => Err(match language() {
                    Language::Chinese => format!(
                        "{} 个物种无法翻译",
                        c.missing.len() + c.untranslated.len() + c.repeated.len()
                    ),
                    Language::English => format!(
                        "{} species cannot be translated",
                        c.missing.len() + c.untranslated.len() + c.repeated.len()
                    ),
                }),
                Some(c) => Ok(match language() {
                    Language::Chinese => format!("{} 个物种都能翻译", count(c.species)),
                    Language::English => {
                        format!("all {} species can be translated", count(c.species))
                    }
                }),
                None => Err(t("选择列", "choose the columns").to_string()),
            },
        };
        let cwd = table
            .as_ref()
            .map(|t| self.output_root(t))
            .and_then(|root| root.ancestors().find(|d| d.is_dir()).map(Path::to_path_buf))
            .unwrap_or_else(std::env::temp_dir);
        Plan { args, cwd, ready }
    }

    pub fn values(&self, cx: &App) -> Vec<(&'static str, String)> {
        vec![
            ("table", self.table.read(cx).value().to_string()),
            ("taglist", self.taglist.read(cx).value().to_string()),
            ("from", self.from.clone().unwrap_or_default()),
            ("to", self.to.clone().unwrap_or_default()),
            (
                "output",
                self.custom_output
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            ),
        ]
    }

    pub fn restore(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let settings = cx.default_global::<crate::settings::Settings>().clone();
        let value = |field: &str| {
            settings
                .form_value("translate", field)
                .filter(|v| !v.is_empty())
                .map(str::to_string)
        };
        self.from = value("from");
        self.to = value("to");
        self.custom_output = value("output").map(PathBuf::from);
        // The taglist is usually the same for every table.
        for (input, key) in [(&self.taglist, "taglist"), (&self.table, "table")] {
            if let Some(path) = value(key) {
                input.update(cx, |input, cx| input.set_value(path, window, cx));
            }
        }
        self.check(cx);
    }

    fn column_chips(
        &self,
        id: &'static str,
        chosen: Option<&String>,
        cx: &mut Context<Self>,
    ) -> Div {
        let tokens = cx.tokens().clone();
        let Files::Read(_, taglist) = &self.files else {
            return div();
        };
        let mut row = h_flex().flex_wrap().gap(tokens.px("space-1"));
        for (i, name) in taglist.header.iter().enumerate() {
            let selected = chosen == Some(name);
            let name = name.clone();
            row = row.child(
                div()
                    .id((id, i))
                    .px(tokens.px("space-2"))
                    .py(px(3.))
                    .rounded(tokens.px("radius-sm"))
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_size(tokens.px("text-sm"))
                    .cursor_pointer()
                    .map(|chip| {
                        if selected {
                            chip.bg(tokens.color("primary"))
                                .text_color(tokens.color("primary-foreground"))
                        } else {
                            chip.bg(tokens.color("surface"))
                                .border_1()
                                .border_color(tokens.color("input-border"))
                        }
                    })
                    .child(name.clone())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if id == "from" {
                            this.from = Some(name.clone());
                        } else {
                            this.to = Some(name.clone());
                        }
                        cx.notify();
                    })),
            );
        }
        row
    }

    fn coverage_notice(&self, cx: &mut Context<Self>) -> Option<Div> {
        let coverage = self.coverage()?;
        let tokens = cx.tokens().clone();
        if coverage.ok() {
            return Some(help_line(
                match language() {
                    Language::Chinese => format!(
                        "表中 {} 个物种都能在标签表里找到翻译。",
                        count(coverage.species)
                    ),
                    Language::English => format!(
                        "All {} species in the table have a translation.",
                        count(coverage.species)
                    ),
                },
                Tone::Quiet,
                cx,
            ));
        }
        let list = |items: &[(String, usize)]| {
            items
                .iter()
                .take(30)
                .map(|(name, rows)| match language() {
                    Language::Chinese => format!("{name}（{} 行）", count(*rows)),
                    Language::English => format!("{name} ({} rows)", count(*rows)),
                })
                .collect::<Vec<_>>()
                .join(t("、", ", "))
        };
        let mut lines = Vec::new();
        if !coverage.missing.is_empty() {
            lines.push(match language() {
                Language::Chinese => format!(
                    "{} 个物种中有 {} 个不在标签表里：{}",
                    count(coverage.species),
                    coverage.missing.len(),
                    list(&coverage.missing)
                ),
                Language::English => format!(
                    "{} of {} species are not in the taglist: {}",
                    coverage.missing.len(),
                    count(coverage.species),
                    list(&coverage.missing)
                ),
            });
        }
        if !coverage.untranslated.is_empty() {
            lines.push(match language() {
                Language::Chinese => format!(
                    "{} 个物种在“到”这一列是空的：{}",
                    coverage.untranslated.len(),
                    list(&coverage.untranslated)
                ),
                Language::English => format!(
                    "{} species have an empty translation: {}",
                    coverage.untranslated.len(),
                    list(&coverage.untranslated)
                ),
            });
        }
        if !coverage.repeated.is_empty() {
            lines.push(match language() {
                Language::Chinese => format!(
                    "标签表里重复出现：{}（每个名字只能有一个翻译）",
                    coverage.repeated.join("、")
                ),
                Language::English => format!(
                    "Listed more than once in the taglist: {} (each needs one translation)",
                    coverage.repeated.join(", ")
                ),
            });
        }
        Some(
            v_flex()
                .gap(tokens.px("space-2"))
                .p(tokens.px("space-4"))
                .rounded(tokens.px("radius-sm"))
                .bg(tokens.color("warning-soft"))
                .children(
                    lines
                        .into_iter()
                        .map(|line| div().text_size(tokens.px("text-sm")).child(line)),
                )
                .child(
                    h_flex()
                        .gap(tokens.px("space-2"))
                        .child(help_line(
                            t(
                                "可以在标签表里补上后重新检查。",
                                "Add them to the taglist, then check again.",
                            ),
                            Tone::Quiet,
                            cx,
                        ))
                        .child(
                            Button::new("recheck")
                                .outline()
                                .compact()
                                .label(t("重新检查", "Check again"))
                                .on_click(cx.listener(|this, _, _, cx| this.check(cx))),
                        ),
                ),
        )
    }
}

impl Render for TranslateForm {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.tokens().clone();
        let table = Self::path(&self.table, cx);
        let output_root = table.as_ref().map(|t| self.output_root(t));
        let locked = self.locked;
        let path_row =
            |input: &Entity<InputState>, id: &'static str, which: usize, cx: &mut Context<Self>| {
                h_flex()
                    .gap(tokens.px("space-2"))
                    .child(
                        div().flex_1().child(
                            Input::new(input)
                                .large()
                                .disabled(locked)
                                .bg(tokens.color("surface")),
                        ),
                    )
                    .child(
                        Button::new(id)
                            .label(t("选择…", "Choose…"))
                            .outline()
                            .large()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                let input = if which == 0 {
                                    this.table.clone()
                                } else {
                                    this.taglist.clone()
                                };
                                let paths = cx.prompt_for_paths(PathPromptOptions {
                                    files: true,
                                    directories: false,
                                    multiple: false,
                                    prompt: None,
                                });
                                cx.spawn_in(window, async move |_, cx| {
                                    if let Ok(Ok(Some(paths))) = paths.await
                                        && let Some(path) = paths.into_iter().next()
                                    {
                                        let _ = input.update_in(cx, |input, window, cx| {
                                            input.set_value(
                                                path.to_string_lossy().to_string(),
                                                window,
                                                cx,
                                            );
                                            cx.emit(InputEvent::Change);
                                        });
                                    }
                                })
                                .detach();
                            })),
                    )
            };
        let read = match &self.files {
            Files::Read(table, taglist) => Some((table.rows.len(), taglist.rows.len())),
            _ => None,
        };
        let mut files = card(cx)
            .child(accept_drop(
                field(t("要替换的表", "Table to translate"), cx)
                    .child(path_row(&self.table, "choose-table", 0, cx))
                    .child(self.checks[0].clone())
                    .when_some(read, |f, (rows, _)| {
                        f.child(help_line(
                            match language() {
                                Language::Chinese => format!("{} 行", count(rows)),
                                Language::English => format!("{} rows", count(rows)),
                            },
                            Tone::Quiet,
                            cx,
                        ))
                    }),
                &self.table,
                cx,
            ))
            .child(accept_drop(
                field(t("标签表", "Taglist"), cx)
                    .child(path_row(&self.taglist, "choose-taglist", 1, cx))
                    .child(self.checks[1].clone())
                    .when_some(read, |f, (_, tags)| {
                        f.child(help_line(
                            match language() {
                                Language::Chinese => format!("{} 个标签", count(tags)),
                                Language::English => format!("{} tags", count(tags)),
                            },
                            Tone::Quiet,
                            cx,
                        ))
                    }),
                &self.taglist,
                cx,
            ));
        if let Files::Failed(err) = &self.files {
            files = files.child(help_line(
                format!("{} {err}", t("无法读取：", "Cannot read:")),
                Tone::Danger,
                cx,
            ));
        }
        let columns = matches!(self.files, Files::Read(..)).then(|| {
            let (from, to) = (self.from.clone(), self.to.clone());
            card(cx)
                .child(
                    field(t("从这一列", "From the column"), cx).child(self.column_chips(
                        "from",
                        from.as_ref(),
                        cx,
                    )),
                )
                .child(
                    field(t("换成这一列", "To the column"), cx).child(self.column_chips(
                        "to",
                        to.as_ref(),
                        cx,
                    )),
                )
                .children(self.coverage_notice(cx))
        });
        let output_card = card(cx).child(output_field(
            output_root.as_deref(),
            self.custom_output.is_some(),
            cx,
            |this: &mut Self, path, cx| {
                this.custom_output = path;
                cx.notify();
            },
        ));
        v_flex()
            .gap(tokens.px("space-4"))
            .child(files)
            .children(columns)
            .child(output_card)
    }
}

#[cfg(test)]
mod tests {
    use super::{Coverage, coverage};
    use crate::table::Csv;

    #[test]
    fn coverage_finds_what_serval_would_refuse() {
        let csv = |header: &[&str], rows: &[&[&str]]| Csv {
            header: header.iter().map(|s| s.to_string()).collect(),
            rows: rows
                .iter()
                .map(|r| r.iter().map(|s| s.to_string()).collect())
                .collect(),
        };
        let table = csv(
            &["path", "species"],
            &[
                &["a", "Fox"],
                &["b", "Fox"],
                &["c", "Wolf"],
                &["d", "Pika"],
                &["e", ""],
            ],
        );
        let taglist = csv(
            &["tag", "tagCN"],
            &[
                &["Fox", "赤狐"],
                &["Pika", ""],
                &["Deer", "鹿"],
                &["Deer", "马鹿"],
            ],
        );
        assert_eq!(
            coverage(&table, &taglist, "tag", "tagCN"),
            Coverage {
                species: 3,
                missing: vec![("Wolf".into(), 1)],
                untranslated: vec![("Pika".into(), 1)],
                repeated: vec!["Deer".into()],
            }
        );
    }
}
