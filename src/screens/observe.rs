//! 标签获取 Get tags: `serval observe` (board: shell.html, observe-ready).

use super::Plan;
use super::form::{FolderCount, Tone, card, count_folder, field, help_line};
use crate::i18n::{count, t};
use crate::output;
use crate::theme::TokensExt;
use gpui_kit::component::button::Button;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::{ActiveTheme, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// What Waxbill found in the input folder.
#[derive(Clone, Debug, PartialEq)]
enum Found {
    Nothing,
    Counting,
    Counted(FolderCount),
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
    count_generation: usize,
    _subscriptions: Vec<Subscription>,
}

impl ObserveForm {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t("选择或拖入一个文件夹", "Choose or drop a folder"))
        });
        let subscription = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, _, cx| {
            if matches!(event, InputEvent::Change) {
                this.input_changed(cx);
            }
        });
        ObserveForm {
            input,
            read_sidecars: false,
            images: true,
            videos: true,
            found: Found::Nothing,
            custom_output: None,
            advanced: false,
            count_generation: 0,
            _subscriptions: vec![subscription],
        }
    }

    /// Fill the form for a design state, with what Waxbill would have found.
    pub fn set_example(&mut self, path: &str, found: FolderCount, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| input.set_value(path.to_string(), window, cx));
        self.found = Found::Counted(found);
        self.count_generation += 1; // no count runs for the example path
        cx.notify();
    }

    pub fn set_input(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| input.set_value(path, window, cx));
        self.input_changed(cx);
    }

    fn input_path(&self, cx: &App) -> Option<PathBuf> {
        let value = self.input.read(cx).value();
        let value = value.trim();
        (!value.is_empty()).then(|| PathBuf::from(value))
    }

    /// Count the folder a moment after typing stops.
    fn input_changed(&mut self, cx: &mut Context<Self>) {
        self.count_generation += 1;
        let generation = self.count_generation;
        let Some(path) = self.input_path(cx) else {
            self.found = Found::Nothing;
            cx.notify();
            return;
        };
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(300)).await;
            let still_current = this
                .update(cx, |this, cx| {
                    let current = this.count_generation == generation;
                    if current {
                        this.found = if path.is_dir() { Found::Counting } else { Found::Missing };
                        cx.notify();
                    }
                    current && path.is_dir()
                })
                .unwrap_or(false);
            if !still_current {
                return;
            }
            let counted = cx.background_executor().spawn(async move { count_folder(&path) }).await;
            let _ = this.update(cx, |this, cx| {
                if this.count_generation == generation {
                    this.found = Found::Counted(counted);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn choose_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: None,
        });
        let input = self.input.clone();
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
    }

    fn output_root(&self, input: &Path) -> PathBuf {
        self.custom_output
            .clone()
            .unwrap_or_else(|| output::default_root(input))
    }

    pub fn plan(&self, cx: &App) -> Plan {
        let input = self.input_path(cx);
        let mut args = vec!["observe".to_string(), "--progress".into(), "json".into()];
        if let Some(input) = &input {
            args.push("-o".into());
            args.push(output::command_dir(&self.output_root(input), "observe").to_string_lossy().into());
        }
        if self.read_sidecars {
            args.push("-x".into());
        }
        match (self.images, self.videos) {
            (true, false) => args.push("--image".into()),
            (false, true) => args.push("--video".into()),
            _ => {}
        }
        if let Some(input) = &input {
            args.push(input.to_string_lossy().into());
        }
        let ready = match (&input, &self.found) {
            (None, _) => Err(t("选择媒体文件夹", "choose the media folder").to_string()),
            (Some(_), Found::Missing) => Err(t("媒体文件夹不存在", "the media folder does not exist").to_string()),
            _ if !self.images && !self.videos => {
                Err(t("至少选择一种媒体类型", "choose at least one media type").to_string())
            }
            (Some(_), Found::Counted(found)) => {
                let files = if self.read_sidecars {
                    found.sidecars
                } else {
                    (if self.images { found.images } else { 0 }) + (if self.videos { found.videos } else { 0 })
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
            .and_then(|root| root.ancestors().find(|dir| dir.is_dir()).map(Path::to_path_buf))
            .unwrap_or_else(std::env::temp_dir);
        Plan { args, cwd, ready }
    }

    fn found_line(&self, cx: &App) -> Option<Div> {
        let text = match &self.found {
            Found::Nothing => return None,
            Found::Counting => t("正在统计…", "Counting…").to_string(),
            Found::Missing => {
                return Some(help_line(t("找不到这个文件夹", "This folder does not exist"), Tone::Danger, cx));
            }
            Found::Counted(found) if self.read_sidecars => match crate::i18n::language() {
                crate::i18n::Language::Chinese => format!("找到 {} 个 XMP 旁注文件", count(found.sidecars)),
                crate::i18n::Language::English => format!("Found {} XMP sidecars", count(found.sidecars)),
            },
            Found::Counted(found) => match crate::i18n::language() {
                crate::i18n::Language::Chinese => {
                    format!("找到 {} 张图片、{} 个视频", count(found.images), count(found.videos))
                }
                crate::i18n::Language::English => {
                    format!("Found {} images and {} videos", count(found.images), count(found.videos))
                }
            },
        };
        Some(help_line(text, Tone::Quiet, cx))
    }
}

impl Render for ObserveForm {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.tokens().clone();
        let output_root = self.input_path(cx).map(|input| self.output_root(&input));
        let is_default_output = self.custom_output.is_none();

        let input_card = card(cx)
            .child(
                field(t("媒体文件夹", "Media folder"), cx)
                    .child(
                        h_flex()
                            .gap(tokens.px("space-2"))
                            .child(div().flex_1().child(Input::new(&self.input).large().bg(tokens.color("surface"))))
                            .child(
                                Button::new("choose-input")
                                    .label(t("选择…", "Choose…"))
                                    .outline()
                                    .large()
                                    .on_click(cx.listener(|this, _, window, cx| this.choose_folder(window, cx))),
                            ),
                    )
                    .children(self.found_line(cx)),
            )
            .child(
                field(t("读取来源", "Read from"), cx).child(
                    RadioGroup::horizontal("source")
                        .child(Radio::new("source-media").label(t("媒体文件", "Media files")))
                        .child(Radio::new("source-xmp").label(t("XMP 旁注文件", "XMP sidecars")))
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

        let output_card = card(cx).child(
            field(t("输出位置", "Output location"), cx)
                .child(
                    h_flex()
                        .gap(tokens.px("space-2"))
                        .child(
                            h_flex()
                                .flex_1()
                                .h(tokens.px("field-h"))
                                .px(tokens.px("space-3"))
                                .border_1()
                                .border_color(tokens.color("input-border"))
                                .rounded(tokens.px("radius-sm"))
                                .justify_between()
                                .child(
                                    div()
                                        .text_size(tokens.px("text-md"))
                                        .when(output_root.is_none(), |d| d.text_color(cx.theme().muted_foreground))
                                        .child(
                                            output_root
                                                .as_ref()
                                                .map(|p| p.to_string_lossy().to_string())
                                                .unwrap_or_else(|| t("选择媒体文件夹后确定", "Set after choosing the folder").to_string()),
                                        ),
                                )
                                .when(is_default_output && output_root.is_some(), |d| {
                                    d.child(
                                        div()
                                            .text_size(tokens.px("text-sm"))
                                            .text_color(cx.theme().muted_foreground)
                                            .child(t("默认", "default")),
                                    )
                                }),
                        )
                        .child(Button::new("choose-output").label(t("更改…", "Change…")).outline().large()),
                )
                .child(help_line(
                    t("默认放在输入文件夹旁边，不会混进媒体文件夹。", "By default next to the input folder, never inside it."),
                    Tone::Quiet,
                    cx,
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
                ),
        );

        v_flex().gap(tokens.px("space-4")).child(input_card).child(output_card)
    }
}
