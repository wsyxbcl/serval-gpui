//! 媒体提取 Extract media: `serval extract` (boards: data.html extract-from-data, questions.html extract-keep-dirs,
//! run-states.html extract-*). The table is read first: species are chosen from the real values with counts, the
//! match is counted and sized before the run, and the kept folders are picked on a real path.

use super::Plan;
use super::form::{Tone, card, field, help_line, output_field, segmented};
use crate::i18n::{Language, count, language, t};
use crate::output;
use crate::table::{Record, TagTable, date_of, detect_level};
use crate::theme::TokensExt;
use gpui_kit::component::button::Button;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme, Sizable as _, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const VISIBLE_SPECIES: usize = 8;
/// Rating choices: all, ≥ 3, ≥ 4, 5.
const RATINGS: [Option<(f64, bool)>; 4] = [
    None,
    Some((3.0, false)),
    Some((4.0, false)),
    Some((5.0, true)),
];

enum TableState {
    Nothing,
    Reading,
    Read(Arc<TagTable>),
    Failed(String),
}

/// What the current choice matches.
#[derive(Default)]
struct Matched {
    records: usize,
    files: usize,
    deployments: usize,
    /// A matching file, its tags joined as `--rename` names it ("雪豹__SL01__").
    sample: Option<(String, String)>,
}

pub struct ExtractForm {
    csv: Entity<InputState>,
    search: Entity<InputState>,
    table: TableState,
    /// Species with their record counts under the rating choice, most first; untagged records counted apart.
    species: Vec<(String, usize)>,
    untagged: usize,
    selected: BTreeSet<String>,
    rating: usize,
    show_all: bool,
    level: Option<usize>,
    keep_dirs: Option<usize>,
    /// The kept folders of the last run, applied when its table is read again.
    restored_keep_dirs: Option<usize>,
    species_folders: bool,
    rename: bool,
    custom_output: Option<PathBuf>,
    matched: Matched,
    bytes: Option<u64>,
    generation: usize,
    _subscriptions: Vec<Subscription>,
}

impl ExtractForm {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let csv = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t("选择 tags.csv", "Choose tags.csv"))
        });
        let search =
            cx.new(|cx| InputState::new(window, cx).placeholder(t("搜索物种", "Search species")));
        let subscriptions = vec![
            cx.subscribe_in(&csv, window, |this, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    this.csv_changed(cx);
                }
            }),
            cx.subscribe_in(&search, window, |_, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
        ];
        ExtractForm {
            csv,
            search,
            table: TableState::Nothing,
            species: Vec::new(),
            untagged: 0,
            selected: BTreeSet::new(),
            rating: 0,
            show_all: false,
            level: None,
            keep_dirs: None,
            restored_keep_dirs: None,
            species_folders: false,
            rename: false,
            custom_output: None,
            matched: Matched::default(),
            bytes: None,
            generation: 0,
            _subscriptions: subscriptions,
        }
    }

    fn csv_path(&self, cx: &App) -> Option<PathBuf> {
        let value = self.csv.read(cx).value();
        let value = value.trim();
        (!value.is_empty()).then(|| PathBuf::from(value))
    }

    pub fn set_csv(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        self.csv
            .update(cx, |input, cx| input.set_value(path, window, cx));
        self.csv_changed(cx);
    }

    fn csv_changed(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        let generation = self.generation;
        let Some(path) = self.csv_path(cx).filter(|p| p.is_file()) else {
            self.table = TableState::Nothing;
            self.refresh(cx);
            return;
        };
        self.table = TableState::Reading;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { TagTable::read(&path) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation == generation {
                    this.table_read(read, cx);
                }
            });
        })
        .detach();
    }

    fn table_read(&mut self, read: Result<TagTable, String>, cx: &mut Context<Self>) {
        match read {
            Ok(table) => {
                self.level = detect_level(table.records.iter().map(|r| r.path.as_str()));
                // Keep from the deployment folder down, as the board suggests; the user can change it.
                let depth = table
                    .sample_path()
                    .map(|p| p.split(['/', '\\']).count())
                    .unwrap_or(0);
                self.keep_dirs = self
                    .restored_keep_dirs
                    .take()
                    .or_else(|| self.level.map(|level| depth.saturating_sub(level + 1)));
                let names: HashSet<&str> =
                    table.records.iter().map(|r| r.species.as_str()).collect();
                self.selected.retain(|s| names.contains(s.as_str()));
                self.table = TableState::Read(Arc::new(table));
            }
            Err(err) => self.table = TableState::Failed(err),
        }
        self.refresh(cx);
    }

    fn rating_matches(&self, record: &Record) -> bool {
        match RATINGS[self.rating] {
            None => true,
            Some((min, exact)) => record
                .rating
                .parse::<f64>()
                .is_ok_and(|r| if exact { r == min } else { r >= min }),
        }
    }

    fn record_matches(&self, record: &Record) -> bool {
        self.rating_matches(record)
            && (self.selected.is_empty() || self.selected.contains(&record.species))
    }

    /// Recount species under the rating choice, and what the whole choice matches; then size it in the background.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        let TableState::Read(table) = &self.table else {
            self.species.clear();
            self.matched = Matched::default();
            self.bytes = None;
            cx.notify();
            return;
        };
        let table = table.clone();
        let mut counts: HashMap<&str, usize> = HashMap::new();
        let mut untagged = 0;
        for record in table.records.iter().filter(|r| self.rating_matches(r)) {
            if record.species.is_empty() {
                untagged += 1;
            } else {
                *counts.entry(record.species.as_str()).or_default() += 1;
            }
        }
        let mut species: Vec<(String, usize)> = counts
            .into_iter()
            .map(|(s, n)| (s.to_string(), n))
            .collect();
        // Chosen species stay listed even when the rating leaves none of them.
        for chosen in &self.selected {
            if !species.iter().any(|(s, _)| s == chosen) {
                species.push((chosen.clone(), 0));
            }
        }
        species.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        self.species = species;
        self.untagged = untagged;

        let mut matched = Matched::default();
        let mut paths: Vec<&str> = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();
        let mut deployments: HashSet<&str> = HashSet::new();
        let mut tags: HashMap<&str, (BTreeSet<&str>, BTreeSet<&str>)> = HashMap::new();
        let nothing_chosen = self.selected.is_empty() && self.rating == 0;
        if !nothing_chosen {
            for record in table.records.iter().filter(|r| self.record_matches(r)) {
                matched.records += 1;
                if seen.insert(record.path.as_str()) {
                    paths.push(&record.path);
                }
                if let Some(deployment) = table.deployment(record, self.level) {
                    deployments.insert(deployment);
                }
                if paths.first() == Some(&record.path.as_str()) || tags.is_empty() {
                    let entry = tags.entry(record.path.as_str()).or_default();
                    if !record.species.is_empty() {
                        entry.0.insert(&record.species);
                    }
                    if !record.individual.is_empty() {
                        entry.1.insert(&record.individual);
                    }
                }
            }
        }
        matched.files = paths.len();
        matched.deployments = deployments.len();
        matched.sample = paths.first().map(|path| {
            let (species, individuals) = tags.get(path).cloned().unwrap_or_default();
            let mut prefix = if species.is_empty() {
                "untagged".to_string()
            } else {
                species.into_iter().collect::<Vec<_>>().join("+")
            };
            if !individuals.is_empty() {
                prefix.push_str("__");
                prefix.push_str(&individuals.into_iter().collect::<Vec<_>>().join("+"));
            }
            (path.to_string(), format!("{}__", file_name_part(&prefix)))
        });
        let files: Vec<String> = paths.iter().map(|p| p.to_string()).collect();
        self.matched = matched;
        self.bytes = None;
        cx.notify();
        if files.is_empty() {
            return;
        }
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let bytes = cx
                .background_executor()
                .spawn(async move { size_of(&files) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation == generation {
                    this.bytes = Some(bytes);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn choose_csv(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = this.update_in(cx, |this, window, cx| {
                    this.set_csv(path.to_string_lossy().to_string(), window, cx)
                });
            }
        })
        .detach();
    }

    fn output_root(&self, csv: &Path) -> PathBuf {
        self.custom_output
            .clone()
            .unwrap_or_else(|| output::default_root(csv))
    }

    /// Serval's filter for the choice: a single species as `-f species`, a rating alone as `-f rating`, anything
    /// else as an advanced expression.
    fn filter(&self) -> Option<(&'static str, String)> {
        let rating = RATINGS[self.rating].map(|(min, exact)| {
            if exact {
                format!("{min}")
            } else {
                format!(">={min}")
            }
        });
        match (self.selected.len(), rating) {
            (0, None) => None,
            (1, None) => Some(("species", self.selected.first().unwrap().clone())),
            (0, Some(rating)) => Some(("rating", rating)),
            (_, rating) => {
                let species = self
                    .selected
                    .iter()
                    .map(|s| format!("sp:{}", filter_value(s)))
                    .collect::<Vec<_>>()
                    .join(" or ");
                Some((
                    "advanced",
                    match rating {
                        Some(rating) if self.selected.len() > 1 => {
                            format!("({species}) and r:{rating}")
                        }
                        Some(rating) => format!("{species} and r:{rating}"),
                        None => species,
                    },
                ))
            }
        }
    }

    /// The folder this choice is copied into: dated and named after the choice, so running the same choice again
    /// continues it (finished files are skipped) instead of starting a new copy.
    fn run_folder(&self) -> String {
        let mut name: Vec<String> = self.selected.iter().take(3).cloned().collect();
        if self.selected.len() > 3 {
            name.push(match language() {
                Language::Chinese => format!("等{}种", self.selected.len()),
                Language::English => format!("and{}more", self.selected.len() - 3),
            });
        }
        let mut name = name.join("+");
        match RATINGS[self.rating] {
            Some((min, true)) => name.push_str(&format!("_rating{min}")),
            Some((min, false)) => name.push_str(&format!("_rating{min}up")),
            None => {}
        }
        let today = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        format!(
            "{}_{}",
            date_of(today).replace('-', ""),
            file_name_part(&name).replace('.', "")
        )
    }

    pub fn plan(&self, cx: &App) -> Plan {
        let csv = self.csv_path(cx);
        let mut args = vec!["extract".to_string()];
        if let Some((kind, value)) = self.filter() {
            args.extend(["-f".into(), kind.into(), "-v".into(), value]);
        }
        if let Some(keep) = self.keep_dirs {
            args.extend(["--keep-dirs".into(), keep.to_string()]);
        }
        if self.rename {
            args.push("--rename".into());
        }
        if self.species_folders {
            args.extend([
                "--use-subdir".into(),
                "--subdir-type".into(),
                "species".into(),
            ]);
        }
        args.extend(["--progress".into(), "json".into()]);
        if let Some(csv) = &csv {
            let dir =
                output::command_dir(&self.output_root(csv), "extract").join(self.run_folder());
            args.extend([
                "-o".into(),
                dir.to_string_lossy().into(),
                csv.to_string_lossy().into(),
            ]);
        }
        let ready = match (&csv, &self.table) {
            (None, _) => Err(t("选择标签表", "choose the tags table").to_string()),
            (_, TableState::Failed(err)) => Err(format!(
                "{} {err}",
                t("表格无法读取：", "cannot read the table:")
            )),
            (Some(path), TableState::Nothing) if !path.is_file() => {
                Err(t("找不到这个表格", "the table does not exist").to_string())
            }
            (_, TableState::Reading | TableState::Nothing) => {
                Err(t("正在读取表格…", "reading the table…").to_string())
            }
            _ if self.filter().is_none() => {
                Err(t("选择要提取的物种或评分", "choose the species or a rating").to_string())
            }
            _ if self.matched.records == 0 => {
                Err(t("没有符合条件的记录", "nothing matches").to_string())
            }
            _ if self.keep_dirs.is_none() => {
                Err(t("选择保留的文件夹", "choose the folders to keep").to_string())
            }
            _ => Ok(self.match_line()),
        };
        let cwd = csv
            .as_ref()
            .map(|csv| self.output_root(csv))
            .and_then(|root| {
                root.ancestors()
                    .find(|dir| dir.is_dir())
                    .map(Path::to_path_buf)
            })
            .unwrap_or_else(std::env::temp_dir);
        Plan { args, cwd, ready }
    }

    /// "1,412 个文件，约 8.3 GB"
    fn match_line(&self) -> String {
        let files = count(self.matched.files);
        match (language(), self.bytes) {
            (Language::Chinese, Some(bytes)) => format!("{files} 个文件，约 {}", size_text(bytes)),
            (Language::Chinese, None) => format!("{files} 个文件"),
            (Language::English, Some(bytes)) => {
                format!("{files} files, about {}", size_text(bytes))
            }
            (Language::English, None) => format!("{files} files"),
        }
    }

    pub fn values(&self, cx: &App) -> Vec<(&'static str, String)> {
        let flag = |b: bool| if b { "1" } else { "0" }.to_string();
        vec![
            ("csv", self.csv.read(cx).value().to_string()),
            (
                "species",
                self.selected.iter().cloned().collect::<Vec<_>>().join("\t"),
            ),
            ("rating", self.rating.to_string()),
            (
                "keep_dirs",
                self.keep_dirs.map(|k| k.to_string()).unwrap_or_default(),
            ),
            ("rename", flag(self.rename)),
            ("species_folders", flag(self.species_folders)),
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
        let value = |field: &str| settings.form_value("extract", field);
        self.rename = value("rename") == Some("1");
        self.species_folders = value("species_folders") == Some("1");
        self.custom_output = value("output").filter(|v| !v.is_empty()).map(PathBuf::from);
        self.selected = value("species")
            .map(|s| {
                s.split('\t')
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        self.rating = value("rating")
            .and_then(|r| r.parse().ok())
            .filter(|&r| r < RATINGS.len())
            .unwrap_or(0);
        self.restored_keep_dirs = value("keep_dirs").and_then(|k| k.parse().ok());
        if let Some(csv) = value("csv").filter(|v| !v.is_empty()) {
            self.set_csv(csv.to_string(), window, cx);
        }
    }

    /// Fill the form like the boards (extract-from-data, extract-keep-dirs).
    pub fn set_example(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path = "/data/2024_spring_serval_output/serval_observe/tags_2024_spring.csv";
        self.csv.update(cx, |input, cx| {
            input.set_value(path.to_string(), window, cx)
        });
        self.selected = ["雪豹".to_string(), "兔狲".to_string()].into();
        self.rename = true;
        self.generation += 1; // the example table is not read from disk
        self.table_read(Ok(example_table()), cx);
        self.generation += 1; // nor sized
        self.bytes = Some(8_300_000_000);
    }

    fn table_line(&self, cx: &App) -> Div {
        match &self.table {
            TableState::Nothing => match self.csv_path(cx) {
                Some(path) if !path.is_file() => help_line(
                    t("找不到这个表格", "This table does not exist"),
                    Tone::Danger,
                    cx,
                ),
                _ => help_line(
                    t(
                        "标签获取生成的 tags.csv，或标注员填好的表格",
                        "The tags.csv from 标签获取, or a table filled in by labellers",
                    ),
                    Tone::Quiet,
                    cx,
                ),
            },
            TableState::Reading => help_line(t("正在读取…", "Reading…"), Tone::Quiet, cx),
            TableState::Failed(err) => help_line(
                format!("{} {err}", t("无法读取：", "Cannot read:")),
                Tone::Danger,
                cx,
            ),
            TableState::Read(table) => {
                let times = table.records.iter().filter_map(|r| r.time);
                let range = times.clone().min().zip(times.max());
                let species = self.species.len();
                let deployments = table.deployment_count(self.level);
                let mut parts = vec![match language() {
                    Language::Chinese => format!("{} 行", count(table.records.len())),
                    Language::English => format!("{} rows", count(table.records.len())),
                }];
                parts.push(match language() {
                    Language::Chinese => format!("{} 个物种", count(species)),
                    Language::English => format!("{} species", count(species)),
                });
                if self.level.is_some() {
                    parts.push(match language() {
                        Language::Chinese => format!("{} 个部署", count(deployments)),
                        Language::English => format!("{} deployments", count(deployments)),
                    });
                }
                if let Some((first, last)) = range {
                    parts.push(format!(
                        "{} {} {}",
                        date_of(first),
                        t("至", "to"),
                        date_of(last)
                    ));
                }
                help_line(parts.join(" · "), Tone::Quiet, cx)
            }
        }
    }

    fn render_species(&self, cx: &mut Context<Self>) -> Div {
        let tokens = cx.tokens().clone();
        let query = self.search.read(cx).value().trim().to_lowercase();
        let listed: Vec<&(String, usize)> = self
            .species
            .iter()
            .filter(|(name, _)| query.is_empty() || name.to_lowercase().contains(&query))
            .collect();
        let max = listed
            .iter()
            .map(|(_, n)| *n)
            .max()
            .unwrap_or(1)
            .max(self.untagged)
            .max(1);
        let visible = if self.show_all || !query.is_empty() {
            listed.len()
        } else {
            VISIBLE_SPECIES.min(listed.len())
        };
        let hidden = listed.len() - visible;
        let bar = |n: usize, cx: &App| {
            div()
                .w(px(80.))
                .h(px(4.))
                .rounded_full()
                .bg(cx.theme().muted)
                .child(
                    div()
                        .h_full()
                        .rounded_full()
                        .w(relative(n as f32 / max as f32))
                        .bg(tokens.color("primary").opacity(0.55)),
                )
        };
        let row = |content: AnyElement, n: usize, cx: &App| {
            h_flex()
                .gap(tokens.px("space-3"))
                .px(tokens.px("space-3"))
                .py(px(5.))
                .child(div().flex_1().min_w_0().child(content))
                .child(
                    div()
                        .w(px(56.))
                        .text_right()
                        .text_size(tokens.px("text-sm"))
                        .child(count(n)),
                )
                .child(bar(n, cx))
        };
        let mut list = v_flex()
            .border_1()
            .border_color(cx.theme().border)
            .rounded(tokens.px("radius-sm"))
            .bg(tokens.color("surface"))
            .child(
                div()
                    .p(tokens.px("space-1"))
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(Input::new(&self.search).appearance(false)),
            );
        for (i, (name, n)) in listed.into_iter().take(visible).enumerate() {
            let name = name.clone();
            let checked = self.selected.contains(&name);
            let checkbox = Checkbox::new(("species", i))
                .label(name.clone())
                .checked(checked)
                .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                    if *checked {
                        this.selected.insert(name.clone());
                    } else {
                        this.selected.remove(&name);
                    }
                    this.refresh(cx);
                }));
            list = list.child(row(checkbox.into_any_element(), *n, cx));
        }
        if self.untagged > 0 && query.is_empty() {
            let label = div()
                .pl(px(24.))
                .text_color(cx.theme().muted_foreground)
                .child(t("（无物种标签）", "(no species tag)"));
            list = list.child(row(label.into_any_element(), self.untagged, cx));
        }
        if hidden > 0 {
            list = list.child(
                div()
                    .id("more-species")
                    .px(tokens.px("space-3"))
                    .py(tokens.px("space-2"))
                    .cursor_pointer()
                    .text_size(tokens.px("text-sm"))
                    .text_color(cx.theme().muted_foreground)
                    .child(match language() {
                        Language::Chinese => format!("… 另外 {hidden} 个"),
                        Language::English => format!("… {hidden} more"),
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_all = true;
                        cx.notify();
                    })),
            );
        }
        list
    }

    fn render_summary(&self, cx: &App) -> Option<Div> {
        self.filter()?;
        let tokens = cx.tokens();
        let figure = |value: String, unit: &'static str| {
            h_flex()
                .gap(tokens.px("space-1"))
                .items_baseline()
                .child(
                    div()
                        .text_size(tokens.px("text-lg"))
                        .font_semibold()
                        .child(value),
                )
                .child(div().text_color(cx.theme().muted_foreground).child(unit))
        };
        let mut row = h_flex()
            .gap(tokens.px("space-5"))
            .px(tokens.px("space-4"))
            .py(tokens.px("space-3"))
            .rounded(tokens.px("radius-sm"))
            .bg(tokens.color("info-soft"))
            .child(figure(count(self.matched.files), t("个文件", "files")))
            .child(figure(count(self.matched.records), t("条记录", "records")));
        if let Some(bytes) = self.bytes {
            let text = size_text(bytes);
            let (value, unit) = text.split_once(' ').unwrap_or((&text, ""));
            row = row.child(
                h_flex()
                    .gap(tokens.px("space-1"))
                    .items_baseline()
                    .child(
                        div()
                            .text_size(tokens.px("text-lg"))
                            .font_semibold()
                            .child(value.to_string()),
                    )
                    .child(
                        div()
                            .text_color(cx.theme().muted_foreground)
                            .child(unit.to_string()),
                    ),
            );
        }
        if self.level.is_some() {
            row = row.child(div().text_color(cx.theme().muted_foreground).child(
                match language() {
                    Language::Chinese => format!("来自 {} 个部署", count(self.matched.deployments)),
                    Language::English => {
                        format!("from {} deployments", count(self.matched.deployments))
                    }
                },
            ));
        }
        Some(row)
    }

    fn render_keep(&self, cx: &mut Context<Self>) -> Option<Div> {
        let (sample, prefix) = self.matched.sample.clone().or_else(|| match &self.table {
            TableState::Read(table) => table.sample_path().map(|p| (p.to_string(), String::new())),
            _ => None,
        })?;
        let tokens = cx.tokens().clone();
        let components: Vec<&str> = sample.split(['/', '\\']).collect();
        let last = components.len() - 1;
        let top = self.keep_dirs.map(|k| last - k.min(last));
        let mono = cx.theme().mono_font_family.clone();
        let chip = |id: usize, label: String| {
            div()
                .id(("keep", id))
                .px(tokens.px("space-2"))
                .py(px(3.))
                .rounded(tokens.px("radius-sm"))
                .child(label)
        };
        let mut row = h_flex()
            .flex_wrap()
            .gap(tokens.px("space-1"))
            .p(tokens.px("space-2"))
            .rounded(tokens.px("radius-sm"))
            .bg(cx.theme().muted)
            .font_family(mono.clone())
            .text_size(tokens.px("text-sm"));
        let file_only = self.keep_dirs == Some(0);
        row = row.child(
            chip(0, t("只要文件", "File only").to_string())
                .map(|c| {
                    if file_only {
                        c.bg(tokens.color("primary"))
                            .text_color(tokens.color("primary-foreground"))
                    } else {
                        c.bg(tokens.color("surface"))
                            .border_1()
                            .border_color(tokens.color("input-border"))
                    }
                })
                .cursor_pointer()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.keep_dirs = Some(0);
                    cx.notify();
                })),
        );
        row = row.child(div().text_color(cx.theme().border).child("|"));
        for (level, name) in components.iter().enumerate() {
            if level == 0 && name.is_empty() {
                continue;
            }
            if level > 1 || (level == 1 && !components[0].is_empty()) {
                row = row.child(div().text_color(cx.theme().muted_foreground).child("/"));
            }
            let c = chip(level + 1, name.to_string());
            row = row.child(if level == 0 || level == last {
                c.text_color(cx.theme().muted_foreground)
            } else {
                let keep = last - level;
                let c = if top == Some(level) {
                    c.bg(tokens.color("primary"))
                        .text_color(tokens.color("primary-foreground"))
                } else if top.is_some_and(|top| level > top) {
                    c.bg(tokens.color("primary").opacity(0.12))
                        .border_1()
                        .border_color(tokens.color("primary").opacity(0.25))
                } else {
                    c.bg(tokens.color("surface"))
                        .border_1()
                        .border_color(tokens.color("input-border"))
                };
                c.cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.keep_dirs = Some(keep);
                        cx.notify();
                    }))
            });
        }
        let note = match (top, language()) {
            (None, Language::Chinese) => "点选要保留的最上层文件夹。".to_string(),
            (None, Language::English) => "Click the top folder to keep.".to_string(),
            (Some(top), lang) if top == last => match lang {
                Language::Chinese => {
                    "只复制文件，不保留文件夹（同名文件会被识别为冲突）。".to_string()
                }
                Language::English => {
                    "Files only, no folders (files with the same name are reported as conflicts)."
                        .to_string()
                }
            },
            (Some(top), Language::Chinese) => format!(
                "从 {} 开始保留 {} 层。输出示例：",
                components[top],
                last - top
            ),
            (Some(top), Language::English) => format!(
                "Keeps {} folders from {}. Example:",
                last - top,
                components[top]
            ),
        };
        let example = top.map(|top| {
            let kept = components[top..last].join("/");
            let subdir = if self.species_folders {
                self.selected
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "species".into())
            } else {
                String::new()
            };
            let name = format!(
                "{}{}",
                if self.rename { prefix.as_str() } else { "" },
                components[last]
            );
            let parts: Vec<&str> = [kept.as_str(), subdir.as_str(), name.as_str()]
                .into_iter()
                .filter(|p| !p.is_empty())
                .collect();
            format!("…/serval_extract/{}/{}", self.run_folder(), parts.join("/"))
        });
        Some(
            field(t("保留的文件夹", "Folders to keep"), cx)
                .child(row)
                .child(help_line(note, Tone::Quiet, cx))
                .when_some(example, |f, example| {
                    f.child(
                        div()
                            .font_family(mono)
                            .text_size(tokens.px("text-xs"))
                            .text_color(cx.theme().muted_foreground)
                            .child(example),
                    )
                }),
        )
    }
}

impl Render for ExtractForm {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.tokens().clone();
        let csv = self.csv_path(cx);
        let output_root = csv.as_ref().map(|csv| self.output_root(csv));
        let table_read = matches!(self.table, TableState::Read(_));

        let table_card = card(cx).child(
            field(t("标签表", "Tags table"), cx)
                .child(
                    h_flex()
                        .gap(tokens.px("space-2"))
                        .child(
                            div()
                                .flex_1()
                                .child(Input::new(&self.csv).large().bg(tokens.color("surface"))),
                        )
                        .child(
                            Button::new("choose-csv")
                                .label(t("选择…", "Choose…"))
                                .outline()
                                .large()
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.choose_csv(window, cx)),
                                ),
                        ),
                )
                .child(self.table_line(cx)),
        );

        let choose_card = table_read.then(|| {
            let rating = field(t("评分", "Rating"), cx).child(h_flex().child(segmented(
                "extract-rating",
                &[t("全部", "All"), "≥ 3", "≥ 4", "5"],
                self.rating,
                {
                    let this = cx.entity().downgrade();
                    move |index, _, cx| {
                        let _ = this.update(cx, |this, cx| {
                            this.rating = index;
                            this.refresh(cx);
                        });
                    }
                },
            )));
            card(cx)
                .child(
                    div()
                        .text_size(tokens.px("text-sm"))
                        .font_medium()
                        .text_color(cx.theme().muted_foreground)
                        .child(t("选择要提取的记录", "Choose the records to extract")),
                )
                .child(
                    h_flex()
                        .items_start()
                        .gap(tokens.px("space-5"))
                        .child(div().flex_1().child(self.render_species(cx)))
                        .child(div().w(px(260.)).child(rating)),
                )
                .children(self.render_summary(cx))
        });

        let keep_card = table_read.then(|| {
            card(cx).children(self.render_keep(cx)).child(
                field(t("整理方式", "Organise"), cx)
                    .child(
                        h_flex()
                            .gap(tokens.px("space-5"))
                            .child(
                                Checkbox::new("species-folders")
                                    .label(t("按物种分子文件夹", "A folder per species"))
                                    .checked(self.species_folders)
                                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                        this.species_folders = *checked;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Checkbox::new("rename")
                                    .label(t("文件名加上标签", "Add the tags to file names"))
                                    .checked(self.rename)
                                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                        this.rename = *checked;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .when(self.rename, |f| {
                        f.child(help_line(
                            t(
                                "文件名格式：物种__个体__原文件名，多个值用 + 连接，例如 雪豹+岩羊__SL01__IMG_0001.JPG",
                                "Names: species__individuals__original name, values joined by +, e.g. 雪豹+岩羊__SL01__IMG_0001.JPG",
                            ),
                            Tone::Quiet,
                            cx,
                        ))
                    }),
            )
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
            .child(table_card)
            .children(choose_card)
            .children(keep_card)
            .child(output_card)
    }
}

/// A value in Serval's filter grammar: unquoted unless it would end the value early (`)`, ` and `, ` or `).
fn filter_value(value: &str) -> String {
    let lower = value.to_lowercase();
    if value.contains([')', '(', '"', '\'']) || lower.contains(" and ") || lower.contains(" or ") {
        if value.contains('"') {
            format!("'{value}'")
        } else {
            format!("\"{value}\"")
        }
    } else {
        value.to_string()
    }
}

/// A tag as part of a file or folder name: no path separators or characters some file systems reject (as Serval).
fn file_name_part(text: &str) -> String {
    text.chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// Media files plus their sidecars, as extract copies them.
fn size_of(paths: &[String]) -> u64 {
    paths
        .iter()
        .map(|path| {
            let path = Path::new(path);
            let media = if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("xmp"))
            {
                path.with_extension("")
            } else {
                path.to_path_buf()
            };
            let sidecar = media.with_added_extension("xmp");
            [media, sidecar]
                .iter()
                .filter_map(|p| std::fs::metadata(p).ok())
                .map(|m| m.len())
                .sum::<u64>()
        })
        .sum()
}

fn size_text(bytes: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < units.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", units[unit])
    }
}

/// A table like the boards': 14,106 rows over 42 deployments, 2024-03-02 to 2024-06-28.
fn example_table() -> TagTable {
    let species: Vec<(String, usize)> = [
        ("雪豹", 1_212),
        ("兔狲", 288),
        ("岩羊", 2_031),
        ("赤狐", 644),
        ("", 5_880),
    ]
    .iter()
    .map(|(s, n)| (s.to_string(), *n))
    .chain((0..26).map(|i| {
        (
            format!("物种 {:02}", i + 1),
            155 + if i < 22 { 2 } else { 1 },
        )
    }))
    .collect();
    let start = 19_784; // 2024-03-02
    let mut records = Vec::new();
    let mut i = 0usize;
    for (name, n) in species {
        for _ in 0..n {
            let dep = i % 42;
            let col = ["a", "b", "c"][dep % 3];
            records.push(Record {
                path: format!(
                    "/data/2024_spring/col_{col}/dep{:02}_col_{col}/100MEDIA/IMG_{:04}.JPG",
                    dep + 1,
                    i / 42 + 1
                ),
                time: Some((start + (i % 119) as i64) * 86_400 + 3600 * 7),
                species: name.clone(),
                individual: if name == "雪豹" && i.is_multiple_of(5) {
                    "SL01".into()
                } else {
                    String::new()
                },
                rating: ["", "3", "4", "5"][i % 4].into(),
            });
            i += 1;
        }
    }
    records.truncate(14_106);
    TagTable {
        records,
        camtrap_dp: false,
    }
}
