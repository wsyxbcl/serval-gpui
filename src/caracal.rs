//! Opening a table in the hosted Caracal (Design/spec/browse.md §2): the page cannot be handed a local file, so
//! Waxbill opens the right mode with the table's name in the fragment (which never leaves the browser), and
//! copies the table's path for the file dialog.

use gpui_kit::{App, ClipboardItem};
use std::path::Path;

pub const URL: &str = "https://caracal.hinature.cn";

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Time,
    Species,
    Review,
}

impl Mode {
    fn key(self) -> &'static str {
        match self {
            Mode::Time => "time",
            Mode::Species => "species",
            Mode::Review => "review",
        }
    }

    /// The analysis a table Serval wrote is made for: observe's tags table → species, xmp init's table → time.
    pub fn for_output(path: &Path) -> Option<Mode> {
        let name = path.file_name()?.to_string_lossy().to_lowercase();
        if !name.ends_with(".csv") {
            return None;
        }
        if name.starts_with("tags_") {
            Some(Mode::Species)
        } else if name.starts_with("xmp_init_") {
            Some(Mode::Time)
        } else {
            None
        }
    }
}

/// `https://caracal.hinature.cn/#review&table=tags_%E5%9C%B0%E9%9D%92.csv`
pub fn link(mode: Mode, table: &Path) -> String {
    let name = table.file_name().unwrap_or_default().to_string_lossy();
    let encoded: String = name
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect();
    format!("{URL}/#{}&table={encoded}", mode.key())
}

pub fn open(mode: Mode, table: &Path, cx: &mut App) {
    cx.write_to_clipboard(ClipboardItem::new_string(
        table.to_string_lossy().into_owned(),
    ));
    cx.open_url(&link(mode, table));
}

#[cfg(test)]
mod tests {
    use super::{Mode, link};
    use std::path::Path;

    #[test]
    fn the_link_names_the_mode_and_the_table() {
        let table = Path::new("/data/地青_serval_output/serval_observe/tags_地青 xmp.csv");
        assert_eq!(Mode::for_output(table), Some(Mode::Species));
        assert_eq!(
            link(Mode::Species, table),
            "https://caracal.hinature.cn/#species&table=tags_%E5%9C%B0%E9%9D%92%20xmp.csv"
        );
    }
}
