use gpui::*;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

mod commands;
mod capture_helper;
mod help_texts;
mod i18n;
mod text_input;

use capture_helper::{CaptureHelperModel, CaptureHelperPromptKind};
use commands::{CommandKind, CommandState, XmpSubcommand};
use i18n::{t, Language};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use text_input::{bind_text_input_keys, TextInput, TextInputSubmitted};

struct RootView {
    command_state: CommandState,
    language: Language,
    serval_binary_path: Option<String>,
    observe_input: Entity<TextInput>,
    observe_output_input: Entity<TextInput>,
    capture_input: Entity<TextInput>,
    capture_output_input: Entity<TextInput>,
    xmp_source_input: Entity<TextInput>,
    xmp_output_input: Entity<TextInput>,
    xmp_csv_input: Entity<TextInput>,
    xmp_dir_input: Entity<TextInput>,
    extract_csv_input: Entity<TextInput>,
    extract_value_input: Entity<TextInput>,
    extract_output_input: Entity<TextInput>,
    translate_csv_input: Entity<TextInput>,
    translate_taglist_input: Entity<TextInput>,
    translate_from_input: Entity<TextInput>,
    translate_to_input: Entity<TextInput>,
    translate_output_input: Entity<TextInput>,
    pty_input: Entity<TextInput>,
    output_log: String,
    running: bool,
    pty_writer: Option<Arc<Mutex<Box<dyn Write + Send>>>>,
    output_scroll_handle: ScrollHandle,
    capture_helper: CaptureHelperModel,
    helper_mode: bool,
    command_panel_open: bool,
    input_panel_open: bool,
    preview_panel_open: bool,
    help_cache: HashMap<String, String>,
    command_help_open: bool,
    command_help_key: Option<String>,
    hover_help_key: Option<String>,
    option_help_position: Option<Point<Pixels>>,
    cursor_position: Point<Pixels>,
}

struct SetupView {
    root: Entity<RootView>,
    serval_binary_input: Entity<TextInput>,
}

fn apply_browse_result(
    result: Result<Option<Vec<PathBuf>>>,
    input: Entity<TextInput>,
    root: Entity<RootView>,
    language: Language,
    app: &mut App,
) {
    match result {
        Ok(Some(paths)) => {
            if let Some(path) = paths.first() {
                let value = path.to_string_lossy().to_string();
                let _ = input.update(app, |input, cx| {
                    input.set_value(value, cx);
                });
            }
        }
        Ok(None) => {}
        Err(err) => {
            let _ = root.update(app, |view, cx| {
                view.append_output(
                    format!("{}: {err}", t(language, "message.path_dialog_error")),
                    cx,
                );
            });
        }
    }
}

fn spawn_path_prompt(
    cx: &mut App,
    options: PathPromptOptions,
    input: Entity<TextInput>,
    root: Entity<RootView>,
    language: Language,
) {
    let receiver = cx.prompt_for_paths(options);
    cx.spawn(async move |cx| {
        if let Ok(result) = receiver.await {
            let _ = cx.update(|app| {
                apply_browse_result(result, input, root, language, app);
            });
        }
    })
    .detach();
}

fn browse_into_text_input(
    window: &mut Window,
    cx: &mut App,
    options: PathPromptOptions,
    input: Entity<TextInput>,
    root: Entity<RootView>,
    language: Language,
) {
    #[cfg(windows)]
    {
        window.defer(cx, move |_window, cx| {
            spawn_path_prompt(cx, options, input, root, language);
        });
    }

    #[cfg(not(windows))]
    {
        let _ = window;
        spawn_path_prompt(cx, options, input, root, language);
    }
}
impl RootView {
    fn set_language(&mut self, language: Language, cx: &mut Context<Self>) {
        if self.language != language {
            self.language = language;
            self.help_cache.clear();
            self.command_help_key = None;
            self.hover_help_key = None;
            self.option_help_position = None;

            self.observe_input.update(cx, |input, cx| {
                input.set_placeholder(t(language, "placeholder.observe_media_dir"), cx);
            });
            self.observe_output_input.update(cx, |input, cx| {
                input.set_placeholder(t(language, "placeholder.optional_output_dir"), cx);
            });
            self.capture_input.update(cx, |input, cx| {
                input.set_placeholder(t(language, "placeholder.tags_csv"), cx);
            });
            self.capture_output_input.update(cx, |input, cx| {
                input.set_placeholder(t(language, "placeholder.optional_output_dir"), cx);
            });
            self.xmp_source_input.update(cx, |input, cx| {
                input.set_placeholder(t(language, "placeholder.source_dir"), cx);
            });
            self.xmp_output_input.update(cx, |input, cx| {
                input.set_placeholder(t(language, "placeholder.output_dir"), cx);
            });
            self.xmp_csv_input.update(cx, |input, cx| {
                input.set_placeholder(t(language, "placeholder.csv_file"), cx);
            });
            self.xmp_dir_input.update(cx, |input, cx| {
                input.set_placeholder(t(language, "placeholder.directory"), cx);
            });
            self.extract_csv_input.update(cx, |input, cx| {
                input.set_placeholder(t(language, "placeholder.tags_csv"), cx);
            });
            self.extract_value_input.update(cx, |input, cx| {
                input.set_placeholder(t(language, "placeholder.filter_value"), cx);
            });
            self.extract_output_input.update(cx, |input, cx| {
                input.set_placeholder(t(language, "placeholder.optional_output_dir"), cx);
            });
            self.translate_csv_input.update(cx, |input, cx| {
                input.set_placeholder(t(language, "placeholder.tags_csv"), cx);
            });
            self.translate_taglist_input.update(cx, |input, cx| {
                input.set_placeholder(t(language, "placeholder.taglist_csv"), cx);
            });
            self.translate_from_input.update(cx, |input, cx| {
                input.set_placeholder(t(language, "placeholder.translate_from"), cx);
            });
            self.translate_to_input.update(cx, |input, cx| {
                input.set_placeholder(t(language, "placeholder.translate_to"), cx);
            });
            self.translate_output_input.update(cx, |input, cx| {
                input.set_placeholder(t(language, "placeholder.optional_output_dir"), cx);
            });
            self.pty_input.update(cx, |input, cx| {
                input.set_placeholder(t(language, "placeholder.interactive_input"), cx);
            });

            if self.command_help_open {
                let key = self.current_help_key();
                self.command_help_key = Some(key.clone());
                self.ensure_help_loaded_for_key(key, cx);
                return;
            }
            cx.notify();
        }
    }

    fn executable_program(&self) -> String {
        self.serval_binary_path
            .as_ref()
            .filter(|s| !s.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| "serval".to_string())
    }

    fn command_preview(&self) -> String {
        let base = self.command_state.preview();
        let program = self.executable_program();
        if program == "serval" {
            base
        } else {
            base.replacen("serval", &program, 1)
        }
    }

    fn set_serval_binary_path(&mut self, path: Option<String>, cx: &mut Context<Self>) {
        self.serval_binary_path = path.and_then(|p| {
            let trimmed = p.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        });
        self.help_cache.clear();
        self.hover_help_key = None;
        self.option_help_position = None;
        if self.command_help_open {
            let key = self.current_help_key();
            self.command_help_key = Some(key.clone());
            self.ensure_help_loaded_for_key(key, cx);
        } else {
            cx.notify();
        }
    }

    fn select_command(&mut self, kind: CommandKind, cx: &mut Context<Self>) {
        if self.command_state.kind != kind {
            self.command_state.kind = kind;
            self.capture_helper.reset();
            self.hover_help_key = None;
            self.option_help_position = None;
            if self.command_help_open {
                let key = self.current_help_key();
                self.command_help_key = Some(key.clone());
                self.ensure_help_loaded_for_key(key, cx);
            } else {
                cx.notify();
            }
        }
    }

    fn append_output(&mut self, line: impl AsRef<str>, cx: &mut Context<Self>) {
        let mut line = line.as_ref().to_string();
        if line.contains('\t') {
            line = line.replace('\t', "    ");
        }
        if line.contains('\r') {
            line = line.replace('\r', "");
        }
        if line.contains('\u{001b}') {
            line = strip_ansi_escapes(&line);
        }
        if !self.output_log.is_empty()
            && !self.output_log.ends_with('\n')
            && !line.starts_with('\n')
        {
            self.output_log.push('\n');
        }
        self.output_log.push_str(&line);
        if !self.output_log.ends_with('\n') {
            self.output_log.push('\n');
        }
        self.output_scroll_handle.scroll_to_bottom();
        cx.notify();
    }

    fn append_output_chunk(&mut self, chunk: impl AsRef<str>, cx: &mut Context<Self>) {
        let chunk = sanitize_terminal_output(chunk.as_ref());
        if chunk.is_empty() {
            return;
        }

        self.output_log.push_str(&chunk);
        self.output_scroll_handle.scroll_to_bottom();
        cx.notify();
    }

    fn reset_capture_helper(&mut self, cx: &mut Context<Self>) {
        if self.capture_helper.reset() {
            cx.notify();
        }
    }

    fn observe_capture_helper_output(&mut self, chunk: &str, cx: &mut Context<Self>) {
        let chunk = sanitize_terminal_output(chunk);
        if self.capture_helper.observe_output(&chunk) {
            cx.notify();
        }
    }

    fn set_pty_writer(&mut self, writer: Option<Arc<Mutex<Box<dyn Write + Send>>>>, cx: &mut Context<Self>) {
        self.pty_writer = writer;
        cx.notify();
    }

    fn write_pty_value(&mut self, value: &str, cx: &mut Context<Self>) -> bool {
        let writer = self.pty_writer.clone();
        if let Some(writer) = writer {
            if let Ok(mut guard) = writer.lock() {
                let _ = guard.write_all(value.as_bytes());
                let newline = if cfg!(windows) { b"\r\n".as_slice() } else { b"\n".as_slice() };
                let _ = guard.write_all(newline);
                let _ = guard.flush();
                if self.command_state.kind == CommandKind::Capture
                    && self.capture_helper.record_submission(value)
                {
                    cx.notify();
                }
                true
            } else {
                self.append_output(t(self.language, "message.pty_writer_locked"), cx);
                false
            }
        } else {
            self.append_output(t(self.language, "message.no_pty_input"), cx);
            false
        }
    }
    fn send_pty_input(&mut self, cx: &mut Context<Self>) {
        let input_value = self.pty_input.read(cx).value().to_string();
        if input_value.trim().is_empty() {
            return;
        }

        if self.write_pty_value(&input_value, cx) {
            self.pty_input.update(cx, |input, cx| input.set_value("", cx));
        }
    }

    fn resolve_input_dir(&self) -> Option<PathBuf> {
        match self.command_state.kind {
            CommandKind::Observe => {
                let path = PathBuf::from(self.command_state.observe.media_dir.trim());
                if path.as_os_str().is_empty() {
                    None
                } else {
                    Some(path)
                }
            }
            CommandKind::Capture => {
                let csv = PathBuf::from(self.command_state.capture.csv_path.trim());
                if csv.as_os_str().is_empty() {
                    None
                } else {
                    csv.parent().map(|p| p.to_path_buf())
                }
            }
            CommandKind::Xmp => match self.command_state.xmp.subcommand {
                XmpSubcommand::Copy | XmpSubcommand::Init | XmpSubcommand::Remove => {
                    let path = PathBuf::from(self.command_state.xmp.source_dir.trim());
                    if path.as_os_str().is_empty() {
                        None
                    } else {
                        Some(path)
                    }
                }
                XmpSubcommand::Update => {
                    let csv = PathBuf::from(self.command_state.xmp.csv_path.trim());
                    if csv.as_os_str().is_empty() {
                        None
                    } else {
                        csv.parent().map(|p| p.to_path_buf())
                    }
                }
                XmpSubcommand::Sync => {
                    if !self.command_state.xmp.dir.trim().is_empty() {
                        Some(PathBuf::from(self.command_state.xmp.dir.trim()))
                    } else {
                        let csv = PathBuf::from(self.command_state.xmp.csv_path.trim());
                        if csv.as_os_str().is_empty() {
                            None
                        } else {
                            csv.parent().map(|p| p.to_path_buf())
                        }
                    }
                }
            },
            CommandKind::Extract => {
                let csv = PathBuf::from(self.command_state.extract.csv_path.trim());
                if csv.as_os_str().is_empty() {
                    None
                } else {
                    csv.parent().map(|p| p.to_path_buf())
                }
            }
            CommandKind::Translate => {
                let csv = PathBuf::from(self.command_state.translate.csv_path.trim());
                if csv.as_os_str().is_empty() {
                    None
                } else {
                    csv.parent().map(|p| p.to_path_buf())
                }
            }
        }
    }

    fn resolve_output_dir(&self) -> Option<PathBuf> {
        match self.command_state.kind {
            CommandKind::Observe => self
                .command_state
                .observe
                .output_dir
                .as_ref()
                .filter(|s| !s.trim().is_empty())
                .map(PathBuf::from)
                .or_else(|| Some(PathBuf::from("./serval_output/serval_observe"))),
            CommandKind::Capture => self
                .command_state
                .capture
                .output_dir
                .as_ref()
                .filter(|s| !s.trim().is_empty())
                .map(PathBuf::from)
                .or_else(|| Some(PathBuf::from("./serval_output/serval_capture"))),
            CommandKind::Xmp => {
                if self.command_state.xmp.subcommand == XmpSubcommand::Copy {
                    if self.command_state.xmp.output_dir.trim().is_empty() {
                        None
                    } else {
                        Some(PathBuf::from(self.command_state.xmp.output_dir.trim()))
                    }
                } else {
                    None
                }
            }
            CommandKind::Extract => self
                .command_state
                .extract
                .output_dir
                .as_ref()
                .filter(|s| !s.trim().is_empty())
                .map(PathBuf::from)
                .or_else(|| Some(PathBuf::from("./serval_output/serval_extract"))),
            CommandKind::Translate => self
                .command_state
                .translate
                .output_dir
                .as_ref()
                .filter(|s| !s.trim().is_empty())
                .map(PathBuf::from)
                .or_else(|| Some(PathBuf::from("./serval_output/serval_translate"))),
        }
    }

    fn current_help_key(&self) -> String {
        match self.command_state.kind {
            CommandKind::Observe => "observe".to_string(),
            CommandKind::Capture => "capture".to_string(),
            CommandKind::Xmp => match self.command_state.xmp.subcommand {
                XmpSubcommand::Copy => "xmp-copy".to_string(),
                XmpSubcommand::Init => "xmp-init".to_string(),
                XmpSubcommand::Update => "xmp-update".to_string(),
                XmpSubcommand::Remove => "xmp-remove".to_string(),
                XmpSubcommand::Sync => "xmp-sync".to_string(),
            },
            CommandKind::Extract => "extract".to_string(),
            CommandKind::Translate => "translate".to_string(),
        }
    }

    fn help_text_for_key(&self, key: &str) -> String {
        self.help_cache.get(key).cloned().unwrap_or_else(|| {
            t(self.language, "message.help_missing").to_string()
        })
    }

    fn set_hover_help_key(&mut self, key: &str, cx: &mut Context<Self>) {
        if !self.helper_mode {
            return;
        }
        self.hover_help_key = Some(key.to_string());
        self.option_help_position = Some(self.cursor_position);
        self.ensure_help_loaded_for_key(key.to_string(), cx);
    }

    fn clear_hover_help_key(&mut self, key: &str, cx: &mut Context<Self>) {
        if self.hover_help_key.as_deref() == Some(key) {
            self.hover_help_key = None;
            self.option_help_position = None;
            cx.notify();
        }
    }

    fn ensure_help_loaded_for_key(&mut self, key: String, cx: &mut Context<Self>) {
        if self.help_cache.contains_key(&key) {
            return;
        }

        let text = help_texts::text_for_key(self.language, &key)
            .map(|s| s.to_string())
            .unwrap_or_else(|| t(self.language, "message.help_missing").to_string());
        self.help_cache.insert(key, text);
        cx.notify();
    }

    fn render_capture_helper_panel(&self, entity: Entity<Self>) -> AnyElement {
        let Some(prompt) = self.capture_helper.prompt().cloned() else {
            return div().into_any_element();
        };

        let language = self.language;
        let last_submission = self.capture_helper.last_submission().map(str::to_string);

        let panel = div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .pt(px(8.0))
            .border_t_1()
            .border_color(rgb(0x374151))
            .child(
                div()
                    .text_color(rgb(0x9CA3AF))
                    .child("Capture Helper"),
            )
            .child(div().text_color(rgb(0xF9FAFB)).child(prompt.prompt.clone()))
            .child(if let Some(sample_path) = prompt.sample_path.clone() {
                div()
                    .text_color(rgb(0xD1D5DB))
                    .font_family("monospace")
                    .text_size(px(12.0))
                    .child(sample_path)
            } else {
                div()
            })
            .child(if prompt.options.is_empty() {
                div()
            } else {
                prompt.options.into_iter().fold(
                    div().flex().flex_row().flex_wrap().gap(px(8.0)),
                    |container, option| {
                        let entity_click = entity.clone();
                        let value = option.value.clone();
                        let button_text = format!("{}: {}", option.value, option.label);
                        container.child(
                            div()
                                .id(SharedString::from(format!("capture-helper-{}", value)))
                                .bg(rgb(0x111827))
                                .text_color(rgb(0xF9FAFB))
                                .p(px(8.0))
                                .cursor_pointer()
                                .on_click(move |_, _, cx| {
                                    entity_click.update(cx, |view, cx| {
                                        view.write_pty_value(&value, cx);
                                    });
                                })
                                .child(button_text),
                        )
                    },
                )
            })
            .child(if prompt.kind == CaptureHelperPromptKind::Minutes {
                div()
                    .text_color(rgb(0x9CA3AF))
                    .child(t(language, "message.capture_prompt_enter_hint"))
            } else {
                div()
            })
            .child(if let Some(last_submission) = last_submission {
                div()
                    .text_color(rgb(0x9CA3AF))
                    .child(format!("{}: {}", t(language, "label.last_sent"), last_submission))
            } else {
                div()
            });

        panel.into_any_element()
    }
}

impl Render for SetupView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let root = self.root.clone();
        let serval_binary_input = self.serval_binary_input.clone();
        let language = root.read(cx).language;
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .p(px(16.0))
            .bg(rgb(0xFFFFFF))
            .child(div().text_color(rgb(0x6B7280)).child(t(language, "setup.language")))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(8.0))
                    .child({
                        let root = root.clone();
                        div()
                            .id("setup-lang-en")
                            .bg(rgb(if language == Language::En { 0x111827 } else { 0xF3F4F6 }))
                            .text_color(rgb(if language == Language::En { 0xF9FAFB } else { 0x111827 }))
                            .p(px(8.0))
                            .cursor_pointer()
                            .on_click(move |_, _, cx| {
                                root.update(cx, |view, cx| view.set_language(Language::En, cx));
                            })
                            .child(t(language, "setup.language_en"))
                    })
                    .child({
                        let root = root.clone();
                        div()
                            .id("setup-lang-zh-cn")
                            .bg(rgb(if language == Language::ZhCn { 0x111827 } else { 0xF3F4F6 }))
                            .text_color(rgb(if language == Language::ZhCn { 0xF9FAFB } else { 0x111827 }))
                            .p(px(8.0))
                            .cursor_pointer()
                            .on_click(move |_, _, cx| {
                                root.update(cx, |view, cx| view.set_language(Language::ZhCn, cx));
                            })
                            .child(t(language, "setup.language_zh_cn"))
                    }),
            )
            .child(div().text_color(rgb(0x6B7280)).child(t(language, "setup.serval_binary")))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(8.0))
                    .child(div().flex_grow().child(serval_binary_input.clone()))
                    .child({
                        let root = root.clone();
                        let serval_binary_input = serval_binary_input.clone();
                        div()
                            .id("browse-serval-binary")
                            .bg(rgb(0xF3F4F6))
                            .text_color(rgb(0x111827))
                            .p(px(8.0))
                            .cursor_pointer()
                            .on_click(move |_, window, cx| {
                                browse_into_text_input(
                                    window,
                                    cx,
                                    PathPromptOptions {
                                        files: true,
                                        directories: false,
                                        multiple: false,
                                        prompt: Some(
                                            t(language, "setup.select_serval_executable").into(),
                                        ),
                                    },
                                    serval_binary_input.clone(),
                                    root.clone(),
                                    language,
                                );
                            })
                            .child(t(language, "action.browse"))
                    }),
            )
            .child({
                let root = root.clone();
                let serval_binary_input = serval_binary_input.clone();
                div()
                    .id("save-serval-binary")
                    .bg(rgb(0x111827))
                    .text_color(rgb(0xF9FAFB))
                    .p(px(8.0))
                    .cursor_pointer()
                    .on_click(move |_, window, cx| {
                        let value = serval_binary_input.read(cx).value().to_string();
                        root.update(cx, |view, cx| {
                            view.set_serval_binary_path(Some(value), cx);
                            view.append_output(t(language, "message.binary_updated"), cx);
                        });
                        window.remove_window();
                    })
                    .child(t(language, "action.save"))
            })
    }
}

enum OutputEvent {
    Line(String),
    Chunk(String),
    Error(String),
    Exit(i32),
    PtyWriter(Arc<Mutex<Box<dyn Write + Send>>>),
}

fn strip_ansi_escapes(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{001b}' {
            match chars.peek().copied() {
                Some('[') => {
                    let _ = chars.next();
                    while let Some(next) = chars.next() {
                        if next.is_ascii_alphabetic() {
                            break;
                        }
                    }
                }
                Some(']') => {
                    let _ = chars.next();
                    while let Some(next) = chars.next() {
                        if next == '\u{0007}' {
                            break;
                        }
                        if next == '\u{001b}' && matches!(chars.peek().copied(), Some('\\')) {
                            let _ = chars.next();
                            break;
                        }
                    }
                }
                _ => {}
            }
            continue;
        }
        if ch.is_control() && ch != '\n' && ch != '\t' {
            continue;
        }
        out.push(ch);
    }
    out
}

fn sanitize_terminal_output(input: &str) -> String {
    let mut line = input.to_string();
    if line.contains('\t') {
        line = line.replace('\t', "    ");
    }
    if line.contains('\r') {
        line = line.replace('\r', "");
    }
    if line.contains('\u{001b}') {
        line = strip_ansi_escapes(&line);
    }
    line
}

impl Render for RootView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let entity_observe = entity.clone();
        let entity_capture = entity.clone();
        let entity_xmp = entity.clone();
        let entity_extract = entity.clone();
        let entity_translate = entity.clone();
        let preview = self.command_preview();
        let active_binary = self.executable_program();
        let observe_selected = self.command_state.kind == CommandKind::Observe;
        let capture_selected = self.command_state.kind == CommandKind::Capture;
        let xmp_selected = self.command_state.kind == CommandKind::Xmp;
        let extract_selected = self.command_state.kind == CommandKind::Extract;
        let translate_selected = self.command_state.kind == CommandKind::Translate;
        let language = self.language;
        let helper_mode = self.helper_mode;
        let running = self.running;
        let command_panel_open = self.command_panel_open;
        let input_panel_open = self.input_panel_open;
        let preview_panel_open = self.preview_panel_open;
        let hover_help_key = self.hover_help_key.clone();
        let option_help_position = self.option_help_position;
        let command_help_key = self
            .command_help_key
            .clone()
            .unwrap_or_else(|| self.current_help_key());
        let command_help_text = self.help_text_for_key(&command_help_key);
        let option_help_text = hover_help_key
            .as_ref()
            .map(|key| self.help_text_for_key(key));
        let output_text = if self.output_log.is_empty() {
            t(language, "message.output_placeholder").to_string()
        } else {
            self.output_log.clone()
        };
        let capture_helper_panel = if capture_selected && running {
            self.render_capture_helper_panel(entity.clone())
        } else {
            div().into_any_element()
        };

        let observe_input = self.observe_input.clone();
        let observe_output_input = self.observe_output_input.clone();
        let capture_input = self.capture_input.clone();
        let capture_output_input = self.capture_output_input.clone();
        let xmp_source_input = self.xmp_source_input.clone();
        let xmp_output_input = self.xmp_output_input.clone();
        let xmp_csv_input = self.xmp_csv_input.clone();
        let xmp_dir_input = self.xmp_dir_input.clone();
        let extract_csv_input = self.extract_csv_input.clone();
        let extract_value_input = self.extract_value_input.clone();
        let extract_output_input = self.extract_output_input.clone();
        let translate_csv_input = self.translate_csv_input.clone();
        let translate_taglist_input = self.translate_taglist_input.clone();
        let translate_from_input = self.translate_from_input.clone();
        let translate_to_input = self.translate_to_input.clone();
        let translate_output_input = self.translate_output_input.clone();
        let pty_input = self.pty_input.clone();

        let input_section = if observe_selected {
            div()
                .flex()
                .flex_col()
                .gap(px(12.0))
                .child(div().text_color(rgb(0x6B7280)).child(t(language, "label.media_dir")))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(8.0))
                        .child(div().flex_grow().child(observe_input.clone()))
                        .child({
                            let entity = entity.clone();
                            div()
                                .id("browse-observe")
                                .bg(rgb(0xF3F4F6))
                                .text_color(rgb(0x111827))
                                .p(px(8.0))
                                .cursor_pointer()
                                .on_click(move |_, window, cx| {
                                    browse_into_text_input(
                                        window,
                                        cx,
                                        PathPromptOptions {
                                            files: false,
                                            directories: true,
                                            multiple: false,
                                            prompt: Some(
                                                t(language, "prompt.select_media_directory").into(),
                                            ),
                                        },
                                        observe_input.clone(),
                                        entity.clone(),
                                        language,
                                    );
                                })
                                .child(t(language, "action.browse"))
                        }),
                )
                .child(div().text_color(rgb(0x6B7280)).child(t(language, "label.output_dir")))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(8.0))
                        .child(div().flex_grow().child(observe_output_input.clone()))
                        .child({
                            let entity = entity.clone();
                            div()
                                .id("browse-observe-output")
                                .bg(rgb(0xF3F4F6))
                                .text_color(rgb(0x111827))
                                .p(px(8.0))
                                .cursor_pointer()
                                .on_click(move |_, window, cx| {
                                    browse_into_text_input(
                                        window,
                                        cx,
                                        PathPromptOptions {
                                            files: false,
                                            directories: true,
                                            multiple: false,
                                            prompt: Some(
                                                t(language, "prompt.select_output_directory").into(),
                                            ),
                                        },
                                        observe_output_input.clone(),
                                        entity.clone(),
                                        language,
                                    );
                                })
                                .child(t(language, "action.browse"))
                        }),
                )
                .child(div().text_color(rgb(0x6B7280)).child(t(language, "label.options")))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .gap(px(8.0))
                        .child({
                            let entity = entity.clone();
                            let entity_click = entity.clone();
                            let entity_hover = entity.clone();
                            let active = self.command_state.observe.xmp;
                            div()
                                .id("toggle-observe-xmp")
                                .bg(rgb(if active { 0x111827 } else { 0xF3F4F6 }))
                                .text_color(rgb(if active { 0xF9FAFB } else { 0x111827 }))
                                .p(px(8.0))
                                .cursor_pointer()
                                .on_click(move |_, _, cx| {
                                    entity_click.update(cx, |view, cx| {
                                        view.command_state.observe.xmp =
                                            !view.command_state.observe.xmp;
                                        cx.notify();
                                    });
                                })
                                .on_hover(move |hovered, _, cx| {
                                    entity_hover.update(cx, |view, cx| {
                                        if *hovered {
                                            view.set_hover_help_key("observe|--xmp", cx);
                                        } else {
                                            view.clear_hover_help_key("observe|--xmp", cx);
                                        }
                                    });
                                })
                                .child(t(language, "opt.xmp"))
                        })
                        .child({
                            let entity = entity.clone();
                            let entity_click = entity.clone();
                            let entity_hover = entity.clone();
                            let active = self.command_state.observe.subject;
                            div()
                                .id("toggle-observe-subject")
                                .bg(rgb(if active { 0x111827 } else { 0xF3F4F6 }))
                                .text_color(rgb(if active { 0xF9FAFB } else { 0x111827 }))
                                .p(px(8.0))
                                .cursor_pointer()
                                .on_click(move |_, _, cx| {
                                    entity_click.update(cx, |view, cx| {
                                        view.command_state.observe.subject =
                                            !view.command_state.observe.subject;
                                        cx.notify();
                                    });
                                })
                                .on_hover(move |hovered, _, cx| {
                                    entity_hover.update(cx, |view, cx| {
                                        if *hovered {
                                            view.set_hover_help_key("observe|--subject", cx);
                                        } else {
                                            view.clear_hover_help_key("observe|--subject", cx);
                                        }
                                    });
                                })
                                .child(t(language, "opt.subject"))
                        })
                        .child({
                            let entity = entity.clone();
                            let entity_click = entity.clone();
                            let entity_hover = entity.clone();
                            let active = self.command_state.observe.modified_time;
                            div()
                                .id("toggle-observe-modified-time")
                                .bg(rgb(if active { 0x111827 } else { 0xF3F4F6 }))
                                .text_color(rgb(if active { 0xF9FAFB } else { 0x111827 }))
                                .p(px(8.0))
                                .cursor_pointer()
                                .on_click(move |_, _, cx| {
                                    entity_click.update(cx, |view, cx| {
                                        view.command_state.observe.modified_time =
                                            !view.command_state.observe.modified_time;
                                        cx.notify();
                                    });
                                })
                                .on_hover(move |hovered, _, cx| {
                                    entity_hover.update(cx, |view, cx| {
                                        if *hovered {
                                            view.set_hover_help_key("observe|--modified-time", cx);
                                        } else {
                                            view.clear_hover_help_key("observe|--modified-time", cx);
                                        }
                                    });
                                })
                                .child(t(language, "opt.modified_time"))
                        })
                        .child({
                            let entity = entity.clone();
                            let entity_click = entity.clone();
                            let entity_hover = entity.clone();
                            let active = self.command_state.observe.video_only;
                            div()
                                .id("toggle-observe-video")
                                .bg(rgb(if active { 0x111827 } else { 0xF3F4F6 }))
                                .text_color(rgb(if active { 0xF9FAFB } else { 0x111827 }))
                                .p(px(8.0))
                                .cursor_pointer()
                                .on_click(move |_, _, cx| {
                                    entity_click.update(cx, |view, cx| {
                                        view.command_state.observe.video_only =
                                            !view.command_state.observe.video_only;
                                        if view.command_state.observe.video_only {
                                            view.command_state.observe.image_only = false;
                                        }
                                        cx.notify();
                                    });
                                })
                                .on_hover(move |hovered, _, cx| {
                                    entity_hover.update(cx, |view, cx| {
                                        if *hovered {
                                            view.set_hover_help_key("observe|--video", cx);
                                        } else {
                                            view.clear_hover_help_key("observe|--video", cx);
                                        }
                                    });
                                })
                                .child(t(language, "opt.video_only"))
                        })
                        .child({
                            let entity = entity.clone();
                            let entity_click = entity.clone();
                            let entity_hover = entity.clone();
                            let active = self.command_state.observe.image_only;
                            div()
                                .id("toggle-observe-image")
                                .bg(rgb(if active { 0x111827 } else { 0xF3F4F6 }))
                                .text_color(rgb(if active { 0xF9FAFB } else { 0x111827 }))
                                .p(px(8.0))
                                .cursor_pointer()
                                .on_click(move |_, _, cx| {
                                    entity_click.update(cx, |view, cx| {
                                        view.command_state.observe.image_only =
                                            !view.command_state.observe.image_only;
                                        if view.command_state.observe.image_only {
                                            view.command_state.observe.video_only = false;
                                        }
                                        cx.notify();
                                    });
                                })
                                .on_hover(move |hovered, _, cx| {
                                    entity_hover.update(cx, |view, cx| {
                                        if *hovered {
                                            view.set_hover_help_key("observe|--image", cx);
                                        } else {
                                            view.clear_hover_help_key("observe|--image", cx);
                                        }
                                    });
                                })
                                .child(t(language, "opt.image_only"))
                        })
                        .child({
                            let entity = entity.clone();
                            let entity_click = entity.clone();
                            let entity_hover = entity.clone();
                            let active = self.command_state.observe.debug;
                            div()
                                .id("toggle-observe-debug")
                                .bg(rgb(if active { 0x111827 } else { 0xF3F4F6 }))
                                .text_color(rgb(if active { 0xF9FAFB } else { 0x111827 }))
                                .p(px(8.0))
                                .cursor_pointer()
                                .on_click(move |_, _, cx| {
                                    entity_click.update(cx, |view, cx| {
                                        view.command_state.observe.debug =
                                            !view.command_state.observe.debug;
                                        cx.notify();
                                    });
                                })
                                .on_hover(move |hovered, _, cx| {
                                    entity_hover.update(cx, |view, cx| {
                                        if *hovered {
                                            view.set_hover_help_key("observe|--debug", cx);
                                        } else {
                                            view.clear_hover_help_key("observe|--debug", cx);
                                        }
                                    });
                                })
                                .child(t(language, "opt.debug"))
                        })
                        .child({
                            let entity = entity.clone();
                            let entity_click = entity.clone();
                            let entity_hover = entity.clone();
                            let active = self.command_state.observe.independent;
                            div()
                                .id("toggle-observe-independent")
                                .bg(rgb(if active { 0x111827 } else { 0xF3F4F6 }))
                                .text_color(rgb(if active { 0xF9FAFB } else { 0x111827 }))
                                .p(px(8.0))
                                .cursor_pointer()
                                .on_click(move |_, _, cx| {
                                    entity_click.update(cx, |view, cx| {
                                        view.command_state.observe.independent =
                                            !view.command_state.observe.independent;
                                        cx.notify();
                                    });
                                })
                                .on_hover(move |hovered, _, cx| {
                                    entity_hover.update(cx, |view, cx| {
                                        if *hovered {
                                            view.set_hover_help_key("observe|--independent", cx);
                                        } else {
                                            view.clear_hover_help_key("observe|--independent", cx);
                                        }
                                    });
                                })
                                .child(t(language, "opt.independent"))
                        }),
                )
        } else if capture_selected {
            div()
                .flex()
                .flex_col()
                .gap(px(12.0))
                .child(div().text_color(rgb(0x6B7280)).child(t(language, "label.csv_path")))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(8.0))
                        .child(div().flex_grow().child(capture_input.clone()))
                        .child({
                            let entity = entity.clone();
                            div()
                                .id("browse-capture")
                                .bg(rgb(0xF3F4F6))
                                .text_color(rgb(0x111827))
                                .p(px(8.0))
                                .cursor_pointer()
                                .on_click(move |_, window, cx| {
                                    browse_into_text_input(
                                        window,
                                        cx,
                                        PathPromptOptions {
                                            files: true,
                                            directories: false,
                                            multiple: false,
                                            prompt: Some(t(language, "prompt.select_tags_csv").into()),
                                        },
                                        capture_input.clone(),
                                        entity.clone(),
                                        language,
                                    );
                                })
                                .child(t(language, "action.browse"))
                        }),
                )
                .child(div().text_color(rgb(0x6B7280)).child(t(language, "label.output_dir")))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(8.0))
                        .child(div().flex_grow().child(capture_output_input.clone()))
                        .child({
                            let entity = entity.clone();
                            div()
                                .id("browse-capture-output")
                                .bg(rgb(0xF3F4F6))
                                .text_color(rgb(0x111827))
                                .p(px(8.0))
                                .cursor_pointer()
                                .on_click(move |_, window, cx| {
                                    browse_into_text_input(
                                        window,
                                        cx,
                                        PathPromptOptions {
                                            files: false,
                                            directories: true,
                                            multiple: false,
                                            prompt: Some(
                                                t(language, "prompt.select_output_directory").into(),
                                            ),
                                        },
                                        capture_output_input.clone(),
                                        entity.clone(),
                                        language,
                                    );
                                })
                                .child(t(language, "action.browse"))
                        }),
                )
                .child(div().text_color(rgb(0x6B7280)).child(t(language, "label.options")))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .gap(px(8.0))
                        .child({
                            let entity = entity.clone();
                            let entity_click = entity.clone();
                            let entity_hover = entity.clone();
                            let active = self.command_state.capture.event;
                            div()
                                .id("toggle-capture-event")
                                .bg(rgb(if active { 0x111827 } else { 0xF3F4F6 }))
                                .text_color(rgb(if active { 0xF9FAFB } else { 0x111827 }))
                                .p(px(8.0))
                                .cursor_pointer()
                                .on_click(move |_, _, cx| {
                                    entity_click.update(cx, |view, cx| {
                                        view.command_state.capture.event =
                                            !view.command_state.capture.event;
                                        cx.notify();
                                    });
                                })
                                .on_hover(move |hovered, _, cx| {
                                    entity_hover.update(cx, |view, cx| {
                                        if *hovered {
                                            view.set_hover_help_key("capture|--event", cx);
                                        } else {
                                            view.clear_hover_help_key("capture|--event", cx);
                                        }
                                    });
                                })
                                .child(t(language, "opt.event"))
                        })
                        .child({
                            let entity = entity.clone();
                            let entity_click = entity.clone();
                            let entity_hover = entity.clone();
                            let active = self.command_state.capture.no_exclude;
                            div()
                                .id("toggle-capture-no-exclude")
                                .bg(rgb(if active { 0x111827 } else { 0xF3F4F6 }))
                                .text_color(rgb(if active { 0xF9FAFB } else { 0x111827 }))
                                .p(px(8.0))
                                .cursor_pointer()
                                .on_click(move |_, _, cx| {
                                    entity_click.update(cx, |view, cx| {
                                        view.command_state.capture.no_exclude =
                                            !view.command_state.capture.no_exclude;
                                        cx.notify();
                                    });
                                })
                                .on_hover(move |hovered, _, cx| {
                                    entity_hover.update(cx, |view, cx| {
                                        if *hovered {
                                            view.set_hover_help_key("capture|--no-exclude", cx);
                                        } else {
                                            view.clear_hover_help_key("capture|--no-exclude", cx);
                                        }
                                    });
                                })
                                .child(t(language, "opt.no_exclude"))
                        })
                        .child({
                            let entity = entity.clone();
                            let entity_click = entity.clone();
                            let entity_hover = entity.clone();
                            let active = self.command_state.capture.camtrap_dp;
                            div()
                                .id("toggle-capture-camtrap")
                                .bg(rgb(if active { 0x111827 } else { 0xF3F4F6 }))
                                .text_color(rgb(if active { 0xF9FAFB } else { 0x111827 }))
                                .p(px(8.0))
                                .cursor_pointer()
                                .on_click(move |_, _, cx| {
                                    entity_click.update(cx, |view, cx| {
                                        view.command_state.capture.camtrap_dp =
                                            !view.command_state.capture.camtrap_dp;
                                        cx.notify();
                                    });
                                })
                                .on_hover(move |hovered, _, cx| {
                                    entity_hover.update(cx, |view, cx| {
                                        if *hovered {
                                            view.set_hover_help_key("capture|--camtrap-dp", cx);
                                        } else {
                                            view.clear_hover_help_key("capture|--camtrap-dp", cx);
                                        }
                                    });
                                })
                                .child(t(language, "opt.camtrap_dp"))
                        }),
                )
        } else if xmp_selected {
            div()
                .flex()
                .flex_col()
                .gap(px(12.0))
                .child(div().text_color(rgb(0x6B7280)).child(t(language, "label.xmp_subcommand")))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .gap(px(8.0))
                        .child({
                            let entity = entity.clone();
                            let entity_click = entity.clone();
                            let active = self.command_state.xmp.subcommand == XmpSubcommand::Copy;
                            div()
                                .id("xmp-copy")
                                .bg(rgb(if active { 0x111827 } else { 0xF3F4F6 }))
                                .text_color(rgb(if active { 0xF9FAFB } else { 0x111827 }))
                                .p(px(8.0))
                                .cursor_pointer()
                                    .on_click(move |_, _, cx| {
                                        entity_click.update(cx, |view, cx| {
                                            view.command_state.xmp.subcommand = XmpSubcommand::Copy;
                                            if view.command_help_open {
                                                let key = view.current_help_key();
                                                view.command_help_key = Some(key.clone());
                                                view.ensure_help_loaded_for_key(key, cx);
                                            } else {
                                                cx.notify();
                                            }
                                        });
                                    })
                                    .child(t(language, "opt.copy"))
                        })
                        .child({
                            let entity = entity.clone();
                            let entity_click = entity.clone();
                            let active = self.command_state.xmp.subcommand == XmpSubcommand::Init;
                            div()
                                .id("xmp-init")
                                .bg(rgb(if active { 0x111827 } else { 0xF3F4F6 }))
                                .text_color(rgb(if active { 0xF9FAFB } else { 0x111827 }))
                                .p(px(8.0))
                                .cursor_pointer()
                                    .on_click(move |_, _, cx| {
                                        entity_click.update(cx, |view, cx| {
                                            view.command_state.xmp.subcommand = XmpSubcommand::Init;
                                            if view.command_help_open {
                                                let key = view.current_help_key();
                                                view.command_help_key = Some(key.clone());
                                                view.ensure_help_loaded_for_key(key, cx);
                                            } else {
                                                cx.notify();
                                            }
                                        });
                                    })
                                    .child(t(language, "opt.init"))
                        })
                        .child({
                            let entity = entity.clone();
                            let entity_click = entity.clone();
                            let active = self.command_state.xmp.subcommand == XmpSubcommand::Update;
                            div()
                                .id("xmp-update")
                                .bg(rgb(if active { 0x111827 } else { 0xF3F4F6 }))
                                .text_color(rgb(if active { 0xF9FAFB } else { 0x111827 }))
                                .p(px(8.0))
                                .cursor_pointer()
                                    .on_click(move |_, _, cx| {
                                        entity_click.update(cx, |view, cx| {
                                            view.command_state.xmp.subcommand = XmpSubcommand::Update;
                                            if view.command_help_open {
                                                let key = view.current_help_key();
                                                view.command_help_key = Some(key.clone());
                                                view.ensure_help_loaded_for_key(key, cx);
                                            } else {
                                                cx.notify();
                                            }
                                        });
                                    })
                                    .child(t(language, "opt.update"))
                        })
                        .child({
                            let entity = entity.clone();
                            let entity_click = entity.clone();
                            let active = self.command_state.xmp.subcommand == XmpSubcommand::Remove;
                            div()
                                .id("xmp-remove")
                                .bg(rgb(if active { 0x111827 } else { 0xF3F4F6 }))
                                .text_color(rgb(if active { 0xF9FAFB } else { 0x111827 }))
                                .p(px(8.0))
                                .cursor_pointer()
                                    .on_click(move |_, _, cx| {
                                        entity_click.update(cx, |view, cx| {
                                            view.command_state.xmp.subcommand = XmpSubcommand::Remove;
                                            if view.command_help_open {
                                                let key = view.current_help_key();
                                                view.command_help_key = Some(key.clone());
                                                view.ensure_help_loaded_for_key(key, cx);
                                            } else {
                                                cx.notify();
                                            }
                                        });
                                    })
                                    .child(t(language, "opt.remove"))
                        })
                        .child({
                            let entity = entity.clone();
                            let entity_click = entity.clone();
                            let active = self.command_state.xmp.subcommand == XmpSubcommand::Sync;
                            div()
                                .id("xmp-sync")
                                .bg(rgb(if active { 0x111827 } else { 0xF3F4F6 }))
                                .text_color(rgb(if active { 0xF9FAFB } else { 0x111827 }))
                                .p(px(8.0))
                                .cursor_pointer()
                                    .on_click(move |_, _, cx| {
                                        entity_click.update(cx, |view, cx| {
                                            view.command_state.xmp.subcommand = XmpSubcommand::Sync;
                                            if view.command_help_open {
                                                let key = view.current_help_key();
                                                view.command_help_key = Some(key.clone());
                                                view.ensure_help_loaded_for_key(key, cx);
                                            } else {
                                                cx.notify();
                                            }
                                        });
                                    })
                                    .child(t(language, "opt.sync"))
                        }),
                )
                .child(if self.command_state.xmp.subcommand == XmpSubcommand::Copy
                    || self.command_state.xmp.subcommand == XmpSubcommand::Init
                    || self.command_state.xmp.subcommand == XmpSubcommand::Remove
                {
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .child(div().text_color(rgb(0x6B7280)).child(t(language, "label.source_dir")))
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .gap(px(8.0))
                                .child(div().flex_grow().child(xmp_source_input.clone()))
                                .child({
                                    let entity = entity.clone();
                                    div()
                                        .id("browse-xmp-source")
                                        .bg(rgb(0xF3F4F6))
                                        .text_color(rgb(0x111827))
                                        .p(px(8.0))
                                        .cursor_pointer()
                                        .on_click(move |_, window, cx| {
                                            browse_into_text_input(
                                                window,
                                                cx,
                                                PathPromptOptions {
                                                    files: false,
                                                    directories: true,
                                                    multiple: false,
                                                    prompt: Some(
                                                        t(language, "prompt.select_source_directory")
                                                            .into(),
                                                    ),
                                                },
                                                xmp_source_input.clone(),
                                                entity.clone(),
                                                language,
                                            );
                                        })
                                        .child(t(language, "action.browse"))
                                }),
                        )
                        .child(if self.command_state.xmp.subcommand == XmpSubcommand::Copy {
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(8.0))
                                .child(div().text_color(rgb(0x6B7280)).child(t(language, "label.output_dir")))
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .gap(px(8.0))
                                        .child(div().flex_grow().child(xmp_output_input.clone()))
                                        .child({
                                            let entity = entity.clone();
                                            div()
                                                .id("browse-xmp-output")
                                                .bg(rgb(0xF3F4F6))
                                                .text_color(rgb(0x111827))
                                                .p(px(8.0))
                                                .cursor_pointer()
                                                .on_click(move |_, window, cx| {
                                                    browse_into_text_input(
                                                        window,
                                                        cx,
                                                        PathPromptOptions {
                                                            files: false,
                                                            directories: true,
                                                            multiple: false,
                                                            prompt: Some(
                                                                t(
                                                                    language,
                                                                    "prompt.select_output_directory",
                                                                )
                                                                .into(),
                                                            ),
                                                        },
                                                        xmp_output_input.clone(),
                                                        entity.clone(),
                                                        language,
                                                    );
                                                })
                                                .child(t(language, "action.browse"))
                                        }),
                                )
                        } else {
                            div()
                        })
                } else if self.command_state.xmp.subcommand == XmpSubcommand::Update {
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .child(div().text_color(rgb(0x6B7280)).child(t(language, "label.csv_path")))
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .gap(px(8.0))
                                .child(div().flex_grow().child(xmp_csv_input.clone()))
                                .child({
                                    let entity = entity.clone();
                                    div()
                                        .id("browse-xmp-csv")
                                        .bg(rgb(0xF3F4F6))
                                        .text_color(rgb(0x111827))
                                        .p(px(8.0))
                                        .cursor_pointer()
                                        .on_click(move |_, window, cx| {
                                            browse_into_text_input(
                                                window,
                                                cx,
                                                PathPromptOptions {
                                                    files: true,
                                                    directories: false,
                                                    multiple: false,
                                                    prompt: Some(
                                                        t(language, "prompt.select_csv_path").into(),
                                                    ),
                                                },
                                                xmp_csv_input.clone(),
                                                entity.clone(),
                                                language,
                                            );
                                        })
                                        .child(t(language, "action.browse"))
                                }),
                        )
                        .child(div().text_color(rgb(0x6B7280)).child(t(language, "label.options")))
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .flex_wrap()
                                .gap(px(8.0))
                                .child({
                                    let entity = entity.clone();
                                    let entity_click = entity.clone();
                                    let entity_hover = entity.clone();
                                    let active = self.command_state.xmp.datetime;
                                    div()
                                        .id("xmp-update-datetime")
                                        .bg(rgb(if active { 0x111827 } else { 0xF3F4F6 }))
                                        .text_color(rgb(if active { 0xF9FAFB } else { 0x111827 }))
                                        .p(px(8.0))
                                        .cursor_pointer()
                                        .on_click(move |_, _, cx| {
                                            entity_click.update(cx, |view, cx| {
                                                view.command_state.xmp.datetime =
                                                    !view.command_state.xmp.datetime;
                                                cx.notify();
                                            });
                                        })
                                        .on_hover(move |hovered, _, cx| {
                                            entity_hover.update(cx, |view, cx| {
                                                if *hovered {
                                                    view.set_hover_help_key("xmp-update|--datetime", cx);
                                                } else {
                                                    view.clear_hover_help_key("xmp-update|--datetime", cx);
                                                }
                                            });
                                        })
                                        .child(t(language, "opt.datetime"))
                                })
                                .child({
                                    let entity = entity.clone();
                                    let entity_click = entity.clone();
                                    let entity_hover = entity.clone();
                                    let active = self.command_state.xmp.tag_type.as_deref() == Some("species");
                                    div()
                                        .id("xmp-tag-species")
                                        .bg(rgb(if active { 0x111827 } else { 0xF3F4F6 }))
                                        .text_color(rgb(if active { 0xF9FAFB } else { 0x111827 }))
                                        .p(px(8.0))
                                        .cursor_pointer()
                                        .on_click(move |_, _, cx| {
                                            entity_click.update(cx, |view, cx| {
                                                view.command_state.xmp.tag_type = Some("species".to_string());
                                                view.command_state.xmp.datetime = false;
                                                cx.notify();
                                            });
                                        })
                                        .on_hover(move |hovered, _, cx| {
                                            entity_hover.update(cx, |view, cx| {
                                                if *hovered {
                                                    view.set_hover_help_key("xmp-update|species", cx);
                                                } else {
                                                    view.clear_hover_help_key("xmp-update|species", cx);
                                                }
                                            });
                                        })
                                        .child(t(language, "opt.species"))
                                })
                                .child({
                                    let entity = entity.clone();
                                    let entity_click = entity.clone();
                                    let entity_hover = entity.clone();
                                    let active = self.command_state.xmp.tag_type.as_deref() == Some("individual");
                                    div().id("xmp-tag-individual")
                                        .bg(rgb(if active { 0x111827 } else { 0xF3F4F6 }))
                                        .text_color(rgb(if active { 0xF9FAFB } else { 0x111827 }))
                                        .p(px(8.0)).cursor_pointer()
                                        .on_click(move |_, _, cx| {
                                            entity_click.update(cx, |view, cx| {
                                                view.command_state.xmp.tag_type = Some("individual".to_string());
                                                view.command_state.xmp.datetime = false;
                                                cx.notify();
                                            });
                                        })
                                        .on_hover(move |hovered, _, cx| {
                                            entity_hover.update(cx, |view, cx| {
                                                if *hovered {
                                                    view.set_hover_help_key("xmp-update|individual", cx);
                                                } else {
                                                    view.clear_hover_help_key("xmp-update|individual", cx);
                                                }
                                            });
                                        })
                                        .child(t(language, "opt.individual"))
                                })
                                .child({
                                    let entity = entity.clone();
                                    let entity_click = entity.clone();
                                    let entity_hover = entity.clone();
                                    let active = self.command_state.xmp.tag_type.as_deref() == Some("count");
                                    div().id("xmp-tag-count")
                                        .bg(rgb(if active { 0x111827 } else { 0xF3F4F6 }))
                                        .text_color(rgb(if active { 0xF9FAFB } else { 0x111827 }))
                                        .p(px(8.0)).cursor_pointer()
                                        .on_click(move |_, _, cx| {
                                            entity_click.update(cx, |view, cx| {
                                                view.command_state.xmp.tag_type = Some("count".to_string());
                                                view.command_state.xmp.datetime = false;
                                                cx.notify();
                                            });
                                        })
                                        .on_hover(move |hovered, _, cx| {
                                            entity_hover.update(cx, |view, cx| {
                                                if *hovered {
                                                    view.set_hover_help_key("xmp-update|count", cx);
                                                } else {
                                                    view.clear_hover_help_key("xmp-update|count", cx);
                                                }
                                            });
                                        })
                                        .child(t(language, "opt.count"))
                                })
                                .child({
                                    let entity = entity.clone();
                                    let entity_click = entity.clone();
                                    let entity_hover = entity.clone();
                                    let active = self.command_state.xmp.tag_type.as_deref() == Some("sex");
                                    div().id("xmp-tag-sex")
                                        .bg(rgb(if active { 0x111827 } else { 0xF3F4F6 }))
                                        .text_color(rgb(if active { 0xF9FAFB } else { 0x111827 }))
                                        .p(px(8.0)).cursor_pointer()
                                        .on_click(move |_, _, cx| {
                                            entity_click.update(cx, |view, cx| {
                                                view.command_state.xmp.tag_type = Some("sex".to_string());
                                                view.command_state.xmp.datetime = false;
                                                cx.notify();
                                            });
                                        })
                                        .on_hover(move |hovered, _, cx| {
                                            entity_hover.update(cx, |view, cx| {
                                                if *hovered {
                                                    view.set_hover_help_key("xmp-update|sex", cx);
                                                } else {
                                                    view.clear_hover_help_key("xmp-update|sex", cx);
                                                }
                                            });
                                        })
                                        .child(t(language, "opt.sex"))
                                })
                                .child({
                                    let entity = entity.clone();
                                    let entity_click = entity.clone();
                                    let entity_hover = entity.clone();
                                    let active = self.command_state.xmp.tag_type.as_deref() == Some("bodypart");
                                    div().id("xmp-tag-bodypart")
                                        .bg(rgb(if active { 0x111827 } else { 0xF3F4F6 }))
                                        .text_color(rgb(if active { 0xF9FAFB } else { 0x111827 }))
                                        .p(px(8.0)).cursor_pointer()
                                        .on_click(move |_, _, cx| {
                                            entity_click.update(cx, |view, cx| {
                                                view.command_state.xmp.tag_type = Some("bodypart".to_string());
                                                view.command_state.xmp.datetime = false;
                                                cx.notify();
                                            });
                                        })
                                        .on_hover(move |hovered, _, cx| {
                                            entity_hover.update(cx, |view, cx| {
                                                if *hovered {
                                                    view.set_hover_help_key("xmp-update|bodypart", cx);
                                                } else {
                                                    view.clear_hover_help_key("xmp-update|bodypart", cx);
                                                }
                                            });
                                        })
                                        .child(t(language, "opt.bodypart"))
                                }),
                        )
                } else {
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .child(div().text_color(rgb(0x6B7280)).child(t(language, "label.dir_optional")))
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .gap(px(8.0))
                                .child(div().flex_grow().child(xmp_dir_input.clone()))
                                .child({
                                    let entity = entity.clone();
                                    div()
                                        .id("browse-xmp-dir")
                                        .bg(rgb(0xF3F4F6))
                                        .text_color(rgb(0x111827))
                                        .p(px(8.0))
                                        .cursor_pointer()
                                        .on_click(move |_, window, cx| {
                                            browse_into_text_input(
                                                window,
                                                cx,
                                                PathPromptOptions {
                                                    files: false,
                                                    directories: true,
                                                    multiple: false,
                                                    prompt: Some(
                                                        t(language, "prompt.select_directory").into(),
                                                    ),
                                                },
                                                xmp_dir_input.clone(),
                                                entity.clone(),
                                                language,
                                            );
                                        })
                                        .child(t(language, "action.browse"))
                                }),
                        )
                        .child(div().text_color(rgb(0x6B7280)).child(t(language, "label.csv_path_optional")))
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .gap(px(8.0))
                                .child(div().flex_grow().child(xmp_csv_input.clone()))
                                .child({
                                    let entity = entity.clone();
                                    div()
                                        .id("browse-xmp-sync-csv")
                                        .bg(rgb(0xF3F4F6))
                                        .text_color(rgb(0x111827))
                                        .p(px(8.0))
                                        .cursor_pointer()
                                        .on_click(move |_, window, cx| {
                                            browse_into_text_input(
                                                window,
                                                cx,
                                                PathPromptOptions {
                                                    files: true,
                                                    directories: false,
                                                    multiple: false,
                                                    prompt: Some(
                                                        t(language, "prompt.select_csv_path").into(),
                                                    ),
                                                },
                                                xmp_csv_input.clone(),
                                                entity.clone(),
                                                language,
                                            );
                                        })
                                        .child(t(language, "action.browse"))
                                }),
                        )
                })
        } else if extract_selected {
            div()
                .flex()
                .flex_col()
                .gap(px(12.0))
                .child(div().text_color(rgb(0x6B7280)).child(t(language, "label.csv_path")))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(8.0))
                        .child(div().flex_grow().child(extract_csv_input.clone()))
                        .child({
                            let entity = entity.clone();
                            div()
                                .id("browse-extract-csv")
                                .bg(rgb(0xF3F4F6))
                                .text_color(rgb(0x111827))
                                .p(px(8.0))
                                .cursor_pointer()
                                .on_click(move |_, window, cx| {
                                    browse_into_text_input(
                                        window,
                                        cx,
                                        PathPromptOptions {
                                            files: true,
                                            directories: false,
                                            multiple: false,
                                            prompt: Some(t(language, "prompt.select_tags_csv").into()),
                                        },
                                        extract_csv_input.clone(),
                                        entity.clone(),
                                        language,
                                    );
                                })
                                .child(t(language, "action.browse"))
                        }),
                )
                .child(div().text_color(rgb(0x6B7280)).child(t(language, "label.filter_type")))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .gap(px(8.0))
                        .child({
                            let entity = entity.clone();
                            let active = self.command_state.extract.filter_type == "species";
                            div()
                                .id("extract-filter-species")
                                .bg(rgb(if active { 0x111827 } else { 0xF3F4F6 }))
                                .text_color(rgb(if active { 0xF9FAFB } else { 0x111827 }))
                                .p(px(8.0))
                                .cursor_pointer()
                                .on_click(move |_, _, cx| {
                                    entity.update(cx, |view, cx| {
                                        view.command_state.extract.filter_type =
                                            "species".to_string();
                                        cx.notify();
                                    });
                                })
                                .child(t(language, "opt.species"))
                        })
                        .child({
                            let entity = entity.clone();
                            let active = self.command_state.extract.filter_type == "path";
                            div()
                                .id("extract-filter-path")
                                .bg(rgb(if active { 0x111827 } else { 0xF3F4F6 }))
                                .text_color(rgb(if active { 0xF9FAFB } else { 0x111827 }))
                                .p(px(8.0))
                                .cursor_pointer()
                                .on_click(move |_, _, cx| {
                                    entity.update(cx, |view, cx| {
                                        view.command_state.extract.filter_type =
                                            "path".to_string();
                                        cx.notify();
                                    });
                                })
                                .child(t(language, "opt.path"))
                        })
                        .child({
                            let entity = entity.clone();
                            let active = self.command_state.extract.filter_type == "individual";
                            div()
                                .id("extract-filter-individual")
                                .bg(rgb(if active { 0x111827 } else { 0xF3F4F6 }))
                                .text_color(rgb(if active { 0xF9FAFB } else { 0x111827 }))
                                .p(px(8.0))
                                .cursor_pointer()
                                .on_click(move |_, _, cx| {
                                    entity.update(cx, |view, cx| {
                                        view.command_state.extract.filter_type =
                                            "individual".to_string();
                                        cx.notify();
                                    });
                                })
                                .child(t(language, "opt.individual"))
                        })
                        .child({
                            let entity = entity.clone();
                            let active = self.command_state.extract.filter_type == "rating";
                            div()
                                .id("extract-filter-rating")
                                .bg(rgb(if active { 0x111827 } else { 0xF3F4F6 }))
                                .text_color(rgb(if active { 0xF9FAFB } else { 0x111827 }))
                                .p(px(8.0))
                                .cursor_pointer()
                                .on_click(move |_, _, cx| {
                                    entity.update(cx, |view, cx| {
                                        view.command_state.extract.filter_type =
                                            "rating".to_string();
                                        cx.notify();
                                    });
                                })
                                .child(t(language, "opt.rating"))
                        })
                        .child({
                            let entity = entity.clone();
                            let active = self.command_state.extract.filter_type == "event";
                            div()
                                .id("extract-filter-event")
                                .bg(rgb(if active { 0x111827 } else { 0xF3F4F6 }))
                                .text_color(rgb(if active { 0xF9FAFB } else { 0x111827 }))
                                .p(px(8.0))
                                .cursor_pointer()
                                .on_click(move |_, _, cx| {
                                    entity.update(cx, |view, cx| {
                                        view.command_state.extract.filter_type =
                                            "event".to_string();
                                        cx.notify();
                                    });
                                })
                                .child(t(language, "opt.event"))
                        })
                        .child({
                            let entity = entity.clone();
                            let active = self.command_state.extract.filter_type == "custom";
                            div()
                                .id("extract-filter-custom")
                                .bg(rgb(if active { 0x111827 } else { 0xF3F4F6 }))
                                .text_color(rgb(if active { 0xF9FAFB } else { 0x111827 }))
                                .p(px(8.0))
                                .cursor_pointer()
                                .on_click(move |_, _, cx| {
                                    entity.update(cx, |view, cx| {
                                        view.command_state.extract.filter_type =
                                            "custom".to_string();
                                        cx.notify();
                                    });
                                })
                                .child(t(language, "opt.custom"))
                        })
                        .child({
                            let entity = entity.clone();
                            let active = self.command_state.extract.filter_type == "advanced";
                            div()
                                .id("extract-filter-advanced")
                                .bg(rgb(if active { 0x111827 } else { 0xF3F4F6 }))
                                .text_color(rgb(if active { 0xF9FAFB } else { 0x111827 }))
                                .p(px(8.0))
                                .cursor_pointer()
                                .on_click(move |_, _, cx| {
                                    entity.update(cx, |view, cx| {
                                        view.command_state.extract.filter_type =
                                            "advanced".to_string();
                                        cx.notify();
                                    });
                                })
                                .child(t(language, "opt.advanced"))
                        }),
                )
                .child(div().text_color(rgb(0x6B7280)).child(t(language, "label.value")))
                .child(div().flex().flex_row().gap(px(8.0)).child(div().flex_grow().child(extract_value_input.clone())))
                .child(div().text_color(rgb(0x6B7280)).child(t(language, "label.output_dir")))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(8.0))
                        .child(div().flex_grow().child(extract_output_input.clone()))
                        .child({
                            let entity = entity.clone();
                            div()
                                .id("browse-extract-output")
                                .bg(rgb(0xF3F4F6))
                                .text_color(rgb(0x111827))
                                .p(px(8.0))
                                .cursor_pointer()
                                .on_click(move |_, window, cx| {
                                    browse_into_text_input(
                                        window,
                                        cx,
                                        PathPromptOptions {
                                            files: false,
                                            directories: true,
                                            multiple: false,
                                            prompt: Some(
                                                t(language, "prompt.select_output_directory").into(),
                                            ),
                                        },
                                        extract_output_input.clone(),
                                        entity.clone(),
                                        language,
                                    );
                                })
                                .child(t(language, "action.browse"))
                        }),
                )
                .child(div().text_color(rgb(0x6B7280)).child(t(language, "label.options")))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .gap(px(8.0))
                        .child({
                            let entity = entity.clone();
                            let entity_click = entity.clone();
                            let entity_hover = entity.clone();
                            let active = self.command_state.extract.rename;
                            div().id("extract-rename")
                                .bg(rgb(if active { 0x111827 } else { 0xF3F4F6 }))
                                .text_color(rgb(if active { 0xF9FAFB } else { 0x111827 }))
                                .p(px(8.0))
                                .cursor_pointer()
                                .on_click(move |_, _, cx| {
                                    entity_click.update(cx, |view, cx| {
                                        view.command_state.extract.rename =
                                            !view.command_state.extract.rename;
                                        cx.notify();
                                    });
                                })
                                .on_hover(move |hovered, _, cx| {
                                    entity_hover.update(cx, |view, cx| {
                                        if *hovered {
                                            view.set_hover_help_key("extract|--rename", cx);
                                        } else {
                                            view.clear_hover_help_key("extract|--rename", cx);
                                        }
                                    });
                                })
                                .child(t(language, "opt.rename"))
                        })
                        .child({
                            let entity = entity.clone();
                            let entity_click = entity.clone();
                            let entity_hover = entity.clone();
                            let active = self.command_state.extract.skip_existing;
                            div().id("extract-skip-existing")
                                .bg(rgb(if active { 0x111827 } else { 0xF3F4F6 }))
                                .text_color(rgb(if active { 0xF9FAFB } else { 0x111827 }))
                                .p(px(8.0))
                                .cursor_pointer()
                                .on_click(move |_, _, cx| {
                                    entity_click.update(cx, |view, cx| {
                                        view.command_state.extract.skip_existing =
                                            !view.command_state.extract.skip_existing;
                                        cx.notify();
                                    });
                                })
                                .on_hover(move |hovered, _, cx| {
                                    entity_hover.update(cx, |view, cx| {
                                        if *hovered {
                                            view.set_hover_help_key("extract|--skip-existing", cx);
                                        } else {
                                            view.clear_hover_help_key("extract|--skip-existing", cx);
                                        }
                                    });
                                })
                                .child(t(language, "opt.skip_existing"))
                        })
                        .child({
                            let entity = entity.clone();
                            let entity_click = entity.clone();
                            let entity_hover = entity.clone();
                            let active = self.command_state.extract.use_subdir;
                            div().id("extract-use-subdir")
                                .bg(rgb(if active { 0x111827 } else { 0xF3F4F6 }))
                                .text_color(rgb(if active { 0xF9FAFB } else { 0x111827 }))
                                .p(px(8.0))
                                .cursor_pointer()
                                .on_click(move |_, _, cx| {
                                    entity_click.update(cx, |view, cx| {
                                        view.command_state.extract.use_subdir =
                                            !view.command_state.extract.use_subdir;
                                        cx.notify();
                                    });
                                })
                                .on_hover(move |hovered, _, cx| {
                                    entity_hover.update(cx, |view, cx| {
                                        if *hovered {
                                            view.set_hover_help_key("extract|--use-subdir", cx);
                                        } else {
                                            view.clear_hover_help_key("extract|--use-subdir", cx);
                                        }
                                    });
                                })
                                .child(t(language, "opt.use_subdir"))
                        }),
                )
                .child(div().text_color(rgb(0x6B7280)).child(t(language, "label.subdir_type")))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .gap(px(8.0))
                        .child({
                            let entity = entity.clone();
                            let active = self.command_state.extract.subdir_type.as_deref() == Some("species");
                            div().id("extract-subdir-species")
                                .bg(rgb(if active { 0x111827 } else { 0xF3F4F6 }))
                                .text_color(rgb(if active { 0xF9FAFB } else { 0x111827 }))
                                .p(px(8.0)).cursor_pointer()
                                .on_click(move |_, _, cx| {
                                    entity.update(cx, |view, cx| {
                                        view.command_state.extract.subdir_type = Some("species".to_string());
                                        cx.notify();
                                    });
                                }).child(t(language, "opt.species"))
                        })
                        .child({
                            let entity = entity.clone();
                            let active = self.command_state.extract.subdir_type.as_deref() == Some("individual");
                            div().id("extract-subdir-individual")
                                .bg(rgb(if active { 0x111827 } else { 0xF3F4F6 }))
                                .text_color(rgb(if active { 0xF9FAFB } else { 0x111827 }))
                                .p(px(8.0)).cursor_pointer()
                                .on_click(move |_, _, cx| {
                                    entity.update(cx, |view, cx| {
                                        view.command_state.extract.subdir_type = Some("individual".to_string());
                                        cx.notify();
                                    });
                                }).child(t(language, "opt.individual"))
                        })
                        .child({
                            let entity = entity.clone();
                            let active = self.command_state.extract.subdir_type.as_deref() == Some("rating");
                            div().id("extract-subdir-rating")
                                .bg(rgb(if active { 0x111827 } else { 0xF3F4F6 }))
                                .text_color(rgb(if active { 0xF9FAFB } else { 0x111827 }))
                                .p(px(8.0)).cursor_pointer()
                                .on_click(move |_, _, cx| {
                                    entity.update(cx, |view, cx| {
                                        view.command_state.extract.subdir_type = Some("rating".to_string());
                                        cx.notify();
                                    });
                                }).child(t(language, "opt.rating"))
                        })
                        .child({
                            let entity = entity.clone();
                            let active = self.command_state.extract.subdir_type.as_deref() == Some("custom");
                            div().id("extract-subdir-custom")
                                .bg(rgb(if active { 0x111827 } else { 0xF3F4F6 }))
                                .text_color(rgb(if active { 0xF9FAFB } else { 0x111827 }))
                                .p(px(8.0)).cursor_pointer()
                                .on_click(move |_, _, cx| {
                                    entity.update(cx, |view, cx| {
                                        view.command_state.extract.subdir_type = Some("custom".to_string());
                                        cx.notify();
                                    });
                                }).child(t(language, "opt.custom"))
                        }),
                )
        } else {
            div()
                .flex()
                .flex_col()
                .gap(px(12.0))
                .child(div().text_color(rgb(0x6B7280)).child(t(language, "label.csv_path")))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(8.0))
                        .child(div().flex_grow().child(translate_csv_input.clone()))
                        .child({
                            let entity = entity.clone();
                            div()
                                .id("browse-translate-csv")
                                .bg(rgb(0xF3F4F6))
                                .text_color(rgb(0x111827))
                                .p(px(8.0))
                                .cursor_pointer()
                                .on_click(move |_, window, cx| {
                                    browse_into_text_input(
                                        window,
                                        cx,
                                        PathPromptOptions {
                                            files: true,
                                            directories: false,
                                            multiple: false,
                                            prompt: Some(t(language, "prompt.select_tags_csv").into()),
                                        },
                                        translate_csv_input.clone(),
                                        entity.clone(),
                                        language,
                                    );
                                })
                                .child(t(language, "action.browse"))
                        }),
                )
                .child(div().text_color(rgb(0x6B7280)).child(t(language, "label.taglist_path")))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(8.0))
                        .child(div().flex_grow().child(translate_taglist_input.clone()))
                        .child({
                            let entity = entity.clone();
                            div()
                                .id("browse-translate-taglist")
                                .bg(rgb(0xF3F4F6))
                                .text_color(rgb(0x111827))
                                .p(px(8.0))
                                .cursor_pointer()
                                .on_click(move |_, window, cx| {
                                    browse_into_text_input(
                                        window,
                                        cx,
                                        PathPromptOptions {
                                            files: true,
                                            directories: false,
                                            multiple: false,
                                            prompt: Some(
                                                t(language, "prompt.select_taglist_csv").into(),
                                            ),
                                        },
                                        translate_taglist_input.clone(),
                                        entity.clone(),
                                        language,
                                    );
                                })
                                .child(t(language, "action.browse"))
                        }),
                )
                .child(div().text_color(rgb(0x6B7280)).child(t(language, "label.from")))
                .child(div().flex().flex_row().gap(px(8.0)).child(div().flex_grow().child(translate_from_input.clone())))
                .child(div().text_color(rgb(0x6B7280)).child(t(language, "label.to")))
                .child(div().flex().flex_row().gap(px(8.0)).child(div().flex_grow().child(translate_to_input.clone())))
                .child(div().text_color(rgb(0x6B7280)).child(t(language, "label.output_dir")))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(8.0))
                        .child(div().flex_grow().child(translate_output_input.clone()))
                        .child({
                            let entity = entity.clone();
                            div()
                                .id("browse-translate-output")
                                .bg(rgb(0xF3F4F6))
                                .text_color(rgb(0x111827))
                                .p(px(8.0))
                                .cursor_pointer()
                                .on_click(move |_, window, cx| {
                                    browse_into_text_input(
                                        window,
                                        cx,
                                        PathPromptOptions {
                                            files: false,
                                            directories: true,
                                            multiple: false,
                                            prompt: Some(
                                                t(language, "prompt.select_output_directory").into(),
                                            ),
                                        },
                                        translate_output_input.clone(),
                                        entity.clone(),
                                        language,
                                    );
                                })
                                .child(t(language, "action.browse"))
                        }),
                )
        };

        div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(24.0))
            .bg(rgb(0xF7F4EF))
            .text_color(rgb(0x1E1E1E))
            .on_mouse_move({
                let entity = entity.clone();
                move |event: &MouseMoveEvent, _, cx| {
                    entity.update(cx, |view, cx| {
                        view.cursor_position = event.position;
                        if view.hover_help_key.is_some() {
                            view.option_help_position = Some(event.position);
                            cx.notify();
                        }
                    });
                }
            })
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(div().child(t(language, "app.title")))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(0x6B7280))
                                    .child(format!(
                                        "{}: {active_binary}",
                                        t(language, "app.active_binary")
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap(px(8.0))
                            .child({
                                let entity_open_input = entity.clone();
                                div()
                                    .id("open-input-dir")
                                    .bg(rgb(0xF3F4F6))
                                    .text_color(rgb(0x111827))
                                    .p(px(8.0))
                                    .cursor_pointer()
                                    .on_click(move |_, _, cx| {
                                        entity_open_input.update(cx, |view, cx| {
                                            if let Some(path) = view.resolve_input_dir() {
                                                cx.reveal_path(&path);
                                            } else {
                                                view.append_output(
                                                    t(language, "message.no_input_dir"),
                                                    cx,
                                                );
                                            }
                                        });
                                    })
                                    .child(t(language, "action.open_input_dir"))
                            })
                            .child({
                                let entity_open_output = entity.clone();
                                div()
                                    .id("open-output-dir")
                                    .bg(rgb(0xF3F4F6))
                                    .text_color(rgb(0x111827))
                                    .p(px(8.0))
                                    .cursor_pointer()
                                    .on_click(move |_, _, cx| {
                                        entity_open_output.update(cx, |view, cx| {
                                            if let Some(path) = view.resolve_output_dir() {
                                                cx.reveal_path(&path);
                                            } else {
                                                view.append_output(
                                                    t(language, "message.no_output_dir"),
                                                    cx,
                                                );
                                            }
                                        });
                                    })
                                    .child(t(language, "action.open_output_dir"))
                            })
                            .child({
                                let entity_setup = entity.clone();
                                div()
                                    .id("open-setup")
                                    .bg(rgb(0xF3F4F6))
                                    .text_color(rgb(0x111827))
                                    .p(px(8.0))
                                    .cursor_pointer()
                                    .on_click(move |_, _window, cx| {
                                        let setup_state = entity_setup.read(cx);
                                        let current =
                                            setup_state.serval_binary_path.clone().unwrap_or_default();
                                        let language = setup_state.language;
                                        let root_for_window = entity_setup.clone();
                                        let _ = cx.open_window(
                                            WindowOptions::default(),
                                            move |_, app| {
                                                let root = root_for_window.clone();
                                                let initial = current.clone();
                                                let placeholder = t(
                                                    language,
                                                    "setup.serval_binary_placeholder",
                                                )
                                                .to_string();
                                                app.new(move |cx| {
                                                    let input = cx.new(|cx| {
                                                        TextInput::new_with_value(
                                                            cx,
                                                            placeholder.clone(),
                                                            initial.clone(),
                                                        )
                                                    });
                                                    SetupView {
                                                        root: root.clone(),
                                                        serval_binary_input: input,
                                                    }
                                                })
                                            },
                                        );
                                    })
                                    .child(t(language, "action.setup"))
                            })
                            .child({
                                let entity_helper = entity.clone();
                                div()
                                    .id("toggle-helper-mode")
                                    .bg(rgb(if helper_mode { 0x111827 } else { 0xF3F4F6 }))
                                    .text_color(rgb(if helper_mode { 0xF9FAFB } else { 0x111827 }))
                                    .p(px(8.0))
                                    .cursor_pointer()
                                    .on_click(move |_, _, cx| {
                                        entity_helper.update(cx, |view, cx| {
                                            view.helper_mode = !view.helper_mode;
                                            if view.helper_mode {
                                                let key = view.current_help_key();
                                                view.command_help_open = true;
                                                view.command_help_key = Some(key.clone());
                                                view.ensure_help_loaded_for_key(key, cx);
                                            } else {
                                                view.command_help_open = false;
                                                view.command_help_key = None;
                                                view.hover_help_key = None;
                                                view.option_help_position = None;
                                                cx.notify();
                                            }
                                        });
                                    })
                                    .child(if helper_mode {
                                        t(language, "action.helper_mode_on")
                                    } else {
                                        t(language, "action.helper_mode_off")
                                    })
                            })
                            .child({
                                let entity_run = entity.clone();
                                div()
                                    .id("run-button")
                                    .bg(rgb(0x111827))
                                    .text_color(rgb(0xF9FAFB))
                                    .p(px(8.0))
                                    .cursor_pointer()
                                    .on_click(move |_, window, cx| {
                                let entity = entity_run.clone();
                                let (program, args, use_pty) = match entity_run.update(cx, |view, cx| {
                                    if view.running {
                                        view.append_output(t(language, "message.already_running"), cx);
                                        return None;
                                    }

                                    match view.command_state.build_command() {
                                        Ok(command) => {
                                            let executable = view.executable_program();
                                            let display = format!(
                                                "$ {} {}",
                                                executable,
                                                command.1.join(" ")
                                            );
                                            view.append_output(display, cx);
                                            view.running = true;
                                            let use_pty = matches!(view.command_state.kind, CommandKind::Capture);
                                            if use_pty {
                                                view.reset_capture_helper(cx);
                                            }
                                            Some((executable, command.1, use_pty))
                                        }
                                        Err(message) => {
                                            view.append_output(message, cx);
                                            None
                                        }
                                    }
                                }) {
                                    Some(command) => command,
                                    None => return,
                                };

                                let (tx, rx) = mpsc::channel::<OutputEvent>();

                                if use_pty {
                                    thread::spawn(move || {
                                        let pty_system = native_pty_system();
                                        let pair = match pty_system.openpty(PtySize {
                                            rows: 24,
                                            cols: 120,
                                            pixel_width: 0,
                                            pixel_height: 0,
                                        }) {
                                            Ok(pair) => pair,
                                            Err(err) => {
                                                let _ = tx.send(OutputEvent::Error(format!(
                                                    "{}: {err}",
                                                    t(language, "message.failed_open_pty")
                                                )));
                                                let _ = tx.send(OutputEvent::Exit(1));
                                                return;
                                            }
                                        };

                                        let mut cmd = CommandBuilder::new(&program);
                                        cmd.args(&args);

                                        let mut child = match pair.slave.spawn_command(cmd) {
                                            Ok(child) => child,
                                            Err(err) => {
                                                let _ = tx.send(OutputEvent::Error(format!(
                                                    "{}: {err}",
                                                    t(language, "message.failed_start_pty")
                                                )));
                                                let _ = tx.send(OutputEvent::Exit(1));
                                                return;
                                            }
                                        };
                                        drop(pair.slave);

                                        if let Ok(writer) = pair.master.take_writer() {
                                            let _ = tx.send(OutputEvent::PtyWriter(Arc::new(
                                                Mutex::new(writer),
                                            )));
                                        }

                                        let mut reader = match pair.master.try_clone_reader() {
                                            Ok(reader) => reader,
                                            Err(err) => {
                                                let _ = tx.send(OutputEvent::Error(format!(
                                                    "{}: {err}",
                                                    t(language, "message.failed_read_pty_output")
                                                )));
                                                let _ = tx.send(OutputEvent::Exit(1));
                                                return;
                                            }
                                        };

                                        let mut buf = [0u8; 8192];
                                        loop {
                                            match reader.read(&mut buf) {
                                                Ok(0) => break,
                                                Ok(n) => {
                                                    let text = String::from_utf8_lossy(&buf[..n])
                                                        .to_string();
                                                    let _ = tx.send(OutputEvent::Chunk(text));
                                                }
                                                Err(err) => {
                                                    let _ = tx.send(OutputEvent::Error(format!(
                                                        "{}: {err}",
                                                        t(language, "message.pty_read_error")
                                                    )));
                                                    break;
                                                }
                                            }
                                        }

                                        let status = match child.wait() {
                                            Ok(status) => status.exit_code() as i32,
                                            Err(_) => 1,
                                        };
                                        let _ = tx.send(OutputEvent::Exit(status));
                                    });
                                } else {
                                    thread::spawn(move || {
                                        let mut child = match Command::new(&program)
                                            .args(&args)
                                            .stdout(Stdio::piped())
                                            .stderr(Stdio::piped())
                                            .spawn()
                                        {
                                            Ok(child) => child,
                                            Err(err) => {
                                                let _ = tx.send(OutputEvent::Error(format!(
                                                    "{}: {err}",
                                                    t(language, "message.failed_start_process")
                                                )));
                                                let _ = tx.send(OutputEvent::Exit(1));
                                                return;
                                            }
                                        };

                                        let stdout = child.stdout.take();
                                        let stderr = child.stderr.take();

                                        if let Some(stdout) = stdout {
                                            let tx_out = tx.clone();
                                            thread::spawn(move || {
                                                let reader = BufReader::new(stdout);
                                                for line in reader.lines() {
                                                    match line {
                                                        Ok(line) => {
                                                            let _ = tx_out
                                                                .send(OutputEvent::Line(line));
                                                        }
                                                        Err(err) => {
                                                            let _ = tx_out.send(
                                                                OutputEvent::Error(format!(
                                                                    "{}: {err}",
                                                                    t(language, "message.stdout_error")
                                                                )),
                                                            );
                                                            break;
                                                        }
                                                    }
                                                }
                                            });
                                        }

                                        if let Some(stderr) = stderr {
                                            let tx_err = tx.clone();
                                            thread::spawn(move || {
                                                let reader = BufReader::new(stderr);
                                                for line in reader.lines() {
                                                    match line {
                                                        Ok(line) => {
                                                            let _ = tx_err
                                                                .send(OutputEvent::Line(line));
                                                        }
                                                        Err(err) => {
                                                            let _ = tx_err.send(
                                                                OutputEvent::Error(format!(
                                                                    "{}: {err}",
                                                                    t(language, "message.stderr_error")
                                                                )),
                                                            );
                                                            break;
                                                        }
                                                    }
                                                }
                                            });
                                        }

                                        let status = match child.wait() {
                                            Ok(status) => status.code().unwrap_or(1),
                                            Err(_) => 1,
                                        };
                                        let _ = tx.send(OutputEvent::Exit(status));
                                    });
                                }

                                window
                                    .spawn(cx, async move |cx| {
                                        let mut finished = false;
                                        while !finished {
                                            while let Ok(event) = rx.try_recv() {
                                                let entity = entity.clone();
                                                match event {
                                                    OutputEvent::Chunk(chunk) => {
                                                        let _ = cx.update(|_window, app| {
                                                            entity.update(app, |view, cx| {
                                                                if view.command_state.kind
                                                                    == CommandKind::Capture
                                                                {
                                                                    view.observe_capture_helper_output(
                                                                        &chunk, cx,
                                                                    );
                                                                }
                                                                view.append_output_chunk(chunk, cx);
                                                            })
                                                        });
                                                    }
                                                    OutputEvent::Line(line) => {
                                                        let _ = cx.update(|_window, app| {
                                                            entity.update(app, |view, cx| {
                                                                view.append_output(line, cx);
                                                            })
                                                        });
                                                    }
                                                    OutputEvent::Error(message) => {
                                                        let _ = cx.update(|_window, app| {
                                                            entity.update(app, |view, cx| {
                                                                view.append_output(message, cx);
                                                            })
                                                        });
                                                    }
                                                    OutputEvent::PtyWriter(writer) => {
                                                        let _ = cx.update(|_window, app| {
                                                            entity.update(app, |view, cx| {
                                                                view.set_pty_writer(Some(writer), cx);
                                                            })
                                                        });
                                                    }
                                                    OutputEvent::Exit(code) => {
                                                        let _ = cx.update(|_window, app| {
                                                            entity.update(app, |view, cx| {
                                                                view.append_output(
                                                                    format!(
                                                                        "{}: {code}",
                                                                        t(language, "message.process_exited")
                                                                    ),
                                                                    cx,
                                                                );
                                                                view.running = false;
                                                                view.set_pty_writer(None, cx);
                                                                view.reset_capture_helper(cx);
                                                            })
                                                        });
                                                        finished = true;
                                                    }
                                                }
                                            }
                                            Timer::after(Duration::from_millis(50)).await;
                                        }
                                    })
                                    .detach();
                            })
                                    .child(t(language, "action.run"))
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .bg(rgb(0xFFFFFF))
                    .p(px(16.0))
                    .child({
                        let entity = entity.clone();
                        div()
                            .id("toggle-command-panel")
                            .flex()
                            .flex_row()
                            .justify_between()
                            .items_center()
                            .cursor_pointer()
                            .on_click(move |_, _, cx| {
                                entity.update(cx, |view, cx| {
                                    view.command_panel_open = !view.command_panel_open;
                                    cx.notify();
                                });
                            })
                            .child(div().text_color(rgb(0x6B7280)).child(t(language, "panel.command")))
                            .child(div().text_color(rgb(0x6B7280)).child(if command_panel_open {
                                t(language, "action.collapse")
                            } else {
                                t(language, "action.expand")
                            }))
                    })
                    .child(if command_panel_open {
                        div()
                            .flex()
                            .flex_row()
                            .gap(px(8.0))
                            .child({
                                let entity_click = entity_observe.clone();
                                div()
                                    .id("command-observe")
                                    .bg(rgb(if observe_selected { 0x111827 } else { 0xF3F4F6 }))
                                    .text_color(rgb(if observe_selected { 0xF9FAFB } else { 0x111827 }))
                                    .p(px(8.0))
                                    .cursor_pointer()
                                    .on_click(move |_, _, cx| {
                                        entity_click.update(cx, |view, cx| {
                                            view.select_command(CommandKind::Observe, cx);
                                        });
                                    })
                                    .child(t(language, "cmd.observe"))
                            })
                            .child({
                                let entity_click = entity_capture.clone();
                                div()
                                    .id("command-capture")
                                    .bg(rgb(if capture_selected { 0x111827 } else { 0xF3F4F6 }))
                                    .text_color(rgb(if capture_selected { 0xF9FAFB } else { 0x111827 }))
                                    .p(px(8.0))
                                    .cursor_pointer()
                                    .on_click(move |_, _, cx| {
                                        entity_click.update(cx, |view, cx| {
                                            view.select_command(CommandKind::Capture, cx);
                                        });
                                    })
                                    .child(t(language, "cmd.capture"))
                            })
                            .child({
                                let entity_click = entity_xmp.clone();
                                div()
                                    .id("command-xmp")
                                    .bg(rgb(if xmp_selected { 0x111827 } else { 0xF3F4F6 }))
                                    .text_color(rgb(if xmp_selected { 0xF9FAFB } else { 0x111827 }))
                                    .p(px(8.0))
                                    .cursor_pointer()
                                    .on_click(move |_, _, cx| {
                                        entity_click.update(cx, |view, cx| {
                                            view.select_command(CommandKind::Xmp, cx);
                                        });
                                    })
                                    .child(t(language, "cmd.xmp"))
                            })
                            .child({
                                let entity_click = entity_extract.clone();
                                div()
                                    .id("command-extract")
                                    .bg(rgb(if extract_selected { 0x111827 } else { 0xF3F4F6 }))
                                    .text_color(rgb(if extract_selected { 0xF9FAFB } else { 0x111827 }))
                                    .p(px(8.0))
                                    .cursor_pointer()
                                    .on_click(move |_, _, cx| {
                                        entity_click.update(cx, |view, cx| {
                                            view.select_command(CommandKind::Extract, cx);
                                        });
                                    })
                                    .child(t(language, "cmd.extract"))
                            })
                            .child({
                                let entity_click = entity_translate.clone();
                                div()
                                    .id("command-translate")
                                    .bg(rgb(if translate_selected { 0x111827 } else { 0xF3F4F6 }))
                                    .text_color(rgb(if translate_selected { 0xF9FAFB } else { 0x111827 }))
                                    .p(px(8.0))
                                    .cursor_pointer()
                                    .on_click(move |_, _, cx| {
                                        entity_click.update(cx, |view, cx| {
                                            view.select_command(CommandKind::Translate, cx);
                                        });
                                    })
                                    .child(t(language, "cmd.translate"))
                            })
                    } else {
                        div()
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .bg(rgb(0xFFFFFF))
                    .p(px(16.0))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .justify_between()
                            .items_center()
                            .child(div().text_color(rgb(0x6B7280)).child(t(language, "panel.inputs")))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child({
                                        let entity = entity.clone();
                                        div()
                                            .id("toggle-input-panel")
                                            .text_color(rgb(0x6B7280))
                                            .cursor_pointer()
                                            .on_click(move |_, _, cx| {
                                                entity.update(cx, |view, cx| {
                                                    view.input_panel_open = !view.input_panel_open;
                                                    cx.notify();
                                                });
                                            })
                                            .child(if input_panel_open {
                                                t(language, "action.collapse")
                                            } else {
                                                t(language, "action.expand")
                                            })
                                    }),
                            ),
                    )
                    .child(if input_panel_open { input_section } else { div() }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .bg(rgb(0xFFFFFF))
                    .p(px(16.0))
                    .child({
                        let entity = entity.clone();
                        div()
                            .id("toggle-preview-panel")
                            .flex()
                            .flex_row()
                            .justify_between()
                            .items_center()
                            .cursor_pointer()
                            .on_click(move |_, _, cx| {
                                entity.update(cx, |view, cx| {
                                    view.preview_panel_open = !view.preview_panel_open;
                                    cx.notify();
                                });
                            })
                            .child(div().text_color(rgb(0x6B7280)).child(t(language, "panel.command_preview")))
                            .child(div().text_color(rgb(0x6B7280)).child(if preview_panel_open {
                                t(language, "action.collapse")
                            } else {
                                t(language, "action.expand")
                            }))
                    })
                    .child(if preview_panel_open {
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .justify_end()
                                    .child({
                                        let entity = entity.clone();
                                        div()
                                            .id("copy-command")
                                            .bg(rgb(0xF3F4F6))
                                            .text_color(rgb(0x111827))
                                            .p(px(8.0))
                                            .cursor_pointer()
                                            .on_click(move |_, _, cx| {
                                                entity.update(cx, |view, cx| {
                                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                                        view.command_preview(),
                                                    ));
                                                });
                                            })
                                            .child(t(language, "action.copy_command"))
                                    }),
                            )
                            .child(
                                div()
                                    .bg(rgb(0xF3F4F6))
                                    .text_color(rgb(0x111827))
                                    .p(px(8.0))
                                    .child(preview),
                            )
                    } else {
                        div()
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .bg(rgb(0x0B0F1A))
                    .text_color(rgb(0xE5E7EB))
                    .p(px(16.0))
                    .flex_grow()
                    .min_h(px(220.0))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .justify_between()
                            .items_center()
                            .child(div().text_color(rgb(0x9CA3AF)).child(t(language, "panel.output")))
                            .child({
                                let entity = entity.clone();
                                div()
                                    .id("copy-log")
                                    .bg(rgb(0x111827))
                                    .text_color(rgb(0xF9FAFB))
                                    .p(px(8.0))
                                    .cursor_pointer()
                                    .on_click(move |_, _, cx| {
                                        entity.update(cx, |view, cx| {
                                            cx.write_to_clipboard(ClipboardItem::new_string(
                                                view.output_log.clone(),
                                            ));
                                        });
                                    })
                                    .child(t(language, "action.copy_log"))
                            }),
                    )
                    .child(
                        div()
                            .font_family("monospace")
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .whitespace_nowrap()
                            .id("output-scroll")
                            .track_scroll(&self.output_scroll_handle)
                            .flex_grow()
                            .w_full()
                            .h_full()
                            .overflow_y_scroll()
                            .overflow_x_scroll()
                            .scrollbar_width(px(8.0))
                            .child(output_text),
                    )
                    .child(capture_helper_panel)
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap(px(8.0))
                            .pt(px(8.0))
                            .child(div().flex_grow().child(pty_input.clone()))
                            .child({
                                let entity = entity.clone();
                                div()
                                    .id("send-pty")
                                    .bg(rgb(0x111827))
                                    .text_color(rgb(0xF9FAFB))
                                    .p(px(8.0))
                                    .cursor_pointer()
                                    .on_click(move |_, _, cx| {
                                        entity.update(cx, |view, cx| {
                                            view.send_pty_input(cx);
                                        });
                                    })
                                    .child(t(language, "action.send"))
                            }),
                    ),
            )
            .child(if helper_mode && self.command_help_open {
                {
                    let base = div()
                        .absolute()
                        .w(px(500.0))
                        .h(px(360.0))
                        .flex()
                        .flex_col()
                        .bg(rgb(0x111827))
                        .opacity(0.94)
                        .text_color(rgb(0xF9FAFB))
                        .border_1()
                        .border_color(rgb(0x374151))
                        .rounded_md()
                        .shadow_lg()
                        .p(px(10.0))
                        .text_size(px(12.0))
                        .line_height(px(18.0))
                        .font_family("monospace")
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .justify_between()
                                .items_center()
                                .child(div().text_color(rgb(0x9CA3AF)).child(t(language, "panel.help")))
                                .child({
                                    let entity = entity.clone();
                                    div()
                                        .id("close-help-overlay")
                                        .cursor_pointer()
                                        .text_color(rgb(0xD1D5DB))
                                        .on_click(move |_, _, cx| {
                                            entity.update(cx, |view, cx| {
                                                view.command_help_open = false;
                                                view.command_help_key = None;
                                                view.hover_help_key = None;
                                                view.option_help_position = None;
                                                cx.notify();
                                            });
                                        })
                                        .child("x")
                                }),
                        )
                        .child(
                            div()
                                .pt(px(6.0))
                                .flex_grow()
                                .id("help-overlay-scroll")
                                .overflow_y_scroll()
                                .overflow_x_scroll()
                                .scrollbar_width(px(8.0))
                                .child(command_help_text),
                        );
                    base.top(px(96.0)).right(px(24.0))
                }
            } else {
                div()
            })
            .child(if helper_mode {
                if let (Some(help_text), Some(pos)) = (option_help_text, option_help_position) {
                div()
                    .absolute()
                    .left(pos.x + px(14.0))
                    .top(pos.y + px(14.0))
                    .w(px(360.0))
                    .h(px(170.0))
                    .flex()
                    .flex_col()
                    .bg(rgb(0x111827))
                    .opacity(0.94)
                    .text_color(rgb(0xF9FAFB))
                    .border_1()
                    .border_color(rgb(0x374151))
                    .rounded_md()
                    .shadow_lg()
                    .p(px(8.0))
                    .text_size(px(12.0))
                    .line_height(px(18.0))
                    .font_family("monospace")
                    .child(
                        div()
                            .text_color(rgb(0x9CA3AF))
                            .child(t(language, "panel.option_help")),
                    )
                    .child(
                        div()
                            .pt(px(4.0))
                            .flex_grow()
                            .id("option-help-overlay-scroll")
                            .overflow_y_scroll()
                            .overflow_x_scroll()
                            .scrollbar_width(px(8.0))
                            .child(help_text),
                    )
                } else {
                    div()
                }
            } else {
                div()
            })
    }
}

fn main() {
    Application::new().run(|app| {
        bind_text_input_keys(app);
        app.open_window(WindowOptions::default(), |_window, app| {
            app.new(|cx| {
                let observe_input = cx.new(|cx| TextInput::new(cx, t(Language::En, "placeholder.observe_media_dir")));
                let observe_output_input =
                    cx.new(|cx| TextInput::new(cx, t(Language::En, "placeholder.optional_output_dir")));
                let capture_input = cx.new(|cx| TextInput::new(cx, t(Language::En, "placeholder.tags_csv")));
                let capture_output_input =
                    cx.new(|cx| TextInput::new(cx, t(Language::En, "placeholder.optional_output_dir")));
                let xmp_source_input = cx.new(|cx| TextInput::new(cx, t(Language::En, "placeholder.source_dir")));
                let xmp_output_input = cx.new(|cx| TextInput::new(cx, t(Language::En, "placeholder.output_dir")));
                let xmp_csv_input = cx.new(|cx| TextInput::new(cx, t(Language::En, "placeholder.csv_file")));
                let xmp_dir_input = cx.new(|cx| TextInput::new(cx, t(Language::En, "placeholder.directory")));
                let extract_csv_input = cx.new(|cx| TextInput::new(cx, t(Language::En, "placeholder.tags_csv")));
                let extract_value_input = cx.new(|cx| TextInput::new(cx, t(Language::En, "placeholder.filter_value")));
                let extract_output_input =
                    cx.new(|cx| TextInput::new(cx, t(Language::En, "placeholder.optional_output_dir")));
                let translate_csv_input = cx.new(|cx| TextInput::new(cx, t(Language::En, "placeholder.tags_csv")));
                let translate_taglist_input =
                    cx.new(|cx| TextInput::new(cx, t(Language::En, "placeholder.taglist_csv")));
                let translate_from_input =
                    cx.new(|cx| TextInput::new(cx, t(Language::En, "placeholder.translate_from")));
                let translate_to_input =
                    cx.new(|cx| TextInput::new(cx, t(Language::En, "placeholder.translate_to")));
                let translate_output_input =
                    cx.new(|cx| TextInput::new(cx, t(Language::En, "placeholder.optional_output_dir")));
                let pty_input = cx.new(|cx| TextInput::new_submit(cx, t(Language::En, "placeholder.interactive_input")));

                cx.observe(&observe_input, |view: &mut RootView, input, cx| {
                    view.command_state.observe.media_dir = input.read(cx).value().to_string();
                    cx.notify();
                })
                .detach();

                cx.observe(&observe_output_input, |view: &mut RootView, input, cx| {
                    let value = input.read(cx).value().to_string();
                    view.command_state.observe.output_dir =
                        if value.trim().is_empty() { None } else { Some(value) };
                    cx.notify();
                })
                .detach();

                cx.observe(&capture_input, |view: &mut RootView, input, cx| {
                    let value = input.read(cx).value().to_string();
                    if view.command_state.capture.csv_path != value {
                        view.command_state.capture.csv_path = value;
                        cx.notify();
                    }
                })
                .detach();

                cx.observe(&capture_output_input, |view: &mut RootView, input, cx| {
                    let value = input.read(cx).value().to_string();
                    view.command_state.capture.output_dir =
                        if value.trim().is_empty() { None } else { Some(value) };
                    cx.notify();
                })
                .detach();

                cx.observe(&xmp_source_input, |view: &mut RootView, input, cx| {
                    view.command_state.xmp.source_dir = input.read(cx).value().to_string();
                    cx.notify();
                })
                .detach();

                cx.observe(&xmp_output_input, |view: &mut RootView, input, cx| {
                    view.command_state.xmp.output_dir = input.read(cx).value().to_string();
                    cx.notify();
                })
                .detach();

                cx.observe(&xmp_csv_input, |view: &mut RootView, input, cx| {
                    view.command_state.xmp.csv_path = input.read(cx).value().to_string();
                    cx.notify();
                })
                .detach();

                cx.observe(&xmp_dir_input, |view: &mut RootView, input, cx| {
                    view.command_state.xmp.dir = input.read(cx).value().to_string();
                    cx.notify();
                })
                .detach();

                cx.observe(&extract_csv_input, |view: &mut RootView, input, cx| {
                    view.command_state.extract.csv_path = input.read(cx).value().to_string();
                    cx.notify();
                })
                .detach();

                cx.observe(&extract_value_input, |view: &mut RootView, input, cx| {
                    view.command_state.extract.value = input.read(cx).value().to_string();
                    cx.notify();
                })
                .detach();

                cx.observe(&extract_output_input, |view: &mut RootView, input, cx| {
                    let value = input.read(cx).value().to_string();
                    view.command_state.extract.output_dir =
                        if value.trim().is_empty() { None } else { Some(value) };
                    cx.notify();
                })
                .detach();

                cx.observe(&translate_csv_input, |view: &mut RootView, input, cx| {
                    view.command_state.translate.csv_path = input.read(cx).value().to_string();
                    cx.notify();
                })
                .detach();

                cx.observe(&translate_taglist_input, |view: &mut RootView, input, cx| {
                    view.command_state.translate.taglist_path = input.read(cx).value().to_string();
                    cx.notify();
                })
                .detach();

                cx.observe(&translate_from_input, |view: &mut RootView, input, cx| {
                    view.command_state.translate.from = input.read(cx).value().to_string();
                    cx.notify();
                })
                .detach();

                cx.observe(&translate_to_input, |view: &mut RootView, input, cx| {
                    view.command_state.translate.to = input.read(cx).value().to_string();
                    cx.notify();
                })
                .detach();

                cx.observe(&translate_output_input, |view: &mut RootView, input, cx| {
                    let value = input.read(cx).value().to_string();
                    view.command_state.translate.output_dir =
                        if value.trim().is_empty() { None } else { Some(value) };
                    cx.notify();
                })
                .detach();

                cx.subscribe(
                    &pty_input,
                    |view: &mut RootView, _input, _event: &TextInputSubmitted, cx| {
                        view.send_pty_input(cx);
                    },
                )
                .detach();

                RootView {
                    command_state: CommandState::default(),
                    language: Language::default(),
                    serval_binary_path: None,
                    observe_input,
                    observe_output_input,
                    capture_input,
                    capture_output_input,
                    xmp_source_input,
                    xmp_output_input,
                    xmp_csv_input,
                    xmp_dir_input,
                    extract_csv_input,
                    extract_value_input,
                    extract_output_input,
                    translate_csv_input,
                    translate_taglist_input,
                    translate_from_input,
                    translate_to_input,
                    translate_output_input,
                    pty_input,
                    output_log: String::new(),
                    running: false,
                    pty_writer: None,
                    output_scroll_handle: ScrollHandle::new(),
                    capture_helper: CaptureHelperModel::default(),
                    helper_mode: false,
                    command_panel_open: true,
                    input_panel_open: true,
                    preview_panel_open: true,
                    help_cache: HashMap::new(),
                    command_help_open: false,
                    command_help_key: None,
                    hover_help_key: None,
                    option_help_position: None,
                    cursor_position: point(px(0.0), px(0.0)),
                }
            })
        })
        .unwrap();
    });
}
