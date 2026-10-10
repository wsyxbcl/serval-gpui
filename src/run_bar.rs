//! The run bar (Design/spec/run.md §1): what will happen, what is happening or what happened, the exact command,
//! and the screen's one primary button.

use crate::i18n::t;
use crate::theme::TokensExt;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::progress::Progress;
use gpui_kit::component::{ActiveTheme, Disableable as _, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

#[derive(Clone, Copy, PartialEq)]
pub enum Tone {
    Idle,
    Running,
    Success,
    Warning,
    Danger,
}

pub struct Bar {
    pub tone: Tone,
    pub title: String,
    pub detail: String,
    /// While running: the fraction done, or `None` when the total is unknown.
    pub progress: Option<Option<f32>>,
    pub command: String,
    pub button: &'static str,
    /// The button's shortcut, as a GPUI keystroke ("secondary-enter").
    pub shortcut: &'static str,
    pub enabled: bool,
}

pub fn run_bar<F: Fn(&ClickEvent, &mut Window, &mut App) + 'static>(
    bar: Bar,
    on_click: F,
    cx: &App,
) -> impl IntoElement + use<F> {
    let tokens = cx.tokens();
    let dot = match bar.tone {
        Tone::Idle => tokens.color("subtle-foreground"),
        Tone::Running => tokens.color("primary"),
        Tone::Success => tokens.color("success"),
        Tone::Warning => tokens.color("warning"),
        Tone::Danger => tokens.color("danger"),
    };
    let command = bar.command.clone();
    let running = bar.tone == Tone::Running;

    v_flex()
        .flex_shrink_0()
        .bg(tokens.color("surface"))
        .border_t_1()
        .border_color(cx.theme().border)
        .when_some(bar.progress, |column, fraction| {
            column.child(
                Progress::new("run-progress")
                    .value(fraction.map(|f| f * 100.0).unwrap_or(0.0))
                    .h(px(3.))
                    .rounded_none(),
            )
        })
        .child(
            h_flex()
                .h(tokens.px("runbar-h"))
                .px(tokens.px("space-6"))
                .gap(tokens.px("space-4"))
                .justify_between()
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap(tokens.px("space-1"))
                        .child(
                            h_flex()
                                .gap(tokens.px("space-2"))
                                .text_size(tokens.px("text-md"))
                                .child(div().size(px(8.)).rounded_full().bg(dot))
                                .child(div().font_semibold().child(bar.title))
                                .when(!bar.detail.is_empty(), |row| {
                                    row.child(
                                        div()
                                            .truncate()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(format!("· {}", bar.detail)),
                                    )
                                }),
                        )
                        .child(
                            div()
                                .font_family(cx.theme().mono_font_family.clone())
                                .text_size(tokens.px("text-xs"))
                                .text_color(cx.theme().muted_foreground)
                                .truncate()
                                .child(bar.command),
                        ),
                )
                .child(
                    h_flex()
                        .gap(tokens.px("space-3"))
                        .child(
                            Button::new("copy-command")
                                .ghost()
                                .label(t("复制命令", "Copy command"))
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(command.clone()))
                                }),
                        )
                        .child(
                            Button::new("run")
                                .map(|button| if running { button.outline() } else { button.primary() })
                                .disabled(!bar.enabled)
                                .label(bar.button)
                                .child(
                                    Kbd::new(Keystroke::parse(bar.shortcut).unwrap())
                                        .outline()
                                        .ml(tokens.px("space-2")),
                                )
                                .on_click(on_click),
                        ),
                ),
        )
}
