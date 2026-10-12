//! Reading a tags table before the run (Design/spec/data.md, questions.md §1–2): row count, a sample path, the
//! deployment level Serval would detect, how many deployments a level gives, and the capture rule on real records.
//! The rules mirror Serval's (`detect_deployment_path_index`, `get_temporal_independence`) so what the form shows is
//! what the run will do.

use std::collections::HashSet;
use std::path::Path;

/// Tags Serval leaves out of capture analysis unless `--no-exclude` is given.
pub const DEFAULT_EXCLUDED: &[&str] = &["Blank", "Useless data", "Unidentified", "Unknown", "Blur"];

/// One record: where it is (a path, or a Camtrap DP deploymentID), when, and what.
#[derive(Clone, Debug)]
pub struct Record {
    pub path: String,
    /// Seconds since 1970-01-01, local time as written in the table.
    pub time: Option<i64>,
    pub species: String,
    pub individual: String,
    pub rating: String,
}

#[derive(Clone, Debug, Default)]
pub struct TagTable {
    pub records: Vec<Record>,
    /// A Camtrap DP observations.csv: the deployment is the `deploymentID` column, no path level is needed.
    pub camtrap_dp: bool,
}

impl TagTable {
    pub fn read(path: &Path) -> Result<TagTable, String> {
        let mut reader = csv::ReaderBuilder::new()
            .flexible(true)
            .from_path(path)
            .map_err(read_error)?;
        let headers = reader.headers().map_err(read_error)?.clone();
        let column = |name: &str| {
            headers
                .iter()
                .position(|h| h.trim_start_matches('\u{feff}') == name)
        };
        let camtrap_dp = column("eventStart").is_some() && column("deploymentID").is_some();
        let rating = column("rating");
        let (path, time, species, individual) = if camtrap_dp {
            (
                column("deploymentID"),
                column("eventStart"),
                column("scientificName"),
                column("individualID"),
            )
        } else {
            (
                column("path"),
                column("datetime").or_else(|| column("datetime_original")),
                column("species"),
                column("individual"),
            )
        };
        let Some(path) = path else {
            return Err("no path column".to_string());
        };
        let mut records = Vec::new();
        for row in reader.records() {
            let row = row.map_err(read_error)?;
            let cell =
                |i: Option<usize>| i.and_then(|i| row.get(i)).unwrap_or("").trim().to_string();
            records.push(Record {
                path: cell(Some(path)),
                time: parse_time(&cell(time)),
                species: cell(species),
                individual: cell(individual),
                rating: cell(rating),
            });
        }
        Ok(TagTable {
            records,
            camtrap_dp,
        })
    }

    pub fn sample_path(&self) -> Option<&str> {
        self.records.first().map(|r| r.path.as_str())
    }

    /// The deployment of a record at `level` (a Camtrap DP deploymentID as it is).
    pub fn deployment<'a>(&self, record: &'a Record, level: Option<usize>) -> Option<&'a str> {
        if self.camtrap_dp {
            return Some(record.path.as_str());
        }
        let level = level?;
        let components: Vec<&str> = record.path.split(['/', '\\']).collect();
        // The last component is the file, never a level.
        (level < components.len().saturating_sub(1)).then(|| components[level])
    }

    pub fn deployment_count(&self, level: Option<usize>) -> usize {
        self.records
            .iter()
            .filter_map(|r| self.deployment(r, level))
            .collect::<HashSet<_>>()
            .len()
    }

    /// Whether paths have different depths (the picker then notes that levels follow the sample).
    pub fn mixed_depths(&self) -> bool {
        let mut depths = self
            .records
            .iter()
            .map(|r| r.path.split(['/', '\\']).count());
        let first = depths.next();
        depths.any(|d| Some(d) != first)
    }
}

/// `yyyy-MM-dd` of a time from [`Record::time`].
pub fn date_of(time: i64) -> String {
    let (y, m, d) = civil_from_days(time.div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

/// The deployment level Serval detects (`detect_deployment_path_index`): skip the levels every path shares, then
/// take the first diverging level, or the next one if it has more distinct names.
pub fn detect_level<'a>(paths: impl IntoIterator<Item = &'a str>) -> Option<usize> {
    let mut level_names: Vec<HashSet<&str>> = Vec::new();
    let mut depth = None;
    for path in paths {
        let components: Vec<&str> = path.split(['/', '\\']).collect();
        if components.len() < 3 {
            return None;
        }
        match depth {
            None => {
                depth = Some(components.len());
                level_names = vec![HashSet::new(); components.len() - 2];
            }
            Some(depth) if depth != components.len() => return None,
            Some(_) => {}
        }
        for (level, name) in components[1..components.len() - 1].iter().enumerate() {
            level_names[level].insert(name);
        }
    }
    let diverge = level_names.iter().position(|names| names.len() > 1)?;
    let level = if diverge + 1 < level_names.len()
        && level_names[diverge + 1].len() > level_names[diverge].len()
    {
        diverge + 1
    } else {
        diverge
    };
    Some(level + 1)
}

/// `yyyy-MM-dd HH:mm:ss` (or with `T`, optionally with seconds fractions or an offset, which are ignored).
fn parse_time(text: &str) -> Option<i64> {
    let text = text.get(..19).unwrap_or(text);
    let number = |range: std::ops::Range<usize>| text.get(range)?.parse::<i64>().ok();
    let (y, m, d) = (number(0..4)?, number(5..7)?, number(8..10)?);
    let (h, min) = (number(11..13).unwrap_or(0), number(14..16).unwrap_or(0));
    let s = number(17..19).unwrap_or(0);
    Some(days_from_civil(y, m, d) * 86_400 + h * 3600 + min * 60 + s)
}

// Howard Hinnant's civil calendar algorithms.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + if m <= 2 { 1 } else { 0 }, m, d)
}

#[cfg(test)]
mod tests {
    use super::{detect_level, parse_time};

    #[test]
    fn deployment_level_is_detected_like_serval() {
        let paths = [
            "/data/2024_spring/col_a/dep01_col_a/100MEDIA/IMG_0001.JPG",
            "/data/2024_spring/col_a/dep02_col_a/100MEDIA/IMG_0001.JPG",
            "/data/2024_spring/col_b/dep03_col_b/100MEDIA/IMG_0001.JPG",
        ];
        assert_eq!(detect_level(paths), Some(4));
        assert_eq!(parse_time("1970-01-02 00:01:05"), Some(86_400 + 65));
    }
}

/// A reading error, said plainly when the table is not UTF-8 (the check below the field offers a repaired copy).
fn read_error(err: csv::Error) -> String {
    match err.kind() {
        csv::ErrorKind::Utf8 { .. } => {
            crate::i18n::t("表格不是 UTF-8 编码", "the table is not UTF-8").to_string()
        }
        _ => err.to_string(),
    }
}

/// A CSV as text: its header and rows (row numbers count the header as line 1, as Serval reports them).
#[derive(Clone, Debug, Default)]
pub struct Csv {
    pub header: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl Csv {
    pub fn read(path: &Path) -> Result<Csv, String> {
        let mut reader = csv::ReaderBuilder::new()
            .flexible(true)
            .from_path(path)
            .map_err(read_error)?;
        let header = reader
            .headers()
            .map_err(read_error)?
            .iter()
            .map(|h| h.trim_start_matches('\u{feff}').trim().to_string())
            .collect();
        let rows = reader
            .records()
            .map(|row| row.map(|r| r.iter().map(|c| c.trim().to_string()).collect()))
            .collect::<Result<_, _>>()
            .map_err(read_error)?;
        Ok(Csv { header, rows })
    }

    pub fn column(&self, name: &str) -> Option<usize> {
        self.header.iter().position(|h| h == name)
    }

    pub fn cell<'a>(&self, row: &'a [String], column: Option<usize>) -> &'a str {
        column
            .and_then(|c| row.get(c))
            .map(String::as_str)
            .unwrap_or("")
    }
}
