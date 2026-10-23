//! Opening a table in the hosted Caracal (Design/spec/browse.md §2, caracal-bridge.md). A web page cannot open a
//! local file from its path, so the link carries the mode, the table's path as a hint (in the fragment, which
//! never leaves the browser), and, with the bridge on, where Waxbill serves the table and its media.

use crate::bridge::Bridge;
use gpui_kit::App;
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

    /// The analysis a table Serval wrote is made for: observe's tags table → species, xmp init's table → time,
    /// capture's events table → species. Events tables open only with the columns Caracal's species analysis reads
    /// (`path`, `datetime`, `species`): not per-individual or Camtrap DP results yet, nor Serval before 0.8.2,
    /// whose time column was `time`.
    pub fn for_output(path: &Path, columns: &[String]) -> Option<Mode> {
        let name = path.file_name()?.to_string_lossy().to_lowercase();
        if !name.ends_with(".csv") {
            return None;
        }
        let has = |column: &str| columns.iter().any(|c| c == column);
        if name.starts_with("tags_") {
            Some(Mode::Species)
        } else if name.starts_with("xmp_init_") {
            Some(Mode::Time)
        } else if name.starts_with("events_") && has("path") && has("datetime") && has("species") {
            Some(Mode::Species)
        } else {
            None
        }
    }
}

/// Percent-encoded UTF-8: everything but letters, digits and `-_.~`.
pub fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// `https://caracal.hinature.cn/#review&table=%2Fdata%2Ftags.csv`, and `&source=<base>` for the bridge.
pub fn link(mode: Mode, table: &Path, source: Option<&str>) -> String {
    let mut link = format!(
        "{URL}/#{}&table={}",
        mode.key(),
        encode(&table.to_string_lossy())
    );
    if let Some(source) = source {
        link.push_str(&format!("&source={}", encode(source)));
    }
    link
}

/// Whether Caracal may read the opened table and its media from Waxbill (Settings › Caracal; on by default).
pub fn bridge_enabled(cx: &App) -> bool {
    cx.try_global::<crate::settings::Settings>()
        .and_then(|s| s.value("caracal_bridge"))
        != Some("0")
}

/// Open `table` in Caracal, served by the bridge when it is on (started on first use).
pub fn open(mode: Mode, table: &Path, cx: &mut App) {
    let source = bridge_enabled(cx)
        .then(|| {
            if !cx.has_global::<Bridge>() {
                match Bridge::start() {
                    Ok(bridge) => cx.set_global(bridge),
                    Err(err) => crate::log::line(format!("Caracal bridge did not start: {err}")),
                }
            }
            cx.try_global::<Bridge>().map(|bridge| bridge.share(table))
        })
        .flatten();
    cx.open_url(&link(mode, table, source.as_deref()));
}

#[cfg(test)]
mod tests {
    use super::{Mode, link};
    use std::path::Path;

    #[test]
    fn the_link_names_the_mode_and_the_table() {
        let table = Path::new("/data/地青_serval_output/serval_observe/tags_地青 xmp.csv");
        assert_eq!(Mode::for_output(table, &[]), Some(Mode::Species));
        // Capture's events table: by species with Serval 0.8.2 opens; per individual, Camtrap DP or the old
        // `time` column do not.
        let events = Path::new("/data/serval_capture/events_species_30m_LR.csv");
        let columns = |list: &str| list.split(',').map(str::to_string).collect::<Vec<_>>();
        assert_eq!(
            Mode::for_output(
                events,
                &columns("path,deployment,datetime,species,event_id")
            ),
            Some(Mode::Species)
        );
        for other in [
            "path,deployment,datetime,individual,event_id",
            "observationid,deployment,datetime,species,event_id",
            "path,deployment,time,species,event_id",
        ] {
            assert_eq!(Mode::for_output(events, &columns(other)), None, "{other}");
        }
        assert_eq!(
            link(Mode::Species, table, None),
            "https://caracal.hinature.cn/#species&table=%2Fdata%2F%E5%9C%B0%E9%9D%92_serval_output%2Fserval_observe%2Ftags_%E5%9C%B0%E9%9D%92%20xmp.csv"
        );
    }
}
