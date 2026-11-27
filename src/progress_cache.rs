//! An immutable tags table and the sidecar fingerprints that produced it. Renaming a completed
//! generation publishes both files together. The per-task OS lock is released even after a crash.
use csv::StringRecord;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub table: PathBuf,
    pub changed: usize,
    pub removed: usize,
    pub failed: usize,
    pub scanned_at: SystemTime,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Stamp {
    size: u64,
    modified_ns: i128,
}

impl Stamp {
    fn read(path: &Path) -> io::Result<Self> {
        let meta = fs::metadata(path)?;
        let modified_ns = match meta.modified()?.duration_since(UNIX_EPOCH) {
            Ok(d) => d.as_nanos() as i128,
            Err(e) => -(e.duration().as_nanos() as i128),
        };
        Ok(Self {
            size: meta.len(),
            modified_ns,
        })
    }
}

#[derive(Serialize, Deserialize)]
struct Index {
    #[serde(default)]
    failed: Option<BTreeSet<PathBuf>>,
    scanned_at: SystemTime,
    files: BTreeMap<PathBuf, Stamp>,
}

fn cache_base() -> Result<PathBuf, String> {
    let env = |name| {
        std::env::var_os(name)
            .filter(|p| !p.is_empty())
            .map(PathBuf::from)
    };
    let base = if cfg!(windows) {
        env("LOCALAPPDATA").map(|p| p.join("Waxbill/Cache"))
    } else if cfg!(target_os = "macos") {
        env("HOME").map(|p| p.join("Library/Caches/Waxbill"))
    } else {
        env("XDG_CACHE_HOME")
            .filter(|p| p.is_absolute())
            .or_else(|| env("HOME").map(|p| p.join(".cache")))
            .map(|p| p.join("waxbill"))
    };
    base.map(|p| p.join("progress"))
        .ok_or_else(|| "Cannot locate the local cache folder".into())
}

fn utf8(path: &Path) -> io::Result<&str> {
    path.to_str()
        .ok_or_else(|| io::Error::other(format!("Path is not UTF-8: {}", path.display())))
}

struct TaskLock(File);

impl Drop for TaskLock {
    fn drop(&mut self) {
        // Concurrent subprocess launches can temporarily inherit an open file description.
        let _ = self.0.unlock();
    }
}

struct Task {
    root: PathBuf,
    dir: PathBuf,
}

impl Task {
    fn new(root: &Path, base: &Path) -> io::Result<Self> {
        let root = fs::canonicalize(root)?;
        if !root.is_dir() {
            return Err(io::Error::other(format!(
                "Not a folder: {}",
                root.display()
            )));
        }
        // Stable FNV-1a; the marker checks the full source path before any reuse or deletion.
        let key = utf8(&root)?.bytes().fold(0xcbf29ce484222325u64, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
        });
        Ok(Self {
            root,
            dir: base.join(format!("task-v1-{key:016x}")),
        })
    }

    fn lock(&self) -> io::Result<TaskLock> {
        fs::create_dir_all(&self.dir)?;
        if !fs::symlink_metadata(&self.dir)?.is_dir() {
            return Err(io::Error::other("Invalid progress cache folder"));
        }
        let lock = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.dir.join("lock"))?;
        lock.try_lock().map_err(|e| {
            io::Error::other(format!(
                "Cannot lock task cache {}: {e}",
                self.dir.display()
            ))
        })?;
        let lock = TaskLock(lock);
        let owner = self.dir.join("source");
        match fs::read(&owner) {
            Ok(bytes) if bytes == utf8(&self.root)?.as_bytes() => (),
            Ok(_) => return Err(io::Error::other("Progress cache belongs to another folder")),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                // A crashed first visit may have left only the lock. Never adopt other contents.
                for entry in self.dir.read_dir()? {
                    if entry?.file_name() != "lock" {
                        return Err(io::Error::other("Unrecognized progress cache contents"));
                    }
                }
                let mut file = File::create_new(owner)?;
                file.write_all(utf8(&self.root)?.as_bytes())?;
                file.sync_all()?;
            }
            Err(e) => return Err(e),
        }
        Ok(lock)
    }

    fn generations(&self) -> io::Result<Vec<(u64, PathBuf)>> {
        let mut generations = Vec::new();
        for entry in self.dir.read_dir()? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(number) = name
                .to_str()
                .and_then(|n| n.strip_prefix("snapshot-"))
                .and_then(|n| n.parse::<u64>().ok())
            else {
                continue;
            };
            if !entry.file_type()?.is_dir() {
                return Err(io::Error::other("Invalid snapshot folder"));
            }
            generations.push((number, entry.path()));
        }
        generations.sort_by_key(|g| g.0);
        Ok(generations)
    }

    fn previous(&self) -> io::Result<Option<(u64, PathBuf, Index)>> {
        let Some((number, path)) = self.generations()?.pop() else {
            return Ok(None);
        };
        let index = serde_json::from_reader(BufReader::new(File::open(path.join("index.json"))?))?;
        let table = path.join("tags.csv");
        if !fs::metadata(&table)?.is_file() {
            return Err(io::Error::other("Invalid cached tags table"));
        }
        Ok(Some((number, table, index)))
    }
}

fn enumerate(root: &Path) -> io::Result<BTreeMap<PathBuf, Stamp>> {
    let mut files = BTreeMap::new();
    let mut folders = vec![root.to_owned()];
    while let Some(folder) = folders.pop() {
        let mut videos = BTreeSet::new();
        let mut sidecars = Vec::new();
        for entry in folder
            .read_dir()
            .map_err(|e| io::Error::other(format!("{}: {e}", folder.display())))?
        {
            let entry = entry?;
            // Match Serval's resource discovery below the explicitly chosen root.
            if entry.file_name().to_str().is_some_and(|n| {
                n.starts_with('.') || n.contains("精选") || n.contains("digikamtempfile")
            }) {
                continue;
            }
            let ty = entry.file_type()?;
            let path = entry.path();
            if ty.is_dir() {
                folders.push(path);
            } else if ty.is_file() {
                let name = entry.file_name();
                let Some(name) = name.to_str() else {
                    if path
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("xmp"))
                    {
                        utf8(&path)?;
                    }
                    continue;
                };
                let Some((stem, ext)) = name.rsplit_once('.') else {
                    continue;
                };
                let media = if ext.eq_ignore_ascii_case("xmp") {
                    sidecars.push(path);
                    stem
                } else {
                    name
                };
                if let Some((_, ext)) = media.rsplit_once('.')
                    && matches!(ext.to_ascii_lowercase().as_str(), "mp4" | "avi" | "mov")
                {
                    // digiKam uses QFileInfo::baseName(), up to the first dot.
                    videos.insert(media.split('.').next().unwrap().to_owned());
                }
            }
        }
        for path in sidecars {
            let screenshot = path
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|name| name.rsplit_once('.'))
                .filter(|(_, ext)| ext.eq_ignore_ascii_case("jpg"))
                .and_then(|(stem, _)| stem.rsplit_once('-'))
                .is_some_and(|(base, ms)| {
                    videos.contains(base) && ms.parse::<u32>().is_ok_and(|n| n.to_string() == ms)
                });
            if !screenshot {
                let stamp = Stamp::read(&path)?;
                files.insert(path, stamp);
            }
        }
    }
    Ok(files)
}

struct Observation {
    table: PathBuf,
    failed: BTreeSet<PathBuf>,
}

fn observe(
    program: &Path,
    root: &Path,
    changed: &BTreeSet<&PathBuf>,
    stage: &Path,
) -> io::Result<Observation> {
    let manifest = stage.join("paths.csv");
    let mut writer = csv::Writer::from_path(&manifest)?;
    writer.write_record(["path"])?;
    for path in changed {
        writer.write_record([utf8(path)?])?;
    }
    writer.flush()?;
    drop(writer);
    let mut child = crate::runner::background_command(program)
        .args(["--progress", "json", "observe"])
        .arg(root)
        .arg("-x")
        .arg("--files-from")
        .arg(manifest)
        .arg("-o")
        .arg(stage)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut failed = BTreeSet::new();
    let mut detail = String::new();
    let mut failure = None;
    let stderr = child.stderr.take().expect("piped stderr");
    for line in BufReader::new(stderr).lines() {
        let line = match line {
            Ok(line) => line,
            Err(e) => {
                failure = Some(e.to_string());
                break;
            }
        };
        if let Ok(event) = serde_json::from_str::<serde_json::Value>(&line) {
            let message = event["message"].as_str().unwrap_or(&line);
            if event["serval"] == "file_error" {
                if let Some(path) = event["path"].as_str() {
                    failed.insert(PathBuf::from(path));
                }
            }
            if event["serval"] == "error" {
                failure.get_or_insert_with(|| message.to_string());
            }
        } else if !line.is_empty() && detail.len() < 2048 {
            detail.push_str(&line);
            detail.push('\n');
        }
    }
    let status = child.wait()?;
    if let Some(message) = failure {
        return Err(io::Error::other(message));
    }
    if !status.success() {
        return Err(io::Error::other(format!(
            "Serval {status}: {}",
            detail.trim()
        )));
    }
    let mut table = None;
    for entry in stage.read_dir()? {
        let entry = entry?;
        let name = entry.file_name();
        if name
            .to_str()
            .is_some_and(|n| n.starts_with("tags_") && n.ends_with(".csv"))
            && entry.file_type()?.is_file()
        {
            if table.is_some() {
                return Err(io::Error::other("Serval produced multiple tags tables"));
            }
            table = Some(entry.path());
        }
    }
    Ok(Observation {
        table: table.ok_or_else(|| io::Error::other("Serval produced no tags table"))?,
        failed,
    })
}

fn merge(
    table: &Path,
    previous: Option<&Path>,
    delta: Option<&Observation>,
    files: &BTreeMap<PathBuf, Stamp>,
    changed: &BTreeSet<&PathBuf>,
) -> io::Result<()> {
    let failed = delta.map(|d| &d.failed);
    if failed.is_some_and(|paths| paths.iter().any(|p| !changed.contains(p))) {
        return Err(io::Error::other("Unexpected failed path in Serval output"));
    }
    let mut header = None;
    let mut rows: BTreeMap<PathBuf, Vec<StringRecord>> = BTreeMap::new();
    for (path, is_delta) in previous
        .map(|p| (p, false))
        .into_iter()
        .chain(delta.map(|d| (d.table.as_path(), true)))
    {
        let mut reader = csv::Reader::from_path(path)?;
        let columns = reader.headers()?.clone();
        let column = columns
            .iter()
            .position(|c| c == "path")
            .ok_or_else(|| io::Error::other("Tags table has no path column"))?;
        if !columns.iter().any(|c| c == "species") {
            return Err(io::Error::other("Tags table has no species column"));
        }
        if !rows.is_empty() && header.as_ref() != Some(&columns) {
            return Err(io::Error::other(
                "Serval tags columns changed. Clear the progress cache and refresh.",
            ));
        }
        header = Some(columns);
        for record in reader.records() {
            let record = record?;
            let key = Path::new(&record[column]);
            if is_delta && !changed.contains(&key.to_owned()) {
                return Err(io::Error::other(format!(
                    "Unexpected path in Serval output: {}",
                    key.display()
                )));
            }
            if files.contains_key(key) && (is_delta || !changed.contains(&key.to_owned())) {
                rows.entry(key.to_owned()).or_default().push(record);
            }
        }
    }
    if rows.len() + failed.map_or(0, BTreeSet::len) != files.len()
        || failed.is_some_and(|paths| paths.iter().any(|p| rows.contains_key(p)))
    {
        return Err(io::Error::other("Tags table is missing requested sidecars"));
    }
    let mut writer = csv::Writer::from_path(table)?;
    writer.write_record(&header.unwrap_or_else(|| StringRecord::from(vec!["path", "species"])))?;
    for row in rows.into_values().flatten() {
        writer.write_record(&row)?;
    }
    writer.flush()?;
    writer.get_ref().sync_all()
}

#[cfg(unix)]
fn sync_dir(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}
#[cfg(not(unix))]
fn sync_dir(_: &Path) -> io::Result<()> {
    Ok(())
}

struct Staging(PathBuf);
impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn refresh_task(
    task: &Task,
    read_changes: impl FnOnce(&Path, &BTreeSet<&PathBuf>, &Path) -> io::Result<Observation>,
) -> io::Result<Snapshot> {
    let _lock = task.lock()?;
    let previous = task.previous()?;
    let files = enumerate(&task.root)?;
    let before = previous.as_ref().map(|(_, _, index)| &index.files);
    let changed: BTreeSet<_> = files
        .iter()
        .filter(|(p, s)| {
            previous
                .as_ref()
                .and_then(|(_, _, i)| i.failed.as_ref())
                .is_none_or(|failed| failed.contains(*p))
                || before.and_then(|b| b.get(*p)) != Some(*s)
        })
        .map(|(p, _)| p)
        .collect();
    let removed = before.map_or(0, |b| b.keys().filter(|p| !files.contains_key(*p)).count());
    if let Some((_, table, _)) = &previous
        && changed.is_empty()
        && removed == 0
    {
        return Ok(Snapshot {
            table: table.clone(),
            changed: 0,
            failed: 0,
            removed,
            scanned_at: SystemTime::now(),
        });
    }
    let number = previous.as_ref().map_or(1, |(n, _, _)| n + 1);
    let stage_path = task.dir.join("staging");
    // Under the task lock this is an interrupted staging run, never a published generation.
    if stage_path.try_exists()? {
        fs::remove_dir_all(&stage_path)?;
    }
    fs::create_dir(&stage_path)?;
    let stage = Staging(stage_path);
    let completed = stage.0.join("complete");
    fs::create_dir(&completed)?;
    let delta = if changed.is_empty() {
        None
    } else {
        let table = read_changes(&task.root, &changed, &stage.0)?;
        for path in &changed {
            if files[*path] != Stamp::read(path)? {
                return Err(io::Error::other(format!(
                    "Changed during refresh: {}",
                    path.display()
                )));
            }
        }
        Some(table)
    };
    merge(
        &completed.join("tags.csv"),
        previous.as_ref().map(|(_, p, _)| p.as_path()),
        delta.as_ref(),
        &files,
        &changed,
    )?;
    let dir = task.dir.join(format!("snapshot-{number}"));
    let scanned_at = SystemTime::now();
    let failed = delta.map(|d| d.failed).unwrap_or_default();
    let result = Snapshot {
        table: dir.join("tags.csv"),
        changed: changed.len() - failed.len(),
        failed: failed.len(),
        removed,
        scanned_at,
    };
    let mut writer = BufWriter::new(File::create(completed.join("index.json"))?);
    serde_json::to_writer(
        &mut writer,
        &Index {
            scanned_at,
            files,
            failed: Some(failed),
        },
    )?;
    writer.flush()?;
    writer.get_ref().sync_all()?;
    drop(writer);
    sync_dir(&completed)?;
    fs::rename(completed, &dir)?;
    // Bounded retention also lets a Caracal tab finish loading the preceding snapshot.
    if let Some((previous_number, _, _)) = previous {
        for (old_number, old_dir) in task.generations()? {
            if old_number < previous_number {
                let _ = fs::remove_dir_all(old_dir);
            }
        }
    }
    sync_dir(&task.dir)?;
    Ok(result)
}

pub fn refresh_with_progress(
    root: &Path,
    program: &Path,
    mut reading: impl FnMut(usize),
) -> Result<Snapshot, String> {
    let base = cache_base()?;
    Task::new(root, &base)
        .and_then(|task| {
            refresh_task(&task, |root, paths, stage| {
                reading(paths.len());
                observe(program, root, paths, stage)
            })
        })
        .map_err(|e| e.to_string())
}

pub fn clear(root: &Path) -> Result<(), String> {
    let base = cache_base()?;
    Task::new(root, &base)
        .and_then(|task| {
            if !task.dir.try_exists()? {
                return Ok(());
            }
            let _lock = task.lock()?;
            for (_, path) in task.generations()? {
                fs::remove_dir_all(path)?;
            }
            let stage = task.dir.join("staging");
            if stage.try_exists()? {
                fs::remove_dir_all(stage)?;
            }
            Ok(())
        })
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refresh_replaces_complete_file_rows_and_keeps_good_snapshot_on_failure() {
        let dir = std::env::temp_dir().join(format!(
            "waxbill-cache-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let root = dir.join("task");
        fs::create_dir_all(&root).unwrap();
        let task = Task::new(&root, &dir.join("cache")).unwrap();
        let a = task.root.join("a.jpg.xmp");
        let b = task.root.join("b.jpg.XMP");
        fs::write(&a, "Fox,Pika").unwrap();
        fs::write(&b, "").unwrap();
        let read =
            |_: &Path, paths: &BTreeSet<&PathBuf>, stage: &Path| -> io::Result<Observation> {
                let table = stage.join("out.csv");
                let mut writer = csv::Writer::from_path(&table)?;
                writer.write_record(["path", "species"])?;
                for p in paths {
                    for s in fs::read_to_string(p)?.split(',') {
                        writer.write_record([utf8(p)?, s])?;
                    }
                }
                writer.flush()?;
                Ok(Observation {
                    table,
                    failed: BTreeSet::new(),
                })
            };
        fs::write(task.root.join("clip.part.MP4"), "").unwrap();
        fs::write(task.root.join("clip-6200.jpg.XMP"), "screenshot").unwrap();
        let keep = task.root.join("clip_6200.jpg.xmp");
        fs::write(&keep, "").unwrap();
        let unmatched = task.root.join("other-6200.jpg.xmp");
        fs::write(&unmatched, "").unwrap();
        let inventory = enumerate(&task.root).unwrap();
        assert_eq!(inventory.len(), 4);
        assert!(inventory.contains_key(&keep));
        assert!(inventory.contains_key(&unmatched));
        fs::remove_file(keep).unwrap();
        fs::remove_file(unmatched).unwrap();
        let first = refresh_task(&task, read).unwrap();
        assert_eq!(first.changed, 2);
        let untouched = refresh_task(&task, |_, _, _| panic!("unexpected observe")).unwrap();
        assert_eq!(untouched.table, first.table);
        fs::write(&a, "Fox").unwrap();
        assert!(refresh_task(&task, |_, _, _| Err(io::Error::other("bad XMP"))).is_err());
        assert_eq!(task.previous().unwrap().unwrap().1, first.table);
        let second = refresh_task(&task, |root, paths, stage| {
            assert_eq!(paths.iter().copied().collect::<Vec<_>>(), vec![&a]);
            read(root, paths, stage)
        })
        .unwrap();
        assert_eq!(second.changed, 1);
        assert!(!fs::read_to_string(second.table).unwrap().contains("Pika"));
        fs::remove_file(b).unwrap();
        assert_eq!(
            refresh_task(&task, |_, _, _| panic!("deletion needs no observe"))
                .unwrap()
                .removed,
            1
        );
        fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod integration {
    use super::*;

    /// Run with SERVAL_TEST_BIN set to a list-capable Serval binary.
    #[test]
    #[ignore = "requires an external Serval executable"]
    fn observe_incremental_roundtrip() {
        let program = PathBuf::from(std::env::var_os("SERVAL_TEST_BIN").unwrap());
        let dir = std::env::temp_dir().join(format!(
            "waxbill-live-scan-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let root = dir.join("batch");
        fs::create_dir_all(&root).unwrap();
        let task = Task::new(&root, &dir.join("cache")).unwrap();
        let scan = || {
            refresh_task(&task, |root, files, stage| {
                observe(&program, root, files, stage)
            })
        };
        let xmp = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:lr="http://ns.adobe.com/lightroom/1.0/"><lr:hierarchicalSubject><rdf:Bag><rdf:li>Species|Fox</rdf:li></rdf:Bag></lr:hierarchicalSubject></rdf:Description></rdf:RDF></x:xmpmeta>"#;
        let file = root.join("animal,1.jpg.XMP");
        fs::write(&file, xmp).unwrap();
        fs::write(root.join("other.jpg.xmp"), xmp).unwrap();
        let first = scan().unwrap();
        assert_eq!(first.changed, 2);
        assert_eq!(scan().unwrap().changed, 0);
        fs::write(&file, xmp.replace("Fox", "Snow leopard")).unwrap();
        let updated = scan().unwrap();
        assert_eq!(updated.changed, 1);
        let mut rows = csv::Reader::from_path(&updated.table).unwrap();
        let col = rows
            .headers()
            .unwrap()
            .iter()
            .position(|name| name == "species")
            .unwrap();
        let labels: BTreeSet<String> = rows.records().map(|r| r.unwrap()[col].to_owned()).collect();
        assert_eq!(
            labels,
            ["Fox".to_string(), "Snow leopard".to_string()].into()
        );
        fs::write(&file, "broken").unwrap();
        fs::write(root.join("other.jpg.xmp"), xmp.replace("Fox", "Pika")).unwrap();
        let partial = scan().unwrap();
        assert_eq!((partial.changed, partial.failed), (1, 1));
        let mut rows = csv::Reader::from_path(partial.table).unwrap();
        let records = rows.records().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(&records[0][col], "Pika");
        assert_eq!(scan().unwrap().failed, 1);
        fs::write(&file, xmp).unwrap();
        assert_eq!(scan().unwrap().changed, 1);
        fs::remove_dir_all(dir).unwrap();
    }
}
