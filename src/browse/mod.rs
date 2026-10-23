//! 查看媒体 View media (formerly 浏览与精选) (Design/spec/browse.md, W16): Waxbill chooses, other tools show. A tags table is
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
    /// Open the table in XMP 更新, writing the whole table.
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
    /// The folder, the files linked, and the listed files not found.
    Made(PathBuf, usize, usize),
    /// None of the chosen media exist; the first one.
    NotFound(PathBuf),
    CannotLink(review::NoLinks),
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
    /// The chosen rows saved for Caracal, and how many files they are; or why they could not be saved.
    caracal: Option<Result<(PathBuf, usize), String>>,
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
            caracal: None,
            _subscriptions: vec![subscription],
        }
    }

    fn csv_path(&self, cx: &App) -> Option<PathBuf> {
        let value = self.csv.read(cx).value();
        let value = value.trim();
        (!value.is_empty()).then(|| PathBuf::from(value))
    }

    /// Open a table here (a next step from 标签读取's result).
    pub fn set_table(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        self.csv
            .update(cx, |input, cx| input.set_value(path, window, cx));
        self.read(cx);
    }

    fn read(&mut self, cx: &mut Context<Self>) {
        let path = self.csv_path(cx).filter(|p| p.is_file());
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
        self.caracal = None;
        cx.notify();
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

    /// The choice's name: `<date>_<species>` (up to three), or `<date>_review` for a rating alone.
    fn choice_name(&self) -> String {
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
        format!(
            "{}_{}",
            date_of(stamp).replace('-', ""),
            if label.is_empty() {
                "review".to_string()
            } else {
                safe(&label)
            }
        )
    }

    /// The review folder for the choice: in `serval_review` next to the data.
    fn choice_target(&self, chosen: &[usize]) -> Option<PathBuf> {
        let first = &self.records.all[*chosen.first()?];
        let data = first
            .media
            .parent()
            .and_then(Path::parent)
            .unwrap_or(Path::new("."))
            .to_path_buf();
        Some(output::command_dir(&output::default_root(&data), "review").join(self.choice_name()))
    }

    /// Build the review folder next to the data and open it in the file manager.
    fn make_review(&mut self, cx: &mut Context<Self>) {
        let chosen = self.chosen();
        let Some(folder) = self.choice_target(&chosen) else {
            return;
        };
        let files: Vec<(PathBuf, PathBuf, String)> = chosen
            .iter()
            .map(|&i| {
                let r = &self.records.all[i];
                let prefix = review::prefix(&r.species, &r.individuals, &r.deployment);
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
                crate::log::line(match &outcome {
                    review::Outcome::Made {
                        folder,
                        files,
                        missing,
                    } => format!(
                        "Review folder {}: {files} linked, {missing} not found",
                        folder.display()
                    ),
                    review::Outcome::NotFound(first) => {
                        format!("Review folder: no media found, e.g. {}", first.display())
                    }
                    review::Outcome::CannotLink(why, err) => {
                        format!("Review folder: cannot link ({why:?}): {err}")
                    }
                    review::Outcome::Failed(err) => format!("Review folder failed: {err}"),
                });
                this.review = match outcome {
                    review::Outcome::Made {
                        folder,
                        files,
                        missing,
                    } => {
                        cx.open_with_system(&folder);
                        Review::Made(folder, files, missing)
                    }
                    review::Outcome::NotFound(first) => Review::NotFound(first),
                    review::Outcome::CannotLink(why, _) => Review::CannotLink(why),
                    review::Outcome::Failed(err) => Review::Failed(err),
                };
                cx.notify();
            });
        })
        .detach();
    }

    /// Open Caracal on the choice (its rows saved as a table next to this one), or on the whole table when
    /// nothing is chosen.
    fn open_caracal(&mut self, table: &Path, cx: &mut Context<Self>) {
        use crate::caracal::{Mode, open};
        let chosen = self.chosen();
        // Next to the table, as `<table>_<date>_<species>.csv`: relative paths in it still lead to the media, and the
        // media's folder need not be writable (a read-only NAS, a disk not connected).
        let stem = (!chosen.is_empty()).then(|| {
            let name = table.file_stem().unwrap_or_default().to_string_lossy();
            table.with_file_name(format!("{name}_{}", self.choice_name()))
        });
        let (Some(stem), Some(Ok(csv))) = (stem, &self.table) else {
            self.caracal = None;
            open(Mode::Review, table, cx);
            return;
        };
        let mut rows: Vec<usize> = chosen
            .iter()
            .flat_map(|&i| self.records.all[i].rows.iter().copied())
            .collect();
        rows.sort_unstable();
        let saved = review::subset(csv, &rows, &stem);
        crate::log::line(match &saved {
            Ok(subset) => format!(
                "Caracal: {} rows of {} saved as {}",
                rows.len(),
                table.display(),
                subset.display()
            ),
            Err(err) => format!("Caracal: cannot save the chosen rows: {err}"),
        });
        if let Ok(subset) = &saved {
            open(Mode::Review, subset, cx);
        }
        self.caracal = Some(saved.map(|subset| (subset, chosen.len())));
        cx.notify();
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
                                    this.caracal = None;
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
                    this.caracal = None;
                    cx.notify();
                });
            },
        );
        let chosen = self.chosen();
        let videos = chosen
            .iter()
            .filter(|&&i| self.records.all[i].video)
            .count();
        v_flex()
            .gap(tokens.px("space-3"))
            .child(
                h_flex()
                    .items_start()
                    .gap(tokens.px("space-5"))
                    .child(
                        v_flex()
                            .flex_1()
                            .gap(tokens.px("space-2"))
                            .child(
                                h_flex()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_size(tokens.px("text-md"))
                                            .font_medium()
                                            .child(t("物种", "Species")),
                                    )
                                    .child(
                                        h_flex()
                                            .gap(tokens.px("space-3"))
                                            .px(tokens.px("space-2"))
                                            .child(choice_link(
                                                "invert-species",
                                                t("反选", "Invert"),
                                                {
                                                    let all: BTreeSet<String> = species
                                                        .iter()
                                                        .map(|(name, _)| name.clone())
                                                        .collect();
                                                    move |this| {
                                                        this.species = all
                                                            .difference(&this.species)
                                                            .cloned()
                                                            .collect();
                                                    }
                                                },
                                                cx,
                                            ))
                                            .child(choice_link(
                                                "clear-species",
                                                t("清空", "Clear"),
                                                |this| {
                                                    this.species.clear();
                                                },
                                                cx,
                                            )),
                                    ),
                            )
                            .child(list),
                    )
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

/// A small text button above the species list (反选, 清空).
fn choice_link(
    id: &'static str,
    label: &'static str,
    change: impl Fn(&mut Browse) + 'static,
    cx: &mut Context<Browse>,
) -> Stateful<Div> {
    div()
        .id(id)
        .cursor_pointer()
        .text_size(cx.tokens().px("text-sm"))
        .text_color(cx.theme().muted_foreground)
        .hover(|style| style.text_color(cx.theme().foreground))
        .child(label)
        .on_click(cx.listener(move |this, _, _, cx| {
            change(this);
            this.review = Review::None;
            this.caracal = None;
            cx.notify();
        }))
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
                t("标签读取生成的 tags.csv", "A tags.csv from Read tags"),
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
        // An edited table is usually another file (Caracal's export), so 应用表格到文件 asks for it.
        let apply = self.action(
            t("应用表格到文件", "Write a table into the files"),
            t(
                "选择编辑后的标签表（例如从 Caracal 导出的），检查改动后写回 XMP 文件。",
                "Choose an edited tags table (for example one exported from Caracal), check the changes, and write it into the XMP files.",
            ),
            Button::new("apply-table")
                .outline()
                .label(t("选择表格…", "Choose a table…"))
                .on_click(cx.listener(|_, _, window, cx| {
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
                            let _ = this.update(cx, |_, cx| cx.emit(BrowseEvent::ApplyTable(path)));
                        }
                    })
                    .detach();
                }))
                .into_any_element(),
            None,
            cx,
        );
        let Some(path) = csv.filter(|_| matches!(self.table, Some(Ok(_)))) else {
            return v_flex()
                .gap(tokens.px("space-4"))
                .child(table_card)
                .child(apply);
        };
        let chosen = self.chosen().len();

        let review_status: Option<Div> = match &self.review {
            Review::None => None,
            Review::Making => Some(help_line(t("正在建立…", "Making it…"), Tone::Quiet, cx)),
            Review::Made(folder, n, missing) => {
                let folder = folder.clone();
                Some(
                    h_flex()
                        .gap(tokens.px("space-2"))
                        .child(help_line(
                            match language() {
                                Language::Chinese if *missing > 0 => format!(
                                    "已建立 {} 个文件的查看文件夹，已在资源管理器中打开；另有 {} 个文件找不到。",
                                    count(*n),
                                    count(*missing)
                                ),
                                Language::Chinese => {
                                    format!("已建立 {} 个文件的查看文件夹，已在资源管理器中打开。", count(*n))
                                }
                                Language::English if *missing > 0 => format!(
                                    "Made a review folder of {} files and opened it; {} files were not found.",
                                    count(*n),
                                    count(*missing)
                                ),
                                Language::English => format!("Made a review folder of {} files and opened it.", count(*n)),
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
            }
            Review::CannotLink(why) => Some(
                h_flex()
                    .gap(tokens.px("space-2"))
                    .child(div().flex_1().min_w_0().child(help_line(
                        match (language(), why) {
                            (Language::Chinese, review::NoLinks::FileSystem(fs)) => format!(
                                "这块磁盘是 {fs} 格式，不支持硬链接（NTFS 磁盘才可以）。可以在 Caracal 中查看所选，或用媒体提取复制出来。"
                            ),
                            (Language::Chinese, review::NoLinks::Network) => {
                                "这个网络位置不支持硬链接（取决于 NAS 的设置）。可以在 Caracal 中查看所选，或用媒体提取复制出来。".to_string()
                            }
                            (Language::Chinese, review::NoLinks::Other(err)) => {
                                format!("无法建立链接（{err}）。可以在 Caracal 中查看所选，或用媒体提取复制出来。")
                            }
                            (Language::English, review::NoLinks::FileSystem(fs)) => format!(
                                "This disk is {fs}, which has no hard links (NTFS disks do). View the choice in Caracal, or copy it with Copy media."
                            ),
                            (Language::English, review::NoLinks::Network) => {
                                "This network location does not allow hard links (it depends on the server). View the choice in Caracal, or copy it with Copy media.".to_string()
                            }
                            (Language::English, review::NoLinks::Other(err)) => format!(
                                "Links cannot be made ({err}). View the choice in Caracal, or copy it with Copy media."
                            ),
                        },
                        Tone::Danger,
                        cx,
                    )))
                    .child({
                        let (csv, species, rating) = (path.clone(), self.species.clone(), self.rating);
                        Button::new("to-extract").outline().compact().label(t("用媒体提取复制", "Copy them instead")).on_click(
                            cx.listener(move |_, _, _, cx| {
                                cx.emit(BrowseEvent::Extract { csv: csv.clone(), species: species.clone(), rating })
                            }),
                        )
                    }),
            ),
            Review::NotFound(first) => Some(help_line(
                match language() {
                    Language::Chinese => format!(
                        "找不到表中的媒体文件，例如 {}。表格是否来自另一台电脑，或移动硬盘的盘符变了？",
                        first.display()
                    ),
                    Language::English => format!(
                        "The media in the table are not found, e.g. {}. Is the table from another computer, or has the drive letter changed?",
                        first.display()
                    ),
                },
                Tone::Danger,
                cx,
            )),
            Review::Failed(err) => Some(help_line(err.clone(), Tone::Danger, cx)),
        };
        // The choice feeds both ways of looking; 应用表格到文件 takes the whole table.
        let choice = card(cx)
            .gap(tokens.px("space-3"))
            .child(
                v_flex()
                    .gap(tokens.px("space-1"))
                    .child(div().font_semibold().child(t("挑选", "Choose")))
                    .child(help_line(
                        t(
                            "按物种和评分挑选，在资源管理器或 Caracal 中查看。不挑选时，Caracal 打开整张表。",
                            "Choose by species and rating, then look in the file manager or Caracal. With nothing chosen, Caracal opens the whole table.",
                        ),
                        Tone::Quiet,
                        cx,
                    )),
            )
            .child(self.render_choice(cx));
        // Hard links look like copies but are the same files (browse.md §3): said before the folder is made.
        let links_warning = div()
            .p(tokens.px("space-3"))
            .rounded(tokens.px("radius-sm"))
            .bg(tokens.color("warning-soft"))
            .text_size(tokens.px("text-sm"))
            .child(t(
                "查看文件夹中的文件是原文件的「链接」：编辑内容会直接改动原文件；删除只删除链接，原文件不受影响。",
                "The files in a review folder are links to the originals: editing them changes the originals; deleting removes only the link.",
            ));
        let explorer = self.action(
            t("在资源管理器中查看所选", "Look at the choice in the file manager"),
            t(
                "生成对应媒体的硬链接：缩略图、系统播放器、复制功能都可直接使用，不会额外占用存储空间。",
                "Makes hard links to the chosen media: thumbnails, players and copying work as usual, with no extra storage.",
            ),
            Button::new("make-review")
                .outline()
                .label(t("建立查看文件夹", "Make a review folder"))
                .disabled(chosen == 0 || matches!(self.review, Review::Making))
                .on_click(cx.listener(|this, _, _, cx| this.make_review(cx)))
                .into_any_element(),
            Some(
                v_flex()
                    .gap(tokens.px("space-2"))
                    .child(links_warning)
                    .children(review_status),
            ),
            cx,
        );
        let caracal_status = self.caracal.as_ref().map(|saved| match saved {
            Ok((subset, n)) => help_line(
                match language() {
                    Language::Chinese => format!(
                        "已将所选 {} 个文件的行另存为 {}，并在 Caracal 中打开。",
                        count(*n),
                        subset.display()
                    ),
                    Language::English => format!(
                        "Saved the rows of the {} chosen files as {} and opened it in Caracal.",
                        count(*n),
                        subset.display()
                    ),
                },
                Tone::Quiet,
                cx,
            ),
            Err(err) => help_line(
                format!("{} {err}", t("无法保存所选：", "Cannot save the choice:")),
                Tone::Danger,
                cx,
            ),
        });
        let caracal = {
            let path = path.clone();
            self.action(
                t("在 Caracal 中查看和编辑", "View and edit in Caracal"),
                t(
                    "在浏览器中打开所选：查看、评分、修改物种后导出 tags.csv，再回到这里「应用表格到文件」。",
                    "Opens the choice in Caracal in the browser: view, rate and fix species, export tags.csv, then come back here to write the table into the files.",
                ),
                Button::new("open-caracal")
                    .outline()
                    .label(t("打开 Caracal", "Open Caracal"))
                    .on_click(cx.listener(move |this, _, _, cx| this.open_caracal(&path, cx)))
                    .into_any_element(),
                caracal_status,
                cx,
            )
        };
        v_flex()
            .gap(tokens.px("space-4"))
            .child(table_card)
            .child(choice)
            .child(explorer)
            .child(caracal)
            .child(apply)
    }
}
