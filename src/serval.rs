//! Finding the Serval program and its version (Design/spec/settings.md §1). Waxbill needs Serval 0.8.0 or later,
//! the first release with flags for every question and `--progress json`.

use std::path::{Path, PathBuf};

pub const MIN_VERSION: (u32, u32, u32) = (0, 8, 0);

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

/// Look for `serval` on PATH and ask it for its version. Blocking; run it in the background.
pub fn detect() -> ServalStatus {
    let Some(program) = find_on_path() else {
        return ServalStatus::Missing;
    };
    check(program)
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
    let name = if cfg!(windows) { "serval.exe" } else { "serval" };
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(name))
        .find(|path| path.is_file())
}

fn parse_version(version: &str) -> Option<(u32, u32, u32)> {
    let mut parts = version.split(['.', '-']).map(|p| p.parse::<u32>().ok());
    Some((parts.next()??, parts.next()??, parts.next()??))
}
