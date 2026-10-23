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
/// one shipped next to Waxbill, then one on PATH: the first that works. One that has gone, is too old or does not
/// start never hides a working one after it; when none works, the first one found says why. Blocking; run it in the
/// background.
pub fn find(chosen: Option<PathBuf>) -> ServalStatus {
    let mut first = None;
    for program in [chosen, next_to_waxbill(), find_on_path()]
        .into_iter()
        .flatten()
    {
        match check(program) {
            ready @ ServalStatus::Ready { .. } => return ready,
            ServalStatus::Missing => {}
            status => {
                first.get_or_insert(status);
            }
        }
    }
    first.unwrap_or(ServalStatus::Missing)
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
