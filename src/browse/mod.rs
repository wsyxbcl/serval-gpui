//! 浏览与精选 Browse and pick (Design/spec/browse.md, W16): Waxbill chooses, other tools show. A tags table is
//! opened and narrowed by species and rating; the choice is then looked at in Caracal (in the browser, where it can
//! also be edited and exported), in the file manager (a review folder of hard links), or copied with extract; an
//! edited table comes back through 应用表格到文件 (`xmp write`).

pub mod records;
mod review;

use crate::i18n::{Language, count, language, t};
use crate::output;
use crate::repair::TableCheck;
use crate::screens::form::{Tone, accept_drop, card, field, help_line, segmented};
use crate::table::{Csv, date_of, detect_level};
use crate::theme::TokensExt;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme, Disableable as _, Sizable as _, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use records::{Filters, Records};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const LISTED: usize = 10;
/// Rating choices, as in extract: all, ≥ 3, ≥ 4, 5.
const RATINGS: [&[u8]; 4] = [&[], &[3, 4, 5], &[4, 5], &[5]];

/// What the shell does for this screen.
pub enum BrowseEvent {
    /// Open the table in 元数据更新, writing the whole table.
    ApplyTable(PathBuf),
    /// Copy the choice with 媒体提取.
    Extract {
        csv: PathBuf,
        species: BTreeSet<String>,
        rating: usize,
    },
}

impl EventEmitter<BrowseEvent> for Browse {}

enum Review {
    None,
    Making,
    Made(PathBuf, usize),
    CannotLink,
    Failed(String),
}

pub struct Browse {
    csv: Entity<InputState>,
    csv_check: Entity<TableCheck>,
    table: Option<Result<Csv, String>>,
    records: Records,
    species: BTreeSet<String>,
    rating: usize,
    show_all: bool,
    review: Review,
    _subscriptions: Vec<Subscription>,
}

impl Browse {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let csv = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t("选择或拖入 tags.csv", "Choose or drop tags.csv"))
        });
        let subscription = cx.subscribe_in(&csv, window, |this, _, event: &InputEvent, _, cx| {
            if matches!(event, InputEvent::Change) {
                this.read(cx);
            }
        });
        Browse {
            csv_check: cx.new(|cx| TableCheck::new(&csv, cx)),
            csv,
            table: None,
            records: Records { all: Vec::new() },
            species: BTreeSet::new(),
            rating: 0,
            show_all: false,
            review: Review::None,
            _subscriptions: vec![subscription],
        }
    }

    fn csv_path(&self, cx: &App) -> Option<PathBuf> {
        let value = self.csv.read(cx).value();
        let value = value.trim();
        (!value.is_empty()).then(|| PathBuf::from(value))
    }

    fn read(&mut self, cx: &mut Context<Self>) {
        let path = self.csv_path(cx).filter(|p| p.is_file());
        if let Some(path) = &path {
            let value = path.to_string_lossy().into_owned();
            crate::settings::Settings::update(cx, |s| s.set_form_value("browse", "table", value));
        }
        self.table = path.map(|p| Csv::read(&p));
        self.records = match &self.table {
            Some(Ok(csv)) => {
                let column = csv.column("path");
                let level = detect_level(csv.rows.iter().map(|r| csv.cell(r, column)));
                Records::from_csv(csv, level)
            }
            _ => Records { all: Vec::new() },
        };
        let records = &self.records;
        self.species
            .retain(|s| records.all.iter().any(|r| r.species.contains(s)));
        self.review = Review::None;
        cx.notify();
    }

    pub fn restore(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let table = cx
            .default_global::<crate::settings::Settings>()
            .form_value("browse", "table")
            .map(str::to_string);
        if let Some(table) = table.filter(|t| !t.is_empty()) {
            self.csv
                .update(cx, |input, cx| input.set_value(table, window, cx));
            self.read(cx);
        }
    }

    fn filters(&self) -> Filters {
        Filters {
            species: self.species.clone(),
            ratings: RATINGS[self.rating].iter().copied().collect(),
            ..Default::default()
        }
    }

    fn chosen(&self) -> Vec<usize> {
        if self.species.is_empty() && self.rating == 0 {
            return Vec::new();
        }
        self.records.filter(&self.filters())
    }

    /// Build the review folder next to the data and open it in the file manager.
    fn make_review(&mut self, cx: &mut Context<Self>) {
        let chosen = self.chosen();
        let Some(first) = chosen.first().map(|&i| &self.records.all[i]) else {
            return;
        };
        let data = first
            .media
            .parent()
            .and_then(Path::parent)
            .unwrap_or(Path::new("."))
            .to_path_buf();
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let label = self
            .species
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join("+");
        let folder_name = format!(
            "{}_{}",
            date_of(stamp).replace('-', ""),
            if label.is_empty() {
                "review".to_string()
            } else {
                safe(&label)
            }
        );
        let folder = output::command_dir(&output::default_root(&data), "review").join(folder_name);
        let files: Vec<(PathBuf, PathBuf, String)> = chosen
            .iter()
            .map(|&i| {
                let r = &self.records.all[i];
                let mut prefix = safe(&r.species.join("+"));
                if !r.deployment.is_empty() {
                    prefix = format!("{prefix}__{}", safe(&r.deployment));
                }
                (r.media.clone(), r.sidecar.clone(), prefix)
            })
            .collect();
        self.review = Review::Making;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_executor()
                .spawn(async move { review::make(&folder, &files) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.review = match outcome {
                    review::Outcome::Made { folder, files } => {
                        cx.open_with_system(&folder);
                        Review::Made(folder, files)
                    }
                    review::Outcome::CannotLink => Review::CannotLink,
                    review::Outcome::Failed(err) => Review::Failed(err),
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn render_choice(&self, cx: &mut Context<Self>) -> Div {
        let tokens = cx.tokens().clone();
        let counts = self.records.counts(&self.filters());
        let species: Vec<&(String, usize)> = counts
            .species
            .iter()
            .filter(|(s, _)| !s.is_empty())
            .collect();
        let hidden = if self.show_all {
            0
        } else {
            species.len().saturating_sub(LISTED)
        };
        let mut list = v_flex().gap(px(2.));
        for (i, (name, n)) in species.iter().take(species.len() - hidden).enumerate() {
            let checked = self.species.contains(name);
            let value = name.clone();
            list = list.child(
                h_flex()
                    .justify_between()
                    .gap(tokens.px("space-2"))
                    .px(tokens.px("space-2"))
                    .py(px(3.))
                    .rounded(tokens.px("radius-sm"))
                    .when(checked, |row| row.bg(cx.theme().muted))
                    .child(
                        div().flex_1().min_w_0().overflow_hidden().child(
                            Checkbox::new(("species", i))
                                .label(name.clone())
                                .checked(checked)
                                .on_click(cx.listener(move |this, on: &bool, _, cx| {
                                    if *on {
                                        this.species.insert(value.clone());
                                    } else {
                                        this.species.remove(&value);
                                    }
                                    this.review = Review::None;
                                    cx.notify();
                                })),
                        ),
                    )
                    .child(
                        div()
                            .text_size(tokens.px("text-sm"))
                            .text_color(cx.theme().muted_foreground)
                            .child(count(*n)),
                    ),
            );
        }
        if hidden > 0 {
            list = list.child(
                div()
                    .id("more-species")
                    .px(tokens.px("space-2"))
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
        let this = cx.entity().downgrade();
        let rating = segmented(
            "browse-rating",
            &[t("全部", "All"), "≥ 3", "≥ 4", "5"],
            self.rating,
            move |index, _, cx| {
                let _ = this.update(cx, |this, cx| {
                    this.rating = index;
                    this.review = Review::None;
                    cx.notify();
                });
            },
        );
        let chosen = self.chosen();
        let videos = chosen
            .iter()
            .filter(|&&i| self.records.all[i].video)
            .count();
        card(cx)
            .child(
                h_flex()
                    .items_start()
                    .gap(tokens.px("space-5"))
                    .child(field(t("物种", "Species"), cx).flex_1().child(list))
                    .child(
                        field(t("评分", "Rating"), cx)
                            .w(px(240.))
                            .child(h_flex().child(rating)),
                    ),
            )
            .child(help_line(
                if chosen.is_empty() {
                    t("选择物种或评分。", "Choose species or a rating.").to_string()
                } else {
                    match language() {
                        Language::Chinese => format!(
                            "已选 {} 个文件（其中 {} 个视频）",
                            count(chosen.len()),
                            count(videos)
                        ),
                        Language::English => format!(
                            "{} files chosen ({} videos)",
                            count(chosen.len()),
                            count(videos)
                        ),
                    }
                },
                Tone::Quiet,
                cx,
            ))
    }

    fn action(
        &self,
        title: &'static str,
        body: &'static str,
        button: AnyElement,
        extra: Option<Div>,
        cx: &App,
    ) -> Div {
        let tokens = cx.tokens();
        card(cx)
            .gap(tokens.px("space-2"))
            .child(
                h_flex()
                    .justify_between()
                    .gap(tokens.px("space-4"))
                    .child(
                        v_flex()
                            .gap(tokens.px("space-1"))
                            .flex_1()
                            .min_w_0()
                            .child(div().font_semibold().child(title))
                            .child(help_line(body, Tone::Quiet, cx)),
                    )
                    .child(div().flex_shrink_0().child(button)),
            )
            .children(extra)
    }
}

/// A tag as part of a file name.
fn safe(text: &str) -> String {
    text.chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect()
}

impl Render for Browse {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.tokens().clone();
        let csv = self.csv_path(cx);
        let summary = match &self.table {
            None if csv.is_some() => help_line(
                t("找不到这个表格", "This table does not exist"),
                Tone::Danger,
                cx,
            ),
            None => help_line(
                t(
                    "标签获取生成的 tags.csv、标注员填好的表，或从 Caracal 导出的表",
                    "A tags.csv from 标签获取, a table filled in by labellers, or one exported from Caracal",
                ),
                Tone::Quiet,
                cx,
            ),
            Some(Err(err)) => help_line(
                format!("{} {err}", t("无法读取：", "Cannot read:")),
                Tone::Danger,
                cx,
            ),
            Some(Ok(table)) => {
                let species: BTreeSet<&String> =
                    self.records.all.iter().flat_map(|r| &r.species).collect();
                let times: Vec<&String> = self
                    .records
                    .all
                    .iter()
                    .map(|r| &r.datetime)
                    .filter(|d| !d.is_empty())
                    .collect();
                let mut text = match language() {
                    Language::Chinese => format!(
                        "{} 行 · {} 个文件 · {} 个物种",
                        count(table.rows.len()),
                        count(self.records.all.len()),
                        count(species.len())
                    ),
                    Language::English => format!(
                        "{} rows · {} files · {} species",
                        count(table.rows.len()),
                        count(self.records.all.len()),
                        count(species.len())
                    ),
                };
                if let (Some(first), Some(last)) = (times.iter().min(), times.iter().max()) {
                    text.push_str(&format!(
                        " · {} {} {}",
                        &first[..first.len().min(10)],
                        t("至", "to"),
                        &last[..last.len().min(10)]
                    ));
                }
                help_line(text, Tone::Quiet, cx)
            }
        };
        let table_card = card(cx).child(accept_drop(
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
                                .on_click(cx.listener(|this, _, window, cx| {
                                    let paths = cx.prompt_for_paths(PathPromptOptions {
                                        files: true,
                                        directories: false,
                                        multiple: false,
                                        prompt: None,
                                    });
                                    let input = this.csv.clone();
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
                        ),
                )
                .child(summary)
                .child(self.csv_check.clone()),
            &self.csv,
            cx,
        ));
        let Some(path) = csv.filter(|_| matches!(self.table, Some(Ok(_)))) else {
            return v_flex().gap(tokens.px("space-4")).child(table_card);
        };
        let chosen = self.chosen().len();

        let caracal = {
            let path = path.clone();
            self.action(
                t("在 Caracal 中查看和编辑", "View and edit in Caracal"),
                t(
                    "在浏览器中打开 Caracal 的 Review：查看、评分、修改物种后导出 tags.csv，再回到这里「应用表格到文件」。",
                    "Opens Caracal's Review in the browser: view, rate and fix species, export tags.csv, then come back to 应用表格到文件.",
                ),
                Button::new("open-caracal")
                    .outline()
                    .label(t("打开 Caracal", "Open Caracal"))
                    .on_click(move |_, _, cx| crate::caracal::open(crate::caracal::Mode::Review, &path, cx))
                    .into_any_element(),
                None,
                cx,
            )
        };

        let review_status: Option<Div> = match &self.review {
            Review::None => None,
            Review::Making => Some(help_line(t("正在建立…", "Making it…"), Tone::Quiet, cx)),
            Review::Made(folder, n) => {
                let folder = folder.clone();
                Some(
                    v_flex().gap(tokens.px("space-2")).child(
                    h_flex()
                        .gap(tokens.px("space-2"))
                        .child(help_line(
                            match language() {
                                Language::Chinese => {
                                    format!("已建立 {} 个文件的查看文件夹（硬链接，不占额外空间），已在资源管理器中打开。", count(*n))
                                }
                                Language::English => format!("Made a review folder of {} files (hard links, no extra space) and opened it.", count(*n)),
                            },
                            Tone::Quiet,
                            cx,
                        ))
                        .child(
                            Button::new("open-review")
                                .ghost()
                                .compact()
                                .label(t("再次打开", "Open again"))
                                .on_click(move |_, _, cx| cx.open_with_system(&folder)),
                        ),
                    )
                    // Hard links look like copies but are the same files (browse.md §3).
                    .child(
                        div()
                            .p(tokens.px("space-3"))
                            .rounded(tokens.px("radius-sm"))
                            .bg(tokens.color("warning-soft"))
                            .text_size(tokens.px("text-sm"))
                            .child(t(
                                "这里的文件就是原文件：在这里旋转或编辑会改动原文件；删除只删除这里的链接，原文件不受影响。",
                                "These are the original files: rotating or editing them here changes the originals; deleting only removes the link here, the originals stay.",
                            )),
                    ),
                )
            }
            Review::CannotLink => Some(
                h_flex()
                    .gap(tokens.px("space-2"))
                    .child(help_line(
                        t(
                            "媒体在另一个磁盘或网络位置，不能建立链接。可以用媒体提取复制这些文件。",
                            "The media are on another drive or a network share, where links cannot be made. Copy them with 媒体提取 instead.",
                        ),
                        Tone::Danger,
                        cx,
                    ))
                    .child({
                        let (csv, species, rating) = (path.clone(), self.species.clone(), self.rating);
                        Button::new("to-extract").outline().compact().label(t("用媒体提取复制", "Copy with 媒体提取")).on_click(
                            cx.listener(move |_, _, _, cx| {
                                cx.emit(BrowseEvent::Extract { csv: csv.clone(), species: species.clone(), rating })
                            }),
                        )
                    }),
            ),
            Review::Failed(err) => Some(help_line(err.clone(), Tone::Danger, cx)),
        };
        let explorer = self.action(
            t("在资源管理器中查看所选", "Look at the choice in the file manager"),
            t(
                "把选中的照片和视频（连同 XMP）以硬链接放进一个查看文件夹并打开：缩略图、照片应用和系统播放器都可直接使用，复制出去的是原文件。",
                "Puts the chosen photos and videos (with their XMP) into a review folder as hard links and opens it: thumbnails, the Photos app and the system player work as usual, and copying out copies the real files.",
            ),
            Button::new("make-review")
                .outline()
                .label(t("建立查看文件夹", "Make a review folder"))
                .disabled(chosen == 0 || matches!(self.review, Review::Making))
                .on_click(cx.listener(|this, _, _, cx| this.make_review(cx)))
                .into_any_element(),
            review_status,
            cx,
        );
        let apply = {
            let path = path.clone();
            self.action(
                t("应用表格到文件", "Write the table into the files"),
                t(
                    "把这张表（例如从 Caracal 导出的）原样写入 XMP：先检查会改什么，再写入。",
                    "Writes this table (for example one exported from Caracal) into the XMP files as it is: first a check of what will change, then the write.",
                ),
                Button::new("apply-table")
                    .outline()
                    .label(t("检查并写入…", "Check and write…"))
                    .on_click(cx.listener(move |_, _, _, cx| cx.emit(BrowseEvent::ApplyTable(path.clone()))))
                    .into_any_element(),
                None,
                cx,
            )
        };
        v_flex()
            .gap(tokens.px("space-4"))
            .child(table_card)
            .child(self.render_choice(cx))
            .child(explorer)
            .child(caracal)
            .child(apply)
    }
}
