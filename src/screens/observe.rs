//! 标签读取 Read tags: `serval observe` (board: shell.html, observe-ready).

use super::Plan;
use super::form::{
    FolderCount, Tone, accept_drop, card, count_folder, field, help_line, output_field, pick_folder,
};
use crate::i18n::{count, t};
use crate::output;
use crate::theme::TokensExt;
use gpui_kit::component::button::Button;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// What Waxbill found in the input folder.
#[derive(Clone, Debug, PartialEq)]
enum Found {
    Nothing,
    Counting,
    Counted(FolderCount),
    List { rows: usize, files: usize },
    Invalid(String),
    Missing,
}

pub struct ObserveForm {
    input: Entity<InputState>,
    read_sidecars: bool,
    images: bool,
    videos: bool,
    found: Found,
    custom_output: Option<PathBuf>,
    advanced: bool,
    debug: bool,
    /// Species whose individual IDs win in images with several species (`--id-species`), remembered per folder.
    id_species: Vec<String>,
    count_generation: usize,
    /// Its command is running: the inputs are disabled (the app also blocks the mouse).
    pub locked: bool,
    pub supports_files_from: bool,
    _subscriptions: Vec<Subscription>,
}

impl ObserveForm {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t(
                "选择或拖入文件夹或 CSV 清单",
                "Choose or drop a folder or CSV list",
            ))
        });
        let subscription = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, _, cx| {
            if matches!(event, InputEvent::Change) {
                this.input_changed(cx);
            }
        });
        ObserveForm {
            input,
            // XMP sidecars by default: media metadata, videos especially, is read less reliably.
            read_sidecars: true,
            images: true,
            videos: true,
            found: Found::Nothing,
            custom_output: None,
            advanced: false,
            debug: false,
            id_species: Vec::new(),
            count_generation: 0,
            locked: false,
            supports_files_from: false,
            _subscriptions: vec![subscription],
        }
    }

    /// Fill the form for a design state, with what Waxbill would have found.
    pub fn set_example(
        &mut self,
        path: &str,
        found: FolderCount,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.input.update(cx, |input, cx| {
            input.set_value(path.to_string(), window, cx)
        });
        self.found = Found::Counted(found);
        self.count_generation += 1; // no count runs for the example path
        cx.notify();
    }

    /// The values remembered for the next launch.
    pub fn values(&self, _cx: &App) -> Vec<(&'static str, String)> {
        let flag = |b: bool| if b { "1" } else { "0" }.to_string();
        vec![
            ("xmp", flag(self.read_sidecars)),
            ("images", flag(self.images)),
            ("videos", flag(self.videos)),
        ]
    }

    /// Restore the values of the last run.
    pub fn restore(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let settings = cx.default_global::<crate::settings::Settings>().clone();
        let value = |field: &str| settings.form_value("observe", field);
        self.read_sidecars = value("xmp") != Some("0");
        self.images = value("images") != Some("0");
        self.videos = value("videos") != Some("0");
    }

    pub fn set_input(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        self.input
            .update(cx, |input, cx| input.set_value(path, window, cx));
        self.input_changed(cx);
    }

    /// The answer to the identified-species question, remembered for this folder.
    pub fn set_id_species(
        &mut self,
        species: Vec<String>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(input) = self.input_path(cx) {
            let key = folder_key(&input);
            let value = species.join("\t");
            crate::settings::Settings::update(cx, |settings| settings.set_value(&key, value));
        }
        self.id_species = species;
        cx.notify();
    }

    fn input_path(&self, cx: &App) -> Option<PathBuf> {
        let value = self.input.read(cx).value();
        let value = value.trim();
        (!value.is_empty())
            .then(|| std::path::absolute(value).unwrap_or_else(|_| PathBuf::from(value)))
    }

    /// Count the folder a moment after typing stops.
    fn input_changed(&mut self, cx: &mut Context<Self>) {
        self.id_species = self
            .input_path(cx)
            .and_then(|input| {
                let settings = cx.default_global::<crate::settings::Settings>();
                settings.value(&folder_key(&input)).map(|v| {
                    v.split('\t')
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .collect()
                })
            })
            .unwrap_or_default();
        self.count_generation += 1;
        let generation = self.count_generation;
        let Some(path) = self.input_path(cx) else {
            self.found = Found::Nothing;
            cx.notify();
            return;
        };
        self.found = Found::Counting;
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(300))
                .await;
            if !this
                .update(cx, |this, _| this.count_generation == generation)
                .unwrap_or(false)
            {
                return;
            }
            let found = cx
                .background_executor()
                .spawn(async move {
                    if path.is_dir() {
                        Found::Counted(count_folder(&path))
                    } else if csv_extension(&path) {
                        preview_list(&path).unwrap_or_else(Found::Invalid)
                    } else {
                        Found::Missing
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.count_generation == generation {
                    this.found = found;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn choose_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
                    this.set_input(path.to_string_lossy().into_owned(), window, cx)
                });
            }
        })
        .detach();
    }

    fn choose_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        pick_folder(window, cx, |this, path, window, cx| {
            this.set_input(path.to_string_lossy().to_string(), window, cx)
        });
    }

    fn output_root(&self, input: &Path) -> PathBuf {
        self.custom_output
            .clone()
            .unwrap_or_else(|| output::default_root(input))
    }

    pub fn plan(&self, cx: &App) -> Plan {
        let input = self.input_path(cx);
        let is_list =
            input.as_deref().is_some_and(csv_extension) && !matches!(self.found, Found::Counted(_));
        let mut args = vec!["observe".to_string()];
        if let Some(input) = &input {
            args.push("-o".into());
            args.push(
                output::command_dir(&self.output_root(input), "observe")
                    .to_string_lossy()
                    .into(),
            );
        }
        if self.read_sidecars {
            args.push("-x".into());
        }
        if self.debug {
            args.push("--debug".into());
        }
        for species in &self.id_species {
            args.extend(["--id-species".into(), species.clone()]);
        }
        match (self.images, self.videos) {
            (true, false) => args.push("--image".into()),
            (false, true) => args.push("--video".into()),
            _ => {}
        }
        if let Some(input) = &input {
            if is_list {
                args.push("--files-from".into());
            }
            args.push(input.to_string_lossy().into());
        }
        let ready = match (&input, &self.found) {
            (None, _) => Err(t(
                "选择媒体文件夹或 CSV 清单",
                "choose a media folder or CSV list",
            )
            .to_string()),
            _ if is_list && !self.supports_files_from => Err(t(
                "请在设置中选择支持 --files-from 的 Serval",
                "choose a Serval with --files-from support in Settings",
            )
            .to_string()),
            (_, Found::Invalid(error)) => Err(error.clone()),
            (_, Found::Counting | Found::Nothing) if is_list => {
                Err(t("正在读取清单…", "Reading the list…").to_string())
            }
            (Some(_), Found::Missing) => {
                Err(t("媒体文件夹不存在", "the media folder does not exist").to_string())
            }
            _ if !self.images && !self.videos => {
                Err(t("至少选择一种媒体类型", "choose at least one media type").to_string())
            }
            (Some(_), Found::Counted(found)) => {
                let files = if self.read_sidecars {
                    found.sidecars
                } else {
                    (if self.images { found.images } else { 0 })
                        + (if self.videos { found.videos } else { 0 })
                };
                Ok(match crate::i18n::language() {
                    crate::i18n::Language::Chinese => format!("将读取 {} 个文件", count(files)),
                    crate::i18n::Language::English => format!("will read {} files", count(files)),
                })
            }
            _ => Ok(String::new()),
        };
        let cwd = input
            .as_ref()
            .map(|input| self.output_root(input))
            .and_then(|root| {
                root.ancestors()
                    .find(|dir| dir.is_dir())
                    .map(Path::to_path_buf)
            })
            .unwrap_or_else(std::env::temp_dir);
        Plan { args, cwd, ready }
    }

    fn found_line(&self, cx: &App) -> Option<Div> {
        let text = match &self.found {
            Found::Nothing => return None,
            Found::Counting => t("正在统计…", "Counting…").to_string(),
            Found::Missing => {
                return Some(help_line(
                    t("找不到这个文件夹", "This folder does not exist"),
                    Tone::Danger,
                    cx,
                ));
            }
            Found::Invalid(message) => return Some(help_line(message.clone(), Tone::Danger, cx)),
            Found::List { rows, files } => match crate::i18n::language() {
                crate::i18n::Language::Chinese => {
                    format!("{} 行，{} 个不同文件路径", count(*rows), count(*files))
                }
                crate::i18n::Language::English => {
                    format!("{} rows, {} unique file paths", count(*rows), count(*files))
                }
            },
            Found::Counted(found) => match crate::i18n::language() {
                crate::i18n::Language::Chinese => format!(
                    "找到 {} 张图片、{} 个视频、{} 个 XMP",
                    count(found.images),
                    count(found.videos),
                    count(found.sidecars)
                ),
                crate::i18n::Language::English => format!(
                    "Found {} images, {} videos and {} XMP sidecars",
                    count(found.images),
                    count(found.videos),
                    count(found.sidecars)
                ),
            },
        };
        Some(help_line(text, Tone::Quiet, cx))
    }
}

impl Render for ObserveForm {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.tokens().clone();
        let output_root = self.input_path(cx).map(|input| self.output_root(&input));

        let input_card = card(cx)
            .child(accept_drop(
                field(t("媒体文件夹或 CSV 清单", "Media folder or CSV list"), cx)
                    .child(
                        h_flex()
                            .gap(tokens.px("space-2"))
                            .child(
                                div().flex_1().child(
                                    Input::new(&self.input)
                                        .large()
                                        .disabled(self.locked)
                                        .bg(tokens.color("surface")),
                                ),
                            )
                            .child(
                                Button::new("choose-input")
                                    .label(t("文件夹…", "Folder…"))
                                    .outline()
                                    .large()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.choose_folder(window, cx)
                                    })),
                            )
                            .child(
                                Button::new("choose-list")
                                    .label("CSV…")
                                    .outline()
                                    .large()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.choose_list(window, cx)
                                    })),
                            ),
                    )
                    .children(self.found_line(cx)),
                &self.input,
                cx,
            ))
            .child(
                field(t("读取来源", "Read from"), cx).child(
                    RadioGroup::horizontal("source")
                        .child(Radio::new("source-media").label(t("媒体文件", "Media files")))
                        .child(Radio::new("source-xmp").label(t("XMP sidecar", "XMP sidecars")))
                        .selected_index(Some(self.read_sidecars as usize))
                        .on_click(cx.listener(|this, index: &usize, _, cx| {
                            this.read_sidecars = *index == 1;
                            cx.notify();
                        })),
                ),
            )
            .child(
                field(t("媒体类型", "Media types"), cx).child(
                    h_flex()
                        .gap(tokens.px("space-5"))
                        .child(
                            Checkbox::new("images")
                                .label(t("图片", "Images"))
                                .checked(self.images)
                                .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                    this.images = *checked;
                                    cx.notify();
                                })),
                        )
                        .child(
                            Checkbox::new("videos")
                                .label(t("视频", "Videos"))
                                .checked(self.videos)
                                .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                    this.videos = *checked;
                                    cx.notify();
                                })),
                        ),
                ),
            );

        let output_card = card(cx)
            .child(output_field(
                output_root.as_deref(),
                self.custom_output.is_some(),
                cx,
                |this: &mut Self, path, cx| {
                    this.custom_output = path;
                    cx.notify();
                },
            ))
            .child(
                h_flex()
                    .id("advanced")
                    .gap(tokens.px("space-1"))
                    .cursor_pointer()
                    .text_size(tokens.px("text-md"))
                    .child(if self.advanced { "▾" } else { "▸" })
                    .child(t("高级选项", "Advanced options"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.advanced = !this.advanced;
                        cx.notify();
                    })),
            )
            .when(self.advanced, |card| {
                card.child(
                    v_flex()
                        .gap(tokens.px("space-1"))
                        .child(
                            Checkbox::new("debug")
                                .label(t("生成 debug 表格（raw.csv）", "Write the debug table (raw.csv)"))
                                .checked(self.debug)
                                .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                    this.debug = *checked;
                                    cx.notify();
                                })),
                        )
                        .child(help_line(
                            t(
                                "包含位点和文件修改时间，可用于修正时间（xmp update --datetime）。",
                                "Adds the deployment and file modification time, for fixing times (xmp update --datetime).",
                            ),
                            Tone::Quiet,
                            cx,
                        )),
                )
            });

        v_flex()
            .gap(tokens.px("space-4"))
            .child(input_card)
            .child(output_card)
    }
}

/// The settings key of a folder's identified species.
fn folder_key(folder: &Path) -> String {
    format!(
        "folder.{}.id_species",
        folder.to_string_lossy().trim_end_matches(['/', '\\'])
    )
}

fn csv_extension(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("csv"))
}

fn preview_list(path: &Path) -> Result<Found, String> {
    let reader = csv::Reader::from_path(path).map_err(|err| err.to_string())?;
    count_list(reader, path.parent().unwrap_or(Path::new(".")))
}

fn count_list<R: std::io::Read>(mut reader: csv::Reader<R>, base: &Path) -> Result<Found, String> {
    let column = reader
        .headers()
        .map_err(|err| err.to_string())?
        .iter()
        .position(|name| name == "path")
        .ok_or_else(|| t("清单缺少 path 列", "the list needs a path column").to_string())?;
    let mut files = HashSet::new();
    let mut rows = 0;
    for record in reader.records() {
        let record = record.map_err(|err| err.to_string())?;
        rows += 1;
        let file = record.get(column).unwrap_or_default();
        if file.is_empty() {
            return Err(format!(
                "{} {}",
                t("path 为空，行号", "empty path at row"),
                rows + 1
            ));
        }
        files.insert(base.join(file));
    }
    if files.is_empty() {
        return Err(t("清单中没有文件", "the list contains no files").to_string());
    }
    Ok(Found::List {
        rows,
        files: files.len(),
    })
}

#[cfg(test)]
mod list_tests {
    use super::{Found, count_list};
    use std::path::Path;

    #[test]
    fn counts_rows_and_distinct_paths_relative_to_the_manifest() {
        let base = std::env::temp_dir();
        let path = base.join("a,1.jpg");
        let input = format!(
            "\u{feff}species,path\r\ncat,\"a,1.jpg\"\r\nfox,\"{}\"\r\ncat,b.jpg\r\n",
            path.display()
        );
        assert_eq!(
            count_list(csv::Reader::from_reader(input.as_bytes()), &base),
            Ok(Found::List { rows: 3, files: 2 })
        );
    }

    #[test]
    fn rejects_lists_without_file_paths() {
        for input in ["species\ncat\n", "path\n", "species,path\ncat,\n"] {
            assert!(
                count_list(csv::Reader::from_reader(input.as_bytes()), Path::new(".")).is_err()
            );
        }
    }
}
