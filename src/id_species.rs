//! The identified-species question after an observe scan (Design/spec/questions.md §4, board questions.html
//! observe-id-species-after-scan): only when Serval reports images whose individual IDs cannot be given to one
//! species. The choices are the real species from those images; the answer is remembered per folder.

use crate::app::Waxbill;
use crate::i18n::{Language, count, language, t};
use crate::run::took;
use crate::screens::ScreenId;
use crate::theme::TokensExt;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::{ActiveTheme, StyledExt, h_flex, v_flex};
use gpui_kit::*;
use std::collections::BTreeSet;

impl Waxbill {
    /// Open the question, with the species that have individual IDs preselected.
    pub(crate) fn ask_id_species(&mut self, cx: &mut Context<Self>) {
        let Some((_, species)) = self
            .runs
            .get(&ScreenId::Observe)
            .and_then(|r| r.unresolved.as_ref())
        else {
            return;
        };
        self.id_species = Some(
            species
                .iter()
                .filter(|s| !s.individuals.is_empty())
                .map(|s| s.name.clone())
                .collect(),
        );
        cx.notify();
    }

    /// The banner that stays in the results after "先不处理".
    pub(crate) fn render_unresolved_banner(&self, cx: &mut Context<Self>) -> Option<Div> {
        let run = self.runs.get(&self.screen)?;
        let (images, _) = run
            .unresolved
            .as_ref()
            .filter(|_| self.screen == ScreenId::Observe && !run.is_running())?;
        let tokens = cx.tokens().clone();
        Some(
            h_flex()
                .justify_between()
                .gap(tokens.px("space-4"))
                .p(tokens.px("space-4"))
                .rounded(tokens.px("radius-md"))
                .bg(tokens.color("warning-soft"))
                .child(
                    v_flex()
                        .gap(tokens.px("space-1"))
                        .child(div().font_semibold().child(match language() {
                            Language::Chinese => format!("{} 张图片的个体无法归到物种", count(*images as usize)),
                            Language::English => format!("The individuals in {} images could not be given to a species", count(*images as usize)),
                        }))
                        .child(div().text_size(tokens.px("text-sm")).child(t(
                            "这些图片有多个物种和个体编号，但没有或有多个“已做个体识别的物种”。表里先按全部组合列出。",
                            "These images have several species and individual IDs, and no single identified species. The table lists every combination for now.",
                        ))),
                )
                .child(
                    Button::new("ask-id-species")
                        .outline()
                        .label(t("设置物种并重新运行", "Choose species and rerun"))
                        .on_click(cx.listener(|this, _, _, cx| this.ask_id_species(cx))),
                ),
        )
    }

    pub(crate) fn render_id_species(&self, cx: &mut Context<Self>) -> Option<Div> {
        let chosen = self.id_species.as_ref()?;
        let run = self.runs.get(&ScreenId::Observe)?;
        let (images, species) = run.unresolved.as_ref()?;
        let tokens = cx.tokens().clone();
        let list = v_flex()
            .gap(tokens.px("space-2"))
            .children(species.iter().enumerate().map(|(i, s)| {
                let name = s.name.clone();
                let mut detail = match language() {
                    Language::Chinese => format!("{} 张", count(s.images as usize)),
                    Language::English => format!("{} images", count(s.images as usize)),
                };
                if !s.individuals.is_empty() {
                    let shown = s
                        .individuals
                        .iter()
                        .take(2)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(t("、", ", "));
                    detail.push_str(&match (language(), s.individuals.len()) {
                        (Language::Chinese, n) if n > 2 => format!(" · {shown} 等 {n} 个编号"),
                        (Language::Chinese, _) => format!(" · {shown}"),
                        (Language::English, n) if n > 2 => {
                            format!(" · {shown} and {} more IDs", n - 2)
                        }
                        (Language::English, _) => format!(" · {shown}"),
                    });
                }
                h_flex()
                    .justify_between()
                    .px(tokens.px("space-3"))
                    .py(tokens.px("space-2"))
                    .rounded(tokens.px("radius-sm"))
                    .border_1()
                    .border_color(cx.theme().border)
                    .child(
                        Checkbox::new(("id-species", i))
                            .label(s.name.clone())
                            .checked(chosen.contains(&name))
                            .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                                if let Some(chosen) = &mut this.id_species {
                                    if *checked {
                                        chosen.insert(name.clone());
                                    } else {
                                        chosen.remove(&name);
                                    }
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .text_size(tokens.px("text-sm"))
                            .text_color(cx.theme().muted_foreground)
                            .child(detail),
                    )
            }));
        let rerun_label = match language() {
            Language::Chinese => format!("重新运行（约 {}）", took(run.elapsed)),
            Language::English => format!("Run again (about {})", took(run.elapsed)),
        };
        let dialog = v_flex()
            .w(px(520.))
            .gap(tokens.px("space-4"))
            .p(tokens.px("space-5"))
            .rounded(tokens.px("radius-lg"))
            .bg(tokens.color("surface"))
            .shadow_lg()
            .child(div().text_size(tokens.px("text-lg")).font_semibold().child(match language() {
                Language::Chinese => format!("{} 张图片的个体编号归属不明确", count(*images as usize)),
                Language::English => format!("Unclear individuals in {} images", count(*images as usize)),
            }))
            .child(div().text_size(tokens.px("text-sm")).child(t(
                "这些图片里同时有多个物种和个体编号，XMP 没有记录编号属于哪个物种。勾选做了个体识别的物种，编号就会归到它们。",
                "These images have several species and individual IDs, and the XMP does not say which species an ID belongs to. Check the species that were individually identified; the IDs go to them.",
            )))
            .child(list)
            .child(
                div()
                    .text_size(tokens.px("text-sm"))
                    .text_color(cx.theme().muted_foreground)
                    .child(t("选择会记住，下次处理这个文件夹时直接使用。", "The choice is remembered for this folder.")),
            )
            .child(
                h_flex()
                    .justify_end()
                    .gap(tokens.px("space-2"))
                    .child(Button::new("id-later").outline().label(t("先不处理", "Not now")).on_click(cx.listener(|this, _, _, cx| {
                        this.id_species = None;
                        cx.notify();
                    })))
                    .child(Button::new("id-rerun").primary().label(rerun_label).on_click(cx.listener(|this, _, window, cx| {
                        let chosen: BTreeSet<String> = this.id_species.take().unwrap_or_default();
                        this.observe.update(cx, |form, cx| form.set_id_species(chosen.into_iter().collect(), window, cx));
                        this.start_decided_run(&[], cx);
                    }))),
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
