//! A review folder for the file manager (Design/spec/browse.md §3): the chosen media and their sidecars as hard
//! links, so Explorer, Photos and the system player show them in one place without copying. Hard links need the
//! same drive and do not exist on exFAT / FAT disks or most network shares; then nothing is made, the message says
//! why, and Caracal or extract (a real copy) is offered. Also the chosen rows as a table for Caracal.

use crate::table::Csv;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub enum Outcome {
    /// Made, with how many listed files were not found.
    Made {
        folder: PathBuf,
        files: usize,
        missing: usize,
    },
    /// None of the listed media exist (a table from another computer, a drive with another letter): nothing is
    /// made. The first missing file, to show.
    NotFound(PathBuf),
    /// Links cannot be made where the media are: why, and the system's error (for the log).
    CannotLink(NoLinks, String),
    Failed(String),
}

/// The name before the original file name, the same as Caracal's copies and `serval extract --rename`:
/// `<species>__<individuals>__<deployment>`. Species and individuals follow Serval's rules (each value without dots
/// and with characters file systems reject as `_`, sorted, unique, joined by `+`, at most about 100 bytes); no
/// individuals leaves that part out, no species is `untagged`.
pub fn prefix(species: &[String], individuals: &[String], deployment: &str) -> String {
    let part = |value: &str| -> String {
        value
            .chars()
            .filter(|c| *c != '.')
            .map(|c| {
                if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
                    || c.is_control()
                {
                    '_'
                } else {
                    c
                }
            })
            .collect::<String>()
            .trim()
            .to_string()
    };
    let joined = |values: &[String]| -> String {
        let values: std::collections::BTreeSet<String> = values
            .iter()
            .map(|v| part(v))
            .filter(|v| !v.is_empty())
            .collect();
        let mut joined = String::new();
        for (i, value) in values.iter().enumerate() {
            if !joined.is_empty() && joined.len() + 1 + value.len() > 100 {
                joined.push_str(&format!("+{} more", values.len() - i));
                break;
            }
            if !joined.is_empty() {
                joined.push('+');
            }
            joined.push_str(value);
        }
        joined
    };
    let species = joined(species);
    let mut prefix = if species.is_empty() {
        "untagged".to_string()
    } else {
        species
    };
    let individuals = joined(individuals);
    if !individuals.is_empty() {
        prefix = format!("{prefix}__{individuals}");
    }
    let deployment = part(deployment);
    if !deployment.is_empty() {
        prefix = format!("{prefix}__{deployment}");
    }
    prefix
}

/// Link each `(media, sidecar, prefix)` into `folder` as `<prefix>__<name>`; a name used twice gets `_1`, `_2`, as
/// Serval and Caracal name their copies.
pub fn make(folder: &Path, files: &[(PathBuf, PathBuf, String)]) -> Outcome {
    if let Err(err) = std::fs::create_dir_all(folder) {
        return Outcome::Failed(format!("{}: {err}", folder.display()));
    }
    let mut used: HashSet<String> = HashSet::new();
    let mut made = 0;
    let mut missing: Vec<&Path> = Vec::new();
    for (media, sidecar, prefix) in files {
        let Some(name) = media.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };
        let mut target = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}__{name}")
        };
        let (stem, ext) = match target.rsplit_once('.') {
            Some((stem, ext)) => (stem.to_string(), format!(".{ext}")),
            None => (target.clone(), String::new()),
        };
        let mut n = 0;
        while !used.insert(target.to_lowercase()) {
            n += 1;
            target = format!("{stem}_{n}{ext}");
        }
        let link = folder.join(&target);
        match std::fs::hard_link(media, &link) {
            Ok(()) => made += 1,
            Err(err) if made == 0 && is_cross_device(&err) => {
                let _ = std::fs::remove_dir(folder);
                return Outcome::CannotLink(
                    why_no_links(media, &err),
                    format!("{}: {err}", media.display()),
                );
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                missing.push(media);
                continue;
            }
            Err(err) => return Outcome::Failed(format!("{}: {err}", media.display())),
        }
        // Either spelling of the sidecar, linked with the spelling it has.
        let upper = sidecar.with_extension("XMP");
        let found = [sidecar, &upper].into_iter().find(|p| p.exists());
        if let Some(found) = found {
            let ext = found
                .extension()
                .map(|e| e.to_string_lossy().into_owned())
                .unwrap_or_default();
            let _ = std::fs::hard_link(found, folder.join(format!("{target}.{ext}")));
        }
    }
    if made == 0
        && let Some(first) = missing.first()
    {
        let _ = std::fs::remove_dir(folder);
        return Outcome::NotFound(first.to_path_buf());
    }
    Outcome::Made {
        folder: folder.to_path_buf(),
        files: made,
        missing: missing.len(),
    }
}

/// The chosen rows of `table` as a table of their own, for Caracal: `<stem>.csv`, or `<stem>_2.csv` … when that
/// exists (it may have been edited). UTF-8 with a BOM, the header and rows as read.
pub fn subset(table: &Csv, rows: &[usize], stem: &Path) -> Result<PathBuf, String> {
    use std::io::Write;
    let mut writer = csv::WriterBuilder::new()
        .flexible(true)
        .from_writer(b"\xEF\xBB\xBF".to_vec());
    writer
        .write_record(&table.header)
        .map_err(|err| err.to_string())?;
    for &row in rows {
        writer
            .write_record(&table.rows[row])
            .map_err(|err| err.to_string())?;
    }
    let bytes = writer.into_inner().map_err(|err| err.to_string())?;
    if let Some(dir) = stem.parent() {
        std::fs::create_dir_all(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    }
    let name = stem.file_name().unwrap_or_default().to_string_lossy();
    for n in 1.. {
        let target = stem.with_file_name(if n == 1 {
            format!("{name}.csv")
        } else {
            format!("{name}_{n}.csv")
        });
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
        {
            Ok(mut file) => {
                file.write_all(&bytes)
                    .map_err(|err| format!("{}: {err}", target.display()))?;
                return Ok(target);
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(format!("{}: {err}", target.display())),
        }
    }
    unreachable!()
}

/// Why a disk cannot hold hard links, for a message that says so.
#[derive(Clone, Debug, PartialEq)]
pub enum NoLinks {
    /// A disk whose file system has no hard links (exFAT, FAT32: most external disks and cards): its name.
    FileSystem(String),
    /// A network location.
    Network,
    /// Something else: the system's error.
    Other(String),
}

fn why_no_links(media: &Path, err: &std::io::Error) -> NoLinks {
    let (file_system, network) = volume(media);
    let shared = media.to_string_lossy().starts_with(r"\\");
    match file_system {
        _ if network || shared => NoLinks::Network,
        Some(fs) if fs.eq_ignore_ascii_case("exfat") => NoLinks::FileSystem("exFAT".into()),
        Some(fs) if ["fat", "fat32", "vfat", "msdos"].contains(&fs.to_lowercase().as_str()) => {
            NoLinks::FileSystem("FAT32".into())
        }
        _ => NoLinks::Other(err.to_string()),
    }
}

/// The file system of the disk holding `path`, and whether it is a network location.
#[cfg(windows)]
fn volume(path: &Path) -> (Option<String>, bool) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        GetDriveTypeW, GetVolumeInformationW, GetVolumePathNameW,
    };
    const DRIVE_REMOTE: u32 = 4;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut root = [0u16; 512];
    let mut name = [0u16; 64];
    // SAFETY: NUL-terminated input, and output buffers with their lengths.
    unsafe {
        if GetVolumePathNameW(wide.as_ptr(), root.as_mut_ptr(), root.len() as u32) == 0 {
            return (None, false);
        }
        let network = GetDriveTypeW(root.as_ptr()) == DRIVE_REMOTE;
        let read = GetVolumeInformationW(
            root.as_ptr(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            name.as_mut_ptr(),
            name.len() as u32,
        );
        let len = name.iter().position(|&c| c == 0).unwrap_or(name.len());
        (
            (read != 0).then(|| String::from_utf16_lossy(&name[..len])),
            network,
        )
    }
}

/// Linux: from the mount table (the mount point that is the longest prefix of `path`).
#[cfg(target_os = "linux")]
fn volume(path: &Path) -> (Option<String>, bool) {
    let mounts = std::fs::read_to_string("/proc/self/mounts").unwrap_or_default();
    let best = mounts
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let (_, point, fs) = (fields.next()?, fields.next()?, fields.next()?);
            path.starts_with(point.replace("\\040", " "))
                .then(|| (point.len(), fs.to_string()))
        })
        .max_by_key(|(len, _)| *len);
    match best {
        Some((_, fs)) => {
            let network =
                fs.starts_with("nfs") || ["cifs", "smb3", "fuse.sshfs"].contains(&fs.as_str());
            (Some(fs), network)
        }
        None => (None, false),
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
fn volume(_path: &Path) -> (Option<String>, bool) {
    (None, false)
}

/// Links across drives or onto shares that do not support them.
fn is_cross_device(err: &std::io::Error) -> bool {
    matches!(
        err.kind(),
        std::io::ErrorKind::CrossesDevices | std::io::ErrorKind::Unsupported
    ) || matches!(
        err.raw_os_error(),
        Some(17 /* Windows ERROR_NOT_SAME_DEVICE */)
            | Some(1 /* Windows ERROR_INVALID_FUNCTION on SMB */)
            | Some(18 /* EXDEV */)
            | Some(95 /* EOPNOTSUPP */)
    )
}

#[cfg(test)]
mod tests {
    use super::{Outcome, make, prefix, subset};

    #[test]
    fn names_match_caracal_and_serval_extract() {
        let list = |values: &[&str]| values.iter().map(|v| v.to_string()).collect::<Vec<_>>();
        // Caracal's examples (2026-10-23).
        assert_eq!(
            prefix(
                &list(&["Tibetan fox", "Plateau pika"]),
                &list(&["F03"]),
                "DQ6061"
            ),
            "Plateau pika+Tibetan fox__F03__DQ6061"
        );
        assert_eq!(prefix(&list(&["雪豹"]), &[], "DQ6061"), "雪豹__DQ6061");
        assert_eq!(prefix(&[], &[], "DQ6062"), "untagged__DQ6062");
        assert_eq!(prefix(&list(&["Bird spp."]), &[], "A/B"), "Bird spp__A_B");
    }

    #[test]
    fn review_folder_links_media_and_sidecars_without_name_clashes() {
        let dir = std::env::temp_dir().join(format!("waxbill-review-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for dep in ["dep1", "dep2"] {
            std::fs::create_dir_all(dir.join(dep)).unwrap();
            std::fs::write(dir.join(dep).join("IMG_0001.JPG"), dep).unwrap();
            std::fs::write(dir.join(dep).join("IMG_0001.JPG.xmp"), "<x/>").unwrap();
        }
        let file = |dep: &str| {
            (
                dir.join(dep).join("IMG_0001.JPG"),
                dir.join(dep).join("IMG_0001.JPG.xmp"),
                "雪豹".to_string(),
            )
        };
        let folder = dir.join("review");
        let gone = (
            dir.join("gone/IMG_9.JPG"),
            dir.join("gone/IMG_9.JPG.xmp"),
            "雪豹".to_string(),
        );
        let Outcome::Made { files, missing, .. } =
            make(&folder, &[file("dep1"), file("dep2"), gone.clone()])
        else {
            panic!("not made")
        };
        assert_eq!((files, missing), (2, 1));
        assert_eq!(
            std::fs::read_to_string(folder.join("雪豹__IMG_0001_1.JPG")).unwrap(),
            "dep2"
        );
        assert!(folder.join("雪豹__IMG_0001_1.JPG.xmp").exists());
        // A table whose media are all elsewhere (another computer, another drive letter): no empty folder.
        let elsewhere = dir.join("review-elsewhere");
        assert!(matches!(make(&elsewhere, &[gone]), Outcome::NotFound(_)));
        assert!(!elsewhere.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_chosen_rows_become_a_table_and_never_overwrite_one() {
        let dir = std::env::temp_dir().join(format!("waxbill-subset-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let row = |cells: &[&str]| cells.iter().map(|c| c.to_string()).collect::<Vec<_>>();
        let table = crate::table::Csv {
            header: row(&["path", "species"]),
            rows: vec![
                row(&["a.JPG", "雪豹"]),
                row(&["b.JPG", "赤狐"]),
                row(&["c.JPG", "雪豹, 赤狐"]),
            ],
        };
        let stem = dir.join("serval_review/20261023_雪豹");
        let first = subset(&table, &[0, 2], &stem).unwrap();
        assert_eq!(
            std::fs::read_to_string(&first).unwrap(),
            "\u{feff}path,species\na.JPG,雪豹\nc.JPG,\"雪豹, 赤狐\"\n"
        );
        assert!(
            subset(&table, &[1], &stem)
                .unwrap()
                .ends_with("20261023_雪豹_2.csv")
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
