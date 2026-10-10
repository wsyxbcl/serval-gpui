//! Pieces every command form shares (Design/spec/shell.md §3): cards, field labels, help lines, path fields, and
//! the quiet line that says what Waxbill found in an input.

use crate::i18n::t;
use crate::theme::TokensExt;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme, Sizable as _, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::{Path, PathBuf};

/// A white card holding one group of fields.
pub fn card(cx: &App) -> Div {
    let tokens = cx.tokens();
    v_flex()
        .gap(tokens.px("space-4"))
        .p(tokens.px("space-5"))
        .bg(tokens.color("surface"))
        .border_1()
        .border_color(cx.theme().border)
        .rounded(tokens.px("radius-md"))
}

/// A field: its label above, its control and help lines below.
pub fn field(label: &'static str, cx: &App) -> Div {
    v_flex().gap(cx.tokens().px("space-2")).child(
        div()
            .text_size(cx.tokens().px("text-md"))
            .font_medium()
            .child(label),
    )
}

#[derive(Clone, Copy, PartialEq)]
pub enum Tone {
    Quiet,
    Danger,
}

/// One line under a field: what Waxbill found, or what is wrong.
pub fn help_line(text: impl Into<SharedString>, tone: Tone, cx: &App) -> Div {
    let tokens = cx.tokens();
    div()
        .text_size(tokens.px("text-sm"))
        .text_color(match tone {
            Tone::Quiet => cx.theme().muted_foreground,
            Tone::Danger => tokens.color("danger"),
        })
        .child(text.into())
}

/// What an input folder holds, counted without running Serval.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FolderCount {
    pub images: usize,
    pub videos: usize,
    pub sidecars: usize,
}

const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png"];
const VIDEO_EXTENSIONS: &[&str] = &["avi", "mp4", "mov"];

/// Count media and sidecars under `dir`, skipping Serval's own output folders (as Serval does).
pub fn count_folder(dir: &Path) -> FolderCount {
    let mut count = FolderCount::default();
    let mut stack: Vec<PathBuf> = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                if name != "serval_output" && !name.ends_with("_serval_output") {
                    stack.push(path);
                }
                continue;
            }
            let ext = name.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("");
            if ext == "xmp" {
                count.sidecars += 1;
            } else if IMAGE_EXTENSIONS.contains(&ext) {
                count.images += 1;
            } else if VIDEO_EXTENSIONS.contains(&ext) {
                count.videos += 1;
            }
        }
    }
    count
}

/// Ask for a folder and hand it to the form.
pub fn pick_folder<V: 'static>(
    window: &mut Window,
    cx: &mut Context<V>,
    apply: impl FnOnce(&mut V, PathBuf, &mut Window, &mut Context<V>) + 'static,
) {
    let paths = cx.prompt_for_paths(PathPromptOptions {
        files: false,
        directories: true,
        multiple: false,
        prompt: None,
    });
    cx.spawn_in(window, async move |this, cx| {
        if let Ok(Ok(Some(paths))) = paths.await
            && let Some(path) = paths.into_iter().next()
        {
            let _ = this.update_in(cx, |this, window, cx| apply(this, path, window, cx));
        }
    })
    .detach();
}

/// The output location field (run.md §5): the resolved root, "默认" or "自定义", "更改…", and "恢复默认" for a
/// custom one. `root` is `None` until the input is chosen.
pub fn output_field<V: 'static>(
    root: Option<&Path>,
    custom: bool,
    cx: &mut Context<V>,
    on_change: impl Fn(&mut V, Option<PathBuf>, &mut Context<V>) + Clone + 'static,
) -> Div {
    let tokens = cx.tokens().clone();
    let muted = cx.theme().muted_foreground;
    let change = on_change.clone();
    field(t("输出位置", "Output location"), cx)
        .child(
            h_flex()
                .gap(tokens.px("space-2"))
                .child(
                    h_flex()
                        .flex_1()
                        .min_w_0()
                        .h(tokens.px("field-h"))
                        .px(tokens.px("space-3"))
                        .gap(tokens.px("space-2"))
                        .bg(tokens.color("surface"))
                        .border_1()
                        .border_color(tokens.color("input-border"))
                        .rounded(tokens.px("radius-sm"))
                        .justify_between()
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_size(tokens.px("text-md"))
                                .when(root.is_none(), |d| d.text_color(muted))
                                .child(
                                    root.map(|p| p.to_string_lossy().to_string())
                                        .unwrap_or_else(|| {
                                            t("选择输入后确定", "Set after choosing the input")
                                                .to_string()
                                        }),
                                ),
                        )
                        .when(root.is_some(), |d| {
                            d.child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(tokens.px("text-sm"))
                                    .text_color(muted)
                                    .child(if custom {
                                        t("自定义", "custom")
                                    } else {
                                        t("默认", "default")
                                    }),
                            )
                        }),
                )
                .child(
                    Button::new("choose-output")
                        .label(t("更改…", "Change…"))
                        .outline()
                        .large()
                        .on_click(cx.listener(move |_, _, window, cx| {
                            let change = change.clone();
                            pick_folder(window, cx, move |this, path, _, cx| {
                                change(this, Some(path), cx)
                            });
                        })),
                ),
        )
        .child(
            h_flex()
                .gap(tokens.px("space-2"))
                .child(help_line(
                    if custom {
                        t(
                            "结果写入你选择的文件夹。",
                            "Results go to the folder you chose.",
                        )
                    } else {
                        t(
                            "默认放在输入文件夹旁边，不会混进媒体文件夹。",
                            "By default next to the input folder, never inside it.",
                        )
                    },
                    Tone::Quiet,
                    cx,
                ))
                .when(custom, |row| {
                    row.child(
                        Button::new("reset-output")
                            .ghost()
                            .compact()
                            .label(t("恢复默认", "Use default"))
                            .on_click(cx.listener(move |this, _, _, cx| on_change(this, None, cx))),
                    )
                }),
        )
}
