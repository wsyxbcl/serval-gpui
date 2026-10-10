//! 浏览与精选 Browse and pick (Design/spec/browse.md, board browse.html): open a tags table, filter across
//! deployments and folders, and look at the images. Nothing is scanned to start: the labels come from the table and
//! thumbnails are made only for what is on screen.

pub mod records;
pub mod thumbs;
mod viewer;
pub mod writer;

use crate::i18n::{Language, count, language, t};
use crate::screens::form::{Tone, help_line, level_picker};
use crate::table::{Csv, detect_level};
use crate::theme::TokensExt;
use gpui_kit::assets::IconName;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::{ActiveTheme, Icon, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use records::{Counts, Filters, Records};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use thumbs::{Thumb, Thumbnails};
use writer::{Queue, SaveState};

/// The Serval program in use, for writing edits (set by the shell when Serval is found).
pub struct ServalProgram(pub Option<PathBuf>);

impl Global for ServalProgram {}

const TILE_MIN_W: f32 = 200.;
const TILE_IMAGE_H: f32 = 130.;
const ROW_H: f32 = 180.;
const LISTED: usize = 8;

enum Source {
    Nothing,
    Reading(PathBuf),
    Read { path: PathBuf, csv: Arc<Csv> },
    Failed(PathBuf, String),
}

pub struct Browse {
    source: Source,
    level: Option<usize>,
    choosing_level: bool,
    records: Records,
    /// Record index by sidecar, to apply what Serval reports after a write.
    by_sidecar: HashMap<PathBuf, usize>,
    filters: Filters,
    visible: Vec<usize>,
    counts: Counts,
    expanded: HashSet<&'static str>,
    focused: Option<usize>,
    thumbs: Arc<Thumbnails>,
    scroll: UniformListScrollHandle,
    /// The viewer is open on the focused record.
    viewer: bool,
    /// Edited records that no longer match the filters: they stay, dimmed, until 刷新 (browse.md §3).
    stale: HashSet<usize>,
    queue: Queue,
    save: SaveState,
    focus: FocusHandle,
    /// Columns of the last drawn grid, for arrow keys.
    columns: usize,
    /// Open the viewer on the first record once the table is read (the browse-viewer design state).
    pub(crate) open_viewer_when_read: bool,
    pub(crate) adding: Option<(
        writer::Field,
        Entity<gpui_kit::component::input::InputState>,
    )>,
}

impl Browse {
    pub fn new(cx: &mut Context<Self>) -> Self {
        // Redraw when thumbnails finish.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(150))
                    .await;
                let alive = this
                    .update(cx, |this, cx| {
                        if this.thumbs.take_fresh() {
                            cx.notify();
                        }
                        if this.queue.idle() {
                            this.flush(cx);
                        }
                    })
                    .is_ok();
                if !alive {
                    break;
                }
            }
        })
        .detach();
        Browse {
            source: Source::Nothing,
            level: None,
            choosing_level: false,
            records: Records { all: Vec::new() },
            by_sidecar: HashMap::new(),
            filters: Filters::default(),
            visible: Vec::new(),
            counts: Counts::default(),
            expanded: HashSet::new(),
            focused: None,
            thumbs: Arc::new(Thumbnails::new()),
            scroll: UniformListScrollHandle::new(),
            viewer: false,
            stale: HashSet::new(),
            queue: Queue::default(),
            save: SaveState::Saved,
            focus: cx.focus_handle(),
            columns: 1,
            open_viewer_when_read: false,
            adding: None,
        }
    }

    pub fn open(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        crate::settings::Settings::update(cx, |s| {
            s.set_form_value("browse", "table", path.to_string_lossy())
        });
        self.source = Source::Reading(path.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let read = cx.background_executor().spawn({
                let path = path.clone();
                async move { Csv::read(&path) }
            });
            let read = read.await;
            let _ = this.update(cx, |this, cx| {
                match read {
                    Ok(csv) => {
                        let column = csv.column("path");
                        let paths: Vec<&str> =
                            csv.rows.iter().map(|r| csv.cell(r, column)).collect();
                        this.level = detect_level(paths.iter().copied());
                        this.source = Source::Read {
                            path,
                            csv: Arc::new(csv),
                        };
                        this.filters = Filters::default();
                        this.rebuild(cx);
                        if std::mem::take(&mut this.open_viewer_when_read)
                            && !this.visible.is_empty()
                        {
                            this.focused = Some(0);
                            this.viewer = true;
                        }
                    }
                    Err(err) => this.source = Source::Failed(path, err),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn restore(&mut self, cx: &mut Context<Self>) {
        let table = cx
            .default_global::<crate::settings::Settings>()
            .form_value("browse", "table")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from);
        if let Some(table) = table.filter(|p| p.is_file()) {
            self.open(table, cx);
        }
    }

    /// Records from the table at the current deployment level.
    fn rebuild(&mut self, cx: &mut Context<Self>) {
        if let Source::Read { csv, .. } = &self.source {
            self.records = Records::from_csv(csv, self.level);
            self.by_sidecar = self
                .records
                .all
                .iter()
                .enumerate()
                .map(|(i, r)| (r.sidecar.clone(), i))
                .collect();
        }
        self.refilter(cx);
    }

    fn refilter(&mut self, cx: &mut Context<Self>) {
        self.visible = self.records.filter(&self.filters);
        self.counts = self.records.counts(&self.filters);
        self.stale.clear();
        self.focused = None;
        self.viewer = false;
        cx.notify();
    }

    fn choose_table(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
                let _ = this.update(cx, |this, cx| this.open(path, cx));
            }
        })
        .detach();
    }

    fn columns(window: &Window, cx: &App) -> usize {
        let tokens = cx.tokens();
        let width = window.viewport_size().width - tokens.px("sidebar-w") - px(230.) - px(48.);
        (((width + px(12.)) / px(TILE_MIN_W + 12.)).floor() as usize).max(1)
    }

    fn toggle<T: Ord + Clone>(set: &mut std::collections::BTreeSet<T>, value: &T, on: bool) {
        if on {
            set.insert(value.clone());
        } else {
            set.remove(value);
        }
    }

    fn section(&self, title: impl Into<SharedString>, cx: &App) -> Div {
        let tokens = cx.tokens();
        div()
            .pt(tokens.px("space-4"))
            .pb(tokens.px("space-1"))
            .text_size(tokens.px("text-sm"))
            .font_medium()
            .text_color(cx.theme().muted_foreground)
            .child(title.into())
    }

    fn facet_row(&self, checkbox: Checkbox, n: usize, checked: bool, cx: &App) -> Div {
        let tokens = cx.tokens();
        h_flex()
            .justify_between()
            .px(tokens.px("space-2"))
            .py(px(3.))
            .rounded(tokens.px("radius-sm"))
            .gap(tokens.px("space-2"))
            .when(checked, |row| row.bg(cx.theme().muted))
            .child(div().flex_1().min_w_0().overflow_hidden().child(checkbox))
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(tokens.px("text-sm"))
                    .text_color(cx.theme().muted_foreground)
                    .child(count(n)),
            )
    }

    fn more(
        &self,
        key: &'static str,
        hidden: usize,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        (hidden > 0).then(|| {
            div()
                .id(key)
                .px(tokens_space(cx))
                .py(px(3.))
                .cursor_pointer()
                .text_size(cx.tokens().px("text-sm"))
                .text_color(cx.theme().muted_foreground)
                .child(match language() {
                    Language::Chinese => format!("… 另外 {hidden} 个"),
                    Language::English => format!("… {hidden} more"),
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.expanded.insert(key);
                    cx.notify();
                }))
        })
    }

    fn render_filters(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.tokens().clone();
        let mut panel = v_flex()
            .id("browse-filters")
            .w(px(230.))
            .flex_shrink_0()
            .h_full()
            .overflow_y_scroll()
            .px(tokens.px("space-3"))
            .pb(tokens.px("space-5"))
            .border_r_1()
            .border_color(cx.theme().border)
            .bg(tokens.color("background"));

        // Source
        panel = panel.child(self.section(t("来源", "Source"), cx));
        let (name, detail) = match &self.source {
            Source::Nothing => (
                t("还没有打开表格", "No table open").to_string(),
                String::new(),
            ),
            Source::Reading(path) => (file_name(path), t("正在读取…", "Reading…").to_string()),
            Source::Failed(path, err) => (file_name(path), err.clone()),
            Source::Read { path, csv } => (
                file_name(path),
                match language() {
                    Language::Chinese => format!("{} 行", count(csv.rows.len())),
                    Language::English => format!("{} rows", count(csv.rows.len())),
                },
            ),
        };
        panel =
            panel.child(
                v_flex()
                    .gap(px(2.))
                    .text_size(tokens.px("text-sm"))
                    .child(div().truncate().child(name))
                    .child(
                        h_flex()
                            .gap(tokens.px("space-2"))
                            .text_color(cx.theme().muted_foreground)
                            .child(detail)
                            .child(
                                div()
                                    .id("choose-table")
                                    .cursor_pointer()
                                    .text_color(tokens.color("accent"))
                                    .child(if matches!(self.source, Source::Nothing) {
                                        t("打开表格…", "Open a table…")
                                    } else {
                                        t("更换", "Change")
                                    })
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.choose_table(window, cx)
                                    })),
                            ),
                    ),
            );
        if !matches!(self.source, Source::Read { .. }) {
            return panel;
        }

        // Species
        panel = panel.child(self.section(t("物种", "Species"), cx));
        let all_species = self.expanded.contains("species");
        let hidden = if all_species {
            0
        } else {
            self.counts.species.len().saturating_sub(LISTED)
        };
        for (i, (name, n)) in self
            .counts
            .species
            .iter()
            .take(if all_species { usize::MAX } else { LISTED })
            .enumerate()
        {
            let checked = self.filters.species.contains(name);
            let value = name.clone();
            let label = if name.is_empty() {
                t("（无物种标签）", "(no species)").to_string()
            } else {
                name.clone()
            };
            let checkbox = Checkbox::new(("sp", i))
                .label(label)
                .checked(checked)
                .on_click(cx.listener(move |this, on: &bool, _, cx| {
                    Self::toggle(&mut this.filters.species, &value, *on);
                    this.refilter(cx);
                }));
            panel = panel.child(self.facet_row(checkbox, *n, checked, cx));
        }
        panel = panel.children(self.more("species", hidden, cx));

        // Rating, with shortcuts
        let shortcut = |id: &'static str,
                        label: &'static str,
                        ratings: &'static [u8],
                        cx: &mut Context<Self>| {
            div()
                .id(id)
                .cursor_pointer()
                .text_color(tokens.color("accent"))
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.filters.ratings = ratings.iter().copied().collect();
                    this.refilter(cx);
                }))
        };
        panel = panel.child(
            h_flex()
                .gap(tokens.px("space-2"))
                .child(self.section(t("评分", "Rating"), cx))
                .child(
                    h_flex()
                        .pt(tokens.px("space-4"))
                        .pb(tokens.px("space-1"))
                        .gap(tokens.px("space-2"))
                        .text_size(tokens.px("text-sm"))
                        .child(shortcut("r-all", t("全部", "All"), &[], cx))
                        .child(shortcut("r-3", "≥3", &[3, 4, 5], cx))
                        .child(shortcut("r-4", "≥4", &[4, 5], cx))
                        .child(shortcut("r-5", "5", &[5], cx)),
                ),
        );
        for rating in 0..=5u8 {
            let checked = self.filters.ratings.contains(&rating);
            let label = if rating == 0 {
                t("未评分", "Unrated").to_string()
            } else {
                "★".repeat(rating as usize)
            };
            let checkbox = Checkbox::new(("rating", rating as usize))
                .label(label)
                .checked(checked)
                .on_click(cx.listener(move |this, on: &bool, _, cx| {
                    Self::toggle(&mut this.filters.ratings, &rating, *on);
                    this.refilter(cx);
                }));
            panel = panel.child(self.facet_row(
                checkbox,
                self.counts.ratings[rating as usize],
                checked,
                cx,
            ));
        }

        // Deployment, with the path level it comes from
        let level_line = match (self.level, language()) {
            (Some(l), Language::Chinese) => format!("部署 · 路径第 {l} 层"),
            (Some(l), Language::English) => format!("Deployment · path level {l}"),
            (None, Language::Chinese) => "部署 · 未选择层级".to_string(),
            (None, Language::English) => "Deployment · no level chosen".to_string(),
        };
        panel = panel.child(
            h_flex()
                .gap(tokens.px("space-2"))
                .child(self.section(level_line, cx))
                .child(
                    div()
                        .id("change-level")
                        .pt(tokens.px("space-4"))
                        .pb(tokens.px("space-1"))
                        .cursor_pointer()
                        .text_size(tokens.px("text-sm"))
                        .text_color(tokens.color("accent"))
                        .child(t("更改", "Change"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.choosing_level = !this.choosing_level;
                            cx.notify();
                        })),
                ),
        );
        if self.choosing_level
            && let Some(sample) = self
                .records
                .all
                .first()
                .map(|r| r.media.to_string_lossy().into_owned())
        {
            panel = panel.child(level_picker(
                &sample,
                self.level,
                cx,
                |this: &mut Self, level, cx| {
                    this.level = Some(level);
                    this.choosing_level = false;
                    this.filters.deployments.clear();
                    this.rebuild(cx);
                },
            ));
        }
        if self.level.is_some() {
            let all = self.expanded.contains("deployments");
            let hidden = if all {
                0
            } else {
                self.counts.deployments.len().saturating_sub(LISTED)
            };
            for (i, (name, n)) in self
                .counts
                .deployments
                .iter()
                .take(if all { usize::MAX } else { LISTED })
                .enumerate()
            {
                let checked = self.filters.deployments.contains(name);
                let value = name.clone();
                let checkbox = Checkbox::new(("dep", i))
                    .label(name.clone())
                    .checked(checked)
                    .on_click(cx.listener(move |this, on: &bool, _, cx| {
                        Self::toggle(&mut this.filters.deployments, &value, *on);
                        this.refilter(cx);
                    }));
                panel = panel.child(self.facet_row(checkbox, *n, checked, cx));
            }
            panel = panel.children(self.more("deployments", hidden, cx));
        }

        // Folder
        panel = panel.child(self.section(t("文件夹", "Folder"), cx));
        let all = self.expanded.contains("folders");
        let hidden = if all {
            0
        } else {
            self.counts.folders.len().saturating_sub(LISTED)
        };
        for (i, (name, n)) in self
            .counts
            .folders
            .iter()
            .take(if all { usize::MAX } else { LISTED })
            .enumerate()
        {
            let checked = self.filters.folders.contains(name);
            let value = name.clone();
            let checkbox = Checkbox::new(("folder", i))
                .label(name.clone())
                .checked(checked)
                .on_click(cx.listener(move |this, on: &bool, _, cx| {
                    Self::toggle(&mut this.filters.folders, &value, *on);
                    this.refilter(cx);
                }));
            panel = panel.child(self.facet_row(checkbox, *n, checked, cx));
        }
        panel.children(self.more("folders", hidden, cx))
    }

    fn render_tile(&self, position: usize, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let tokens = cx.tokens().clone();
        let record = &self.records.all[self.visible[position]];
        let image: AnyElement = if record.video {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(rgb(0x2a3434))
                .child(
                    Icon::new(IconName::Play)
                        .size(px(28.))
                        .text_color(gpui_kit::white()),
                )
                .into_any_element()
        } else {
            match self.thumbs.get(&record.media) {
                Some(Thumb::Ready(path)) => img(path)
                    .size_full()
                    .object_fit(ObjectFit::Cover)
                    .into_any_element(),
                Some(Thumb::None) => div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(cx.theme().muted)
                    .text_size(tokens.px("text-xs"))
                    .text_color(cx.theme().muted_foreground)
                    .child(t("无法预览", "No preview"))
                    .into_any_element(),
                None => div().size_full().bg(cx.theme().muted).into_any_element(),
            }
        };
        let mut caption = record.species.join("+");
        if caption.is_empty() {
            caption = t("（无物种）", "(no species)").to_string();
        }
        if !record.individuals.is_empty() {
            caption.push_str(&format!(" · {}", record.individuals.join("+")));
        }
        let focused = self.focused == Some(position);
        let stale = self.stale.contains(&self.visible[position]);
        v_flex()
            .id(("tile", position))
            .flex_1()
            .min_w_0()
            .rounded(tokens.px("radius-md"))
            .overflow_hidden()
            .bg(tokens.color("surface"))
            .border_2()
            .border_color(if focused {
                tokens.color("primary")
            } else {
                cx.theme().border
            })
            .cursor_pointer()
            .child(
                div()
                    .relative()
                    .h(px(TILE_IMAGE_H))
                    .w_full()
                    .overflow_hidden()
                    .child(div().absolute().inset_0().child(image)),
            )
            .child(
                h_flex()
                    .px(tokens.px("space-2"))
                    .py(tokens.px("space-1"))
                    .gap(tokens.px("space-2"))
                    .text_size(tokens.px("text-sm"))
                    .child(div().flex_1().min_w_0().truncate().child(caption))
                    .child(
                        div()
                            .text_color(cx.theme().muted_foreground)
                            .child(record.deployment.clone()),
                    )
                    .when(record.rating > 0, |row| {
                        row.child(
                            div()
                                .text_color(tokens.color("warning"))
                                .child("★".repeat(record.rating as usize)),
                        )
                    }),
            )
            .when(stale, |tile| tile.opacity(0.45))
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                this.focused = Some(position);
                window.focus(&this.focus, cx);
                if event.click_count() >= 2 {
                    this.open_viewer(cx);
                }
                cx.notify();
            }))
    }

    fn render_grid(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.tokens().clone();
        let columns = Self::columns(window, cx);
        let rows = self.visible.len().div_ceil(columns);
        let header = h_flex()
            .h(px(52.))
            .px(tokens.px("space-4"))
            .gap(tokens.px("space-4"))
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                h_flex()
                    .gap(tokens.px("space-1"))
                    .items_baseline()
                    .child(div().font_semibold().child(count(self.visible.len())))
                    .child(
                        div()
                            .text_color(cx.theme().muted_foreground)
                            .child(match language() {
                                Language::Chinese => {
                                    format!("/ {} 张", count(self.records.all.len()))
                                }
                                Language::English => format!("/ {}", count(self.records.all.len())),
                            }),
                    ),
            )
            .child(
                div()
                    .text_size(tokens.px("text-sm"))
                    .text_color(cx.theme().muted_foreground)
                    .child(t("按时间", "By time")),
            )
            .children((!self.stale.is_empty()).then(|| {
                div()
                    .id("refresh")
                    .px(tokens.px("space-2"))
                    .py(px(2.))
                    .rounded(tokens.px("radius-sm"))
                    .bg(tokens.color("warning-soft"))
                    .text_size(tokens.px("text-sm"))
                    .text_color(tokens.color("warning"))
                    .cursor_pointer()
                    .child(match language() {
                        Language::Chinese => format!("{} 张已不符合筛选 · 刷新", self.stale.len()),
                        Language::English => {
                            format!("{} no longer match · Refresh", self.stale.len())
                        }
                    })
                    .on_click(cx.listener(|this, _, _, cx| this.refilter(cx)))
            }))
            .child(div().flex_1())
            .child(self.save_indicator(cx));
        let grid = uniform_list(
            "browse-grid",
            rows,
            cx.processor(move |this, range: std::ops::Range<usize>, _window, cx| {
                // Only what is on screen (plus one row) asks for thumbnails.
                let mut wanted = HashSet::new();
                for row in range.start..(range.end + 1).min(rows) {
                    for col in 0..columns {
                        if let Some(&i) = this.visible.get(row * columns + col) {
                            wanted.insert(this.records.all[i].media.clone());
                        }
                    }
                }
                this.thumbs.keep_only(&wanted);
                this.columns = columns;
                range
                    .map(|row| {
                        let tokens = cx.tokens().clone();
                        let mut line = h_flex()
                            .w_full()
                            .h(px(ROW_H))
                            .gap(px(12.))
                            .px(tokens.px("space-4"))
                            .pt(px(12.));
                        for col in 0..columns {
                            let position = row * columns + col;
                            line = if position < this.visible.len() {
                                line.child(this.render_tile(position, cx).into_any_element())
                            } else {
                                // Keeps the last row's tiles the same width as the others.
                                line.child(div().flex_1().into_any_element())
                            };
                        }
                        line
                    })
                    .collect()
            }),
        )
        .track_scroll(&self.scroll)
        .w_full()
        .flex_1();
        let empty =
            (self.visible.is_empty() && matches!(self.source, Source::Read { .. })).then(|| {
                div().p(tokens.px("space-6")).child(help_line(
                    t("没有符合筛选的图片。", "Nothing matches the filters."),
                    Tone::Quiet,
                    cx,
                ))
            });
        let hint = h_flex()
            .h(px(36.))
            .px(tokens.px("space-4"))
            .border_t_1()
            .border_color(cx.theme().border)
            .text_size(tokens.px("text-sm"))
            .text_color(cx.theme().muted_foreground)
            .child(t(
                "空格或双击打开 · 1–5 评分 · 0 清除评分 · ⇧ 点击多选",
                "Space or double-click opens · 1–5 rates · 0 clears · ⇧ click selects",
            ));
        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .track_focus(&self.focus)
            .on_key_down(
                cx.listener(|this, event: &KeyDownEvent, window, cx| this.key(event, window, cx)),
            )
            .child(header)
            .children(empty)
            .child(grid)
            .child(hint)
    }
}

fn tokens_space(cx: &App) -> Pixels {
    cx.tokens().px("space-2")
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

impl Render for Browse {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if matches!(self.source, Source::Nothing) {
            let tokens = cx.tokens().clone();
            return h_flex()
                .size_full()
                .child(self.render_filters(cx))
                .child(
                    v_flex().flex_1().p(tokens.px("space-6")).gap(tokens.px("space-2")).child(help_line(
                        t(
                            "打开标签获取生成的 tags.csv，或标注员填好的表格：按物种、评分、部署和文件夹筛选，跨部署浏览。",
                            "Open the tags.csv from 标签获取, or a table filled in by labellers, to filter by species, rating, deployment and folder across deployments.",
                        ),
                        Tone::Quiet,
                        cx,
                    )),
                );
        }
        if self.viewer && self.focused.is_some() {
            return h_flex().size_full().child(self.render_viewer(window, cx));
        }
        h_flex()
            .size_full()
            .child(self.render_filters(cx))
            .child(self.render_grid(window, cx))
    }
}
