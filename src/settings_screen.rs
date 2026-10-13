//! 设置 Settings (Design/spec/settings.md §2, board settings.html) and the first-run card shown on command screens
//! while there is no Serval (board shell.html, first-run).

use crate::app::Waxbill;
use crate::i18n::{Language, language, t};
use crate::screens::form::{Tone, card, help_line, segmented};
use crate::serval::ServalStatus;
use crate::settings::{Appearance, Settings};
use crate::theme::TokensExt;
use gpui_kit::component::button::{Button, ButtonGroup};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::{ActiveTheme, StyledExt, h_flex, v_flex};
use gpui_kit::*;

fn section_title(title: &'static str, cx: &App) -> Div {
    div()
        .text_size(cx.tokens().px("text-sm"))
        .font_medium()
        .text_color(cx.theme().muted_foreground)
        .child(title)
}

impl Waxbill {
    /// The SERVAL card: what is in use and whether it works with Waxbill.
    fn serval_card(&self, cx: &mut Context<Self>) -> Div {
        let tokens = cx.tokens().clone();
        let choose = |label: &'static str, cx: &mut Context<Self>| {
            Button::new("choose-serval")
                .outline()
                .label(label)
                .on_click(cx.listener(|this, _, window, cx| this.choose_serval(window, cx)))
        };
        let program_line = |program: &std::path::Path, note: &'static str, cx: &App| {
            div()
                .font_family(cx.theme().mono_font_family.clone())
                .text_size(tokens.px("text-xs"))
                .text_color(cx.theme().muted_foreground)
                .child(format!("{} · {note}", program.display()))
        };
        let body = match &self.serval {
            ServalStatus::Ready { program, version } => h_flex()
                .justify_between()
                .child(
                    v_flex()
                        .gap(tokens.px("space-1"))
                        .child(
                            h_flex()
                                .gap(tokens.px("space-2"))
                                .child(format!("Serval {version}"))
                                .child(div().size(px(7.)).rounded_full().bg(tokens.color("success"))),
                        )
                        .child(program_line(program, t("兼容", "compatible"), cx)),
                )
                .child(choose(t("更换程序…", "Change program…"), cx)),
            ServalStatus::Unsupported { program, version } => v_flex()
                .gap(tokens.px("space-3"))
                .child(
                    h_flex()
                        .justify_between()
                        .gap(tokens.px("space-4"))
                        .p(tokens.px("space-4"))
                        .rounded(tokens.px("radius-sm"))
                        .bg(tokens.color("danger-soft"))
                        .child(
                            v_flex()
                                .gap(tokens.px("space-1"))
                                .child(div().font_semibold().text_color(tokens.color("danger")).child(
                                    match language() {
                                        Language::Chinese => format!("这个 Serval 版本太旧（{version}）"),
                                        Language::English => format!("This Serval is too old ({version})"),
                                    },
                                ))
                                .child(div().text_size(tokens.px("text-sm")).child(t(
                                    "Waxbill 需要 Serval 0.8 或以上，否则无法传入运行参数。请向项目组获取新版本。",
                                    "Waxbill needs Serval 0.8 or later to pass its settings. Ask the project team for a newer version.",
                                ))),
                        )
                        .child(choose(t("选择其他程序…", "Choose another…"), cx)),
                )
                .child(program_line(program, t("自定义程序", "custom program"), cx)),
            ServalStatus::Missing => self.choose_serval_field(cx),
            ServalStatus::Checking => v_flex().child(help_line(t("正在检查 Serval…", "Checking Serval…"), Tone::Quiet, cx)),
        };
        card(cx).child(section_title("SERVAL", cx)).child(body)
    }

    /// "选择 Serval 程序", for when there is none.
    fn choose_serval_field(&self, cx: &mut Context<Self>) -> Div {
        let tokens = cx.tokens().clone();
        v_flex()
            .gap(tokens.px("space-2"))
            .child(div().font_medium().child(t("选择 Serval 程序", "Choose the Serval program")))
            .child(help_line(
                t(
                    "Serval 程序由项目组提供（serval.exe / serval）。选择后 Waxbill 会检查版本并记住它。",
                    "The project team provides the Serval program (serval.exe / serval). Waxbill checks its version and remembers it.",
                ),
                Tone::Quiet,
                cx,
            ))
            .child(
                h_flex().child(
                    Button::new("choose-serval")
                        .outline()
                        .label(t("选择…", "Choose…"))
                        .on_click(cx.listener(|this, _, window, cx| this.choose_serval(window, cx))),
                ),
            )
            .child(help_line(t("需要 Serval 0.8 或以上。", "Serval 0.8 or later is needed."), Tone::Quiet, cx))
    }

    /// The first-run content of a command screen: there is no Serval yet.
    pub(crate) fn render_first_run(&self, cx: &mut Context<Self>) -> Div {
        let tokens = cx.tokens().clone();
        v_flex()
            .gap(tokens.px("space-5"))
            .child(
                v_flex()
                    .gap(tokens.px("space-1"))
                    .child(div().text_size(tokens.px("text-xl")).font_semibold().child(t("欢迎使用 Waxbill", "Welcome to Waxbill")))
                    .child(help_line(
                        t(
                            "Waxbill 是 Serval 的图形界面。开始之前需要一个 Serval 程序。",
                            "Waxbill is the desktop app for Serval. It needs a Serval program to start.",
                        ),
                        Tone::Quiet,
                        cx,
                    )),
            )
            .child(card(cx).child(section_title("SERVAL", cx)).child(self.choose_serval_field(cx)))
    }

    pub(crate) fn render_settings(&self, cx: &mut Context<Self>) -> Div {
        let tokens = cx.tokens().clone();
        let settings = cx.default_global::<Settings>().clone();
        let row = |label: &'static str, control: ButtonGroup| {
            h_flex()
                .justify_between()
                .py(tokens.px("space-2"))
                .child(label)
                .child(control)
        };
        let interface = card(cx)
            .gap(tokens.px("space-2"))
            .child(section_title(t("界面", "Interface"), cx))
            .child(row(
                t("语言", "Language"),
                segmented(
                    "language",
                    &["中文", "English"],
                    (language() == Language::English) as usize,
                    cx.listener_for_index(|this, index, window, cx| {
                        this.set_language(
                            if index == 1 {
                                Language::English
                            } else {
                                Language::Chinese
                            },
                            window,
                            cx,
                        )
                    }),
                ),
            ))
            .child(div().h(px(1.)).bg(cx.theme().border))
            .child(row(
                t("外观", "Appearance"),
                segmented(
                    "appearance",
                    &[
                        t("跟随系统", "System"),
                        t("浅色", "Light"),
                        t("深色", "Dark"),
                    ],
                    match settings.appearance {
                        Appearance::System => 0,
                        Appearance::Light => 1,
                        Appearance::Dark => 2,
                    },
                    cx.listener_for_index(|this, index, window, cx| {
                        let appearance =
                            [Appearance::System, Appearance::Light, Appearance::Dark][index];
                        this.set_appearance(appearance, window, cx)
                    }),
                ),
            ));
        // The Caracal bridge (caracal-bridge.md): a read-only server on this computer, started on first use.
        let caracal = card(cx)
            .gap(tokens.px("space-2"))
            .child(section_title("CARACAL", cx))
            .child(
                Checkbox::new("caracal-bridge")
                    .label(t(
                        "允许 Caracal 从本机读取所选表格和媒体",
                        "Let Caracal read the chosen table and its media from this computer",
                    ))
                    .checked(crate::caracal::bridge_enabled(cx))
                    .on_click(cx.listener(|_, on: &bool, _, cx| {
                        let value = if *on { "1" } else { "0" };
                        Settings::update(cx, |settings| settings.set_value("caracal_bridge", value));
                        // Switching off also ends the access already given, not only future links.
                        if !*on && let Some(bridge) = cx.try_global::<crate::bridge::Bridge>() {
                            bridge.revoke_all();
                        }
                        cx.notify();
                    })),
            )
            .child(help_line(
                t(
                    "在 Caracal 中打开时，表格和其中列出的照片、视频直接从 Waxbill 读取，不用在浏览器里再选择；只在本机可见，不会上传。浏览器第一次会询问是否允许访问本地网络。",
                    "When opening in Caracal, the table and the photos and videos it lists are read straight from Waxbill, with nothing to pick in the browser; visible only on this computer, nothing is uploaded. The browser asks once to allow local network access.",
                ),
                Tone::Quiet,
                cx,
            ));
        let about = card(cx)
            .gap(tokens.px("space-2"))
            .child(section_title(t("关于", "About"), cx))
            .child(format!("Waxbill {}", env!("CARGO_PKG_VERSION")))
            .child(
                h_flex()
                    .gap(tokens.px("space-1"))
                    .text_size(tokens.px("text-sm"))
                    .text_color(cx.theme().muted_foreground)
                    .child(t("Apache-2.0 许可 ·", "Licensed under Apache-2.0 ·"))
                    .child(
                        div()
                            .id("source-link")
                            .cursor_pointer()
                            .text_color(tokens.color("accent"))
                            .child(t("源代码", "Source code"))
                            .on_click(|_, _, cx| cx.open_url(env!("CARGO_PKG_REPOSITORY"))),
                    ),
            )
            .child(help_line(
                t(
                    "图标设计 @timber3 · © 2026 山水自然保护中心",
                    "Icon by @timber3 · © 2026 Shan Shui Conservation Center",
                ),
                Tone::Quiet,
                cx,
            ));
        v_flex()
            .gap(tokens.px("space-4"))
            .child(self.serval_card(cx))
            .child(interface)
            .child(caracal)
            .child(about)
    }
}

trait ListenerForIndex {
    fn listener_for_index(
        &self,
        f: impl Fn(&mut Waxbill, usize, &mut Window, &mut Context<Waxbill>) + 'static,
    ) -> impl Fn(usize, &mut Window, &mut App) + 'static;
}

impl ListenerForIndex for Context<'_, Waxbill> {
    fn listener_for_index(
        &self,
        f: impl Fn(&mut Waxbill, usize, &mut Window, &mut Context<Waxbill>) + 'static,
    ) -> impl Fn(usize, &mut Window, &mut App) + 'static {
        let this = self.entity().downgrade();
        move |index, window, cx| {
            let _ = this.update(cx, |this, cx| f(this, index, window, cx));
        }
    }
}
