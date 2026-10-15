//! Table check and repair (Design/spec/data.md §2): a chosen CSV is checked for what spreadsheet programs do to
//! it — text that is not UTF-8 (Excel's "CSV" on Chinese Windows saves GBK, "Unicode text" is tab-separated
//! UTF-16) and times rewritten as `2023/10/5 6:09` — and "生成修复后的副本" writes `<name>.utf8.csv` next to it,
//! never touching the original, and points the field at the copy.

use crate::i18n::{Language, count, language, t};
use crate::theme::TokensExt;
use gpui_kit::component::button::Button;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use std::path::{Path, PathBuf};

/// The time columns Serval reads as `yyyy-MM-dd HH:mm:ss`.
const TIME_COLUMNS: &[&str] = &["datetime", "datetime_original", "xmp_update_datetime"];

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Encoding {
    Utf8,
    Gbk,
    Utf16,
}

/// What a repaired copy would change.
#[derive(Clone, Debug, PartialEq)]
pub struct Check {
    pub encoding: Encoding,
    /// Tab-separated (Excel's "Unicode text").
    pub tabs: bool,
    /// Time cells `xmp write` would refuse that can be rewritten, and whether some of them have no seconds.
    pub times: usize,
    pub no_seconds: bool,
    /// Time cells that are not a date and time in year-month-day order (e.g. day first): not rewritten.
    pub unknown_times: usize,
}

impl Check {
    pub fn needs_repair(&self) -> bool {
        self.encoding != Encoding::Utf8 || self.tabs || self.times > 0
    }
}

/// The table as text, and how it was stored.
fn decode(bytes: &[u8]) -> Result<(String, Encoding), String> {
    if let Some(rest) = bytes.strip_prefix(b"\xEF\xBB\xBF") {
        return String::from_utf8(rest.to_vec())
            .map(|text| (text, Encoding::Utf8))
            .map_err(|_| {
                t(
                    "表格以 UTF-8 开头，但内容不是 UTF-8",
                    "the table starts as UTF-8 but is not",
                )
                .into()
            });
    }
    for (bom, encoding) in [
        (b"\xFF\xFE", encoding_rs::UTF_16LE),
        (b"\xFE\xFF", encoding_rs::UTF_16BE),
    ] {
        if let Some(rest) = bytes.strip_prefix(bom) {
            return encoding
                .decode_without_bom_handling_and_without_replacement(rest)
                .map(|text| (text.into_owned(), Encoding::Utf16))
                .ok_or_else(|| t("无法识别表格的编码", "cannot tell the table's encoding").into());
        }
    }
    if let Ok(text) = std::str::from_utf8(bytes) {
        return Ok((text.to_string(), Encoding::Utf8));
    }
    encoding_rs::GB18030
        .decode_without_bom_handling_and_without_replacement(bytes)
        .map(|text| (text.into_owned(), Encoding::Gbk))
        .ok_or_else(|| t("无法识别表格的编码", "cannot tell the table's encoding").into())
}

/// `2023/10/5 6:09`, `2023-10-05 6:09:07`, `2023.10.5T06:09` → `2023-10-05 06:09:00` (or `:07`); `None` if the cell
/// is not a date and time in this order. The flag says whether seconds were missing.
fn normal_time(cell: &str) -> Option<(String, bool)> {
    let (date, time) = cell.trim().split_once([' ', 'T'])?;
    let date: Vec<&str> = date.split(['-', '/', '.']).collect();
    let time: Vec<&str> = time.trim().split(':').collect();
    let [y, m, d] = date[..] else { return None };
    let (h, min, s) = match time[..] {
        [h, min] => (h, min, None),
        [h, min, s] => (h, min, Some(s)),
        _ => return None,
    };
    let number = |text: &str, digits: std::ops::RangeInclusive<usize>, max: u32| {
        let n: u32 = text.parse().ok()?;
        (digits.contains(&text.len()) && text.bytes().all(|b| b.is_ascii_digit()) && n <= max)
            .then_some(n)
    };
    let year = number(y, 4..=4, 9999)?;
    let (month, day) = (number(m, 1..=2, 12)?, number(d, 1..=2, 31)?);
    let (hour, minute) = (number(h, 1..=2, 23)?, number(min, 2..=2, 59)?);
    let second = match s {
        Some(s) => number(s, 2..=2, 59)?,
        None => 0,
    };
    (month >= 1 && day >= 1).then(|| {
        (
            format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}"),
            s.is_none(),
        )
    })
}

/// Whether `xmp write` takes the cell as it is: year-month-day with `-`, a space or `T`, and seconds; zero-padding
/// is optional (Serval parses `%Y-%m-%dT%H:%M:%S`). Polars, which reads times everywhere else (capture, `xmp update`),
/// takes much more, so only 整表写入 checks times.
fn serval_writes(cell: &str) -> bool {
    let cell = cell.trim();
    let date = cell.split([' ', 'T']).next().unwrap_or("");
    !date.contains(['/', '.']) && normal_time(cell).is_some_and(|(_, no_seconds)| !no_seconds)
}

/// The table's rows and what was found; with `times`, the time cells `xmp write` would refuse are rewritten as
/// `yyyy-MM-dd HH:mm:ss` (missing seconds become 00), and cells that are not year-month-day are counted, not changed.
fn read(path: &Path, times: bool) -> Result<(Vec<csv::StringRecord>, Check), String> {
    let bytes = std::fs::read(path).map_err(|err| err.to_string())?;
    let (text, encoding) = decode(&bytes)?;
    let text = text.trim_start_matches('\u{feff}');
    let first_line = text.lines().next().unwrap_or("");
    let tabs = first_line.contains('\t') && !first_line.contains(',');
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .delimiter(if tabs { b'\t' } else { b',' })
        .from_reader(text.as_bytes());
    let mut rows: Vec<csv::StringRecord> = reader
        .records()
        .collect::<Result<_, _>>()
        .map_err(|err| err.to_string())?;
    let mut check = Check {
        encoding,
        tabs,
        times: 0,
        no_seconds: false,
        unknown_times: 0,
    };
    let Some(header) = rows.first().cloned() else {
        return Ok((rows, check));
    };
    let time_columns: Vec<usize> = header
        .iter()
        .enumerate()
        .filter(|(_, name)| times && TIME_COLUMNS.contains(&name.trim()))
        .map(|(i, _)| i)
        .collect();
    for row in rows.iter_mut().skip(1) {
        let mut cells: Vec<String> = row.iter().map(str::to_string).collect();
        let mut changed = false;
        for &column in &time_columns {
            let Some(cell) = cells.get_mut(column) else {
                continue;
            };
            if cell.trim().is_empty() || serval_writes(cell) {
                continue;
            }
            match normal_time(cell) {
                Some((normal, no_seconds)) => {
                    *cell = normal;
                    check.times += 1;
                    check.no_seconds |= no_seconds;
                    changed = true;
                }
                None => check.unknown_times += 1,
            }
        }
        if changed {
            *row = csv::StringRecord::from(cells);
        }
    }
    Ok((rows, check))
}

/// The table as UTF-8 text without a BOM, however it was stored.
pub fn text(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|err| err.to_string())?;
    let (text, _) = decode(&bytes)?;
    Ok(text.trim_start_matches('\u{feff}').to_string())
}

/// The cells of one column as written, in any encoding the check reads.
pub fn column(path: &Path, name: &str) -> Result<Vec<String>, String> {
    let (rows, _) = read(path, false)?;
    let Some(index) = rows
        .first()
        .and_then(|header| header.iter().position(|h| h.trim() == name))
    else {
        return Ok(Vec::new());
    };
    Ok(rows
        .iter()
        .skip(1)
        .filter_map(|row| row.get(index).map(str::to_string))
        .collect())
}

/// What a repaired copy would change; `times` also checks the time columns as `xmp write` reads them.
pub fn check(path: &Path, times: bool) -> Result<Check, String> {
    read(path, times).map(|(_, check)| check)
}

/// Write the repaired copy as UTF-8 with a BOM (Serval reads it, Excel opens it correctly) and return its path.
pub fn repair(path: &Path, times: bool) -> Result<PathBuf, String> {
    let (rows, _) = read(path, times)?;
    let mut writer = csv::WriterBuilder::new()
        .flexible(true)
        .from_writer(b"\xEF\xBB\xBF".to_vec());
    for row in &rows {
        writer.write_record(row).map_err(|err| err.to_string())?;
    }
    let bytes = writer.into_inner().map_err(|err| err.to_string())?;
    write_new_copy(path, &bytes)
}

/// Write `<name>.utf8.csv` next to the table, or `<name>.utf8_2.csv` … when that exists: an earlier copy may have
/// been edited since. Each name is created only if it does not exist (atomically), so two repairs at once never
/// write to the same file.
fn write_new_copy(path: &Path, bytes: &[u8]) -> Result<PathBuf, String> {
    use std::io::Write;
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let stem = stem.trim_end_matches(".utf8");
    for n in 1.. {
        let target = path.with_file_name(if n == 1 {
            format!("{stem}.utf8.csv")
        } else {
            format!("{stem}.utf8_{n}.csv")
        });
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
        {
            Ok(mut file) => {
                file.write_all(bytes).map_err(|err| err.to_string())?;
                return Ok(target);
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err.to_string()),
        }
    }
    unreachable!()
}

enum State {
    Nothing,
    Checked(Check),
    Repairing,
    Failed(String),
}

/// The check under a table field: what is wrong and the repair button; nothing when the table is fine. It follows
/// the field by itself, so a form only places it.
pub struct TableCheck {
    input: Entity<InputState>,
    value: SharedString,
    state: State,
    /// The original, while the field shows the copy made from it.
    repaired_from: Option<(PathBuf, PathBuf)>,
    /// Check the time columns too: only where Serval is strict about them (整表写入 writing times).
    times: bool,
    generation: usize,
    _subscription: Subscription,
}

impl TableCheck {
    pub fn new(input: &Entity<InputState>, cx: &mut Context<Self>) -> Self {
        // Observed rather than subscribed: forms also fill the field without an event when restoring.
        let subscription = cx.observe(input, |this: &mut Self, input, cx| {
            let value = input.read(cx).value();
            if value != this.value {
                this.value = value;
                this.recheck(cx);
            }
        });
        TableCheck {
            input: input.clone(),
            value: SharedString::default(),
            state: State::Nothing,
            repaired_from: None,
            times: false,
            generation: 0,
            _subscription: subscription,
        }
    }

    /// Whether the times in the table matter to the command; set by the form.
    pub fn set_check_times(&mut self, times: bool, cx: &mut Context<Self>) {
        if self.times != times {
            self.times = times;
            self.recheck(cx);
        }
    }

    fn recheck(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        let generation = self.generation;
        self.state = State::Nothing;
        cx.notify();
        let path = PathBuf::from(self.value.trim());
        if !path.is_file()
            || !path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("csv"))
        {
            return;
        }
        let times = self.times;
        cx.spawn(async move |this, cx| {
            let checked = cx
                .background_executor()
                .spawn(async move { check(&path, times) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation == generation {
                    this.state = match checked {
                        Ok(check) => State::Checked(check),
                        Err(err) => State::Failed(err),
                    };
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn repair(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let original = PathBuf::from(self.value.trim());
        // The field may have moved on to another table by the time the copy is written.
        let generation = self.generation;
        self.state = State::Repairing;
        cx.notify();
        let input = self.input.clone();
        let times = self.times;
        cx.spawn_in(window, async move |this, cx| {
            let source = original.clone();
            let repaired = cx
                .background_executor()
                .spawn(async move { repair(&source, times) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| match repaired {
                _ if this.generation != generation => {}
                Ok(copy) => {
                    this.repaired_from = Some((original, copy.clone()));
                    input.update(cx, |input, cx| {
                        input.set_value(copy.to_string_lossy().to_string(), window, cx);
                        cx.emit(InputEvent::Change);
                    });
                }
                Err(err) => {
                    this.state = State::Failed(err);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn problems(check: &Check) -> Vec<String> {
        let mut lines = Vec::new();
        match check.encoding {
            Encoding::Utf8 => {}
            Encoding::Gbk => lines.push(
                t(
                    "这个表格是 GBK 编码（Excel 另存的“CSV”），Serval 只能读取 UTF-8。",
                    "This table is GBK-encoded (Excel's plain \"CSV\"); Serval reads UTF-8 only.",
                )
                .to_string(),
            ),
            Encoding::Utf16 => lines.push(
                t(
                    "这个表格是 UTF-16 编码（Excel 的“Unicode 文本”），Serval 只能读取 UTF-8。",
                    "This table is UTF-16 (Excel's \"Unicode text\"); Serval reads UTF-8 only.",
                )
                .to_string(),
            ),
        }
        if check.tabs {
            lines.push(
                t(
                    "列之间用制表符分隔，Serval 需要逗号。",
                    "Columns are separated by tabs; Serval needs commas.",
                )
                .to_string(),
            );
        }
        if check.times > 0 {
            let seconds = check.no_seconds;
            lines.push(match language() {
                Language::Chinese => format!(
                    "{} 个时间无法写入 XMP，需要 yyyy-MM-dd HH:mm:ss 格式；修复时会改为该格式{}。",
                    count(check.times),
                    if seconds { "，缺少的秒数补为 00" } else { "" }
                ),
                Language::English => format!(
                    "{} times cannot be written to XMP as they are (yyyy-MM-dd HH:mm:ss is needed); the repair rewrites them{}.",
                    count(check.times),
                    if seconds { ", with 00 for missing seconds" } else { "" }
                ),
            });
        }
        if check.unknown_times > 0 {
            lines.push(match language() {
                Language::Chinese => format!(
                    "{} 个时间无法识别（例如日/月/年的顺序），请在表格软件中改为 yyyy-MM-dd HH:mm:ss。",
                    count(check.unknown_times)
                ),
                Language::English => format!(
                    "{} times are not recognised (e.g. day first); change them to yyyy-MM-dd HH:mm:ss in the spreadsheet.",
                    count(check.unknown_times)
                ),
            });
        }
        lines
    }
}

impl Render for TableCheck {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.tokens().clone();
        let current = PathBuf::from(self.value.trim());
        let note = self
            .repaired_from
            .as_ref()
            .filter(|(_, copy)| *copy == current)
            .map(|(original, _)| match language() {
                Language::Chinese => format!(
                    "已改用修复后的副本；原表 {} 未改动。",
                    original.file_name().unwrap_or_default().to_string_lossy()
                ),
                Language::English => format!(
                    "Using the repaired copy; the original {} is unchanged.",
                    original.file_name().unwrap_or_default().to_string_lossy()
                ),
            });
        let (lines, button) = match &self.state {
            State::Checked(check) if check.needs_repair() || check.unknown_times > 0 => {
                (Self::problems(check), check.needs_repair())
            }
            State::Repairing => (
                vec![t("正在生成副本…", "Writing the copy…").to_string()],
                false,
            ),
            State::Failed(err) => (vec![err.clone()], false),
            _ => {
                return div().children(note.map(|note| {
                    crate::screens::form::help_line(note, crate::screens::form::Tone::Quiet, cx)
                }));
            }
        };
        div().child(
            h_flex()
                .gap(tokens.px("space-3"))
                .p(tokens.px("space-3"))
                .rounded(tokens.px("radius-sm"))
                .bg(tokens.color("warning-soft"))
                .justify_between()
                .child(
                    v_flex()
                        .min_w_0()
                        .text_size(tokens.px("text-sm"))
                        .children(lines.into_iter().map(|line| div().child(line))),
                )
                .children(button.then(|| {
                    Button::new("repair-table")
                        .outline()
                        .small()
                        .label(t("生成修复后的副本", "Write a repaired copy"))
                        .on_click(cx.listener(|this, _, window, cx| this.repair(window, cx)))
                })),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{Encoding, check, repair};

    #[test]
    fn repairs_a_gbk_table_with_times_changed_by_wps() {
        let dir = std::env::temp_dir().join(format!("waxbill-repair-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let table = dir.join("labels.csv");
        // WPS without seconds (rewritten), unpadded with seconds (xmp write takes it), day first (only counted).
        let text = "path,species,datetime\n/data/雪豹/1.JPG,雪豹,2023-1-5 6:09\n/data/雪豹/2.JPG,雪豹,2023-1-5 6:10:07\n/data/雪豹/3.JPG,雪豹,05/01/2023 06:12\n";
        let (gbk, _, _) = encoding_rs::GBK.encode(text);
        std::fs::write(&table, &gbk).unwrap();

        let found = check(&table, true).unwrap();
        assert_eq!(
            (
                found.encoding,
                found.times,
                found.no_seconds,
                found.unknown_times
            ),
            (Encoding::Gbk, 1, true, 1)
        );
        // Where Serval reads times leniently (Polars), they are not checked.
        assert_eq!(check(&table, false).unwrap().times, 0);
        let copy = repair(&table, true).unwrap();
        assert_eq!(copy, dir.join("labels.utf8.csv"));
        let repaired = std::fs::read(&copy).unwrap();
        assert_eq!(
            String::from_utf8(repaired).unwrap(),
            "\u{feff}path,species,datetime\n/data/雪豹/1.JPG,雪豹,2023-01-05 06:09:00\n/data/雪豹/2.JPG,雪豹,2023-1-5 6:10:07\n/data/雪豹/3.JPG,雪豹,05/01/2023 06:12\n"
        );
        assert_eq!(std::fs::read(&table).unwrap(), gbk.as_ref());

        // Repairs at the same time each get their own copy.
        let copies: std::collections::HashSet<_> = std::thread::scope(|scope| {
            let repairs: Vec<_> = (0..16)
                .map(|_| scope.spawn(|| repair(&table, true).unwrap()))
                .collect();
            repairs.into_iter().map(|r| r.join().unwrap()).collect()
        });
        assert_eq!(copies.len(), 16);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
