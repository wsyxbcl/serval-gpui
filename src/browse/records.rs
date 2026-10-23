//! Browse records (Design/spec/browse.md §2–3): one record per media file, merged from the table's rows; filters
//! on species, rating, deployment and folder with faceted counts (each list counts under the other filters).

use crate::table::Csv;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    pub media: PathBuf,
    pub sidecar: PathBuf,
    pub species: Vec<String>,
    pub individuals: Vec<String>,
    /// 0 is no rating.
    pub rating: u8,
    pub datetime: String,
    pub deployment: String,
    /// The folder below what every path shares ("DQ01/100MEDIA").
    pub folder: String,
    pub video: bool,
    /// The table rows of this file (a file with two species has two), for writing a chosen subset.
    pub rows: Vec<usize>,
}

pub struct Records {
    pub all: Vec<Record>,
}

/// What the filters keep; an empty set keeps everything.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Filters {
    pub species: BTreeSet<String>,
    pub ratings: BTreeSet<u8>,
    pub deployments: BTreeSet<String>,
    pub folders: BTreeSet<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Facet {
    Species,
    Rating,
    Deployment,
    Folder,
}

/// Counts per value of each facet, under the other facets' filters.
#[derive(Default)]
pub struct Counts {
    pub species: Vec<(String, usize)>,
    pub ratings: [usize; 6],
    pub deployments: Vec<(String, usize)>,
    pub folders: Vec<(String, usize)>,
}

impl Records {
    /// Merge the rows of each file: species and individuals collected, rating and time from the first row that has
    /// them (Serval keeps them equal across a file's rows). `level` is the deployment's path level.
    pub fn from_csv(csv: &Csv, level: Option<usize>) -> Records {
        let (path, species, individual, rating, datetime) = (
            csv.column("path"),
            csv.column("species"),
            csv.column("individual"),
            csv.column("rating"),
            csv.column("datetime")
                .or_else(|| csv.column("datetime_original")),
        );
        let mut by_path: BTreeMap<&str, usize> = BTreeMap::new();
        let mut all: Vec<Record> = Vec::new();
        for (row_index, row) in csv.rows.iter().enumerate() {
            let file = csv.cell(row, path);
            if file.is_empty() {
                continue;
            }
            let i = *by_path.entry(file).or_insert_with(|| {
                let given = PathBuf::from(file);
                let is_xmp = given
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("xmp"));
                let media = if is_xmp {
                    given.with_extension("")
                } else {
                    given.clone()
                };
                let sidecar = if is_xmp {
                    given
                } else {
                    media.with_added_extension("xmp")
                };
                let video = media.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                    matches!(e.to_ascii_lowercase().as_str(), "mp4" | "mov" | "avi")
                });
                let deployment = level
                    .and_then(|l| file.split(['/', '\\']).nth(l))
                    .unwrap_or("")
                    .to_string();
                all.push(Record {
                    media,
                    sidecar,
                    species: Vec::new(),
                    individuals: Vec::new(),
                    rating: 0,
                    datetime: String::new(),
                    deployment,
                    folder: String::new(),
                    video,
                    rows: Vec::new(),
                });
                all.len() - 1
            });
            let record = &mut all[i];
            record.rows.push(row_index);
            let add = |list: &mut Vec<String>, value: &str| {
                if !value.is_empty() && !list.iter().any(|v| v == value) {
                    list.push(value.to_string());
                }
            };
            add(&mut record.species, csv.cell(row, species));
            add(&mut record.individuals, csv.cell(row, individual));
            if record.rating == 0 {
                record.rating = csv
                    .cell(row, rating)
                    .parse::<f64>()
                    .map(|r| r.clamp(0.0, 5.0).round() as u8)
                    .unwrap_or(0);
            }
            if record.datetime.is_empty() {
                record.datetime = csv.cell(row, datetime).to_string();
            }
        }
        // Folders below the common prefix of every file's folder.
        let parents: Vec<PathBuf> = all
            .iter()
            .map(|r| r.media.parent().map(Path::to_path_buf).unwrap_or_default())
            .collect();
        let common = common_prefix(&parents);
        for (record, parent) in all.iter_mut().zip(&parents) {
            let relative = parent.strip_prefix(&common).unwrap_or(parent);
            record.folder = relative.to_string_lossy().replace('\\', "/");
            if record.folder.is_empty() {
                record.folder = parent
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
            }
        }
        all.sort_by(|a, b| {
            a.datetime
                .cmp(&b.datetime)
                .then_with(|| a.media.cmp(&b.media))
        });
        Records { all }
    }

    pub fn keeps(filters: &Filters, record: &Record, skip: Option<Facet>) -> bool {
        let species = skip == Some(Facet::Species)
            || filters.species.is_empty()
            || record.species.iter().any(|s| filters.species.contains(s))
            || (record.species.is_empty() && filters.species.contains(""));
        let rating = skip == Some(Facet::Rating)
            || filters.ratings.is_empty()
            || filters.ratings.contains(&record.rating);
        let deployment = skip == Some(Facet::Deployment)
            || filters.deployments.is_empty()
            || filters.deployments.contains(&record.deployment);
        let folder = skip == Some(Facet::Folder)
            || filters.folders.is_empty()
            || filters.folders.contains(&record.folder);
        species && rating && deployment && folder
    }

    /// The records the filters keep, by index, in time order.
    pub fn filter(&self, filters: &Filters) -> Vec<usize> {
        (0..self.all.len())
            .filter(|&i| Self::keeps(filters, &self.all[i], None))
            .collect()
    }

    pub fn counts(&self, filters: &Filters) -> Counts {
        let mut species: BTreeMap<String, usize> = BTreeMap::new();
        let mut deployments: BTreeMap<String, usize> = BTreeMap::new();
        let mut folders: BTreeMap<String, usize> = BTreeMap::new();
        let mut counts = Counts::default();
        for record in &self.all {
            if Self::keeps(filters, record, Some(Facet::Species)) {
                if record.species.is_empty() {
                    *species.entry(String::new()).or_default() += 1;
                }
                for s in &record.species {
                    *species.entry(s.clone()).or_default() += 1;
                }
            }
            if Self::keeps(filters, record, Some(Facet::Rating)) {
                counts.ratings[record.rating as usize] += 1;
            }
            if Self::keeps(filters, record, Some(Facet::Deployment)) {
                *deployments.entry(record.deployment.clone()).or_default() += 1;
            }
            if Self::keeps(filters, record, Some(Facet::Folder)) {
                *folders.entry(record.folder.clone()).or_default() += 1;
            }
        }
        // Species most first; deployments and folders by name.
        counts.species = species.into_iter().collect();
        counts
            .species
            .sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        counts.deployments = deployments.into_iter().collect();
        counts.folders = folders.into_iter().collect();
        counts
    }
}

fn common_prefix(paths: &[PathBuf]) -> PathBuf {
    let Some(first) = paths.first() else {
        return PathBuf::new();
    };
    let mut prefix: Vec<_> = first.components().collect();
    for path in &paths[1..] {
        let shared = prefix
            .iter()
            .zip(path.components())
            .take_while(|(a, b)| **a == *b)
            .count();
        prefix.truncate(shared);
    }
    prefix.iter().collect()
}

#[cfg(test)]
mod tests {
    use super::{Filters, Records};
    use crate::table::Csv;

    #[test]
    fn rows_merge_per_file_and_counts_are_faceted() {
        let csv = Csv {
            header: ["path", "species", "individual", "rating"]
                .map(String::from)
                .to_vec(),
            rows: [
                ["/d/DQ01/a.jpg", "Snow leopard", "SL01", "5"],
                ["/d/DQ01/a.jpg", "Blue sheep", "", "5"],
                ["/d/DQ02/b.jpg", "Snow leopard", "", ""],
                ["/d/DQ02/c.jpg.xmp", "", "", "3"],
            ]
            .iter()
            .map(|r| r.map(String::from).to_vec())
            .collect(),
        };
        let records = Records::from_csv(&csv, Some(2));
        assert_eq!(records.all.len(), 3);
        let a = records
            .all
            .iter()
            .find(|r| r.media.ends_with("a.jpg"))
            .unwrap();
        assert_eq!(
            (
                a.species.len(),
                a.rating,
                a.deployment.as_str(),
                a.folder.as_str()
            ),
            (2, 5, "DQ01", "DQ01")
        );
        assert!(
            records
                .all
                .iter()
                .any(|r| r.media.ends_with("c.jpg") && r.sidecar.ends_with("c.jpg.xmp"))
        );

        let filters = Filters {
            species: ["Snow leopard".to_string()].into(),
            ..Default::default()
        };
        assert_eq!(records.filter(&filters).len(), 2);
        let counts = records.counts(&filters);
        // The species list still counts every species (its own filter is not applied to it) …
        assert!(
            counts
                .species
                .iter()
                .any(|(s, n)| s == "Blue sheep" && *n == 1)
        );
        // … while the rating list counts only snow leopards.
        assert_eq!(counts.ratings, [1, 0, 0, 0, 0, 1]);
    }
}
