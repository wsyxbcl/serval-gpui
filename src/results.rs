//! The result area above the form after a run (Design/spec/run.md §3, §4): the banner, warnings, produced files
//! and the log.

use crate::i18n::{count, t};
use crate::run::{Outcome, Run};
use crate::theme::TokensExt;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

const MAX_WARNING_ROWS: usize = 200;

pub fn banner(run: &Run, cx: &App) -> Option<Div> {
    let tokens = cx.tokens();
    let warnings = run.warnings.len() + run.errors.iter().filter(|(_, p)| p.is_some()).count();
    let (fill, title, body) = match run.outcome {
        Outcome::Running | Outcome::NeedsDecision => return None,
        Outcome::Finished if warnings == 0 => (
            tokens.color("success-soft"),
            t("完成", "Finished").to_string(),
            run.result_sentence(),
        ),
        Outcome::Finished => (
            tokens.color("warning-soft"),
            match crate::i18n::language() {
                crate::i18n::Language::Chinese => format!("完成，有 {} 条警告", count(warnings)),
                crate::i18n::Language::English => format!("Finished with {} warnings", count(warnings)),
            },
            run.result_sentence(),
        ),
        Outcome::Failed => (
            tokens.color("danger-soft"),
            t("失败", "Failed").to_string(),
            format!(
                "{} {}",
                run.failure(),
                t(
                    "修正后重新运行；已完成的部分会被识别并跳过。",
                    "Fix this and run again; finished work is recognised and skipped."
                )
            ),
        ),
        Outcome::Stopped => (
            tokens.color("info-soft"),
            t("已停止", "Stopped").to_string(),
            t(
                "已写入的文件都是完整的。继续运行会跳过已完成的部分。",
                "Files written so far are complete. Running again skips finished work.",
            )
            .to_string(),
        ),
    };
    Some(
        v_flex()
            .gap(tokens.px("space-1"))
            .p(tokens.px("space-4"))
            .rounded(tokens.px("radius-md"))
            .bg(fill)
            .child(div().font_semibold().child(title))
            .child(div().text_size(tokens.px("text-sm")).child(body)),
    )
}

/// Warnings and per-file errors, at most 200 rows.
pub fn warning_list(run: &Run, cx: &App) -> Option<Div> {
    let tokens = cx.tokens();
    let rows: Vec<&(String, Option<String>)> = run
        .warnings
        .iter()
        .chain(run.errors.iter().filter(|(_, p)| p.is_some()))
        .collect();
    if rows.is_empty() {
        return None;
    }
    let more = rows.len().saturating_sub(MAX_WARNING_ROWS);
    Some(
        v_flex()
            .gap(px(2.))
            .text_size(tokens.px("text-sm"))
            .children(rows.into_iter().take(MAX_WARNING_ROWS).map(|(message, path)| {
                h_flex()
                    .gap(tokens.px("space-2"))
                    .child(div().flex_shrink_0().text_color(tokens.color("warning")).child("•"))
                    .child(div().child(match path {
                        Some(path) => format!("{path} — {message}"),
                        None => message.clone(),
                    }))
            }))
            .when(more > 0, |list| {
                list.child(
                    div()
                        .text_color(cx.theme().muted_foreground)
                        .child(match crate::i18n::language() {
                            crate::i18n::Language::Chinese => format!("另外 {} 条见运行日志", count(more)),
                            crate::i18n::Language::English => format!("{} more in the log", count(more)),
                        }),
                )
            }),
    )
}

/// One row per file or folder the run produced, with "打开".
pub fn output_list(run: &Run, cx: &App) -> Option<Div> {
    if run.outputs.is_empty() {
        return None;
    }
    let tokens = cx.tokens();
    Some(
        v_flex()
            .gap(tokens.px("space-1"))
            .children(run.outputs.iter().enumerate().map(|(i, file)| {
                let path = file.path.clone();
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.to_string_lossy().into_owned());
                let detail = match (file.kind.as_str(), file.rows) {
                    (_, Some(rows)) => match crate::i18n::language() {
                        crate::i18n::Language::Chinese => format!("{} 行", count(rows as usize)),
                        crate::i18n::Language::English => format!("{} rows", count(rows as usize)),
                    },
                    ("folder", _) => t("文件夹", "folder").to_string(),
                    _ => String::new(),
                };
                h_flex()
                    .gap(tokens.px("space-3"))
                    .px(tokens.px("space-3"))
                    .py(tokens.px("space-2"))
                    .rounded(tokens.px("radius-sm"))
                    .bg(tokens.color("surface"))
                    .border_1()
                    .border_color(cx.theme().border)
                    .child(div().flex_1().min_w_0().truncate().font_medium().child(name))
                    .child(
                        div()
                            .text_size(tokens.px("text-sm"))
                            .text_color(cx.theme().muted_foreground)
                            .child(detail),
                    )
                    .child(
                        Button::new(("open-output", i))
                            .ghost()
                            .label(t("打开", "Open"))
                            .on_click(move |_, _, cx| cx.open_with_system(&path)),
                    )
            })),
    )
}

/// The log: Serval's ordinary output and the command line, monospace. Collapsed after success.
pub fn log(run: &Run, on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static, cx: &App) -> Div {
    let tokens = cx.tokens();
    let text = run.log.join("\n");
    v_flex()
        .gap(tokens.px("space-2"))
        .child(
            h_flex()
                .gap(tokens.px("space-3"))
                .child(
                    h_flex()
                        .id("log-toggle")
                        .gap(tokens.px("space-1"))
                        .cursor_pointer()
                        .child(if run.log_open { "▾" } else { "▸" })
                        .child(t("运行日志", "Run log"))
                        .on_click(on_toggle),
                )
                .child({
                    let text = text.clone();
                    Button::new("copy-log")
                        .ghost()
                        .label(t("复制日志", "Copy log"))
                        .on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(text.clone())))
                }),
        )
        .when(run.log_open, |column| {
            column.child(
                div()
                    .id("log")
                    .max_h(px(320.))
                    .overflow_y_scroll()
                    .p(tokens.px("space-3"))
                    .rounded(tokens.px("radius-sm"))
                    .bg(cx.theme().muted)
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_size(tokens.px("text-xs"))
                    .children(run.log.iter().map(|line| div().child(line.clone()))),
            )
        })
}
