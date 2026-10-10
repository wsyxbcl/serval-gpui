//! Reading a tags table before the run (Design/spec/data.md, questions.md §1–2): row count, a sample path, the
//! deployment level Serval would detect, how many deployments a level gives, and the capture rule on real records.
//! The rules mirror Serval's (`detect_deployment_path_index`, `get_temporal_independence`) so what the form shows is
//! what the run will do.

use std::collections::{HashMap, HashSet};
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
            .map_err(|err| err.to_string())?;
        let headers = reader.headers().map_err(|err| err.to_string())?.clone();
        let column = |name: &str| {
            headers
                .iter()
                .position(|h| h.trim_start_matches('\u{feff}') == name)
        };
        let camtrap_dp = column("eventStart").is_some() && column("deploymentID").is_some();
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
            let row = row.map_err(|err| err.to_string())?;
            let cell =
                |i: Option<usize>| i.and_then(|i| row.get(i)).unwrap_or("").trim().to_string();
            records.push(Record {
                path: cell(Some(path)),
                time: parse_time(&cell(time)),
                species: cell(species),
                individual: cell(individual),
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
    fn deployment<'a>(&self, record: &'a Record, level: Option<usize>) -> Option<&'a str> {
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

    /// The capture rule on one busy day of real data: the deployment and target with the most records, on its
    /// busiest day.
    pub fn example_day(
        &self,
        level: Option<usize>,
        by_individual: bool,
        exclude: bool,
    ) -> Option<ExampleDay> {
        let target = |r: &Record| {
            if by_individual {
                r.individual.clone()
            } else {
                r.species.clone()
            }
        };
        let mut groups: HashMap<(String, String), Vec<i64>> = HashMap::new();
        for record in &self.records {
            let (Some(time), Some(deployment)) = (record.time, self.deployment(record, level))
            else {
                continue;
            };
            let target = target(record);
            if target.is_empty() || (exclude && DEFAULT_EXCLUDED.contains(&record.species.as_str()))
            {
                continue;
            }
            groups
                .entry((deployment.to_string(), target))
                .or_default()
                .push(time);
        }
        let ((deployment, target), times) = groups
            .into_iter()
            .max_by(|a, b| a.1.len().cmp(&b.1.len()).then_with(|| b.0.cmp(&a.0)))?;
        let mut per_day: HashMap<i64, Vec<i64>> = HashMap::new();
        for time in times {
            per_day
                .entry(time.div_euclid(86_400))
                .or_default()
                .push(time);
        }
        let (day, mut times) = per_day
            .into_iter()
            .max_by(|a, b| a.1.len().cmp(&b.1.len()).then_with(|| b.0.cmp(&a.0)))?;
        times.sort_unstable();
        times.dedup();
        Some(ExampleDay {
            deployment,
            target,
            day,
            times,
        })
    }
}

pub struct ExampleDay {
    pub deployment: String,
    pub target: String,
    /// Days since 1970-01-01.
    pub day: i64,
    /// Seconds since 1970-01-01, sorted, without duplicates (Serval keeps one record per time).
    pub times: Vec<i64>,
}

impl ExampleDay {
    /// Which records are independent captures: gaps of at least `min_gap` minutes from the previous record, or
    /// from the previous independent one.
    pub fn independent(&self, min_gap: i64, from_last_independent: bool) -> Vec<bool> {
        independent(&self.times, min_gap * 60, from_last_independent)
    }

    pub fn date(&self) -> String {
        let (y, m, d) = civil_from_days(self.day);
        format!("{y:04}-{m:02}-{d:02}")
    }
}

fn independent(times: &[i64], min_gap: i64, from_last_independent: bool) -> Vec<bool> {
    let mut last: Option<i64> = None;
    times
        .iter()
        .map(|&time| {
            let independent = last.is_none_or(|last| time - last >= min_gap);
            if independent || !from_last_independent {
                last = Some(time);
            }
            independent
        })
        .collect()
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
    use super::{detect_level, independent, parse_time};

    #[test]
    fn capture_rule_matches_serval() {
        // Records every 20 minutes with a 30-minute gap: from the last record only the first counts; from the last
        // independent record, the one 40 minutes after it counts too.
        let times: Vec<i64> = [0, 20, 40, 60].iter().map(|m| m * 60).collect();
        assert_eq!(
            independent(&times, 30 * 60, false),
            [true, false, false, false]
        );
        assert_eq!(
            independent(&times, 30 * 60, true),
            [true, false, true, false]
        );
        // A gap of exactly the interval is independent (Serval: >=).
        assert_eq!(independent(&[0, 1800], 1800, false), [true, true]);
    }

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
