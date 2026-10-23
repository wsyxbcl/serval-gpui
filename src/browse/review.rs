//! A review folder for the file manager (Design/spec/browse.md §3): the chosen media and their sidecars as hard
//! links, so Explorer, Photos and the system player show them in one place without copying. Hard links need the
//! same drive and do not exist on most network shares; then nothing is made and extract (a real copy) is offered.

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
    /// The media are on another drive or a network share: links cannot be made there.
    CannotLink,
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
                return Outcome::CannotLink;
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
    use super::{Outcome, make, prefix};

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
}
