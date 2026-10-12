//! A review folder for the file manager (Design/spec/browse.md §3): the chosen media and their sidecars as hard
//! links, so Explorer, Photos and the system player show them in one place without copying. Hard links need the
//! same drive and do not exist on most network shares; then nothing is made and extract (a real copy) is offered.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub enum Outcome {
    Made {
        folder: PathBuf,
        files: usize,
    },
    /// The media are on another drive or a network share: links cannot be made there.
    CannotLink,
    Failed(String),
}

/// Link each `(media, sidecar, prefix)` into `folder` as `<prefix>__<name>`; a name used twice gets `_2`, `_3`.
pub fn make(folder: &Path, files: &[(PathBuf, PathBuf, String)]) -> Outcome {
    if let Err(err) = std::fs::create_dir_all(folder) {
        return Outcome::Failed(format!("{}: {err}", folder.display()));
    }
    let mut used: HashSet<String> = HashSet::new();
    let mut made = 0;
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
        let mut n = 1;
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
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue, // listed but gone
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
    Outcome::Made {
        folder: folder.to_path_buf(),
        files: made,
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
    use super::{Outcome, make};

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
        let Outcome::Made { files, .. } = make(&folder, &[file("dep1"), file("dep2")]) else {
            panic!("not made")
        };
        assert_eq!(files, 2);
        assert_eq!(
            std::fs::read_to_string(folder.join("雪豹__IMG_0001_2.JPG")).unwrap(),
            "dep2"
        );
        assert!(folder.join("雪豹__IMG_0001_2.JPG.xmp").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
