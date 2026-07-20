#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use gpui::*;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

mod commands;
mod help_texts;
mod i18n;
mod interaction_helper;
mod setup_config;
mod text_input;

use commands::{
    format_shell_command, CommandKind, CommandState, XmpSubcommand, DEFAULT_TRANSLATE_FROM,
    DEFAULT_TRANSLATE_TO, TRANSLATE_COLUMN_OPTIONS,
};
use i18n::{t, Language};
use interaction_helper::InteractionHelperModel;
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, PtySize};
use setup_config::SetupConfig;
use text_input::{bind_text_input_keys, TextInput, TextInputSubmitted};

const APP_ID: &str = "io.github.wsyxbcl.waxbill";
const PROJECT_URL: &str = "https://github.com/wsyxbcl/serval-gpui";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const APP_LICENSE: &str = env!("CARGO_PKG_LICENSE");
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;
const BRAND_LOCKUP_PNG: &[u8] =
    include_bytes!("../assets/icons/io.github.wsyxbcl.waxbill-lockup.png");
const OUTPUT_VIEWPORT_HEIGHT: f32 = 260.0;
const INTERACTION_HELPER_VIEWPORT_HEIGHT: f32 = 180.0;
const SCROLLBAR_WIDTH: f32 = 6.0;
const SCROLLBAR_MIN_THUMB_HEIGHT: f32 = 28.0;

// Shared palette for the chip-style buttons that make up most of the UI.
const CHIP_ACTIVE_BG: u32 = 0x111827;
const CHIP_ACTIVE_FG: u32 = 0xF9FAFB;
const CHIP_IDLE_BG: u32 = 0xF3F4F6;
const CHIP_IDLE_FG: u32 = 0x111827;
const TEXT_MUTED: u32 = 0x6B7280;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RunState {
    Idle,
    Running,
    Success,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScrollbarDragTarget {
    Page,
    Output,
    InteractionHelper,
}

impl RunState {
    fn is_running(self) -> bool {
        matches!(self, Self::Running)
    }

    fn action_key(self) -> &'static str {
        if self.is_running() {
            "action.cancel"
        } else {
            "action.run"
        }
    }

    fn label_key(self) -> &'static str {
        match self {
            Self::Idle => "status.idle",
            Self::Running => "status.running",
            Self::Success => "status.success",
            Self::Failed => "status.failed",
            Self::Cancelled => "status.cancelled",
        }
    }

    fn color(self) -> u32 {
        match self {
            Self::Idle => 0x9CA3AF,
            Self::Running => 0xF59E0B,
            Self::Success => 0x10B981,
            Self::Failed => 0xF87171,
            Self::Cancelled => 0xFBBF24,
        }
    }
}

#[derive(Clone)]
enum RunningProcessHandle {
    Pty {
        // Only consulted by the Windows taskkill path in `kill`.
        #[cfg_attr(not(windows), allow(dead_code))]
        pid: Option<u32>,
        killer: Arc<Mutex<Box<dyn ChildKiller + Send + Sync>>>,
    },
    Process {
        #[cfg_attr(not(windows), allow(dead_code))]
        pid: u32,
        child: Arc<Mutex<std::process::Child>>,
    },
}

#[derive(Clone, Debug)]
enum ServalVersionStatus {
    Checking,
    Configured(String),
    PathFallback(String),
    Missing,
    Error,
}

impl RunningProcessHandle {
    fn kill(&self) -> std::io::Result<()> {
        #[cfg(windows)]
        if let Some(pid) = self.process_id() {
            let status = configure_background_command(Command::new("taskkill"))
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()?;
            if status.success() {
                return Ok(());
            }
        }

        match self {
            Self::Pty { killer, .. } => killer
                .lock()
                .map_err(|_| std::io::Error::other("PTY killer lock poisoned"))?
                .kill(),
            Self::Process { child, .. } => child
                .lock()
                .map_err(|_| std::io::Error::other("Process handle lock poisoned"))?
                .kill(),
        }
    }

    #[cfg(windows)]
    fn process_id(&self) -> Option<u32> {
        match self {
            Self::Pty { pid, .. } => *pid,
            Self::Process { pid, .. } => Some(*pid),
        }
    }
}

/// PTY width. TerminalBuffer wraps at the same width: indicatif pads its
/// frame lines to exactly the terminal width and relies on cursor wrapping
/// (no newlines) to move below printed lines, so buffer and PTY must agree.
const PTY_COLS: u16 = 512;
/// Scrollback cap so a chatty process cannot grow the buffer without bound.
const MAX_BUFFER_LINES: usize = 10_000;
/// Trim in batches so the front-drain cost stays amortized.
const BUFFER_TRIM_BATCH: usize = 1_000;
/// Cap cursor-down moves; a malformed CSI parameter must not allocate
/// millions of blank lines.
const MAX_CURSOR_DOWN: usize = 1_000;

struct TerminalBuffer {
    lines: Vec<Vec<char>>,
    cursor_row: usize,
    cursor_col: usize,
    cols: usize,
    pending_escape: String,
    /// Per-line render cache; entries below `dirty_from` are up to date.
    rendered_lines: Vec<String>,
    dirty_from: usize,
}

impl Default for TerminalBuffer {
    fn default() -> Self {
        Self {
            lines: Vec::new(),
            cursor_row: 0,
            cursor_col: 0,
            cols: PTY_COLS as usize,
            pending_escape: String::new(),
            rendered_lines: Vec::new(),
            dirty_from: 0,
        }
    }
}

impl TerminalBuffer {
    fn append_log_line(&mut self, text: &str) {
        let text = sanitize_log_output(text);
        if text.is_empty() {
            self.ensure_line(self.cursor_row);
            return;
        }

        if !self.current_line_is_empty() && !text.starts_with('\n') {
            self.new_line();
        }

        self.push_chunk(&text);

        if !text.ends_with('\n') {
            self.new_line();
        }
    }

    fn push_chunk(&mut self, text: &str) {
        self.push_chunk_inner(text);
        self.trim_scrollback();
    }

    fn push_chunk_inner(&mut self, text: &str) {
        // Reassemble escape sequences split across PTY read chunks.
        let text = if self.pending_escape.is_empty() {
            text.to_string()
        } else {
            std::mem::take(&mut self.pending_escape) + text
        };
        let mut chars = text.chars().peekable();
        while let Some(ch) = chars.next() {
            match ch {
                '\n' => self.new_line(),
                '\r' => self.cursor_col = 0,
                '\t' => {
                    for _ in 0..4 {
                        self.write_char(' ');
                    }
                }
                '\u{0008}' => {
                    if self.cursor_col > 0 {
                        self.cursor_col -= 1;
                    }
                }
                '\u{001b}' => match chars.peek().copied() {
                    Some('[') => {
                        let _ = chars.next();
                        let mut params = String::new();
                        let mut complete = false;
                        for next in chars.by_ref() {
                            if next.is_ascii_alphabetic() {
                                self.apply_csi(&params, next);
                                complete = true;
                                break;
                            }
                            params.push(next);
                        }
                        if !complete {
                            self.stash_escape(format!("\u{001b}[{params}"));
                            return;
                        }
                    }
                    Some(']') => {
                        let _ = chars.next();
                        let mut consumed = String::new();
                        let mut complete = false;
                        while let Some(next) = chars.next() {
                            if next == '\u{0007}' {
                                complete = true;
                                break;
                            }
                            if next == '\u{001b}' && matches!(chars.peek().copied(), Some('\\')) {
                                let _ = chars.next();
                                complete = true;
                                break;
                            }
                            consumed.push(next);
                        }
                        if !complete {
                            self.stash_escape(format!("\u{001b}]{consumed}"));
                            return;
                        }
                    }
                    None => {
                        self.pending_escape = "\u{001b}".to_string();
                        return;
                    }
                    _ => {}
                },
                _ if ch.is_control() => {}
                _ => self.write_char(ch),
            }
        }
    }

    fn stash_escape(&mut self, sequence: String) {
        // Guard against pathological streams that never terminate a sequence.
        if sequence.len() <= 4096 {
            self.pending_escape = sequence;
        }
    }

    fn render_text(&mut self) -> String {
        let from = self
            .dirty_from
            .min(self.lines.len())
            .min(self.rendered_lines.len());
        self.rendered_lines.truncate(from);
        for line in &self.lines[from..] {
            // Trim the padding indicatif's {wide_msg} adds to fill the
            // (wide) PTY line.
            let text: String = line.iter().collect();
            self.rendered_lines.push(text.trim_end().to_string());
        }
        self.dirty_from = self.lines.len();
        self.rendered_lines.join("\n")
    }

    fn mark_dirty(&mut self, row: usize) {
        self.dirty_from = self.dirty_from.min(row);
    }

    fn clear(&mut self) {
        self.lines.clear();
        self.cursor_row = 0;
        self.cursor_col = 0;
        self.pending_escape.clear();
        self.rendered_lines.clear();
        self.dirty_from = 0;
    }

    fn apply_csi(&mut self, params: &str, command: char) {
        let first_param = params.split(';').next().filter(|part| !part.is_empty());

        match command {
            'K' => match first_param.unwrap_or("0") {
                "0" => {
                    self.ensure_line(self.cursor_row);
                    self.lines[self.cursor_row].truncate(self.cursor_col);
                    self.mark_dirty(self.cursor_row);
                }
                "1" => {
                    self.ensure_line(self.cursor_row);
                    let line = &mut self.lines[self.cursor_row];
                    let end = self.cursor_col.min(line.len());
                    for ch in &mut line[..end] {
                        *ch = ' ';
                    }
                    self.mark_dirty(self.cursor_row);
                }
                "2" => {
                    self.ensure_line(self.cursor_row);
                    self.lines[self.cursor_row].clear();
                    self.cursor_col = 0;
                    self.mark_dirty(self.cursor_row);
                }
                _ => {}
            },
            // Cursor up/down: indicatif redraws its progress area with these
            // when printing lines above the bar (warnings) or when a frame
            // spans multiple visual rows.
            'A' => {
                let n = first_param
                    .and_then(|value| value.parse::<usize>().ok())
                    .unwrap_or(1)
                    .max(1);
                self.cursor_row = self.cursor_row.saturating_sub(n);
                self.ensure_line(self.cursor_row);
            }
            'B' => {
                let n = first_param
                    .and_then(|value| value.parse::<usize>().ok())
                    .unwrap_or(1)
                    .clamp(1, MAX_CURSOR_DOWN);
                self.cursor_row += n;
                self.ensure_line(self.cursor_row);
            }
            // Erase below cursor (console emits "\r\x1b[0J" to clear the bar).
            'J' => {
                if first_param.unwrap_or("0") == "0" {
                    self.ensure_line(self.cursor_row);
                    self.lines[self.cursor_row].truncate(self.cursor_col);
                    self.lines.truncate(self.cursor_row + 1);
                    self.mark_dirty(self.cursor_row);
                }
            }
            // Cursor forward/back within the line (rustyline prompt redraws).
            'C' => {
                let n = first_param
                    .and_then(|value| value.parse::<usize>().ok())
                    .unwrap_or(1)
                    .max(1);
                self.cursor_col = (self.cursor_col + n).min(self.cols);
            }
            'D' => {
                let n = first_param
                    .and_then(|value| value.parse::<usize>().ok())
                    .unwrap_or(1)
                    .max(1);
                self.cursor_col = self.cursor_col.saturating_sub(n);
            }
            _ => {}
        }
    }

    fn write_char(&mut self, ch: char) {
        // Deferred wrap at terminal width, like a real terminal: the cursor
        // stays past the last column until the next character is written.
        if self.cursor_col >= self.cols {
            self.new_line();
        }
        self.ensure_line(self.cursor_row);
        let line = &mut self.lines[self.cursor_row];
        if self.cursor_col < line.len() {
            line[self.cursor_col] = ch;
        } else {
            while line.len() < self.cursor_col {
                line.push(' ');
            }
            line.push(ch);
        }
        self.cursor_col += 1;
        self.mark_dirty(self.cursor_row);
    }

    fn new_line(&mut self) {
        self.cursor_row += 1;
        self.cursor_col = 0;
        self.ensure_line(self.cursor_row);
    }

    fn current_line_is_empty(&mut self) -> bool {
        self.ensure_line(self.cursor_row);
        self.lines[self.cursor_row].is_empty()
    }

    fn ensure_line(&mut self, row: usize) {
        if self.lines.len() <= row {
            self.mark_dirty(self.lines.len());
            while self.lines.len() <= row {
                self.lines.push(Vec::new());
            }
        }
    }

    /// Drop the oldest lines once the buffer exceeds its cap, in batches so
    /// the front-drain cost stays amortized.
    fn trim_scrollback(&mut self) {
        if self.lines.len() <= MAX_BUFFER_LINES + BUFFER_TRIM_BATCH {
            return;
        }
        let remove = self.lines.len() - MAX_BUFFER_LINES;
        self.lines.drain(..remove);
        self.cursor_row = self.cursor_row.saturating_sub(remove);
        if self.rendered_lines.len() >= remove {
            self.rendered_lines.drain(..remove);
        } else {
            self.rendered_lines.clear();
        }
        self.dirty_from = self.dirty_from.saturating_sub(remove);
    }
}

struct RootView {
    command_state: CommandState,
    language: Language,
    serval_binary_path: Option<String>,
    serval_version_status: ServalVersionStatus,
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
    terminal_buffer: TerminalBuffer,
    output_log: String,
    output_dirty: bool,
    run_state: RunState,
    cancel_requested: bool,
    running_command_kind: Option<CommandKind>,
    running_interaction_helper: bool,
    running_process: Option<RunningProcessHandle>,
    pty_writer: Option<Arc<Mutex<Box<dyn Write + Send>>>>,
    output_scroll_handle: ScrollHandle,
    page_scroll_handle: ScrollHandle,
    interaction_helper_scroll_handle: ScrollHandle,
    scrollbar_drag_target: Option<ScrollbarDragTarget>,
    command_help_scroll_handle: ScrollHandle,
    interaction_helper: InteractionHelperModel,
    helper_mode: bool,
    input_panel_open: bool,
    preview_panel_open: bool,
    serval_version_refresh_in_flight: bool,
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

struct AboutView {
    language: Language,
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
                input.update(app, |input, cx| {
                    input.set_value(value, cx);
                });
            }
        }
        Ok(None) => {}
        Err(err) => {
            root.update(app, |view, cx| {
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

fn apply_ui_font<E: Styled>(element: E, language: Language) -> E {
    #[cfg(target_os = "windows")]
    {
        let family = match language {
            Language::En => "Segoe UI",
            Language::ZhCn => "Microsoft YaHei UI",
        };
        element.font_family(family)
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = language;
        element
    }
}

#[cfg(windows)]
fn configure_background_command(mut command: Command) -> Command {
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

#[cfg(not(windows))]
fn configure_background_command(command: Command) -> Command {
    command
}

fn scroll_handle_by_wheel(
    handle: &ScrollHandle,
    event: &ScrollWheelEvent,
    line_height: Pixels,
    allow_x: bool,
) -> bool {
    let max_offset = handle.max_offset();
    if max_offset.height <= px(0.0) && (!allow_x || max_offset.width <= px(0.0)) {
        return false;
    }

    let delta = event.delta.pixel_delta(line_height);
    let mut offset = handle.offset();
    let old_offset = offset;

    offset.y = (offset.y + delta.y).clamp(-max_offset.height, px(0.0));
    if allow_x {
        offset.x = (offset.x + delta.x).clamp(-max_offset.width, px(0.0));
    }

    if offset != old_offset {
        handle.set_offset(offset);
        true
    } else {
        false
    }
}

fn set_scrollbar_offset_from_mouse(
    handle: &ScrollHandle,
    track_bounds: Bounds<Pixels>,
    thumb_height: Pixels,
    mouse_y: Pixels,
) {
    let max_offset = handle.max_offset();
    if max_offset.height <= px(0.0) {
        return;
    }

    let travel = (track_bounds.size.height - thumb_height).max(px(0.0));
    if travel <= px(0.0) {
        return;
    }

    let ratio = ((mouse_y - track_bounds.top() - thumb_height / 2.0) / travel).clamp(0.0, 1.0);
    let mut offset = handle.offset();
    offset.y = -max_offset.height * ratio;
    handle.set_offset(offset);
}

fn render_vertical_scrollbar(
    id: &'static str,
    handle: &ScrollHandle,
    target: ScrollbarDragTarget,
    entity: Entity<RootView>,
    dark: bool,
) -> AnyElement {
    let bounds = handle.bounds();
    let max_offset = handle.max_offset();
    let viewport_height = bounds.size.height;
    if viewport_height <= px(0.0) || max_offset.height <= px(0.0) {
        return div().id(id).into_any_element();
    }

    let content_height = viewport_height + max_offset.height;
    let thumb_height = ((viewport_height / content_height) * viewport_height)
        .clamp(px(SCROLLBAR_MIN_THUMB_HEIGHT), viewport_height);
    let travel = (viewport_height - thumb_height).max(px(0.0));
    let scroll_ratio = (-handle.offset().y / max_offset.height).clamp(0.0, 1.0);
    let thumb_top = travel * scroll_ratio;
    let track_color = if dark { 0x1F2937 } else { 0xE5E7EB };
    let thumb_color = 0x9CA3AF;
    let drag_handle = handle.clone();
    let drag_entity = entity.clone();

    div()
        .id(id)
        .absolute()
        .top(px(0.0))
        .right(px(2.0))
        .h(viewport_height)
        .w(px(SCROLLBAR_WIDTH))
        .bg(rgb(track_color))
        .rounded_lg()
        .cursor_pointer()
        .child(
            canvas(
                |_, _, _| (),
                move |track_bounds, _, window, _| {
                    window.on_mouse_event({
                        let handle = drag_handle.clone();
                        let entity = drag_entity.clone();
                        move |event: &MouseDownEvent, _, _, cx| {
                            if !track_bounds.contains(&event.position) {
                                return;
                            }

                            set_scrollbar_offset_from_mouse(
                                &handle,
                                track_bounds,
                                thumb_height,
                                event.position.y,
                            );
                            entity.update(cx, |view, cx| {
                                view.scrollbar_drag_target = Some(target);
                                cx.notify();
                            });
                            cx.stop_propagation();
                        }
                    });
                    window.on_mouse_event({
                        let entity = drag_entity.clone();
                        move |_: &MouseUpEvent, _, _, cx| {
                            entity.update(cx, |view, cx| {
                                if view.scrollbar_drag_target == Some(target) {
                                    view.scrollbar_drag_target = None;
                                    cx.notify();
                                }
                            });
                        }
                    });
                    window.on_mouse_event({
                        let handle = drag_handle.clone();
                        let entity = drag_entity.clone();
                        move |event: &MouseMoveEvent, _, _, cx| {
                            if !event.dragging() {
                                return;
                            }

                            let is_dragging = entity.read(cx).scrollbar_drag_target == Some(target);
                            if !is_dragging {
                                return;
                            }

                            set_scrollbar_offset_from_mouse(
                                &handle,
                                track_bounds,
                                thumb_height,
                                event.position.y,
                            );
                            cx.notify(entity.entity_id());
                            cx.stop_propagation();
                        }
                    });
                },
            )
            .absolute()
            .size_full(),
        )
        .child(
            div()
                .absolute()
                .top(thumb_top)
                .right(px(0.0))
                .h(thumb_height)
                .w(px(SCROLLBAR_WIDTH))
                .bg(rgb(thumb_color))
                .rounded_lg(),
        )
        .into_any_element()
}

/// Base styling shared by every chip-style button.
fn chip_base(id: &'static str, active: bool) -> Stateful<Div> {
    div()
        .id(id)
        .bg(rgb(if active { CHIP_ACTIVE_BG } else { CHIP_IDLE_BG }))
        .text_color(rgb(if active { CHIP_ACTIVE_FG } else { CHIP_IDLE_FG }))
        .p(px(8.0))
        .cursor_pointer()
}

/// A chip that mutates `RootView` state on click, optionally wired to the
/// hover help overlay via `help_key`.
fn chip(
    id: &'static str,
    label: impl IntoElement,
    active: bool,
    entity: &Entity<RootView>,
    help_key: Option<&'static str>,
    on_click: impl Fn(&mut RootView, &mut Context<RootView>) + 'static,
) -> Stateful<Div> {
    let click_entity = entity.clone();
    let mut el = chip_base(id, active).on_click(move |_, _, cx| {
        click_entity.update(cx, |view, cx| on_click(view, cx));
    });
    if let Some(key) = help_key {
        let hover_entity = entity.clone();
        el = el.on_hover(move |hovered, _, cx| {
            hover_entity.update(cx, |view, cx| {
                if *hovered {
                    view.set_hover_help_key(key, cx);
                } else {
                    view.clear_hover_help_key(key, cx);
                }
            });
        });
    }
    el.child(label)
}

/// A wrapping row of chips.
fn chip_row() -> Div {
    div().flex().flex_row().flex_wrap().gap(px(8.0))
}

fn muted_label(language: Language, key: &'static str) -> Div {
    div().text_color(rgb(TEXT_MUTED)).child(t(language, key))
}

fn bare_input_row(input: &Entity<TextInput>) -> Div {
    div()
        .flex()
        .flex_row()
        .gap(px(8.0))
        .child(div().flex_grow().child(input.clone()))
}

fn input_row(input: &Entity<TextInput>, browse: Stateful<Div>) -> Div {
    bare_input_row(input).child(browse)
}

/// A Browse button that opens a file/directory picker into `input`.
fn browse_button(
    id: &'static str,
    entity: &Entity<RootView>,
    language: Language,
    input: &Entity<TextInput>,
    directories: bool,
    prompt_key: &'static str,
) -> Stateful<Div> {
    let entity = entity.clone();
    let input = input.clone();
    chip_base(id, false)
        .on_click(move |_, window, cx| {
            browse_into_text_input(
                window,
                cx,
                PathPromptOptions {
                    files: !directories,
                    directories,
                    multiple: false,
                    prompt: Some(t(language, prompt_key).into()),
                },
                input.clone(),
                entity.clone(),
                language,
            );
        })
        .child(t(language, "action.browse"))
}

fn brand_lockup_image() -> Arc<Image> {
    static IMAGE: OnceLock<Arc<Image>> = OnceLock::new();

    IMAGE
        .get_or_init(|| {
            Arc::new(Image::from_bytes(
                ImageFormat::Png,
                BRAND_LOCKUP_PNG.to_vec(),
            ))
        })
        .clone()
}

impl RootView {
    fn first_non_empty_output_line(output: &Output) -> Option<String> {
        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout
            .lines()
            .chain(String::from_utf8_lossy(&output.stderr).lines())
            .map(str::trim)
            .find(|line| !line.is_empty())
            .map(str::to_string)
    }

    fn detect_serval_version_status(configured_path: Option<&str>) -> ServalVersionStatus {
        let configured_path = configured_path.and_then(|path| {
            let trimmed = path.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        });

        let program = configured_path.unwrap_or("serval");
        let output = match configure_background_command(Command::new(program))
            .arg("--version")
            .output()
        {
            Ok(output) => output,
            Err(_) => {
                return if configured_path.is_some() {
                    ServalVersionStatus::Error
                } else {
                    ServalVersionStatus::Missing
                };
            }
        };

        if !output.status.success() {
            return if configured_path.is_some() {
                ServalVersionStatus::Error
            } else {
                ServalVersionStatus::Missing
            };
        }

        match Self::first_non_empty_output_line(&output) {
            Some(version) => {
                if configured_path.is_some() {
                    ServalVersionStatus::Configured(version)
                } else {
                    ServalVersionStatus::PathFallback(version)
                }
            }
            None => {
                if configured_path.is_some() {
                    ServalVersionStatus::Error
                } else {
                    ServalVersionStatus::Missing
                }
            }
        }
    }

    fn queue_serval_version_status_refresh_in(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.serval_version_refresh_in_flight {
            return;
        }

        self.serval_version_refresh_in_flight = true;
        let configured_path = self.serval_binary_path.clone();
        let view = cx.weak_entity();
        window
            .spawn(cx, async move |cx| {
                let status = cx
                    .background_executor()
                    .spawn(async move {
                        RootView::detect_serval_version_status(configured_path.as_deref())
                    })
                    .await;

                let _ = view.update(cx, |view, cx| {
                    view.serval_version_status = status;
                    view.serval_version_refresh_in_flight = false;
                    cx.notify();
                });
            })
            .detach();
    }

    fn active_binary_display(&self) -> (String, u32) {
        let active_binary = self.executable_program();

        match &self.serval_version_status {
            ServalVersionStatus::Checking => (
                format!(
                    "{} ({active_binary})",
                    t(self.language, "app.serval_version_loading")
                ),
                0x6B7280,
            ),
            ServalVersionStatus::Configured(version) => {
                (format!("{version} ({active_binary})"), 0x6B7280)
            }
            ServalVersionStatus::PathFallback(version) => {
                (format!("{version} ({active_binary})"), 0x6B7280)
            }
            ServalVersionStatus::Missing => (
                t(self.language, "app.serval_version_not_configured").to_string(),
                0xB45309,
            ),
            ServalVersionStatus::Error => (
                format!(
                    "{active_binary}. {}",
                    t(self.language, "app.serval_version_unavailable")
                ),
                0xDC2626,
            ),
        }
    }

    fn command_uses_pty(kind: CommandKind) -> bool {
        matches!(
            kind,
            CommandKind::Observe | CommandKind::Capture | CommandKind::Xmp | CommandKind::Extract
        )
    }

    fn command_uses_interaction_helper(&self) -> bool {
        match self.command_state.kind {
            CommandKind::Capture | CommandKind::Extract => true,
            CommandKind::Xmp => {
                self.command_state.xmp.subcommand == XmpSubcommand::Init
                    && self.command_state.xmp.info
            }
            _ => false,
        }
    }

    fn set_language(&mut self, language: Language, window: &Window, cx: &mut Context<Self>) {
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

            self.persist_setup_in(window, cx);

            self.refresh_command_help(cx);
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
        self.command_state
            .preview_with_program(&self.executable_program())
    }

    fn set_serval_binary_path(
        &mut self,
        path: Option<String>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.serval_binary_path = path.and_then(|p| {
            let trimmed = p.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        });
        self.serval_version_status = ServalVersionStatus::Checking;
        self.queue_serval_version_status_refresh_in(window, cx);
        self.persist_setup_in(window, cx);
        self.help_cache.clear();
        self.hover_help_key = None;
        self.option_help_position = None;
        self.refresh_command_help(cx);
    }

    fn setup_config(&self) -> SetupConfig {
        SetupConfig {
            language: self.language,
            serval_binary_path: self.serval_binary_path.clone(),
        }
    }

    fn persist_setup_in(&self, window: &Window, cx: &mut Context<Self>) {
        let config = self.setup_config();
        let language = self.language;
        let view = cx.weak_entity();
        window
            .spawn(cx, async move |cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { config.save() })
                    .await;
                if let Err(err) = result {
                    let message = format!("{}: {err}", t(language, "message.failed_save_setup"));
                    let _ = view.update(cx, |view, cx| {
                        view.append_output(message, cx);
                    });
                }
            })
            .detach();
    }

    fn select_command(&mut self, kind: CommandKind, cx: &mut Context<Self>) {
        if self.command_state.kind != kind {
            self.command_state.kind = kind;
            if !self.run_state.is_running() {
                self.interaction_helper.reset();
            }
            self.hover_help_key = None;
            self.option_help_position = None;
            self.refresh_command_help(cx);
        }
    }

    fn append_output(&mut self, line: impl AsRef<str>, cx: &mut Context<Self>) {
        let follow = self.output_follows_tail();
        self.terminal_buffer.append_log_line(line.as_ref());
        self.output_dirty = true;
        if follow {
            self.output_scroll_handle.scroll_to_bottom();
        }
        cx.notify();
    }

    fn append_output_chunk(&mut self, chunk: impl AsRef<str>, cx: &mut Context<Self>) {
        if chunk.as_ref().is_empty() {
            return;
        }

        let follow = self.output_follows_tail();
        self.terminal_buffer.push_chunk(chunk.as_ref());
        self.output_dirty = true;
        if follow {
            self.output_scroll_handle.scroll_to_bottom();
        }
        cx.notify();
    }

    /// Sticky scroll: only keep following the stream while the user has not
    /// scrolled up to read older output.
    fn output_follows_tail(&self) -> bool {
        let max_offset = self.output_scroll_handle.max_offset().height;
        if max_offset <= px(0.0) {
            return true;
        }
        // offset.y runs from 0 at the top to -max_offset at the bottom.
        self.output_scroll_handle.offset().y - px(8.0) <= -max_offset
    }

    /// Rebuild the displayed log lazily, coalescing all chunks that arrived
    /// since the last frame into one render pass.
    fn refresh_output_log(&mut self) {
        if self.output_dirty {
            self.output_log = self.terminal_buffer.render_text();
            self.output_dirty = false;
        }
    }

    fn clear_output(&mut self, cx: &mut Context<Self>) {
        self.terminal_buffer.clear();
        self.output_log.clear();
        self.output_dirty = false;
        cx.notify();
    }

    fn begin_run(
        &mut self,
        kind: CommandKind,
        use_interaction_helper: bool,
        display: String,
        cx: &mut Context<Self>,
    ) {
        self.append_output(display, cx);
        self.run_state = RunState::Running;
        self.cancel_requested = false;
        self.running_command_kind = Some(kind);
        self.running_interaction_helper = use_interaction_helper;
        self.running_process = None;
        self.pty_writer = None;
        self.interaction_helper.reset();
        cx.notify();
    }

    fn finish_run(&mut self, code: i32, cx: &mut Context<Self>) {
        let state = if self.cancel_requested {
            RunState::Cancelled
        } else if code == 0 {
            RunState::Success
        } else {
            RunState::Failed
        };

        self.run_state = state;
        self.cancel_requested = false;
        self.running_command_kind = None;
        self.running_interaction_helper = false;
        self.running_process = None;
        self.pty_writer = None;
        self.interaction_helper.reset();

        if state == RunState::Cancelled {
            self.append_output(t(self.language, "message.process_cancelled"), cx);
        } else {
            self.append_output(
                format!("{}: {code}", t(self.language, "message.process_exited")),
                cx,
            );
        }
        cx.notify();
    }

    fn set_running_process(
        &mut self,
        handle: Option<RunningProcessHandle>,
        cx: &mut Context<Self>,
    ) {
        self.running_process = handle;
        cx.notify();
    }

    fn set_pty_writer(
        &mut self,
        writer: Option<Arc<Mutex<Box<dyn Write + Send>>>>,
        cx: &mut Context<Self>,
    ) {
        self.pty_writer = writer;
        cx.notify();
    }

    fn request_cancel(&mut self, cx: &mut Context<Self>) {
        if !self.run_state.is_running() {
            return;
        }

        if self.cancel_requested {
            return;
        }

        let Some(handle) = self.running_process.clone() else {
            self.append_output(t(self.language, "message.no_running_process"), cx);
            return;
        };

        match handle.kill() {
            Ok(()) => {
                self.cancel_requested = true;
                self.append_output(t(self.language, "message.cancelling"), cx);
                cx.notify();
            }
            Err(err) => {
                self.append_output(
                    format!(
                        "{}: {err}",
                        t(self.language, "message.failed_cancel_process")
                    ),
                    cx,
                );
            }
        }
    }

    fn write_pty_value(&mut self, value: &str, cx: &mut Context<Self>) -> bool {
        let writer = self.pty_writer.clone();
        if let Some(writer) = writer {
            if let Ok(mut guard) = writer.lock() {
                let newline = if cfg!(windows) {
                    b"\r\n".as_slice()
                } else {
                    b"\n".as_slice()
                };
                let mut result = guard.write_all(value.as_bytes());
                if result.is_ok() {
                    result = guard.write_all(newline);
                }
                if result.is_ok() {
                    result = guard.flush();
                }
                drop(guard);

                if let Err(err) = result {
                    self.append_output(
                        format!("{}: {err}", t(self.language, "message.pty_write_error")),
                        cx,
                    );
                    return false;
                }

                if self.running_interaction_helper
                    && self.interaction_helper.record_submission(value)
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
            self.pty_input
                .update(cx, |input, cx| input.set_value("", cx));
        }
    }

    fn observe_interaction_helper_output(&mut self, chunk: &str, cx: &mut Context<Self>) {
        let chunk = sanitize_log_output(chunk);
        if self.interaction_helper.observe_output(&chunk) {
            if self
                .interaction_helper
                .prompt()
                .is_some_and(|prompt| !prompt.options.is_empty())
            {
                self.page_scroll_handle.scroll_to_bottom();
            }
            cx.notify();
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

    fn handle_run_click(
        entity: Entity<Self>,
        language: Language,
        window: &mut Window,
        cx: &mut App,
    ) {
        let entity_for_events = entity.clone();
        let (program, args, use_pty, use_interaction_helper) =
            match entity.update(cx, |view, cx| {
                if view.run_state.is_running() {
                    view.request_cancel(cx);
                    return None;
                }

                match view.command_state.build_command() {
                    Ok(args) => {
                        let executable = view.executable_program();
                        let kind = view.command_state.kind;
                        let display = format!("$ {}", format_shell_command(&executable, &args));
                        let use_interaction_helper = view.command_uses_interaction_helper();
                        view.begin_run(kind, use_interaction_helper, display, cx);
                        let use_pty = RootView::command_uses_pty(kind);
                        Some((executable, args, use_pty, use_interaction_helper))
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
                // Width must match TerminalBuffer's wrap width; see PTY_COLS.
                let pair = match pty_system.openpty(PtySize {
                    rows: 24,
                    cols: PTY_COLS,
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

                let _ = tx.send(OutputEvent::ProcessHandle(RunningProcessHandle::Pty {
                    pid: child.process_id(),
                    killer: Arc::new(Mutex::new(child.clone_killer())),
                }));

                if let Ok(writer) = pair.master.take_writer() {
                    let _ = tx.send(OutputEvent::PtyWriter(Arc::new(Mutex::new(writer))));
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

                let tx_reader = tx.clone();
                thread::spawn(move || {
                    let mut buf = [0u8; 8192];
                    // Carry incomplete trailing UTF-8 sequences over to the
                    // next read so multi-byte characters split across chunk
                    // boundaries don't decode as replacement characters.
                    let mut pending: Vec<u8> = Vec::new();
                    loop {
                        match reader.read(&mut buf) {
                            Ok(0) => {
                                if !pending.is_empty() {
                                    let text = String::from_utf8_lossy(&pending).to_string();
                                    let _ = tx_reader.send(OutputEvent::Chunk(text));
                                }
                                break;
                            }
                            Ok(n) => {
                                pending.extend_from_slice(&buf[..n]);
                                let valid_len = match std::str::from_utf8(&pending) {
                                    Ok(_) => pending.len(),
                                    // error_len() == None means the tail is an
                                    // incomplete (not invalid) sequence.
                                    Err(err) if err.error_len().is_none() => err.valid_up_to(),
                                    Err(_) => pending.len(),
                                };
                                if valid_len > 0 {
                                    let text =
                                        String::from_utf8_lossy(&pending[..valid_len]).to_string();
                                    let _ = tx_reader.send(OutputEvent::Chunk(text));
                                    pending.drain(..valid_len);
                                }
                            }
                            Err(err) => {
                                let _ = tx_reader.send(OutputEvent::Error(format!(
                                    "{}: {err}",
                                    t(language, "message.pty_read_error")
                                )));
                                break;
                            }
                        }
                    }
                });

                let status = match child.wait() {
                    Ok(status) => status.exit_code() as i32,
                    Err(_) => 1,
                };
                let _ = tx.send(OutputEvent::Exit(status));
            });
        } else {
            thread::spawn(move || {
                let child = match configure_background_command(Command::new(&program))
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

                let child = Arc::new(Mutex::new(child));
                let pid = child.lock().ok().map(|child| child.id()).unwrap_or(0);
                let _ = tx.send(OutputEvent::ProcessHandle(RunningProcessHandle::Process {
                    pid,
                    child: child.clone(),
                }));

                let (stdout, stderr) = match child.lock() {
                    Ok(mut child) => (child.stdout.take(), child.stderr.take()),
                    Err(_) => {
                        let _ = tx.send(OutputEvent::Error(
                            t(language, "message.process_wait_error").to_string(),
                        ));
                        let _ = tx.send(OutputEvent::Exit(1));
                        return;
                    }
                };

                if let Some(stdout) = stdout {
                    let tx_out = tx.clone();
                    thread::spawn(move || {
                        let reader = BufReader::new(stdout);
                        for line in reader.lines() {
                            match line {
                                Ok(line) => {
                                    let _ = tx_out.send(OutputEvent::Line(line));
                                }
                                Err(err) => {
                                    let _ = tx_out.send(OutputEvent::Error(format!(
                                        "{}: {err}",
                                        t(language, "message.stdout_error")
                                    )));
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
                                    let _ = tx_err.send(OutputEvent::Line(line));
                                }
                                Err(err) => {
                                    let _ = tx_err.send(OutputEvent::Error(format!(
                                        "{}: {err}",
                                        t(language, "message.stderr_error")
                                    )));
                                    break;
                                }
                            }
                        }
                    });
                }

                loop {
                    let status = match child.lock() {
                        Ok(mut child) => match child.try_wait() {
                            Ok(Some(status)) => Some(status.code().unwrap_or(1)),
                            Ok(None) => None,
                            Err(err) => {
                                let _ = tx.send(OutputEvent::Error(format!(
                                    "{}: {err}",
                                    t(language, "message.process_wait_error")
                                )));
                                Some(1)
                            }
                        },
                        Err(_) => {
                            let _ = tx.send(OutputEvent::Error(
                                t(language, "message.process_wait_error").to_string(),
                            ));
                            Some(1)
                        }
                    };

                    if let Some(status) = status {
                        let _ = tx.send(OutputEvent::Exit(status));
                        break;
                    }

                    thread::sleep(Duration::from_millis(50));
                }
            });
        }

        window
            .spawn(cx, async move |cx| {
                let mut finished = false;
                while !finished {
                    loop {
                        let event = match rx.try_recv() {
                            Ok(event) => event,
                            Err(mpsc::TryRecvError::Empty) => break,
                            Err(mpsc::TryRecvError::Disconnected) => {
                                // The worker thread died without reporting an
                                // exit; end the run instead of polling forever.
                                let _ = cx.update(|_window, app| {
                                    entity_for_events.update(app, |view, cx| {
                                        if view.run_state.is_running() {
                                            view.append_output(
                                                t(language, "message.process_wait_error"),
                                                cx,
                                            );
                                            view.finish_run(1, cx);
                                        }
                                    })
                                });
                                finished = true;
                                break;
                            }
                        };
                        let entity = entity_for_events.clone();
                        match event {
                            OutputEvent::Chunk(chunk) => {
                                let _ = cx.update(|_window, app| {
                                    entity.update(app, |view, cx| {
                                        if use_interaction_helper {
                                            view.observe_interaction_helper_output(&chunk, cx);
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
                            OutputEvent::ProcessHandle(handle) => {
                                let _ = cx.update(|_window, app| {
                                    entity.update(app, |view, cx| {
                                        view.set_running_process(Some(handle), cx);
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
                                        view.finish_run(code, cx);
                                    })
                                });
                                finished = true;
                            }
                        }
                    }
                    if !finished {
                        Timer::after(Duration::from_millis(50)).await;
                    }
                }
            })
            .detach();
    }

    fn resolve_output_dir(&self) -> Option<PathBuf> {
        self.command_state.effective_output_dir().map(PathBuf::from)
    }

    fn set_translate_from_value(&mut self, value: String, cx: &mut Context<Self>) {
        self.command_state.translate.from = value.clone();
        self.translate_from_input
            .update(cx, |input, cx| input.set_value(value, cx));
        cx.notify();
    }

    fn set_translate_to_value(&mut self, value: String, cx: &mut Context<Self>) {
        self.command_state.translate.to = value.clone();
        self.translate_to_input
            .update(cx, |input, cx| input.set_value(value, cx));
        cx.notify();
    }

    fn activate_translate_from_custom(&mut self, cx: &mut Context<Self>) {
        if is_translate_column_preset(&self.command_state.translate.from) {
            self.set_translate_from_value(String::new(), cx);
        } else {
            cx.notify();
        }
    }

    fn activate_translate_to_custom(&mut self, cx: &mut Context<Self>) {
        if is_translate_column_preset(&self.command_state.translate.to) {
            self.set_translate_to_value(String::new(), cx);
        } else {
            cx.notify();
        }
    }

    fn render_auto_output_dir_hint(&self) -> AnyElement {
        let Some(path) = self.command_state.auto_output_dir_hint() else {
            return div().into_any_element();
        };

        div()
            .text_color(rgb(0x6B7280))
            .text_size(px(12.0))
            .child(format!(
                "{} {path}",
                t(self.language, "hint.output_dir_auto")
            ))
            .into_any_element()
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
        self.help_cache
            .get(key)
            .cloned()
            .unwrap_or_else(|| t(self.language, "message.help_missing").to_string())
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

    fn refresh_command_help(&mut self, cx: &mut Context<Self>) {
        if self.command_help_open {
            let key = self.current_help_key();
            self.command_help_key = Some(key.clone());
            self.command_help_scroll_handle
                .set_offset(point(px(0.0), px(0.0)));
            self.ensure_help_loaded_for_key(key, cx);
        } else {
            cx.notify();
        }
    }

    fn ensure_help_loaded_for_key(&mut self, key: String, cx: &mut Context<Self>) {
        if self.help_cache.contains_key(&key) {
            cx.notify();
            return;
        }

        let text = help_texts::text_for_key(self.language, &key)
            .map(|s| s.to_string())
            .unwrap_or_else(|| t(self.language, "message.help_missing").to_string());
        self.help_cache.insert(key, text);
        cx.notify();
    }

    fn render_interaction_helper_panel(&self, entity: Entity<Self>) -> AnyElement {
        let Some(prompt) = self.interaction_helper.prompt().cloned() else {
            return div().into_any_element();
        };

        let language = self.language;
        let has_options = !prompt.options.is_empty();
        let last_submission = self
            .interaction_helper
            .last_submission()
            .map(str::to_string);

        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .pt(px(8.0))
            .border_t_1()
            .border_color(rgb(0x374151))
            .child(muted_label(language, "panel.interaction_helper"))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .relative()
                    .h(px(INTERACTION_HELPER_VIEWPORT_HEIGHT))
                    .id("interaction-helper-scroll")
                    .track_scroll(&self.interaction_helper_scroll_handle)
                    .on_scroll_wheel({
                        let entity = entity.clone();
                        let scroll_handle = self.interaction_helper_scroll_handle.clone();
                        move |event, window, cx| {
                            if scroll_handle_by_wheel(
                                &scroll_handle,
                                event,
                                window.line_height(),
                                false,
                            ) {
                                cx.stop_propagation();
                                cx.notify(entity.entity_id());
                            }
                        }
                    })
                    .overflow_y_scroll()
                    .scrollbar_width(px(8.0))
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .pr(px(12.0))
                            .child(div().text_color(rgb(0x111827)).child(prompt.prompt.clone()))
                            .child(if let Some(sample_path) = prompt.sample_path.clone() {
                                div()
                                    .text_color(rgb(0x4B5563))
                                    .font_family("monospace")
                                    .text_size(px(12.0))
                                    .child(sample_path)
                            } else {
                                div()
                            })
                            .child(if !has_options {
                                div().into_any_element()
                            } else {
                                prompt
                                    .options
                                    .into_iter()
                                    .fold(
                                        div().flex().flex_row().flex_wrap().gap(px(8.0)),
                                        |container, option| {
                                            let entity_click = entity.clone();
                                            let value = option.value.clone();
                                            let button_text =
                                                format!("{}: {}", option.value, option.label);
                                            container.child(
                                                div()
                                                    .id(SharedString::from(format!(
                                                        "interaction-helper-{}",
                                                        value
                                                    )))
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
                                    .into_any_element()
                            })
                            .child(if !has_options {
                                div()
                                    .text_color(rgb(0x4B5563))
                                    .child(t(language, "message.interactive_prompt_enter_hint"))
                            } else {
                                div()
                            })
                            .child(if let Some(last_submission) = last_submission {
                                div().text_color(rgb(0x4B5563)).child(format!(
                                    "{}: {}",
                                    t(language, "label.last_sent"),
                                    last_submission
                                ))
                            } else {
                                div()
                            }),
                    )
                    .child(render_vertical_scrollbar(
                        "interaction-helper-scrollbar",
                        &self.interaction_helper_scroll_handle,
                        ScrollbarDragTarget::InteractionHelper,
                        entity.clone(),
                        false,
                    )),
            )
            .into_any_element()
    }
}

impl Render for SetupView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let root = self.root.clone();
        let serval_binary_input = self.serval_binary_input.clone();
        let language = root.read(cx).language;
        apply_ui_font(
            div()
                .size_full()
                .flex()
                .flex_col()
                .gap(px(10.0))
                .p(px(16.0))
                .bg(rgb(0xFFFFFF)),
            language,
        )
        .child(muted_label(language, "setup.language"))
        .child(
            div()
                .flex()
                .flex_row()
                .gap(px(8.0))
                .child({
                    let root = root.clone();
                    chip_base("setup-lang-en", language == Language::En)
                        .on_click(move |_, window, cx| {
                            root.update(cx, |view, cx| view.set_language(Language::En, window, cx));
                        })
                        .child(t(language, "setup.language_en"))
                })
                .child({
                    let root = root.clone();
                    chip_base("setup-lang-zh-cn", language == Language::ZhCn)
                        .on_click(move |_, window, cx| {
                            root.update(cx, |view, cx| {
                                view.set_language(Language::ZhCn, window, cx)
                            });
                        })
                        .child(t(language, "setup.language_zh_cn"))
                }),
        )
        .child(muted_label(language, "setup.serval_binary"))
        .child(
            div()
                .flex()
                .flex_row()
                .gap(px(8.0))
                .child(div().flex_grow().child(serval_binary_input.clone()))
                .child(browse_button(
                    "browse-serval-binary",
                    &root,
                    language,
                    &serval_binary_input,
                    false,
                    "setup.select_serval_executable",
                )),
        )
        .child({
            let root = root.clone();
            let serval_binary_input = serval_binary_input.clone();
            chip_base("save-serval-binary", true)
                .on_click(move |_, window, cx| {
                    let value = serval_binary_input.read(cx).value().to_string();
                    root.update(cx, |view, cx| {
                        view.set_serval_binary_path(Some(value), window, cx);
                        view.append_output(t(language, "message.binary_updated"), cx);
                    });
                    window.remove_window();
                })
                .child(t(language, "action.save"))
        })
    }
}

impl Render for AboutView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.language;
        let brand_lockup = brand_lockup_image();
        let version_text = format!("v{APP_VERSION}");
        let footer_text = format!("{} {APP_LICENSE}.", t(language, "about.footer_license"));

        apply_ui_font(
            div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(16.0))
                .p(px(20.0))
                .bg(rgb(0xFFFFFF)),
            language,
        )
        .child(div().child(img(brand_lockup).h(px(72.0))))
        .child(
            div()
                .text_center()
                .text_color(rgb(0x6B7280))
                .child(version_text),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(12.0))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_sm()
                                .text_center()
                                .text_color(rgb(0x6B7280))
                                .child(t(language, "about.project_url")),
                        )
                        .child(
                            div()
                                .font_family("monospace")
                                .text_center()
                                .text_color(rgb(0x111827))
                                .child(PROJECT_URL),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_sm()
                                .text_center()
                                .text_color(rgb(0x6B7280))
                                .child(t(language, "about.icon")),
                        )
                        .child(
                            div()
                                .text_center()
                                .text_color(rgb(0x111827))
                                .child(t(language, "about.icon_attribution")),
                        )
                        .child(
                            div()
                                .text_center()
                                .text_color(rgb(0x111827))
                                .child(t(language, "about.wordmark")),
                        ),
                ),
        )
        .child(
            div()
                .text_sm()
                .text_center()
                .text_color(rgb(0x6B7280))
                .child(footer_text),
        )
    }
}

enum OutputEvent {
    Line(String),
    Chunk(String),
    Error(String),
    Exit(i32),
    ProcessHandle(RunningProcessHandle),
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
                    for next in chars.by_ref() {
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

fn sanitize_log_output(input: &str) -> String {
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

fn is_translate_column_preset(value: &str) -> bool {
    TRANSLATE_COLUMN_OPTIONS.contains(&value.trim())
}
impl Render for RootView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if matches!(self.serval_version_status, ServalVersionStatus::Checking)
            && !self.serval_version_refresh_in_flight
        {
            self.queue_serval_version_status_refresh_in(window, cx);
        }

        self.refresh_output_log();

        let entity = cx.entity();
        let brand_lockup = brand_lockup_image();
        let preview = self.command_preview();
        let (active_binary_text, active_binary_color) = self.active_binary_display();
        let observe_selected = self.command_state.kind == CommandKind::Observe;
        let capture_selected = self.command_state.kind == CommandKind::Capture;
        let xmp_selected = self.command_state.kind == CommandKind::Xmp;
        let extract_selected = self.command_state.kind == CommandKind::Extract;
        let translate_from_value = self.command_state.translate.from.clone();
        let translate_to_value = self.command_state.translate.to.clone();
        let translate_from_custom = !is_translate_column_preset(&translate_from_value);
        let translate_to_custom = !is_translate_column_preset(&translate_to_value);
        let language = self.language;
        let helper_mode = self.helper_mode;
        let input_panel_open = self.input_panel_open;
        let preview_panel_open = self.preview_panel_open;
        let running = self.run_state.is_running();
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
        let interaction_helper_panel = if running {
            self.render_interaction_helper_panel(entity.clone())
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
                .child(muted_label(language, "label.media_dir"))
                .child(input_row(
                    &observe_input,
                    browse_button(
                        "browse-observe",
                        &entity,
                        language,
                        &observe_input,
                        true,
                        "prompt.select_media_directory",
                    ),
                ))
                .child(muted_label(language, "label.output_dir"))
                .child(input_row(
                    &observe_output_input,
                    browse_button(
                        "browse-observe-output",
                        &entity,
                        language,
                        &observe_output_input,
                        true,
                        "prompt.select_output_directory",
                    ),
                ))
                .child(self.render_auto_output_dir_hint())
                .child(muted_label(language, "label.options"))
                .child(
                    chip_row()
                        .child(chip(
                            "toggle-observe-xmp",
                            t(language, "opt.xmp"),
                            self.command_state.observe.xmp,
                            &entity,
                            Some("observe|--xmp"),
                            |view, cx| {
                                view.command_state.observe.xmp = !view.command_state.observe.xmp;
                                cx.notify();
                            },
                        ))
                        .child(chip(
                            "toggle-observe-video",
                            t(language, "opt.video_only"),
                            self.command_state.observe.video_only,
                            &entity,
                            Some("observe|--video"),
                            |view, cx| {
                                view.command_state.observe.video_only =
                                    !view.command_state.observe.video_only;
                                if view.command_state.observe.video_only {
                                    view.command_state.observe.image_only = false;
                                }
                                cx.notify();
                            },
                        ))
                        .child(chip(
                            "toggle-observe-image",
                            t(language, "opt.image_only"),
                            self.command_state.observe.image_only,
                            &entity,
                            Some("observe|--image"),
                            |view, cx| {
                                view.command_state.observe.image_only =
                                    !view.command_state.observe.image_only;
                                if view.command_state.observe.image_only {
                                    view.command_state.observe.video_only = false;
                                }
                                cx.notify();
                            },
                        ))
                        .child(chip(
                            "toggle-observe-debug",
                            t(language, "opt.debug"),
                            self.command_state.observe.debug,
                            &entity,
                            Some("observe|--debug"),
                            |view, cx| {
                                view.command_state.observe.debug =
                                    !view.command_state.observe.debug;
                                cx.notify();
                            },
                        )),
                )
        } else if capture_selected {
            div()
                .flex()
                .flex_col()
                .gap(px(12.0))
                .child(muted_label(language, "label.csv_path"))
                .child(input_row(
                    &capture_input,
                    browse_button(
                        "browse-capture",
                        &entity,
                        language,
                        &capture_input,
                        false,
                        "prompt.select_tags_csv",
                    ),
                ))
                .child(muted_label(language, "label.output_dir"))
                .child(input_row(
                    &capture_output_input,
                    browse_button(
                        "browse-capture-output",
                        &entity,
                        language,
                        &capture_output_input,
                        true,
                        "prompt.select_output_directory",
                    ),
                ))
                .child(self.render_auto_output_dir_hint())
                .child(muted_label(language, "label.options"))
                .child(
                    chip_row()
                        .child(chip(
                            "toggle-capture-event",
                            t(language, "opt.event"),
                            self.command_state.capture.event,
                            &entity,
                            Some("capture|--event"),
                            |view, cx| {
                                view.command_state.capture.event =
                                    !view.command_state.capture.event;
                                cx.notify();
                            },
                        ))
                        .child(chip(
                            "toggle-capture-no-exclude",
                            t(language, "opt.no_exclude"),
                            self.command_state.capture.no_exclude,
                            &entity,
                            Some("capture|--no-exclude"),
                            |view, cx| {
                                view.command_state.capture.no_exclude =
                                    !view.command_state.capture.no_exclude;
                                cx.notify();
                            },
                        ))
                        .child(chip(
                            "toggle-capture-camtrap",
                            t(language, "opt.camtrap_dp"),
                            self.command_state.capture.camtrap_dp,
                            &entity,
                            Some("capture|--camtrap-dp"),
                            |view, cx| {
                                view.command_state.capture.camtrap_dp =
                                    !view.command_state.capture.camtrap_dp;
                                cx.notify();
                            },
                        )),
                )
        } else if xmp_selected {
            let subcommand = self.command_state.xmp.subcommand;
            let subcommand_chip = |id, label_key, target: XmpSubcommand| {
                chip(
                    id,
                    t(language, label_key),
                    subcommand == target,
                    &entity,
                    None,
                    move |view, cx| {
                        view.command_state.xmp.subcommand = target;
                        view.refresh_command_help(cx);
                    },
                )
            };
            div()
                .flex()
                .flex_col()
                .gap(px(12.0))
                .child(muted_label(language, "label.xmp_subcommand"))
                .child(
                    chip_row()
                        .child(subcommand_chip("xmp-copy", "opt.copy", XmpSubcommand::Copy))
                        .child(subcommand_chip("xmp-init", "opt.init", XmpSubcommand::Init))
                        .child(subcommand_chip(
                            "xmp-update",
                            "opt.update",
                            XmpSubcommand::Update,
                        ))
                        .child(subcommand_chip(
                            "xmp-remove",
                            "opt.remove",
                            XmpSubcommand::Remove,
                        ))
                        .child(subcommand_chip("xmp-sync", "opt.sync", XmpSubcommand::Sync)),
                )
                .child(
                    if subcommand == XmpSubcommand::Copy
                        || subcommand == XmpSubcommand::Init
                        || subcommand == XmpSubcommand::Remove
                    {
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .child(muted_label(language, "label.source_dir"))
                            .child(input_row(
                                &xmp_source_input,
                                browse_button(
                                    "browse-xmp-source",
                                    &entity,
                                    language,
                                    &xmp_source_input,
                                    true,
                                    "prompt.select_source_directory",
                                ),
                            ))
                            .child(if subcommand == XmpSubcommand::Copy {
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(8.0))
                                    .child(muted_label(language, "label.output_dir"))
                                    .child(input_row(
                                        &xmp_output_input,
                                        browse_button(
                                            "browse-xmp-output",
                                            &entity,
                                            language,
                                            &xmp_output_input,
                                            true,
                                            "prompt.select_output_directory",
                                        ),
                                    ))
                            } else if subcommand == XmpSubcommand::Init {
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(8.0))
                                    .child(muted_label(language, "label.options"))
                                    .child(chip_row().child(chip(
                                        "toggle-xmp-init-info",
                                        t(language, "opt.info"),
                                        self.command_state.xmp.info,
                                        &entity,
                                        Some("xmp-init|--info"),
                                        |view, cx| {
                                            view.command_state.xmp.info =
                                                !view.command_state.xmp.info;
                                            cx.notify();
                                        },
                                    )))
                            } else {
                                div()
                            })
                    } else if subcommand == XmpSubcommand::Update {
                        let tag_chip = |id, label_key, tag: &'static str| {
                            chip(
                                id,
                                t(language, label_key),
                                self.command_state.xmp.tag_type.as_deref() == Some(tag),
                                &entity,
                                Some(match tag {
                                    "species" => "xmp-update|species",
                                    "individual" => "xmp-update|individual",
                                    "count" => "xmp-update|count",
                                    "sex" => "xmp-update|sex",
                                    _ => "xmp-update|bodypart",
                                }),
                                move |view, cx| {
                                    view.command_state.xmp.tag_type = Some(tag.to_string());
                                    view.command_state.xmp.datetime = false;
                                    cx.notify();
                                },
                            )
                        };
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .child(muted_label(language, "label.csv_path"))
                            .child(input_row(
                                &xmp_csv_input,
                                browse_button(
                                    "browse-xmp-csv",
                                    &entity,
                                    language,
                                    &xmp_csv_input,
                                    false,
                                    "prompt.select_csv_path",
                                ),
                            ))
                            .child(muted_label(language, "label.options"))
                            .child(
                                chip_row()
                                    .child(chip(
                                        "xmp-update-datetime",
                                        t(language, "opt.datetime"),
                                        self.command_state.xmp.datetime,
                                        &entity,
                                        Some("xmp-update|--datetime"),
                                        |view, cx| {
                                            view.command_state.xmp.datetime =
                                                !view.command_state.xmp.datetime;
                                            cx.notify();
                                        },
                                    ))
                                    .child(tag_chip("xmp-tag-species", "opt.species", "species"))
                                    .child(tag_chip(
                                        "xmp-tag-individual",
                                        "opt.individual",
                                        "individual",
                                    ))
                                    .child(tag_chip("xmp-tag-count", "opt.count", "count"))
                                    .child(tag_chip("xmp-tag-sex", "opt.sex", "sex"))
                                    .child(tag_chip(
                                        "xmp-tag-bodypart",
                                        "opt.bodypart",
                                        "bodypart",
                                    )),
                            )
                    } else {
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .child(muted_label(language, "label.dir_optional"))
                            .child(input_row(
                                &xmp_dir_input,
                                browse_button(
                                    "browse-xmp-dir",
                                    &entity,
                                    language,
                                    &xmp_dir_input,
                                    true,
                                    "prompt.select_directory",
                                ),
                            ))
                            .child(muted_label(language, "label.csv_path_optional"))
                            .child(input_row(
                                &xmp_csv_input,
                                browse_button(
                                    "browse-xmp-sync-csv",
                                    &entity,
                                    language,
                                    &xmp_csv_input,
                                    false,
                                    "prompt.select_csv_path",
                                ),
                            ))
                    },
                )
        } else if extract_selected {
            let filter_chip = |id, label_key, filter: &'static str| {
                chip(
                    id,
                    t(language, label_key),
                    self.command_state.extract.filter_type == filter,
                    &entity,
                    None,
                    move |view, cx| {
                        view.command_state.extract.filter_type = filter.to_string();
                        cx.notify();
                    },
                )
            };
            let subdir_chip = |id, label_key, subdir: &'static str| {
                chip(
                    id,
                    t(language, label_key),
                    self.command_state.extract.subdir_type.as_deref() == Some(subdir),
                    &entity,
                    None,
                    move |view, cx| {
                        view.command_state.extract.subdir_type = Some(subdir.to_string());
                        cx.notify();
                    },
                )
            };
            div()
                .flex()
                .flex_col()
                .gap(px(12.0))
                .child(muted_label(language, "label.csv_path"))
                .child(input_row(
                    &extract_csv_input,
                    browse_button(
                        "browse-extract-csv",
                        &entity,
                        language,
                        &extract_csv_input,
                        false,
                        "prompt.select_tags_csv",
                    ),
                ))
                .child(muted_label(language, "label.filter_type"))
                .child(
                    chip_row()
                        .child(filter_chip(
                            "extract-filter-species",
                            "opt.extract_species",
                            "species",
                        ))
                        .child(filter_chip(
                            "extract-filter-path",
                            "opt.extract_path",
                            "path",
                        ))
                        .child(filter_chip(
                            "extract-filter-individual",
                            "opt.extract_individual",
                            "individual",
                        ))
                        .child(filter_chip(
                            "extract-filter-rating",
                            "opt.extract_rating",
                            "rating",
                        ))
                        .child(filter_chip(
                            "extract-filter-event",
                            "opt.extract_event",
                            "event",
                        ))
                        .child(filter_chip(
                            "extract-filter-custom",
                            "opt.extract_custom",
                            "custom",
                        ))
                        .child(filter_chip(
                            "extract-filter-advanced",
                            "opt.extract_advanced",
                            "advanced",
                        )),
                )
                .child(muted_label(language, "label.value"))
                .child(bare_input_row(&extract_value_input))
                .child(muted_label(language, "label.output_dir"))
                .child(input_row(
                    &extract_output_input,
                    browse_button(
                        "browse-extract-output",
                        &entity,
                        language,
                        &extract_output_input,
                        true,
                        "prompt.select_output_directory",
                    ),
                ))
                .child(self.render_auto_output_dir_hint())
                .child(muted_label(language, "label.options"))
                .child(
                    chip_row()
                        .child(chip(
                            "extract-rename",
                            t(language, "opt.rename"),
                            self.command_state.extract.rename,
                            &entity,
                            Some("extract|--rename"),
                            |view, cx| {
                                view.command_state.extract.rename =
                                    !view.command_state.extract.rename;
                                cx.notify();
                            },
                        ))
                        .child(chip(
                            "extract-skip-existing",
                            t(language, "opt.skip_existing"),
                            self.command_state.extract.skip_existing,
                            &entity,
                            Some("extract|--skip-existing"),
                            |view, cx| {
                                view.command_state.extract.skip_existing =
                                    !view.command_state.extract.skip_existing;
                                cx.notify();
                            },
                        ))
                        .child(chip(
                            "extract-use-subdir",
                            t(language, "opt.use_subdir"),
                            self.command_state.extract.use_subdir,
                            &entity,
                            Some("extract|--use-subdir"),
                            |view, cx| {
                                view.command_state.extract.use_subdir =
                                    !view.command_state.extract.use_subdir;
                                cx.notify();
                            },
                        )),
                )
                .child(muted_label(language, "label.subdir_type"))
                .child(
                    chip_row()
                        .child(subdir_chip(
                            "extract-subdir-species",
                            "opt.extract_species",
                            "species",
                        ))
                        .child(subdir_chip(
                            "extract-subdir-individual",
                            "opt.extract_individual",
                            "individual",
                        ))
                        .child(subdir_chip(
                            "extract-subdir-rating",
                            "opt.extract_rating",
                            "rating",
                        ))
                        .child(subdir_chip(
                            "extract-subdir-custom",
                            "opt.extract_custom",
                            "custom",
                        )),
                )
        } else {
            let from_chip = |id, label_key, column: &'static str| {
                chip(
                    id,
                    t(language, label_key),
                    translate_from_value == column,
                    &entity,
                    None,
                    move |view, cx| {
                        view.set_translate_from_value(column.to_string(), cx);
                    },
                )
            };
            let to_chip = |id, label_key, column: &'static str| {
                chip(
                    id,
                    t(language, label_key),
                    translate_to_value == column,
                    &entity,
                    None,
                    move |view, cx| {
                        view.set_translate_to_value(column.to_string(), cx);
                    },
                )
            };
            div()
                .flex()
                .flex_col()
                .gap(px(12.0))
                .child(muted_label(language, "label.csv_path"))
                .child(input_row(
                    &translate_csv_input,
                    browse_button(
                        "browse-translate-csv",
                        &entity,
                        language,
                        &translate_csv_input,
                        false,
                        "prompt.select_tags_csv",
                    ),
                ))
                .child(muted_label(language, "label.taglist_path"))
                .child(input_row(
                    &translate_taglist_input,
                    browse_button(
                        "browse-translate-taglist",
                        &entity,
                        language,
                        &translate_taglist_input,
                        false,
                        "prompt.select_taglist_csv",
                    ),
                ))
                .child(muted_label(language, "label.from"))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .child(
                            chip_row()
                                .child(from_chip(
                                    "translate-from-tag",
                                    "translate.preset.tag",
                                    "tag",
                                ))
                                .child(from_chip(
                                    "translate-from-tagcn",
                                    "translate.preset.tag_cn",
                                    "tagCN",
                                ))
                                .child(from_chip(
                                    "translate-from-mazenamecn",
                                    "translate.preset.maze_name_cn",
                                    "mazeNameCN",
                                ))
                                .child(from_chip(
                                    "translate-from-mazescientificname",
                                    "translate.preset.maze_scientific_name",
                                    "mazeScientificName",
                                ))
                                .child(chip(
                                    "translate-from-custom",
                                    t(language, "action.custom"),
                                    translate_from_custom,
                                    &entity,
                                    None,
                                    |view, cx| {
                                        view.activate_translate_from_custom(cx);
                                    },
                                )),
                        )
                        .child(if translate_from_custom {
                            bare_input_row(&translate_from_input)
                        } else {
                            div()
                        }),
                )
                .child(muted_label(language, "label.to"))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .child(
                            chip_row()
                                .child(to_chip("translate-to-tag", "translate.preset.tag", "tag"))
                                .child(to_chip(
                                    "translate-to-tagcn",
                                    "translate.preset.tag_cn",
                                    "tagCN",
                                ))
                                .child(to_chip(
                                    "translate-to-mazenamecn",
                                    "translate.preset.maze_name_cn",
                                    "mazeNameCN",
                                ))
                                .child(to_chip(
                                    "translate-to-mazescientificname",
                                    "translate.preset.maze_scientific_name",
                                    "mazeScientificName",
                                ))
                                .child(chip(
                                    "translate-to-custom",
                                    t(language, "action.custom"),
                                    translate_to_custom,
                                    &entity,
                                    None,
                                    |view, cx| {
                                        view.activate_translate_to_custom(cx);
                                    },
                                )),
                        )
                        .child(if translate_to_custom {
                            bare_input_row(&translate_to_input)
                        } else {
                            div()
                        }),
                )
                .child(muted_label(language, "label.output_dir"))
                .child(input_row(
                    &translate_output_input,
                    browse_button(
                        "browse-translate-output",
                        &entity,
                        language,
                        &translate_output_input,
                        true,
                        "prompt.select_output_directory",
                    ),
                ))
                .child(self.render_auto_output_dir_hint())
        };

        apply_ui_font(
            div()
                .size_full()
                .relative()
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
                }),
            language,
        )
        .child(
            div()
                .id("page-scroll")
                .size_full()
                .track_scroll(&self.page_scroll_handle)
                .on_scroll_wheel({
                    let entity = entity.clone();
                    let scroll_handle = self.page_scroll_handle.clone();
                    move |event, window, cx| {
                        if scroll_handle_by_wheel(
                            &scroll_handle,
                            event,
                            window.line_height(),
                            false,
                        ) {
                            cx.stop_propagation();
                            cx.notify(entity.entity_id());
                        }
                    }
                })
                .overflow_y_scroll()
                .overflow_x_hidden()
                .scrollbar_width(px(8.0))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .p(px(24.0))
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .flex_wrap()
                                .items_center()
                                .justify_between()
                                .gap(px(12.0))
                                .child(div().flex().child(img(brand_lockup).h(px(64.0))))
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .items_end()
                                        .gap(px(8.0))
                                        .child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .items_end()
                                                .gap(px(4.0))
                                                .text_right()
                                                .child(
                                                    div()
                                                        .text_sm()
                                                        .text_color(rgb(active_binary_color))
                                                        .child(format!(
                                                            "{}: {active_binary_text}",
                                                            t(language, "app.active_binary")
                                                        )),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .flex_row()
                                                .flex_wrap()
                                                .justify_end()
                                                .gap(px(8.0))
                                                .child(chip(
                                                    "open-input-dir",
                                                    t(language, "action.open_input_dir"),
                                                    false,
                                                    &entity,
                                                    None,
                                                    move |view, cx| {
                                                        if let Some(path) = view.resolve_input_dir()
                                                        {
                                                            cx.reveal_path(&path);
                                                        } else {
                                                            view.append_output(
                                                                t(language, "message.no_input_dir"),
                                                                cx,
                                                            );
                                                        }
                                                    },
                                                ))
                                                .child(chip(
                                                    "open-output-dir",
                                                    t(language, "action.open_output_dir"),
                                                    false,
                                                    &entity,
                                                    None,
                                                    move |view, cx| {
                                                        if let Some(path) =
                                                            view.resolve_output_dir()
                                                        {
                                                            cx.reveal_path(&path);
                                                        } else {
                                                            view.append_output(
                                                                t(
                                                                    language,
                                                                    "message.no_output_dir",
                                                                ),
                                                                cx,
                                                            );
                                                        }
                                                    },
                                                ))
                                                .child({
                                                    let entity_setup = entity.clone();
                                                    chip_base("open-setup", false)
                                                        .on_click(move |_, _window, cx| {
                                                            let setup_state = entity_setup.read(cx);
                                                            let current = setup_state
                                                                .serval_binary_path
                                                                .clone()
                                                                .unwrap_or_default();
                                                            let language = setup_state.language;
                                                            let root_for_window =
                                                                entity_setup.clone();
                                                            let _ = cx.open_window(
                                                                setup_window_options(cx),
                                                                move |_, app| {
                                                                    let root =
                                                                        root_for_window.clone();
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
                                                                            serval_binary_input:
                                                                                input,
                                                                        }
                                                                    })
                                                                },
                                                            );
                                                        })
                                                        .child(t(language, "action.setup"))
                                                })
                                                .child({
                                                    let entity_about = entity.clone();
                                                    chip_base("open-about", false)
                                                        .on_click(move |_, _window, cx| {
                                                            let language =
                                                                entity_about.read(cx).language;
                                                            let _ = cx.open_window(
                                                                about_window_options(cx),
                                                                move |_, app| {
                                                                    app.new(|_| AboutView {
                                                                        language,
                                                                    })
                                                                },
                                                            );
                                                        })
                                                        .child(t(language, "action.about"))
                                                })
                                                .child(chip(
                                                    "toggle-helper-mode",
                                                    if helper_mode {
                                                        t(language, "action.helper_mode_on")
                                                    } else {
                                                        t(language, "action.helper_mode_off")
                                                    },
                                                    helper_mode,
                                                    &entity,
                                                    None,
                                                    |view, cx| {
                                                        view.helper_mode = !view.helper_mode;
                                                        if view.helper_mode {
                                                            view.command_help_open = true;
                                                            view.refresh_command_help(cx);
                                                        } else {
                                                            view.command_help_open = false;
                                                            view.command_help_key = None;
                                                            view.hover_help_key = None;
                                                            view.option_help_position = None;
                                                            cx.notify();
                                                        }
                                                    },
                                                ))
                                                .child({
                                                    let entity_run = entity.clone();
                                                    chip_base("run-button", true)
                                                        .on_click(move |_, window, cx| {
                                                            RootView::handle_run_click(
                                                                entity_run.clone(),
                                                                language,
                                                                window,
                                                                cx,
                                                            );
                                                        })
                                                        .child(t(
                                                            language,
                                                            self.run_state.action_key(),
                                                        ))
                                                }),
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(8.0))
                                .bg(rgb(0xFFFFFF))
                                .p(px(16.0))
                                .child(muted_label(language, "panel.command"))
                                .child({
                                    let command_chip = |id, label_key, kind: CommandKind| {
                                        chip(
                                            id,
                                            t(language, label_key),
                                            self.command_state.kind == kind,
                                            &entity,
                                            None,
                                            move |view, cx| view.select_command(kind, cx),
                                        )
                                    };
                                    chip_row()
                                        .child(command_chip(
                                            "command-observe",
                                            "cmd.observe",
                                            CommandKind::Observe,
                                        ))
                                        .child(command_chip(
                                            "command-capture",
                                            "cmd.capture",
                                            CommandKind::Capture,
                                        ))
                                        .child(command_chip(
                                            "command-xmp",
                                            "cmd.xmp",
                                            CommandKind::Xmp,
                                        ))
                                        .child(command_chip(
                                            "command-extract",
                                            "cmd.extract",
                                            CommandKind::Extract,
                                        ))
                                        .child(command_chip(
                                            "command-translate",
                                            "cmd.translate",
                                            CommandKind::Translate,
                                        ))
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
                                        .child(muted_label(language, "panel.inputs"))
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
                                                                view.input_panel_open =
                                                                    !view.input_panel_open;
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
                                .child(if input_panel_open {
                                    input_section
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
                                        .child(muted_label(language, "panel.command_preview"))
                                        .child(div().text_color(rgb(0x6B7280)).child(
                                            if preview_panel_open {
                                                t(language, "action.collapse")
                                            } else {
                                                t(language, "action.expand")
                                            },
                                        ))
                                })
                                .child(if preview_panel_open {
                                    div().flex().flex_col().gap(px(8.0)).child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .flex_wrap()
                                            .items_start()
                                            .gap(px(8.0))
                                            .bg(rgb(0xF3F4F6))
                                            .text_color(rgb(0x111827))
                                            .p(px(8.0))
                                            .child(
                                                div().flex_grow().min_w(px(320.0)).child(preview),
                                            )
                                            .child({
                                                let entity = entity.clone();
                                                div()
                                                    .id("copy-command")
                                                    .bg(rgb(0xE5E7EB))
                                                    .text_color(rgb(0x111827))
                                                    .p(px(8.0))
                                                    .cursor_pointer()
                                                    .on_click(move |_, _, cx| {
                                                        entity.update(cx, |view, cx| {
                                                            cx.write_to_clipboard(
                                                                ClipboardItem::new_string(
                                                                    view.command_preview(),
                                                                ),
                                                            );
                                                        });
                                                    })
                                                    .child(t(language, "action.copy_command"))
                                            })
                                            .child({
                                                let entity_run = entity.clone();
                                                chip_base("run-button-preview-panel", true)
                                                    .on_click(move |_, window, cx| {
                                                        RootView::handle_run_click(
                                                            entity_run.clone(),
                                                            language,
                                                            window,
                                                            cx,
                                                        );
                                                    })
                                                    .child(t(language, self.run_state.action_key()))
                                            }),
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
                                .min_h(px(220.0))
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .justify_between()
                                        .items_center()
                                        .child(
                                            div()
                                                .flex()
                                                .flex_row()
                                                .gap(px(8.0))
                                                .items_center()
                                                .child(
                                                    div()
                                                        .text_color(rgb(0x9CA3AF))
                                                        .child(t(language, "panel.output")),
                                                )
                                                .child(
                                                    div()
                                                        .text_color(rgb(self.run_state.color()))
                                                        .child(format!(
                                                            "{}: {}",
                                                            t(language, "label.status"),
                                                            t(language, self.run_state.label_key())
                                                        )),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .flex_row()
                                                .gap(px(8.0))
                                                .child({
                                                    let entity = entity.clone();
                                                    chip_base("copy-log", true)
                                                        .on_click(move |_, _, cx| {
                                                            entity.update(cx, |view, cx| {
                                                                view.refresh_output_log();
                                                                cx.write_to_clipboard(
                                                                    ClipboardItem::new_string(
                                                                        view.output_log.clone(),
                                                                    ),
                                                                );
                                                            });
                                                        })
                                                        .child(t(language, "action.copy_log"))
                                                })
                                                .child({
                                                    let entity = entity.clone();
                                                    chip_base("clear-log", true)
                                                        .on_click(move |_, _, cx| {
                                                            entity.update(cx, |view, cx| {
                                                                view.clear_output(cx);
                                                            });
                                                        })
                                                        .child(t(language, "action.clear_log"))
                                                }),
                                        ),
                                )
                                .child(
                                    div()
                                        .relative()
                                        .h(px(OUTPUT_VIEWPORT_HEIGHT))
                                        .w_full()
                                        .child(
                                            div()
                                                .font_family("monospace")
                                                .text_size(px(12.0))
                                                .line_height(px(16.0))
                                                .whitespace_nowrap()
                                                .id("output-scroll")
                                                .track_scroll(&self.output_scroll_handle)
                                                .on_scroll_wheel({
                                                    let entity = entity.clone();
                                                    let scroll_handle =
                                                        self.output_scroll_handle.clone();
                                                    move |event, window, cx| {
                                                        if scroll_handle_by_wheel(
                                                            &scroll_handle,
                                                            event,
                                                            window.line_height(),
                                                            true,
                                                        ) {
                                                            cx.stop_propagation();
                                                            cx.notify(entity.entity_id());
                                                        }
                                                    }
                                                })
                                                .size_full()
                                                .pr(px(12.0))
                                                .overflow_y_scroll()
                                                .overflow_x_scroll()
                                                .scrollbar_width(px(8.0))
                                                .child(div().flex_none().child(output_text)),
                                        )
                                        .child(render_vertical_scrollbar(
                                            "output-scrollbar",
                                            &self.output_scroll_handle,
                                            ScrollbarDragTarget::Output,
                                            entity.clone(),
                                            true,
                                        )),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .gap(px(8.0))
                                        .pt(px(8.0))
                                        .child(div().flex_grow().child(pty_input.clone()))
                                        .child({
                                            let entity = entity.clone();
                                            chip_base("send-pty", true)
                                                .on_click(move |_, _, cx| {
                                                    entity.update(cx, |view, cx| {
                                                        view.send_pty_input(cx);
                                                    });
                                                })
                                                .child(t(language, "action.send"))
                                        }),
                                ),
                        )
                        .child(interaction_helper_panel),
                ),
        )
        .child(render_vertical_scrollbar(
            "page-scrollbar",
            &self.page_scroll_handle,
            ScrollbarDragTarget::Page,
            entity.clone(),
            false,
        ))
        .child(if helper_mode && self.command_help_open {
            {
                let base = div()
                    .absolute()
                    .w(px(500.0))
                    .h(px(360.0))
                    .occlude()
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
                            .child(
                                div()
                                    .text_color(rgb(0x9CA3AF))
                                    .child(t(language, "panel.help")),
                            )
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
                            .track_scroll(&self.command_help_scroll_handle)
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
        app.open_window(app_window_options(), |_window, app| {
            app.new(|cx| {
                let (setup_config, setup_load_error) = match SetupConfig::load() {
                    Ok(config) => (config, None),
                    Err(err) => (SetupConfig::default(), Some(err.to_string())),
                };
                let language = setup_config.language;
                let serval_binary_path = setup_config.serval_binary_path.clone();
                let observe_input =
                    cx.new(|cx| TextInput::new(cx, t(language, "placeholder.observe_media_dir")));
                let observe_output_input =
                    cx.new(|cx| TextInput::new(cx, t(language, "placeholder.optional_output_dir")));
                let capture_input =
                    cx.new(|cx| TextInput::new(cx, t(language, "placeholder.tags_csv")));
                let capture_output_input =
                    cx.new(|cx| TextInput::new(cx, t(language, "placeholder.optional_output_dir")));
                let xmp_source_input =
                    cx.new(|cx| TextInput::new(cx, t(language, "placeholder.source_dir")));
                let xmp_output_input =
                    cx.new(|cx| TextInput::new(cx, t(language, "placeholder.output_dir")));
                let xmp_csv_input =
                    cx.new(|cx| TextInput::new(cx, t(language, "placeholder.csv_file")));
                let xmp_dir_input =
                    cx.new(|cx| TextInput::new(cx, t(language, "placeholder.directory")));
                let extract_csv_input =
                    cx.new(|cx| TextInput::new(cx, t(language, "placeholder.tags_csv")));
                let extract_value_input =
                    cx.new(|cx| TextInput::new(cx, t(language, "placeholder.filter_value")));
                let extract_output_input =
                    cx.new(|cx| TextInput::new(cx, t(language, "placeholder.optional_output_dir")));
                let translate_csv_input =
                    cx.new(|cx| TextInput::new(cx, t(language, "placeholder.tags_csv")));
                let translate_taglist_input =
                    cx.new(|cx| TextInput::new(cx, t(language, "placeholder.taglist_csv")));
                let translate_from_input = cx.new(|cx| {
                    TextInput::new_with_value(
                        cx,
                        t(language, "placeholder.translate_from"),
                        DEFAULT_TRANSLATE_FROM,
                    )
                });
                let translate_to_input = cx.new(|cx| {
                    TextInput::new_with_value(
                        cx,
                        t(language, "placeholder.translate_to"),
                        DEFAULT_TRANSLATE_TO,
                    )
                });
                let translate_output_input =
                    cx.new(|cx| TextInput::new(cx, t(language, "placeholder.optional_output_dir")));
                let pty_input = cx.new(|cx| {
                    TextInput::new_submit(cx, t(language, "placeholder.interactive_input"))
                });

                cx.observe(&observe_input, |view: &mut RootView, input, cx| {
                    view.command_state.observe.media_dir = input.read(cx).value().to_string();
                    cx.notify();
                })
                .detach();

                cx.observe(&observe_output_input, |view: &mut RootView, input, cx| {
                    let value = input.read(cx).value().to_string();
                    view.command_state.observe.output_dir = if value.trim().is_empty() {
                        None
                    } else {
                        Some(value)
                    };
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
                    view.command_state.capture.output_dir = if value.trim().is_empty() {
                        None
                    } else {
                        Some(value)
                    };
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
                    view.command_state.extract.output_dir = if value.trim().is_empty() {
                        None
                    } else {
                        Some(value)
                    };
                    cx.notify();
                })
                .detach();

                cx.observe(&translate_csv_input, |view: &mut RootView, input, cx| {
                    view.command_state.translate.csv_path = input.read(cx).value().to_string();
                    cx.notify();
                })
                .detach();

                cx.observe(
                    &translate_taglist_input,
                    |view: &mut RootView, input, cx| {
                        view.command_state.translate.taglist_path =
                            input.read(cx).value().to_string();
                        cx.notify();
                    },
                )
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
                    view.command_state.translate.output_dir = if value.trim().is_empty() {
                        None
                    } else {
                        Some(value)
                    };
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

                let mut root = RootView {
                    command_state: CommandState::default(),
                    language,
                    serval_binary_path,
                    serval_version_status: ServalVersionStatus::Checking,
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
                    terminal_buffer: TerminalBuffer::default(),
                    output_log: String::new(),
                    output_dirty: false,
                    run_state: RunState::Idle,
                    cancel_requested: false,
                    running_command_kind: None,
                    running_interaction_helper: false,
                    running_process: None,
                    pty_writer: None,
                    output_scroll_handle: ScrollHandle::new(),
                    page_scroll_handle: ScrollHandle::new(),
                    interaction_helper_scroll_handle: ScrollHandle::new(),
                    scrollbar_drag_target: None,
                    command_help_scroll_handle: ScrollHandle::new(),
                    interaction_helper: InteractionHelperModel::default(),
                    helper_mode: false,
                    input_panel_open: true,
                    preview_panel_open: true,
                    serval_version_refresh_in_flight: false,
                    help_cache: HashMap::new(),
                    command_help_open: false,
                    command_help_key: None,
                    hover_help_key: None,
                    option_help_position: None,
                    cursor_position: point(px(0.0), px(0.0)),
                };

                if let Some(err) = setup_load_error {
                    root.append_output(
                        format!("{}: {err}", t(root.language, "message.failed_load_setup")),
                        cx,
                    );
                }

                root
            })
        })
        .unwrap();
    });
}

fn app_window_options() -> WindowOptions {
    WindowOptions {
        app_id: Some(APP_ID.to_string()),
        ..WindowOptions::default()
    }
}

fn setup_window_options(cx: &App) -> WindowOptions {
    let mut options = app_window_options();
    options.window_bounds = Some(WindowBounds::centered(size(px(760.0), px(240.0)), cx));
    options.window_min_size = Some(size(px(640.0), px(220.0)));
    options
}

fn about_window_options(cx: &App) -> WindowOptions {
    let mut options = app_window_options();
    options.window_bounds = Some(WindowBounds::centered(size(px(720.0), px(420.0)), cx));
    options.window_min_size = Some(size(px(620.0), px(360.0)));
    options
}

#[cfg(test)]
mod tests {
    use super::{
        commands::CommandKind, RootView, TerminalBuffer, BUFFER_TRIM_BATCH, MAX_BUFFER_LINES,
        MAX_CURSOR_DOWN,
    };

    #[test]
    fn carriage_return_overwrites_current_line() {
        let mut buffer = TerminalBuffer::default();
        buffer.push_chunk("Progress 1");
        buffer.push_chunk("\rProgress 2");
        assert_eq!(buffer.render_text(), "Progress 2");
    }

    #[test]
    fn clear_to_end_of_line_removes_old_progress_tail() {
        let mut buffer = TerminalBuffer::default();
        buffer.push_chunk("100% complete");
        buffer.push_chunk("\r42%\u{001b}[K");
        assert_eq!(buffer.render_text(), "42%");
    }

    #[test]
    fn log_lines_start_after_partial_progress_line() {
        let mut buffer = TerminalBuffer::default();
        buffer.push_chunk("Working...");
        buffer.append_log_line("Done");
        assert_eq!(buffer.render_text(), "Working...\nDone\n");
    }

    #[test]
    fn cursor_up_redraw_keeps_lines_printed_above_progress_bar() {
        // indicatif prints a warning above the bar by moving the cursor up
        // over the previously drawn frame, clearing each line, then writing
        // the warning followed by a fresh bar (console::Term sequences).
        let mut buffer = TerminalBuffer::default();
        buffer.push_chunk("intro line\nold bar");
        buffer.push_chunk("\u{001b}[1A\r\u{001b}[2K\u{001b}[1B\r\u{001b}[2K\u{001b}[1A");
        buffer.push_chunk("Warning: something happened\nnew bar");
        assert_eq!(buffer.render_text(), "Warning: something happened\nnew bar");
    }

    #[test]
    fn erase_below_removes_stale_progress_frame() {
        let mut buffer = TerminalBuffer::default();
        buffer.push_chunk("kept line\nbar line one\nbar line two");
        buffer.push_chunk("\u{001b}[1A\r\u{001b}[0J");
        buffer.push_chunk("final bar");
        assert_eq!(buffer.render_text(), "kept line\nfinal bar");
    }

    #[test]
    fn full_width_padded_lines_wrap_below_like_a_terminal() {
        // indicatif prints lines above the bar by padding each line to the
        // terminal width (no newlines) and letting the cursor wrap; the next
        // frame re-anchors with \r and clears only the bar's own row.
        let mut buffer = TerminalBuffer::default();
        let width = buffer.cols;
        let mut warning = String::from("Warning: renamed");
        warning.push_str(&" ".repeat(width - warning.len()));
        buffer.push_chunk("\r\u{001b}[2K");
        buffer.push_chunk(&warning);
        buffer.push_chunk("[===] 1/2 Copying");
        buffer.push_chunk("\r\u{001b}[2K[=====] 2/2 done");
        assert_eq!(buffer.render_text(), "Warning: renamed\n[=====] 2/2 done");
    }

    #[test]
    fn escape_sequences_split_across_chunks_are_reassembled() {
        let mut buffer = TerminalBuffer::default();
        buffer.push_chunk("old bar");
        buffer.push_chunk("\r\u{001b}");
        buffer.push_chunk("[2Knew bar");
        assert_eq!(buffer.render_text(), "new bar");
    }

    /// Debug helper: replay a captured raw PTY stream through the buffer.
    /// GPUI_REPLAY_FILE=/path/to/stream cargo test replay_ -- --nocapture
    #[test]
    fn replay_captured_pty_stream() {
        let Ok(path) = std::env::var("GPUI_REPLAY_FILE") else {
            return;
        };
        let data = std::fs::read(path).expect("readable capture file");
        let mut buffer = TerminalBuffer::default();
        for chunk in data.chunks(8192) {
            buffer.push_chunk(&String::from_utf8_lossy(chunk));
        }
        println!("{}", buffer.render_text());
    }

    #[test]
    fn cached_render_tracks_interleaved_overwrites() {
        let mut buffer = TerminalBuffer::default();
        buffer.push_chunk("intro\nProgress 1");
        assert_eq!(buffer.render_text(), "intro\nProgress 1");
        buffer.push_chunk("\r\u{001b}[2KProgress 2");
        assert_eq!(buffer.render_text(), "intro\nProgress 2");
        buffer.push_chunk("\nDone");
        assert_eq!(buffer.render_text(), "intro\nProgress 2\nDone");
        buffer.clear();
        assert_eq!(buffer.render_text(), "");
    }

    #[test]
    fn scrollback_is_capped_for_chatty_processes() {
        let mut buffer = TerminalBuffer::default();
        for _ in 0..(MAX_BUFFER_LINES + BUFFER_TRIM_BATCH + 10) {
            buffer.push_chunk("line\n");
        }
        assert!(buffer.lines.len() <= MAX_BUFFER_LINES + BUFFER_TRIM_BATCH + 1);
        assert!(buffer.render_text().ends_with("line\n"));
    }

    #[test]
    fn malformed_cursor_down_parameter_is_clamped() {
        let mut buffer = TerminalBuffer::default();
        buffer.push_chunk("top\n\u{001b}[999999999Bbottom");
        assert!(buffer.lines.len() <= MAX_CURSOR_DOWN + 2);
    }

    #[test]
    fn xmp_commands_use_pty_for_progress_rendering() {
        assert!(RootView::command_uses_pty(CommandKind::Xmp));
        assert!(!RootView::command_uses_pty(CommandKind::Translate));
    }
}
