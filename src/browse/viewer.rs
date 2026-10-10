//! The viewer and editing (Design/spec/browse.md §4–5, board browse.html browse-viewer): one image at screen size,
//! ← → through the current filter order, rating 1–5 and species/individual edits written in the background.

use super::records::Records;
use super::writer::{self, Field, SaveState};
use super::{Browse, ServalProgram};
use crate::i18n::{Language, count, language, t};
use crate::protocol::{FileState, Outcome};
use crate::theme::TokensExt;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme, Icon, Sizable as _, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

impl Browse {
    pub(super) fn open_viewer(&mut self, cx: &mut Context<Self>) {
        if self.focused.is_some() {
            self.viewer = true;
            cx.notify();
        }
    }

    fn close_viewer(&mut self, cx: &mut Context<Self>) {
        self.viewer = false;
        self.adding = None;
        self.flush(cx);
        cx.notify();
    }

    /// Move the focus by `delta` in the current order; moving on writes the queued edits.
    fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(focused) = self.focused else {
            if !self.visible.is_empty() {
                self.focused = Some(0);
            }
            cx.notify();
            return;
        };
        let next = (focused as isize + delta).clamp(0, self.visible.len() as isize - 1) as usize;
        if next != focused {
            self.focused = Some(next);
            self.adding = None;
            self.flush(cx);
            self.scroll
                .scroll_to_item(next / self.columns.max(1), ScrollStrategy::Center);
        }
        cx.notify();
    }

    pub(super) fn key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        let columns = if self.viewer {
            1
        } else {
            self.columns.max(1) as isize
        };
        match key {
            "left" => self.step(-1, cx),
            "right" => self.step(1, cx),
            "up" if !self.viewer => self.step(-columns, cx),
            "down" if !self.viewer => self.step(columns, cx),
            "space" | "enter" if !self.viewer => self.open_viewer(cx),
            "escape" if self.viewer => self.close_viewer(cx),
            "0" | "1" | "2" | "3" | "4" | "5" if !event.keystroke.modifiers.modified() => {
                if let Some(position) = self.focused {
                    self.rate(self.visible[position], key.parse().unwrap(), cx);
                }
            }
            _ => return,
        }
        cx.stop_propagation();
    }

    /// After an edit: counts update at once; a record that no longer matches stays where it is, dimmed.
    fn edited(&mut self, index: usize, cx: &mut Context<Self>) {
        self.counts = self.records.counts(&self.filters);
        if !Records::keeps(&self.filters, &self.records.all[index], None) {
            self.stale.insert(index);
        } else {
            self.stale.remove(&index);
        }
        cx.notify();
    }

    pub(super) fn rate(&mut self, index: usize, rating: u8, cx: &mut Context<Self>) {
        let record = &mut self.records.all[index];
        if record.rating == rating {
            return;
        }
        record.rating = rating;
        self.queue.rating(&record.sidecar.clone(), rating);
        self.edited(index, cx);
    }

    fn set_list(&mut self, index: usize, field: Field, list: Vec<String>, cx: &mut Context<Self>) {
        let record = &mut self.records.all[index];
        let current = match field {
            Field::Species => &mut record.species,
            Field::Individuals => &mut record.individuals,
        };
        if *current == list {
            return;
        }
        let before = std::mem::replace(current, list.clone());
        let sidecar = record.sidecar.clone();
        self.queue.list(&sidecar, field, &before, list);
        self.edited(index, cx);
    }

    /// Write the queue with one `serval xmp set` call.
    pub(super) fn flush(&mut self, cx: &mut Context<Self>) {
        if self.queue.is_empty() || matches!(self.save, SaveState::Writing(_)) {
            return;
        }
        let Some(program) = cx.try_global::<ServalProgram>().and_then(|p| p.0.clone()) else {
            self.save = SaveState::Failed(
                t("需要 Serval 才能写入", "Serval is needed to write").to_string(),
            );
            cx.notify();
            return;
        };
        let edits = self.queue.take();
        self.save = SaveState::Writing(edits.len());
        cx.notify();
        let log = super::thumbs::cache_dir().join("logs").join("browse.log");
        let _ = std::fs::create_dir_all(log.parent().unwrap());
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { writer::write(&program, &edits, &log) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.save = match result {
                    Ok(states) => this.apply(states),
                    Err(err) => SaveState::Failed(err),
                };
                if !this.queue.is_empty() {
                    this.flush(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Records hold what Serval reports is on disk; a conflict or error is shown with its reason.
    fn apply(&mut self, states: Vec<FileState>) -> SaveState {
        let mut problem = None;
        for state in states {
            if let Some(&i) = self.by_sidecar.get(&state.path)
                && let Some(labels) = &state.labels
            {
                let record = &mut self.records.all[i];
                record.species = labels.species.clone();
                record.individuals = labels.individuals.clone();
                record.rating = labels.rating.clamp(0, 5) as u8;
            }
            if matches!(state.result, Some(Outcome::Conflict | Outcome::Error)) && problem.is_none()
            {
                let name = state
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                problem = Some(format!("{name}: {}", state.message.unwrap_or_default()));
            }
        }
        match problem {
            Some(problem) => SaveState::Failed(problem),
            None => SaveState::Saved,
        }
    }

    pub(super) fn save_indicator(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = cx.tokens().clone();
        let pending = self.queue.len();
        let (dot, text) = match (&self.save, pending) {
            (SaveState::Writing(n), _) => (
                tokens.color("primary"),
                match language() {
                    Language::Chinese => format!("正在写入 {n} 项…"),
                    Language::English => format!("Writing {n}…"),
                },
            ),
            (SaveState::Failed(reason), _) => (
                tokens.color("danger"),
                format!("{} · {reason}", t("写入失败", "Not saved")),
            ),
            (SaveState::Saved, 0) => (
                tokens.color("success"),
                t("✓ 已保存", "✓ Saved").to_string(),
            ),
            (SaveState::Saved, n) => (
                tokens.color("subtle-foreground"),
                match language() {
                    Language::Chinese => format!("{n} 项待写入"),
                    Language::English => format!("{n} to write"),
                },
            ),
        };
        let failed = matches!(self.save, SaveState::Failed(_));
        h_flex()
            .id("save-state")
            .gap(tokens.px("space-2"))
            .max_w(px(360.))
            .text_size(tokens.px("text-sm"))
            .text_color(if failed {
                tokens.color("danger")
            } else {
                cx.theme().muted_foreground
            })
            .child(div().flex_shrink_0().size(px(7.)).rounded_full().bg(dot))
            .child(div().truncate().child(text))
            .when(failed, |row| {
                row.cursor_pointer().on_click(cx.listener(|this, _, _, cx| {
                    this.save = SaveState::Saved;
                    cx.notify();
                }))
            })
            .into_any_element()
    }

    fn start_adding(&mut self, field: Field, window: &mut Window, cx: &mut Context<Self>) {
        let input = cx
            .new(|cx| InputState::new(window, cx).placeholder(t("输入后回车", "Type, then Enter")));
        let index = self.focused.map(|p| self.visible[p]);
        let is_species = matches!(field, Field::Species);
        cx.subscribe_in(
            &input,
            window,
            move |this, input, event: &InputEvent, _, cx| match event {
                InputEvent::PressEnter { .. } => {
                    let value = input.read(cx).value().trim().to_string();
                    if let Some(index) = index
                        && !value.is_empty()
                    {
                        this.add_value(index, is_species, value, cx);
                    }
                    this.adding = None;
                    cx.notify();
                }
                InputEvent::Change => cx.notify(),
                _ => {}
            },
        )
        .detach();
        input.update(cx, |input, cx| input.focus(window, cx));
        self.adding = Some((field, input));
        cx.notify();
    }

    fn add_value(&mut self, index: usize, species: bool, value: String, cx: &mut Context<Self>) {
        let record = &self.records.all[index];
        let mut list = if species {
            record.species.clone()
        } else {
            record.individuals.clone()
        };
        if !list.contains(&value) {
            list.push(value);
        }
        let field = if species {
            Field::Species
        } else {
            Field::Individuals
        };
        self.set_list(index, field, list, cx);
    }

    fn chips(
        &self,
        index: usize,
        species: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let tokens = cx.tokens().clone();
        let record = &self.records.all[index];
        let list = if species {
            record.species.clone()
        } else {
            record.individuals.clone()
        };
        let mut row = h_flex().flex_wrap().gap(tokens.px("space-1"));
        for (i, value) in list.iter().enumerate() {
            let remaining: Vec<String> = list.iter().filter(|v| *v != value).cloned().collect();
            row = row.child(
                h_flex()
                    .gap(tokens.px("space-1"))
                    .px(tokens.px("space-2"))
                    .py(px(2.))
                    .rounded(tokens.px("radius-sm"))
                    .bg(cx.theme().muted)
                    .child(value.clone())
                    .child(
                        div()
                            .id((if species { "rm-sp" } else { "rm-ind" }, i))
                            .cursor_pointer()
                            .text_color(cx.theme().muted_foreground)
                            .child("×")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let field = if species {
                                    Field::Species
                                } else {
                                    Field::Individuals
                                };
                                this.set_list(index, field, remaining.clone(), cx);
                            })),
                    ),
            );
        }
        let adding_here = self
            .adding
            .as_ref()
            .is_some_and(|(f, _)| matches!(f, Field::Species) == species);
        if adding_here {
            let (_, input) = self.adding.as_ref().unwrap();
            let query = input.read(cx).value().to_lowercase();
            // Suggestions from the table's own values.
            let suggestions: Vec<String> = if species {
                self.counts
                    .species
                    .iter()
                    .map(|(s, _)| s.clone())
                    .filter(|s| !s.is_empty())
                    .collect()
            } else {
                let mut all: Vec<String> = self
                    .records
                    .all
                    .iter()
                    .flat_map(|r| r.individuals.iter().cloned())
                    .collect();
                all.sort();
                all.dedup();
                all
            };
            let suggestions: Vec<String> = suggestions
                .into_iter()
                .filter(|s| {
                    !list.contains(s) && (query.is_empty() || s.to_lowercase().contains(&query))
                })
                .take(6)
                .collect();
            return v_flex()
                .gap(tokens.px("space-1"))
                .child(row)
                .child(Input::new(input).small())
                .child(h_flex().flex_wrap().gap(tokens.px("space-1")).children(
                    suggestions.into_iter().enumerate().map(|(i, s)| {
                        div()
                            .id(("suggest", i))
                            .px(tokens.px("space-2"))
                            .py(px(1.))
                            .rounded(tokens.px("radius-sm"))
                            .border_1()
                            .border_color(cx.theme().border)
                            .text_size(tokens.px("text-sm"))
                            .cursor_pointer()
                            .child(s.clone())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.add_value(index, species, s.clone(), cx);
                                this.adding = None;
                                cx.notify();
                            }))
                    }),
                ));
        }
        let _ = window;
        row.child(
            div()
                .id(if species { "add-sp" } else { "add-ind" })
                .px(tokens.px("space-2"))
                .cursor_pointer()
                .text_color(cx.theme().muted_foreground)
                .child(if species {
                    t("＋ 添加", "+ Add")
                } else {
                    "＋"
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.start_adding(
                        if species {
                            Field::Species
                        } else {
                            Field::Individuals
                        },
                        window,
                        cx,
                    )
                })),
        )
    }

    pub(super) fn render_viewer(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let tokens = cx.tokens().clone();
        let position = self.focused.unwrap();
        let index = self.visible[position];
        let record = self.records.all[index].clone();
        let muted = cx.theme().muted_foreground;

        let header = h_flex()
            .h(px(52.))
            .px(tokens.px("space-4"))
            .gap(tokens.px("space-4"))
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                Button::new("back")
                    .ghost()
                    .label(t("← 网格", "← Grid"))
                    .on_click(cx.listener(|this, _, _, cx| this.close_viewer(cx))),
            )
            .child(
                h_flex()
                    .gap(tokens.px("space-1"))
                    .child(div().font_semibold().child(count(position + 1)))
                    .child(
                        div()
                            .text_color(muted)
                            .child(format!("/ {}", count(self.visible.len()))),
                    ),
            )
            .child(div().flex_1())
            .child(self.save_indicator(cx))
            .child({
                let media = record.media.clone();
                Button::new("reveal")
                    .ghost()
                    .label(t("在文件夹中显示", "Show in folder"))
                    .on_click(move |_, _, cx| cx.reveal_path(&media))
            })
            .child({
                let media = record.media.clone();
                Button::new("open")
                    .ghost()
                    .label(t("用默认程序打开", "Open with default app"))
                    .on_click(move |_, _, cx| cx.open_with_system(&media))
            });

        let image: AnyElement = if record.video {
            v_flex()
                .items_center()
                .gap(tokens.px("space-3"))
                .child(
                    Icon::new(IconName::Play)
                        .size(px(48.))
                        .text_color(gpui_kit::white()),
                )
                .child(div().text_color(gpui_kit::white()).child(t(
                    "视频请用默认程序播放",
                    "Play videos with the default app",
                )))
                .into_any_element()
        } else {
            img(record.media.clone())
                .size_full()
                .object_fit(ObjectFit::Contain)
                .into_any_element()
        };
        // The next and previous images load ahead, out of sight.
        let ahead: Vec<AnyElement> = [position.checked_sub(1), Some(position + 1)]
            .into_iter()
            .flatten()
            .filter_map(|p| self.visible.get(p))
            .filter(|&&i| !self.records.all[i].video)
            .map(|&i| {
                img(self.records.all[i].media.clone())
                    .size(px(1.))
                    .opacity(0.)
                    .into_any_element()
            })
            .collect();
        let arrow =
            |id: &'static str, label: &'static str, delta: isize, cx: &mut Context<Self>| {
                div()
                    .id(id)
                    .px(tokens.px("space-3"))
                    .py(tokens.px("space-5"))
                    .cursor_pointer()
                    .text_color(gpui_kit::white().opacity(0.7))
                    .text_size(px(22.))
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| this.step(delta, cx)))
            };
        let stage = h_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .bg(rgb(0x111716))
            .track_focus(&self.focus)
            .on_key_down(
                cx.listener(|this, event: &KeyDownEvent, window, cx| this.key(event, window, cx)),
            )
            .child(arrow("prev", "‹", -1, cx))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .py(tokens.px("space-4"))
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .flex_1()
                            .w_full()
                            .min_h_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(image),
                    )
                    .child(
                        div()
                            .pt(tokens.px("space-2"))
                            .text_size(tokens.px("text-xs"))
                            .text_color(gpui_kit::white().opacity(0.7))
                            .child(record.name()),
                    )
                    .children(ahead),
            )
            .child(arrow("next", "›", 1, cx));

        let label = |text: &'static str| {
            div()
                .pt(tokens.px("space-4"))
                .pb(tokens.px("space-1"))
                .text_size(tokens.px("text-sm"))
                .text_color(muted)
                .child(text)
        };
        let stars = h_flex().gap(px(2.)).children((1..=5u8).map(|n| {
            div()
                .id(("star", n as usize))
                .cursor_pointer()
                .text_size(px(22.))
                .text_color(if n <= record.rating {
                    tokens.color("warning")
                } else {
                    cx.theme().border
                })
                .child("★")
                .on_click(cx.listener(move |this, _, _, cx| {
                    let current = this.records.all[index].rating;
                    // Clicking the current rating clears it.
                    this.rate(index, if current == n { 0 } else { n }, cx);
                }))
        }));
        let border = cx.theme().border;
        let key_hint = |keys: &'static str, text: &'static str| {
            h_flex()
                .gap(tokens.px("space-2"))
                .child(
                    div()
                        .px(tokens.px("space-1"))
                        .rounded(px(3.))
                        .border_1()
                        .border_color(border)
                        .text_size(tokens.px("text-xs"))
                        .child(keys),
                )
                .child(text)
        };
        let side = v_flex()
            .id("viewer-side")
            .w(px(300.))
            .flex_shrink_0()
            .h_full()
            .overflow_y_scroll()
            .p(tokens.px("space-4"))
            .bg(tokens.color("surface"))
            .border_l_1()
            .border_color(cx.theme().border)
            .child(div().font_semibold().child(record.name()))
            .child(
                div()
                    .text_size(tokens.px("text-sm"))
                    .text_color(muted)
                    .child(format!("{}/{}", record.folder, record.name())),
            )
            .child(label(t("评分", "Rating")))
            .child(stars)
            .child(label(t("物种", "Species")))
            .child(self.chips(index, true, window, cx))
            .child(label(t("个体", "Individuals")))
            .child(self.chips(index, false, window, cx))
            .child(label(t("时间", "Time")))
            .child(if record.datetime.is_empty() {
                "—".to_string()
            } else {
                record.datetime.clone()
            })
            .when(!record.deployment.is_empty(), |side| {
                side.child(label(t("部署", "Deployment")))
                    .child(record.deployment.clone())
            })
            .child(
                v_flex()
                    .pt(tokens.px("space-5"))
                    .gap(tokens.px("space-1"))
                    .text_size(tokens.px("text-sm"))
                    .text_color(muted)
                    .child(key_hint("← →", t("上一张 / 下一张", "Previous / next")))
                    .child(key_hint("1–5", t("评分 · 0 清除", "Rate · 0 clears")))
                    .child(key_hint("Esc", t("回到网格", "Back to the grid"))),
            );

        v_flex()
            .size_full()
            .child(header)
            .child(h_flex().flex_1().min_h_0().child(stage).child(side))
    }
}
