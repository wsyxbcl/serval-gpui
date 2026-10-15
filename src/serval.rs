//! Finding the Serval program and its version (Design/spec/settings.md §1). Waxbill needs Serval 0.8.0 or later,
//! the first release with flags for every question and `--progress json`.

use gpui_kit::Global;
use std::path::{Path, PathBuf};

pub const MIN_VERSION: (u32, u32, u32) = (0, 8, 0);

/// "0.8.0": the version messages name, from the one constant the check uses.
pub fn min_version() -> String {
    let (major, minor, patch) = MIN_VERSION;
    format!("{major}.{minor}.{patch}")
}

#[derive(Clone, Debug, PartialEq)]
pub enum ServalStatus {
    Checking,
    Ready { program: PathBuf, version: String },
    Unsupported { program: PathBuf, version: String },
    Missing,
}

impl ServalStatus {
    pub fn program(&self) -> Option<&Path> {
        match self {
            ServalStatus::Ready { program, .. } => Some(program),
            _ => None,
        }
    }
}

/// The Serval to use (settings.md §1): the one chosen in Settings (or with `--serval`) when it works; otherwise the
/// one shipped next to Waxbill, then one on PATH. A chosen program that has gone or is too old does not hide a
/// working one shipped with Waxbill. Blocking; run it in the background.
pub fn find(chosen: Option<PathBuf>) -> ServalStatus {
    let chosen = chosen.map(check);
    if let Some(ready @ ServalStatus::Ready { .. }) = chosen {
        return ready;
    }
    let found = next_to_waxbill().or_else(find_on_path).map(check);
    match (found, chosen) {
        (Some(ready @ ServalStatus::Ready { .. }), _) => ready,
        (_, Some(chosen)) if chosen != ServalStatus::Missing => chosen,
        (found, _) => found.unwrap_or(ServalStatus::Missing),
    }
}

fn program_name() -> &'static str {
    if cfg!(windows) {
        "serval.exe"
    } else {
        "serval"
    }
}

/// `serval.exe` in Waxbill's own folder, as the release archive ships it.
fn next_to_waxbill() -> Option<PathBuf> {
    let program = std::env::current_exe().ok()?.parent()?.join(program_name());
    program.is_file().then_some(program)
}

pub fn check(program: PathBuf) -> ServalStatus {
    let version = crate::runner::background_command(&program)
        .arg("--version")
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| {
            // "Serval 0.8.0"
            String::from_utf8_lossy(&out.stdout)
                .split_whitespace()
                .last()
                .map(str::to_string)
        });
    match version {
        Some(version) if parse_version(&version).is_some_and(|v| v >= MIN_VERSION) => {
            ServalStatus::Ready { program, version }
        }
        Some(version) => ServalStatus::Unsupported { program, version },
        None => ServalStatus::Missing,
    }
}

fn find_on_path() -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(program_name()))
        .find(|path| path.is_file())
}

fn parse_version(version: &str) -> Option<(u32, u32, u32)> {
    let mut parts = version.split(['.', '-']).map(|p| p.parse::<u32>().ok());
    Some((parts.next()??, parts.next()??, parts.next()??))
}

/// The Serval program in use, for work started outside a run (the write preview), set when Serval is found.
pub struct ServalProgram(pub Option<PathBuf>);

impl Global for ServalProgram {}
