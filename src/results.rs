//! The result area above the form after a run (Design/spec/run.md §3, §4): the banner, warnings, produced files
//! and the log.

use crate::i18n::{count, t};
use crate::run::{Outcome, Run};
use crate::theme::TokensExt;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

const MAX_WARNING_ROWS: usize = 200;

pub fn banner(run: &Run, cx: &App) -> Option<Div> {
    let tokens = cx.tokens();
    let warnings = run.warnings.len() + run.errors.iter().filter(|(_, p)| p.is_some()).count();
    let (fill, title, body) = match run.outcome {
        Outcome::Running | Outcome::NeedsDecision => return None,
        Outcome::Finished if run.is_preview() => (
            tokens.color("success-soft"),
            t("预览完成", "Preview finished").to_string(),
            t(
                "没有改动任何文件。将要做的改动列在下方运行日志里；确认无误后按「按此执行」。",
                "Nothing was changed. The planned changes are listed in the run log below; when they look right, press Run it for real.",
            )
            .to_string(),
        ),
        Outcome::Finished if warnings == 0 => (
            tokens.color("success-soft"),
            t("完成", "Finished").to_string(),
            run.result_sentence(),
        ),
        Outcome::Finished => (
            tokens.color("warning-soft"),
            match crate::i18n::language() {
                crate::i18n::Language::Chinese => format!("完成，有 {} 条警告", count(warnings)),
                crate::i18n::Language::English => {
                    format!("Finished with {} warnings", count(warnings))
                }
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
            t("已暂停", "Paused").to_string(),
            t(
                "已写入的文件都是完整的。继续会跳过已完成的部分；取消则回到就绪。",
                "Files written so far are complete. Continue skips finished work; Cancel goes back to Ready.",
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
            .children(
                rows.into_iter()
                    .take(MAX_WARNING_ROWS)
                    .map(|(message, path)| {
                        h_flex()
                            .gap(tokens.px("space-2"))
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_color(tokens.color("warning"))
                                    .child("•"),
                            )
                            .child(div().flex_1().min_w_0().child(match path {
                                Some(path) => format!("{path} — {message}"),
                                None => message.clone(),
                            }))
                    }),
            )
            .when(more > 0, |list| {
                list.child(div().text_color(cx.theme().muted_foreground).child(
                    match crate::i18n::language() {
                        crate::i18n::Language::Chinese => {
                            format!("另外 {} 条见运行日志", count(more))
                        }
                        crate::i18n::Language::English => {
                            format!("{} more in the log", count(more))
                        }
                    },
                ))
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
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_medium()
                            .child(name),
                    )
                    .child(
                        div()
                            .text_size(tokens.px("text-sm"))
                            .text_color(cx.theme().muted_foreground)
                            .child(detail),
                    )
                    .children(crate::caracal::Mode::for_output(&path).map(|mode| {
                        let path = path.clone();
                        Button::new(("caracal-output", i))
                            .ghost()
                            .label(t("在 Caracal 中打开", "Open in Caracal"))
                            .on_click(move |_, _, cx| crate::caracal::open(mode, &path, cx))
                    }))
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
pub fn log(
    run: &Run,
    on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Div {
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
                        .on_click(move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(text.clone()))
                        })
                }),
        )
        .when(run.log_open, |column| {
            let scroll = run.log_scroll.clone();
            let at_edge = scroll.clone();
            column.child(
                // The page only gets the wheel once the log is at its top or bottom: GPUI scrolls every area
                // under the pointer, and the page moving with it would carry the log away while reading.
                div()
                    .on_scroll_wheel(move |event, window, cx| {
                        let dy = event.delta.pixel_delta(window.line_height()).y;
                        let (offset, max) = (at_edge.offset().y, at_edge.max_offset().y);
                        let room = if dy < px(0.) {
                            offset > -max
                        } else {
                            offset < px(0.)
                        };
                        if dy != px(0.) && room {
                            cx.stop_propagation();
                        }
                    })
                    .child(
                        div()
                            .id("log")
                            .max_h(px(320.))
                            .overflow_y_scroll()
                            .track_scroll(&scroll)
                            .vertical_scrollbar(&scroll)
                            .p(tokens.px("space-3"))
                            .rounded(tokens.px("radius-sm"))
                            .bg(cx.theme().muted)
                            .font_family(cx.theme().mono_font_family.clone())
                            .text_size(tokens.px("text-xs"))
                            .children(run.log.iter().enumerate().map(|(i, line)| {
                                div()
                                    .debug_selector(move || format!("log-line-{i}"))
                                    .child(line.clone())
                            })),
                    ),
            )
        })
}

#[cfg(test)]
mod tests {
    use crate::run::Run;
    use gpui_kit::component::theme::ThemeMode;
    use gpui_kit::component::v_flex;
    use gpui_kit::{
        Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, ScrollDelta,
        ScrollWheelEvent, StatefulInteractiveElement as _, Styled as _, TestAppContext,
        VisualTestContext, Window, div, point, px,
    };

    struct Page {
        run: Run,
    }

    impl Render for Page {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            // As the app lays out a screen: a scrolling page with the results in it.
            v_flex().size_full().child(
                div()
                    .id("screen")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(
                        v_flex()
                            .child(div().h(px(100.)).debug_selector(|| "above".into()))
                            .child(super::log(&self.run, |_, _, _| {}, cx))
                            .child(div().h(px(1500.))),
                    ),
            )
        }
    }

    #[gpui_kit::test]
    fn the_log_scrolls_inside_the_scrolling_page(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::theme::init(ThemeMode::Light, cx);
        });
        let mut run = Run::failed_to_start("serval observe".into(), "error".into());
        run.log_open = true;
        run.log.extend((0..200).map(|i| format!("line {i}")));
        let (_, cx) = cx.add_window_view(|_, _| Page { run });
        cx.run_until_parked();
        cx.update(|window, cx| _ = window.draw(cx));

        let offset = |cx: &mut VisualTestContext| {
            cx.debug_bounds("log-line-2").unwrap().top() - cx.debug_bounds("above").unwrap().top()
        };
        for delta in [
            ScrollDelta::Pixels(point(px(0.), px(-60.))),
            ScrollDelta::Lines(point(0., -3.)),
        ] {
            let before = offset(cx);
            let page = cx.debug_bounds("above").unwrap().top();
            let line = cx.debug_bounds("log-line-5").unwrap();
            cx.simulate_event(ScrollWheelEvent {
                position: line.center(),
                delta,
                ..Default::default()
            });
            cx.update(|window, cx| _ = window.draw(cx));
            eprintln!(
                "{delta:?}: log {before:?} → {:?}, page {page:?} → {:?}",
                offset(cx),
                cx.debug_bounds("above").unwrap().top()
            );
            assert!(
                offset(cx) < before,
                "the log did not scroll: {before:?} → {:?}",
                offset(cx)
            );
            assert_eq!(
                cx.debug_bounds("above").unwrap().top(),
                page,
                "the page moved with the log"
            );
        }
        // At the bottom of the log, the wheel scrolls the page again.
        let line = cx.debug_bounds("log-line-5").unwrap();
        for _ in 0..2 {
            cx.simulate_event(ScrollWheelEvent {
                position: line.center(),
                delta: ScrollDelta::Pixels(point(px(0.), px(-5000.))),
                ..Default::default()
            });
            cx.update(|window, cx| _ = window.draw(cx));
        }
        assert!(
            cx.debug_bounds("above").unwrap().top() < px(0.),
            "the page did not take over"
        );
    }
}
