//! The conflicts dialog (Design/spec/questions.md §3, board run-states.html extract-conflicts): Serval checked every
//! target, found some that already hold different files, and stopped before writing anything (exit code 3). One
//! choice, then the same command runs again with `--on-existing`.

use crate::app::Waxbill;
use crate::i18n::{Language, count, language, t};
use crate::run::{Needs, Outcome};
use crate::theme::TokensExt;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

const LISTED: usize = 200;
const CHOICES: [&str; 3] = ["skip", "replace", "rename"];

fn reason_words(reason: &str) -> &'static str {
    match reason {
        "file-differs" => t("文件不同", "file differs"),
        "sidecar-differs" => t("XMP 不同", "XMP differs"),
        "unrelated-sidecar" => t("旁边有无关的 XMP", "an unrelated XMP is next to it"),
        _ => t("内容不同", "differs"),
    }
}

/// The last two components of a target: enough to recognise it ("dep01/IMG_0042.JPG").
fn short_target(target: &str) -> String {
    let parts: Vec<&str> = target
        .split(['/', '\\'])
        .filter(|p| !p.is_empty())
        .collect();
    parts[parts.len().saturating_sub(2)..].join("/")
}

/// "IMG_0042.JPG" → "IMG_0042_1.JPG", as Serval names a renamed copy.
fn renamed(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((stem, ext)) => format!("{stem}_1.{ext}"),
        None => format!("{name}_1"),
    }
}

impl Waxbill {
    pub(crate) fn needs_decision(&self) -> Option<&Needs> {
        let run = self.runs.get(&self.screen)?;
        (run.outcome == Outcome::NeedsDecision)
            .then_some(run.needs.as_ref())
            .flatten()
    }

    pub(crate) fn render_conflicts(&self, cx: &mut Context<Self>) -> Option<Div> {
        let needs = self.needs_decision()?;
        let tokens = cx.tokens().clone();
        let example = needs
            .items
            .first()
            .map(|item| {
                renamed(
                    item.target
                        .rsplit(['/', '\\'])
                        .next()
                        .unwrap_or("IMG_0042.JPG"),
                )
            })
            .unwrap_or_else(|| "IMG_0042_1.JPG".into());
        let title = match language() {
            Language::Chinese => format!("{} 个目标已存在，内容不同", count(needs.count as usize)),
            Language::English => format!(
                "{} targets already exist with different content",
                count(needs.count as usize)
            ),
        };
        let mut body = String::new();
        if needs.ready > 0 {
            body.push_str(&match language() {
                Language::Chinese => {
                    format!("其余 {} 个可直接复制。", count(needs.ready as usize))
                }
                Language::English => format!(
                    "The other {} can be copied directly. ",
                    count(needs.ready as usize)
                ),
            });
        }
        if needs.already > 0 {
            body.push_str(&match language() {
                Language::Chinese => format!("{} 个已经复制过。", count(needs.already as usize)),
                Language::English => {
                    format!("{} are already in place. ", count(needs.already as usize))
                }
            });
        }
        body.push_str(t("还没有写入任何文件。", "Nothing has been written yet."));
        let more = (needs.count as usize).saturating_sub(needs.items.len().min(LISTED));
        let list =
            v_flex()
                .id("conflict-list")
                .max_h(px(180.))
                .overflow_y_scroll()
                .p(tokens.px("space-3"))
                .gap(px(2.))
                .rounded(tokens.px("radius-sm"))
                .bg(cx.theme().muted)
                .text_size(tokens.px("text-sm"))
                .children(needs.items.iter().take(LISTED).map(|item| {
                    h_flex()
                        .justify_between()
                        .gap(tokens.px("space-3"))
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .font_family(cx.theme().mono_font_family.clone())
                                .child(short_target(&item.target)),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_color(cx.theme().muted_foreground)
                                .child(
                                    item.reasons
                                        .iter()
                                        .map(|r| reason_words(r))
                                        .collect::<Vec<_>>()
                                        .join(t("、", ", ")),
                                ),
                        )
                }))
                .when(more > 0, |list| {
                    list.child(div().text_color(cx.theme().muted_foreground).child(
                        match language() {
                            Language::Chinese => format!("… 另外 {} 个", count(more)),
                            Language::English => format!("… {} more", count(more)),
                        },
                    ))
                });
        let options = [
            (
                t("跳过它们", "Skip them"),
                t(
                    "保留已有的文件，只复制其余的。",
                    "Keep the files there and copy the rest.",
                )
                .to_string(),
            ),
            (
                t("替换它们", "Replace them"),
                t(
                    "用这次的文件覆盖已有的。",
                    "Overwrite them with these files.",
                )
                .to_string(),
            ),
            (
                t("另存为新名称", "Save under new names"),
                match language() {
                    Language::Chinese => {
                        format!("保留两份，新的命名为 {example}（XMP 同名跟随）。")
                    }
                    Language::English => {
                        format!("Keep both; the new one becomes {example} (its XMP follows).")
                    }
                },
            ),
        ];
        let choices =
            v_flex()
                .gap(tokens.px("space-2"))
                .children(options.into_iter().enumerate().map(|(i, (label, help))| {
                    let chosen = self.conflict_choice == i;
                    v_flex()
                        .id(("conflict-choice", i))
                        .gap(px(2.))
                        .px(tokens.px("space-4"))
                        .py(tokens.px("space-3"))
                        .rounded(tokens.px("radius-sm"))
                        .border_1()
                        .cursor_pointer()
                        .map(|card| {
                            if chosen {
                                card.border_color(tokens.color("primary"))
                                    .bg(tokens.color("primary").opacity(0.06))
                            } else {
                                card.border_color(cx.theme().border)
                            }
                        })
                        .child(
                            h_flex()
                                .gap(tokens.px("space-2"))
                                .child(
                                    div()
                                        .size(px(14.))
                                        .rounded_full()
                                        .border_1()
                                        .border_color(if chosen {
                                            tokens.color("primary")
                                        } else {
                                            tokens.color("input-border")
                                        })
                                        .when(chosen, |dot| dot.border_4()),
                                )
                                .child(div().font_medium().child(label)),
                        )
                        .child(
                            div()
                                .pl(px(22.))
                                .text_size(tokens.px("text-sm"))
                                .text_color(cx.theme().muted_foreground)
                                .child(help),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.conflict_choice = i;
                            cx.notify();
                        }))
                }));
        let screen = self.screen;
        let dialog = v_flex()
            .w(px(560.))
            .max_h(relative(0.9))
            .gap(tokens.px("space-4"))
            .p(tokens.px("space-5"))
            .rounded(tokens.px("radius-lg"))
            .bg(tokens.color("surface"))
            .shadow_lg()
            .child(
                div()
                    .text_size(tokens.px("text-lg"))
                    .font_semibold()
                    .child(title),
            )
            .child(div().text_size(tokens.px("text-sm")).child(body))
            .child(list)
            .child(choices)
            .child(
                h_flex()
                    .justify_end()
                    .gap(tokens.px("space-2"))
                    .child(
                        Button::new("conflict-cancel")
                            .outline()
                            .label(t("取消", "Cancel"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                // Nothing was written: back to Ready.
                                this.runs.remove(&screen);
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("conflict-continue")
                            .primary()
                            .label(t("继续", "Continue"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                let choice = CHOICES[this.conflict_choice];
                                this.conflict_choice = 0; // asked again next time (questions.md §3)
                                this.start_decided_run(&["--on-existing", choice], cx);
                            })),
                    ),
            );
        Some(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(tokens.color("overlay"))
                .child(dialog),
        )
    }
}
