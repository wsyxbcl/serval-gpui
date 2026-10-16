//! Help (帮助, F1): a panel beside the screen with the screen's written text (`help/zh/*.md`) and, on request,
//! Serval's own `--help` for the command (Design/spec/shell.md §3).

use crate::app::Waxbill;
use crate::i18n::{Language, language, t};
use crate::screens::ScreenId;
use crate::theme::TokensExt;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme, Sizable as _, StyledExt, h_flex, v_flex};
use gpui_kit::*;
use std::path::PathBuf;

/// The written help of a screen.
fn text(screen: ScreenId) -> Option<&'static str> {
    use ScreenId::*;
    Some(match screen {
        Observe => include_str!("../help/zh/observe.md"),
        Capture => include_str!("../help/zh/capture.md"),
        Browse => include_str!("../help/zh/browse.md"),
        XmpInit => include_str!("../help/zh/xmp_init.md"),
        XmpUpdate => include_str!("../help/zh/update.md"),
        XmpSync => include_str!("../help/zh/xmp_sync.md"),
        Extract => include_str!("../help/zh/extract.md"),
        XmpCopy => include_str!("../help/zh/xmp_copy.md"),
        Rename => include_str!("../help/zh/rename.md"),
        Align => include_str!("../help/zh/align.md"),
        XmpRemove => include_str!("../help/zh/xmp_remove.md"),
        Translate => include_str!("../help/zh/translate.md"),
        Settings => return None,
    })
}

/// The Serval commands behind a screen, for their `--help`.
fn commands(screen: ScreenId) -> &'static [&'static [&'static str]] {
    use ScreenId::*;
    match screen {
        Observe => &[&["observe"]],
        Capture => &[&["capture"]],
        Browse | XmpUpdate => &[&["xmp", "update"], &["xmp", "write"]],
        XmpInit => &[&["xmp", "init"]],
        XmpSync => &[&["xmp", "sync"]],
        Extract => &[&["extract"]],
        XmpCopy => &[&["xmp", "copy"]],
        Rename => &[&["rename"]],
        Align => &[&["align"]],
        XmpRemove => &[&["xmp", "remove"]],
        Translate => &[&["translate"]],
        Settings => &[],
    }
}

pub fn has_help(screen: ScreenId) -> bool {
    text(screen).is_some()
}

/// Serval's `--help` for each command of the screen, as one Markdown section.
fn serval_help(program: PathBuf, screen: ScreenId) -> String {
    let mut out = String::new();
    for command in commands(screen) {
        let help = crate::runner::background_command(&program)
            .args(command.iter())
            .arg("--help")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default();
        out.push_str(&format!(
            "\n**serval {}**\n\n```\n{}\n```\n",
            command.join(" "),
            help.trim_end()
        ));
    }
    out
}

/// Help's state in the app: open or not, Serval's `--help` shown, and what was fetched.
#[derive(Default)]
pub struct HelpState {
    pub open: bool,
    pub serval_open: bool,
    pub serval: std::collections::HashMap<ScreenId, String>,
}

impl Waxbill {
    pub(crate) fn toggle_help(&mut self, cx: &mut Context<Self>) {
        self.help.open = !self.help.open && has_help(self.screen);
        cx.notify();
    }

    fn toggle_serval_help(&mut self, cx: &mut Context<Self>) {
        self.help.serval_open = !self.help.serval_open;
        let screen = self.screen;
        if self.help.serval_open
            && !self.help.serval.contains_key(&screen)
            && let Some(program) = self.serval.program().map(PathBuf::from)
        {
            cx.spawn(async move |this, cx| {
                let help = cx
                    .background_executor()
                    .spawn(async move { serval_help(program, screen) })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    this.help.serval.insert(screen, help);
                    cx.notify();
                });
            })
            .detach();
        }
        cx.notify();
    }

    pub(crate) fn render_help(&self, cx: &mut Context<Self>) -> Option<Div> {
        if !self.help.open {
            return None;
        }
        let text = text(self.screen)?;
        let tokens = cx.tokens().clone();
        let mut markdown = String::new();
        if language() == Language::English {
            markdown.push_str("*Help is in Chinese for now.*\n\n");
        }
        // The panel's header names the screen; the text's own title would repeat it.
        markdown.push_str(
            text.split_once('\n')
                .filter(|(first, _)| first.starts_with("# "))
                .map_or(text, |(_, rest)| rest),
        );
        if self.help.serval_open {
            markdown.push_str(&format!(
                "\n## {}\n",
                t("命令行参数", "Command-line options")
            ));
            match self.help.serval.get(&self.screen) {
                Some(help) => markdown.push_str(help),
                None if self.serval.program().is_none() => {
                    markdown.push_str(t("需要 Serval 才能显示。", "Shown once Serval is found."))
                }
                None => markdown.push_str(t("正在读取…", "Reading…")),
            }
        }
        let screen_index = self.screen as usize;
        Some(
            v_flex()
                .w(px(420.))
                .flex_shrink_0()
                .h_full()
                .bg(tokens.color("surface"))
                .border_l_1()
                .border_color(cx.theme().border)
                .child(
                    h_flex()
                        .justify_between()
                        .px(tokens.px("space-5"))
                        .py(tokens.px("space-3"))
                        .border_b_1()
                        .border_color(cx.theme().border)
                        .child(div().font_semibold().child(format!(
                            "{} · {}",
                            t("帮助", "Help"),
                            self.screen.label()
                        )))
                        .child(
                            Button::new("help-close")
                                .ghost()
                                .small()
                                .icon(IconName::Close)
                                .on_click(cx.listener(|this, _, _, cx| this.toggle_help(cx))),
                        ),
                )
                .child(
                    div().flex_1().min_h_0().pl(tokens.px("space-5")).child(
                        TextView::markdown(
                            (
                                "help-text",
                                screen_index * 4
                                    + self.help.serval_open as usize * 2
                                    + self.help.serval.contains_key(&self.screen) as usize,
                            ),
                            markdown,
                        )
                        .scrollable(true)
                        .selectable(true)
                        .size_full()
                        // Room for the scrollbar: the text wraps at the view's width, the bar sits over its edge.
                        .pr(tokens.px("space-6"))
                        .py(tokens.px("space-2")),
                    ),
                )
                .child(
                    h_flex()
                        .px(tokens.px("space-5"))
                        .py(tokens.px("space-2"))
                        .border_t_1()
                        .border_color(cx.theme().border)
                        .child(
                            Button::new("help-serval")
                                .ghost()
                                .small()
                                .label(if self.help.serval_open {
                                    t("隐藏命令行参数", "Hide command-line options")
                                } else {
                                    t("显示命令行参数", "Show command-line options")
                                })
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.toggle_serval_help(cx)),
                                ),
                        ),
                ),
        )
    }
}
