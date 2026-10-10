//! Pieces every command form shares (Design/spec/shell.md §3): cards, field labels, help lines, path fields, and
//! the quiet line that says what Waxbill found in an input.

use crate::theme::TokensExt;
use gpui_kit::component::{ActiveTheme, StyledExt, v_flex};
use gpui_kit::*;
use std::path::{Path, PathBuf};

/// A white card holding one group of fields.
pub fn card(cx: &App) -> Div {
    let tokens = cx.tokens();
    v_flex()
        .gap(tokens.px("space-4"))
        .p(tokens.px("space-5"))
        .bg(tokens.color("surface"))
        .border_1()
        .border_color(cx.theme().border)
        .rounded(tokens.px("radius-md"))
}

/// A field: its label above, its control and help lines below.
pub fn field(label: &'static str, cx: &App) -> Div {
    v_flex().gap(cx.tokens().px("space-2")).child(
        div()
            .text_size(cx.tokens().px("text-md"))
            .font_medium()
            .child(label),
    )
}

#[derive(Clone, Copy, PartialEq)]
pub enum Tone {
    Quiet,
    Danger,
}

/// One line under a field: what Waxbill found, or what is wrong.
pub fn help_line(text: impl Into<SharedString>, tone: Tone, cx: &App) -> Div {
    let tokens = cx.tokens();
    div()
        .text_size(tokens.px("text-sm"))
        .text_color(match tone {
            Tone::Quiet => cx.theme().muted_foreground,
            Tone::Danger => tokens.color("danger"),
        })
        .child(text.into())
}

/// What an input folder holds, counted without running Serval.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FolderCount {
    pub images: usize,
    pub videos: usize,
    pub sidecars: usize,
}

const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png"];
const VIDEO_EXTENSIONS: &[&str] = &["avi", "mp4", "mov"];

/// Count media and sidecars under `dir`, skipping Serval's own output folders (as Serval does).
pub fn count_folder(dir: &Path) -> FolderCount {
    let mut count = FolderCount::default();
    let mut stack: Vec<PathBuf> = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                if name != "serval_output" && !name.ends_with("_serval_output") {
                    stack.push(path);
                }
                continue;
            }
            let ext = name.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("");
            if ext == "xmp" {
                count.sidecars += 1;
            } else if IMAGE_EXTENSIONS.contains(&ext) {
                count.images += 1;
            } else if VIDEO_EXTENSIONS.contains(&ext) {
                count.videos += 1;
            }
        }
    }
    count
}
