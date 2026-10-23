//! `waxbill.log` beside `settings.conf`: what happened, for a bug report (start-up, the Serval found, every run and
//! how it ended, review folders, the Caracal bridge). One line per event, times in UTC; at about 1 MB the log
//! moves to `waxbill.old.log` and starts again.

use std::io::Write;
use std::path::PathBuf;

const MAX_BYTES: u64 = 1_000_000;

pub fn path() -> Option<PathBuf> {
    Some(crate::settings::config_dir()?.join("waxbill.log"))
}

/// Append one line. Failing to log never disturbs Waxbill.
pub fn line(text: impl AsRef<str>) {
    let Some(path) = path() else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = std::fs::rename(&path, path.with_file_name("waxbill.old.log"));
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(
            file,
            "{} {}",
            now(),
            text.as_ref().replace(['\n', '\r'], " ")
        );
    }
}

/// "2026-10-23 07:04:05 UTC"
fn now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let day = secs.rem_euclid(86_400);
    format!(
        "{} {:02}:{:02}:{:02} UTC",
        crate::table::date_of(secs),
        day / 3600,
        day % 3600 / 60,
        day % 60
    )
}
