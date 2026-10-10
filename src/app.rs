//! The main window (Design/spec/shell.md): title bar with the Serval pill, the sidebar, and the current screen with
//! its run bar.

use crate::i18n::Language;
use crate::i18n::t;
use crate::results;
use crate::run::{Outcome, Run};
use crate::run_bar::{Bar, Tone, run_bar};
use crate::runner::Running;
use crate::screens::capture::CaptureForm;
use crate::screens::extract::ExtractForm;
use crate::screens::form::FolderCount;
use crate::screens::observe::ObserveForm;
use crate::screens::simple::{self, SimpleForm};
use crate::screens::translate::TranslateForm;
use crate::screens::update::UpdateForm;
use crate::screens::{Form, ScreenId, groups};
use crate::serval::{self, ServalStatus};
use crate::settings::{Appearance, Settings};
use crate::theme;
use crate::theme::TokensExt;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::theme::ThemeMode;
use gpui_kit::component::{ActiveTheme, Icon, StyledExt, TitleBar, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

actions!(waxbill, [RunCommand, StopCommand, OpenSettings]);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("secondary-enter", RunCommand, None),
        KeyBinding::new("secondary-.", StopCommand, None),
        KeyBinding::new("secondary-,", OpenSettings, None),
    ]);
}

const APP_ICON: &[u8] = include_bytes!("../assets/icons/io.github.wsyxbcl.waxbill.png");

/// Named states from the design boards (`data-state`), opened with `--design-state <name>` so screenshots can be
/// compared with the boards.
pub const DESIGN_STATES: &[&str] = &[
    "observe-ready",
    "dark-observe-ready",
    "first-run",
    "settings-ready",
    "settings-unsupported",
    "capture-ready",
    "extract-from-data",
    "extract-keep-dirs",
    "extract-conflicts",
    "observe-id-species-after-scan",
];

/// What the command line asks for at launch.
pub struct Launch {
    /// A Serval program for this launch only (not remembered).
    pub serval: Option<PathBuf>,
    /// Light or dark was given on the command line, so the setting does not apply.
    pub appearance_given: bool,
    /// A folder to open in 标签获取, or a table to open in 独立捕获.
    pub open: Option<PathBuf>,
    /// Start the run as soon as Serval is found.
    pub run: bool,
}

pub struct Waxbill {
    pub(crate) screen: ScreenId,
    pub(crate) observe: Entity<ObserveForm>,
    capture: Entity<CaptureForm>,
    extract: Entity<ExtractForm>,
    /// Screens that are only paths, checkboxes and choices.
    simple: HashMap<ScreenId, Entity<SimpleForm>>,
    translate: Entity<TranslateForm>,
    browse: Entity<crate::browse::Browse>,
    update: Entity<UpdateForm>,
    /// The choice in the conflicts dialog: skip, replace or rename (questions.md §3).
    pub(crate) conflict_choice: usize,
    /// The identified-species question while it is open: the checked species (questions.md §4).
    pub(crate) id_species: Option<std::collections::BTreeSet<String>>,
    pub(crate) serval: ServalStatus,
    /// The last run of each screen; at most one is running (shell.md §2).
    pub(crate) runs: HashMap<ScreenId, Run>,
    focus: FocusHandle,
    icon: Arc<Image>,
}

impl Waxbill {
    pub fn new(
        design_state: Option<&str>,
        launch: Launch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let observe = cx.new(|cx| ObserveForm::new(window, cx));
        let capture = cx.new(|cx| CaptureForm::new(window, cx));
        let extract = cx.new(|cx| ExtractForm::new(window, cx));
        let translate = cx.new(|cx| TranslateForm::new(window, cx));
        let browse = cx.new(|cx| crate::browse::Browse::new(window, cx));
        cx.subscribe_in(
            &browse,
            window,
            |this: &mut Self, _, event: &crate::browse::BrowseEvent, window, cx| match event {
                crate::browse::BrowseEvent::ApplyTable(path) => {
                    let path = path.to_string_lossy().into_owned();
                    this.update
                        .update(cx, |form, cx| form.apply_table(path, window, cx));
                    this.go(ScreenId::XmpUpdate, cx);
                }
                crate::browse::BrowseEvent::Extract {
                    csv,
                    species,
                    rating,
                } => {
                    let (csv, species, rating) =
                        (csv.to_string_lossy().into_owned(), species.clone(), *rating);
                    this.extract.update(cx, |form, cx| {
                        form.set_selection(csv, species, rating, window, cx)
                    });
                    this.go(ScreenId::Extract, cx);
                }
            },
        )
        .detach();
        let update = cx.new(|cx| UpdateForm::new(window, cx));
        let simple: HashMap<ScreenId, Entity<SimpleForm>> = [
            (ScreenId::XmpInit, &simple::XMP_INIT),
            (ScreenId::Rename, &simple::RENAME),
            (ScreenId::Align, &simple::ALIGN),
            (ScreenId::XmpCopy, &simple::XMP_COPY),
            (ScreenId::XmpRemove, &simple::XMP_REMOVE),
            (ScreenId::XmpSync, &simple::XMP_SYNC),
        ]
        .into_iter()
        .map(|(id, spec)| (id, cx.new(|cx| SimpleForm::new(spec, window, cx))))
        .collect();
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let mut app = Waxbill {
            screen: ScreenId::Observe,
            observe,
            capture,
            extract,
            simple,
            translate,
            browse,
            update,
            conflict_choice: 0,
            id_species: None,
            serval: ServalStatus::Checking,
            runs: HashMap::new(),
            focus,
            icon: Arc::new(Image::from_bytes(ImageFormat::Png, APP_ICON.to_vec())),
        };
        if design_state.is_none() && !launch.appearance_given {
            app.apply_appearance(window, cx);
            cx.observe_window_appearance(window, |this, window, cx| {
                this.apply_appearance(window, cx)
            })
            .detach();
        }
        let example_serval = |version: &str, program: &str| ServalStatus::Ready {
            program: PathBuf::from(program),
            version: version.into(),
        };
        match design_state {
            Some("first-run") => app.serval = ServalStatus::Missing,
            Some(state @ ("extract-from-data" | "extract-keep-dirs" | "extract-conflicts")) => {
                app.serval = example_serval("0.8.0", "serval");
                app.screen = ScreenId::Extract;
                app.extract
                    .update(cx, |form, cx| form.set_example(window, cx));
                if state == "extract-conflicts" {
                    app.runs.insert(ScreenId::Extract, Run::example_conflicts());
                }
            }
            Some("observe-id-species-after-scan") => {
                app.serval = example_serval("0.8.0", "serval");
                app.observe.update(cx, |form, cx| {
                    form.set_example(
                        "/data/2024_spring",
                        FolderCount {
                            images: 12_480,
                            videos: 1_302,
                            sidecars: 0,
                        },
                        window,
                        cx,
                    )
                });
                app.runs
                    .insert(ScreenId::Observe, Run::example_unresolved());
                app.ask_id_species(cx);
            }
            Some("capture-ready") => {
                app.serval = example_serval("0.8.0", "serval");
                app.screen = ScreenId::Capture;
                app.capture
                    .update(cx, |form, cx| form.set_example(window, cx));
            }
            Some("settings-ready") => {
                app.serval = example_serval("0.8.0", "C:\\Program Files\\Serval\\serval.exe");
                app.screen = ScreenId::Settings;
            }
            Some("settings-unsupported") => {
                app.serval = ServalStatus::Unsupported {
                    program: PathBuf::from("/usr/local/bin/serval"),
                    version: "0.6.7".into(),
                };
                app.screen = ScreenId::Settings;
            }
            Some("observe-ready" | "dark-observe-ready") => {
                app.serval = example_serval("0.8.0", "serval");
                app.observe.update(cx, |form, cx| {
                    form.set_example(
                        "/data/2024_spring",
                        FolderCount {
                            images: 12_480,
                            videos: 1_302,
                            sidecars: 0,
                        },
                        window,
                        cx,
                    )
                });
            }
            _ => {
                app.observe.update(cx, |form, cx| form.restore(window, cx));
                app.capture.update(cx, |form, cx| form.restore(window, cx));
                app.extract.update(cx, |form, cx| form.restore(window, cx));
                app.translate
                    .update(cx, |form, cx| form.restore(window, cx));
                app.update.update(cx, |form, cx| form.restore(window, cx));
                app.browse
                    .update(cx, |browse, cx| browse.restore(window, cx));
                for form in app.simple.values() {
                    form.update(cx, |form, cx| form.restore(window, cx));
                }
                if let Some(screen) = cx
                    .default_global::<Settings>()
                    .value("screen")
                    .and_then(ScreenId::from_key)
                {
                    app.screen = screen;
                }
                if let Some(path) = &launch.open {
                    let text = path.to_string_lossy().to_string();
                    if path
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("csv"))
                    {
                        app.screen = ScreenId::Capture;
                        app.capture
                            .update(cx, |form, cx| form.set_csv(text, window, cx));
                    } else {
                        app.observe
                            .update(cx, |form, cx| form.set_input(text, window, cx));
                    }
                }
                let program = launch
                    .serval
                    .or_else(|| cx.default_global::<Settings>().serval.clone());
                app.detect_serval(program, launch.run, cx);
            }
        }
        app
    }

    fn detect_serval(&mut self, program: Option<PathBuf>, run: bool, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let status = cx
                .background_executor()
                .spawn(async move {
                    match program {
                        Some(program) => serval::check(program),
                        None => serval::detect(),
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                cx.set_global(crate::serval::ServalProgram(
                    status.program().map(Into::into),
                ));
                this.serval = status;
                // The write preview needs Serval.
                this.update.update(cx, |form, cx| form.recheck(cx));
                cx.notify();
            });
            if run {
                // Wait for the folder count, which the plan needs.
                cx.background_executor()
                    .timer(Duration::from_millis(1500))
                    .await;
                let _ = this.update(cx, |this, cx| this.run_or_stop(cx));
            }
        })
        .detach();
    }

    /// Light, dark, or the system's appearance, from Settings.
    fn apply_appearance(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mode = match cx.default_global::<Settings>().appearance {
            Appearance::Light => ThemeMode::Light,
            Appearance::Dark => ThemeMode::Dark,
            Appearance::System => ThemeMode::from(window.appearance()),
        };
        theme::set_mode(mode, cx);
    }

    pub(crate) fn set_appearance(
        &mut self,
        appearance: Appearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        Settings::update(cx, |settings| settings.appearance = appearance);
        self.apply_appearance(window, cx);
    }

    pub(crate) fn set_language(
        &mut self,
        language: Language,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        crate::i18n::set_language(language);
        Settings::update(cx, |settings| settings.language = language);
        window.refresh();
        cx.notify();
    }

    /// Choose the Serval program; it is checked and remembered (settings.md §1).
    pub(crate) fn choose_serval(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let Some(program) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.serval = ServalStatus::Checking;
                Settings::update(cx, |settings| settings.serval = Some(program.clone()));
                this.detect_serval(Some(program), false, cx);
            });
        })
        .detach();
    }

    /// Show a screen; it is also where the next launch opens.
    fn go(&mut self, screen: ScreenId, cx: &mut Context<Self>) {
        self.screen = screen;
        Settings::update(cx, |settings| {
            settings.set_value("screen", format!("{screen:?}"))
        });
        cx.notify();
    }

    fn running_screen(&self) -> Option<ScreenId> {
        self.runs
            .iter()
            .find(|(_, run)| run.is_running())
            .map(|(id, _)| *id)
    }

    /// Run bar button and Ctrl+Enter: run the current screen's command, or stop it while it runs.
    fn run_or_stop(&mut self, cx: &mut Context<Self>) {
        if self.running_screen() == Some(self.screen) {
            self.stop(cx);
            return;
        }
        self.start_run(&[], cx);
    }

    /// Run again after a decision (the conflicts dialog).
    pub(crate) fn start_decided_run(&mut self, extra: &[&str], cx: &mut Context<Self>) {
        self.runs.remove(&self.screen);
        self.start_run(extra, cx);
    }

    /// Start the current screen's command, with `extra` arguments (a decision such as `--on-existing skip`).
    fn start_run(&mut self, extra: &[&str], cx: &mut Context<Self>) {
        if self.running_screen().is_some() {
            return;
        }
        let (Some(form), Some(program)) = (self.form(), self.serval.program().map(PathBuf::from))
        else {
            return;
        };
        let plan = form.plan(cx);
        if plan.ready.is_err() {
            return;
        }
        form.remember(cx);
        let mut args = plan.args.clone();
        // Decisions go before the positional arguments at the end (`-o <dir> <input>`).
        let at = args
            .iter()
            .position(|a| a == "--progress")
            .unwrap_or(args.len());
        for (i, arg) in extra.iter().enumerate() {
            args.insert(at + i, arg.to_string());
        }
        let command = crate::screens::display_command(&args);
        let run = match Running::start(&program, &args, &plan.cwd) {
            Ok(process) => Run::new(command, process),
            Err(err) => Run::failed_to_start(command, format!("{}: {err}", program.display())),
        };
        let screen = self.screen;
        self.runs.insert(screen, run);
        cx.notify();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let running = this
                    .update(cx, |this, cx| {
                        let Some(run) = this.runs.get_mut(&screen) else {
                            return false;
                        };
                        // Redraw at least once a second for the elapsed and remaining time.
                        run.poll();
                        let running = run.is_running();
                        if !running && screen == ScreenId::Observe && run.unresolved.is_some() {
                            this.ask_id_species(cx);
                        }
                        cx.notify();
                        running
                    })
                    .unwrap_or(false);
                if !running {
                    break;
                }
            }
        })
        .detach();
    }

    fn stop(&mut self, cx: &mut Context<Self>) {
        if let Some(screen) = self.running_screen()
            && let Some(run) = self.runs.get_mut(&screen)
        {
            run.stop();
            cx.notify();
        }
    }

    /// The run bar for the current screen: from its run if there is one, else from the form's plan.
    fn bar(&self, form: &Form, cx: &App) -> Bar {
        let plan = form.plan(cx);
        let command = crate::screens::display_command(&plan.args);
        let serval_ready = matches!(self.serval, ServalStatus::Ready { .. });
        let other_running = self.running_screen().is_some_and(|s| s != self.screen);
        let idle = |title: &str, detail: String, enabled: bool| Bar {
            tone: Tone::Idle,
            title: title.to_string(),
            detail,
            progress: None,
            command: command.clone(),
            button: t("运行", "Run"),
            shortcut: "secondary-enter",
            enabled,
        };
        let ready = |button: &'static str| match (&plan.ready, serval_ready, other_running) {
            (_, false, _) if self.serval == ServalStatus::Missing => (
                false,
                t("选择程序后即可运行", "choose the program to run").to_string(),
            ),
            (_, false, _) => (
                false,
                t(
                    "Serval 版本过旧，请在设置中更换",
                    "Serval is too old; change it in Settings",
                )
                .to_string(),
            ),
            (_, _, true) => (
                false,
                t("另一个命令正在运行", "another command is running").to_string(),
            ),
            (Err(missing), _, _) => (false, missing.clone()),
            (Ok(_), _, _) => (true, button.to_string()),
        };
        let Some(run) = self.runs.get(&self.screen) else {
            return match ready("") {
                (true, _) => idle(
                    t("就绪", "Ready"),
                    plan.ready.clone().unwrap_or_default(),
                    true,
                ),
                (false, why) if !serval_ready => {
                    idle(t("需要 Serval", "Serval needed"), why, false)
                }
                (false, why) => idle(t("还不能运行", "Not ready"), why, false),
            };
        };
        let warnings = run.warnings.len() + run.errors.iter().filter(|(_, p)| p.is_some()).count();
        let (tone, title, detail, button) = match run.outcome {
            Outcome::Running => {
                return Bar {
                    tone: Tone::Running,
                    title: t("运行中", "Running").to_string(),
                    detail: run.progress_line(),
                    progress: Some(run.fraction()),
                    command: run.command.clone(),
                    button: t("停止", "Stop"),
                    shortcut: "secondary-.",
                    enabled: true,
                };
            }
            Outcome::Finished if warnings == 0 => (
                Tone::Success,
                t("完成", "Finished").to_string(),
                run.result_sentence(),
                t("再次运行", "Run again"),
            ),
            Outcome::Finished => (
                Tone::Warning,
                match crate::i18n::language() {
                    crate::i18n::Language::Chinese => format!("完成，有 {warnings} 条警告"),
                    crate::i18n::Language::English => format!("Finished with {warnings} warnings"),
                },
                run.result_sentence(),
                t("再次运行", "Run again"),
            ),
            Outcome::NeedsDecision => (
                Tone::Warning,
                t("需要你的选择", "Needs your decision").to_string(),
                t("还没有写入任何文件", "nothing has been written yet").to_string(),
                t("运行", "Run"),
            ),
            Outcome::Failed => (
                Tone::Danger,
                t("失败", "Failed").to_string(),
                run.failure(),
                t("重新运行", "Run again"),
            ),
            Outcome::Stopped => (
                Tone::Idle,
                t("已停止", "Stopped").to_string(),
                run.progress_line(),
                t("继续运行", "Continue"),
            ),
        };
        let (enabled, _) = ready(button);
        Bar {
            tone,
            title,
            detail,
            progress: None,
            command: if enabled {
                command
            } else {
                run.command.clone()
            },
            button,
            shortcut: "secondary-enter",
            enabled,
        }
    }

    fn form(&self) -> Option<Form> {
        match self.screen {
            ScreenId::Observe => Some(Form::Observe(self.observe.clone())),
            ScreenId::Capture => Some(Form::Capture(self.capture.clone())),
            ScreenId::Extract => Some(Form::Extract(self.extract.clone())),
            ScreenId::Translate => Some(Form::Translate(self.translate.clone())),
            ScreenId::XmpUpdate => Some(Form::Update(self.update.clone())),
            id if self.simple.contains_key(&id) => Some(Form::Simple(self.simple[&id].clone())),
            _ => None,
        }
    }

    fn render_title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.tokens();
        let (dot, label) = match &self.serval {
            ServalStatus::Checking => (
                tokens.color("subtle-foreground"),
                t("正在查找 Serval…", "Looking for Serval…").to_string(),
            ),
            ServalStatus::Ready { version, .. } => {
                (tokens.color("success"), format!("Serval {version}"))
            }
            ServalStatus::Unsupported { version, .. } => (
                tokens.color("danger"),
                format!("Serval {version} · {}", t("版本过旧", "too old")),
            ),
            ServalStatus::Missing => (
                tokens.color("danger"),
                t("未找到 Serval", "Serval not found").to_string(),
            ),
        };
        TitleBar::new().h(tokens.px("titlebar-h")).child(
            h_flex()
                .w_full()
                .pr(tokens.px("space-2"))
                .justify_between()
                .child(
                    h_flex()
                        .gap(tokens.px("space-2"))
                        .child(img(self.icon.clone()).size(px(20.)))
                        .child(
                            div()
                                .text_size(tokens.px("text-md"))
                                .font_medium()
                                .child("Waxbill"),
                        ),
                )
                .child(
                    h_flex()
                        .gap(tokens.px("space-3"))
                        .child(
                            h_flex()
                                .id("serval-pill")
                                .gap(tokens.px("space-2"))
                                .px(tokens.px("space-3"))
                                .py(px(3.))
                                .rounded_full()
                                .bg(cx.theme().muted)
                                .text_size(tokens.px("text-sm"))
                                .cursor_pointer()
                                .child(div().size(px(7.)).rounded_full().bg(dot))
                                .child(label)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.go(ScreenId::Settings, cx);
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("settings")
                                .ghost()
                                .icon(Icon::new(IconName::Settings))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.go(ScreenId::Settings, cx);
                                    cx.notify();
                                })),
                        ),
                ),
        )
    }

    fn sidebar_item(&self, id: ScreenId, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.tokens();
        let active = self.screen == id;
        h_flex()
            .id(SharedString::from(format!("nav-{id:?}")))
            .relative()
            .h(px(34.))
            .px(tokens.px("space-3"))
            .gap(tokens.px("space-3"))
            .rounded(tokens.px("radius-sm"))
            .text_size(tokens.px("text-md"))
            .cursor_pointer()
            .when(active, |item| {
                item.bg(cx.theme().sidebar_accent)
                    .text_color(cx.theme().foreground)
                    .child(
                        div()
                            .absolute()
                            .left(px(-8.))
                            .top(px(8.))
                            .w(px(3.))
                            .h(px(18.))
                            .rounded_full()
                            .bg(tokens.color("accent")),
                    )
            })
            .when(!active, |item| {
                item.hover(|item| item.bg(cx.theme().sidebar_accent.opacity(0.6)))
            })
            .child(
                Icon::new(id.icon())
                    .size(px(15.))
                    .text_color(cx.theme().muted_foreground),
            )
            .child(id.label())
            .on_click(cx.listener(move |this, _, _, cx| {
                this.go(id, cx);
                cx.notify();
            }))
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.tokens().clone();
        let mut list = v_flex().gap(px(2.));
        for (title, items) in groups() {
            list = list.child(
                div()
                    .px(tokens.px("space-3"))
                    .pt(tokens.px("space-4"))
                    .pb(tokens.px("space-1"))
                    .text_size(tokens.px("text-sm"))
                    .text_color(cx.theme().muted_foreground)
                    .child(title),
            );
            for id in items {
                list = list.child(self.sidebar_item(*id, cx));
            }
        }
        v_flex()
            .w(tokens.px("sidebar-w"))
            .flex_shrink_0()
            .h_full()
            .px(tokens.px("space-2"))
            .pb(tokens.px("space-3"))
            .justify_between()
            .bg(cx.theme().sidebar)
            .text_color(cx.theme().sidebar_foreground)
            .border_r_1()
            .border_color(cx.theme().border)
            .child(list)
            .child(self.sidebar_item(ScreenId::Settings, cx))
    }

    fn render_results(&self, cx: &mut Context<Self>) -> Option<Div> {
        let run = self.runs.get(&self.screen)?;
        if run.is_running() {
            return None;
        }
        let tokens = cx.tokens().clone();
        let screen = self.screen;
        Some(
            v_flex()
                .gap(tokens.px("space-3"))
                .children(results::banner(run, cx))
                .children(self.render_unresolved_banner(cx))
                .children(results::warning_list(run, cx))
                .children(results::output_list(run, cx))
                .child(results::log(
                    run,
                    cx.listener(move |this, _, _, cx| {
                        if let Some(run) = this.runs.get_mut(&screen) {
                            run.log_open = !run.log_open;
                            cx.notify();
                        }
                    }),
                    cx,
                )),
        )
    }

    fn render_heading(&self, summary: &'static str, help: bool, cx: &App) -> Div {
        let tokens = cx.tokens();
        v_flex()
            .gap(tokens.px("space-1"))
            .child(
                div()
                    .text_size(tokens.px("text-xl"))
                    .font_semibold()
                    .child(self.screen.label()),
            )
            .child(
                h_flex()
                    .gap(tokens.px("space-1"))
                    .text_size(tokens.px("text-sm"))
                    .text_color(cx.theme().muted_foreground)
                    .child(summary)
                    .when(help, |line| {
                        line.child(
                            div()
                                .text_color(tokens.color("accent"))
                                .child(t("帮助", "Help")),
                        )
                    }),
            )
    }

    fn render_screen(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = cx.tokens().clone();
        let form = self.form();
        let content = if self.screen == ScreenId::Browse {
            // Browse hands the looking to other tools: no run bar of its own (browse.md).
            v_flex()
                .gap(tokens.px("space-5"))
                .child(self.render_heading(
                    t(
                        "打开标签表，按物种和评分选出要看的记录，再在 Caracal、资源管理器中查看，或把改好的表写入文件。",
                        "Open a tags table, choose records by species and rating, then look at them in Caracal or the file manager, or write an edited table into the files.",
                    ),
                    false,
                    cx,
                ))
                .child(self.browse.clone())
        } else if self.screen == ScreenId::Settings {
            v_flex()
                .gap(tokens.px("space-5"))
                .child(self.render_heading(
                    t(
                        "修改会立即生效并保存。",
                        "Changes apply at once and are saved.",
                    ),
                    false,
                    cx,
                ))
                .child(self.render_settings(cx))
        } else if self.serval == ServalStatus::Missing && form.is_some() {
            self.render_first_run(cx)
        } else {
            let summary = form.as_ref().map(|form| form.summary(cx)).unwrap_or_else(|| {
                t(
                    "这个界面正在按新设计重建，暂时请在命令行使用。",
                    "This screen is being rebuilt to the new design; use the command line for now.",
                )
            });
            v_flex()
                .gap(tokens.px("space-5"))
                .child(self.render_heading(summary, form.is_some(), cx))
                .children(self.render_results(cx))
                .children(form.as_ref().map(Form::view))
        };
        let body = div()
            .id("screen")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .child(
                div()
                    .max_w(tokens.px("content-max-w"))
                    .px(tokens.px("space-6"))
                    .py(tokens.px("space-5"))
                    .child(content),
            );
        let bar = form.as_ref().map(|form| self.bar(form, cx));
        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(body)
            .children(
                bar.map(|bar| run_bar(bar, cx.listener(|this, _, _, cx| this.run_or_stop(cx)), cx)),
            )
            .into_any_element()
    }
}

impl Render for Waxbill {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dialog = self
            .render_conflicts(cx)
            .or_else(|| self.render_id_species(cx));
        v_flex()
            .relative()
            .size_full()
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &RunCommand, _, cx| this.run_or_stop(cx)))
            .on_action(cx.listener(|this, _: &StopCommand, _, cx| this.stop(cx)))
            .on_action(cx.listener(|this, _: &OpenSettings, _, cx| {
                this.go(ScreenId::Settings, cx);
                cx.notify();
            }))
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(self.render_title_bar(cx))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .items_start()
                    .child(self.render_sidebar(cx))
                    .child(self.render_screen(cx)),
            )
            .children(dialog)
    }
}
