//! Two questions about a run (Design/spec/run.md §1): cancelling a paused run, with the choice to delete the
//! output folder it created, and closing the window while a command runs.

use crate::app::Waxbill;
use crate::i18n::t;
use crate::run::Outcome;
use crate::theme::TokensExt;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::{ActiveTheme, Disableable as _, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::{Path, PathBuf};

/// The cancel question while it is open.
#[derive(Default)]
pub struct CancelAsk {
    pub delete: bool,
    pub deleting: bool,
    pub error: Option<String>,
}

/// The folder a run will create: the highest missing ancestor of its `-o` folder, so cancelling can remove exactly
/// what the run made. Only for commands that copy into it (extract, align without moving): deleting anything else
/// could lose originals.
pub fn created_output(args: &[String]) -> Option<PathBuf> {
    let copies = match args.first().map(String::as_str) {
        Some("extract") => true,
        Some("align") => !args.iter().any(|a| a == "--move-mode"),
        _ => false,
    };
    let dir = args
        .iter()
        .position(|a| a == "-o")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)?;
    if !copies || dir.exists() {
        return None;
    }
    let mut top: &Path = &dir;
    while let Some(parent) = top
        .parent()
        .filter(|p| !p.as_os_str().is_empty() && !p.exists())
    {
        top = parent;
    }
    Some(top.to_path_buf())
}

fn dialog(title: &'static str, body: &'static str, cx: &App) -> Div {
    let tokens = cx.tokens();
    v_flex()
        .w(px(480.))
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
}

fn overlay(dialog: Div, cx: &App) -> Div {
    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(cx.tokens().color("overlay"))
        .occlude()
        .child(dialog)
}

impl Waxbill {
    /// 取消 on a paused run: back to Ready at once, or ask first when there is a folder that could be deleted.
    pub(crate) fn cancel(&mut self, cx: &mut Context<Self>) {
        let Some(run) = self.runs.get(&self.screen) else {
            return;
        };
        if run.outcome != Outcome::Stopped {
            return;
        }
        if run.created_output.as_ref().is_some_and(|dir| dir.exists()) {
            self.cancel_ask = Some(CancelAsk::default());
        } else {
            self.runs.remove(&self.screen);
        }
        cx.notify();
    }

    fn confirm_cancel(&mut self, cx: &mut Context<Self>) {
        let screen = self.screen;
        let folder = self
            .runs
            .get(&screen)
            .and_then(|run| run.created_output.clone());
        let Some(ask) = &mut self.cancel_ask else {
            return;
        };
        let Some(folder) = folder.filter(|_| ask.delete) else {
            self.cancel_ask = None;
            self.runs.remove(&screen);
            cx.notify();
            return;
        };
        ask.deleting = true;
        ask.error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let removed = cx
                .background_executor()
                .spawn(async move { std::fs::remove_dir_all(&folder) })
                .await;
            let _ = this.update(cx, |this, cx| {
                match removed {
                    Ok(()) => {
                        this.cancel_ask = None;
                        this.runs.remove(&screen);
                    }
                    Err(err) => {
                        if let Some(ask) = &mut this.cancel_ask {
                            ask.deleting = false;
                            ask.error =
                                Some(format!("{} {err}", t("删除失败：", "Could not delete:")));
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn render_cancel(&self, cx: &mut Context<Self>) -> Option<Div> {
        let ask = self.cancel_ask.as_ref()?;
        let folder = self.runs.get(&self.screen)?.created_output.clone()?;
        let tokens = cx.tokens().clone();
        let body = dialog(
            t("取消这次运行？", "Cancel this run?"),
            t(
                "已写入的文件都是完整的，默认保留。取消后不能再继续这次运行。",
                "Files written so far are complete and are kept unless you choose otherwise. A cancelled run cannot be continued.",
            ),
            cx,
        )
        .child(
            v_flex()
                .gap(tokens.px("space-1"))
                .child(
                    Checkbox::new("cancel-delete")
                        .label(t(
                            "同时删除这次运行新建的文件夹",
                            "Also delete the folder this run created",
                        ))
                        .checked(ask.delete)
                        .on_click(cx.listener(|this, on: &bool, _, cx| {
                            if let Some(ask) = &mut this.cancel_ask {
                                ask.delete = *on;
                            }
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .pl(px(24.))
                        .font_family(cx.theme().mono_font_family.clone())
                        .text_size(tokens.px("text-xs"))
                        .text_color(cx.theme().muted_foreground)
                        .child(folder.to_string_lossy().to_string()),
                ),
        )
        .when_some(ask.error.clone(), |d, err| {
            d.child(
                div()
                    .text_size(tokens.px("text-sm"))
                    .text_color(tokens.color("danger"))
                    .child(err),
            )
        })
        .child(
            h_flex()
                .justify_end()
                .gap(tokens.px("space-2"))
                .child(
                    Button::new("cancel-back")
                        .outline()
                        .label(t("返回", "Back"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.cancel_ask = None;
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("cancel-confirm")
                        .primary()
                        .disabled(ask.deleting)
                        .label(if ask.deleting {
                            t("正在删除…", "Deleting…")
                        } else {
                            t("取消运行", "Cancel run")
                        })
                        .on_click(cx.listener(|this, _, _, cx| this.confirm_cancel(cx))),
                ),
        );
        Some(overlay(body, cx))
    }

    /// The window is being closed: go ahead unless a command runs, else ask.
    pub(crate) fn should_close(&mut self, cx: &mut Context<Self>) -> bool {
        if self.running_screen().is_none() {
            return true;
        }
        self.close_asked = true;
        cx.notify();
        false
    }

    pub(crate) fn render_close(&self, cx: &mut Context<Self>) -> Option<Div> {
        if !self.close_asked || self.running_screen().is_none() {
            return None;
        }
        let tokens = cx.tokens().clone();
        let stopping = self.quit_when_stopped;
        let body = dialog(
            t("命令还在运行", "A command is still running"),
            t(
                "停止后，已写入的文件都是完整的，下次运行同样的命令会跳过已完成的部分。",
                "After stopping, files written so far are complete; running the same command again skips finished work.",
            ),
            cx,
        )
        .child(
            h_flex()
                .justify_end()
                .gap(tokens.px("space-2"))
                .child(
                    Button::new("close-keep")
                        .outline()
                        .disabled(stopping)
                        .label(t("继续运行", "Keep running"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.close_asked = false;
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("close-stop")
                        .primary()
                        .disabled(stopping)
                        .label(if stopping {
                            t("正在停止…", "Stopping…")
                        } else {
                            t("停止并退出", "Stop and quit")
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.quit_when_stopped = true;
                            this.stop(cx);
                        })),
                ),
        );
        Some(overlay(body, cx))
    }
}

#[cfg(test)]
mod tests {
    use super::created_output;

    #[test]
    fn only_a_folder_a_copying_run_creates_is_offered_for_deletion() {
        let dir = std::env::temp_dir().join(format!("waxbill-created-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("data_serval_output/serval_extract/20261012_Red fox");
        let args = |command: &str, extra: &[&str]| -> Vec<String> {
            [command, "-o", out.to_str().unwrap()]
                .iter()
                .chain(extra)
                .map(|s| s.to_string())
                .collect()
        };
        // The highest folder that does not exist yet, so nothing that was there before is removed.
        assert_eq!(
            created_output(&args("extract", &[])),
            Some(dir.join("data_serval_output"))
        );
        // Moving puts the originals there; commands that write in place have nothing to offer.
        assert_eq!(created_output(&args("align", &["--move-mode"])), None);
        assert_eq!(created_output(&args("observe", &[])), None);
        std::fs::create_dir_all(&out).unwrap();
        assert_eq!(created_output(&args("extract", &[])), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
