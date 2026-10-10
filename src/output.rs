//! Where a run writes by default (Design/spec/run.md §5): next to the input, never inside it.

use std::path::{Path, PathBuf};

const OUTPUT_SUFFIX: &str = "_serval_output";

/// The output root for `input`: a sibling named `<input>_serval_output`, or, for an input already inside such a
/// folder (e.g. a tags.csv written by observe), that folder.
pub fn default_root(input: &Path) -> PathBuf {
    if let Some(root) = input.ancestors().skip(1).find(|dir| {
        dir.file_name()
            .is_some_and(|name| name.to_string_lossy().ends_with(OUTPUT_SUFFIX))
    }) {
        return root.to_path_buf();
    }
    let input = input.components().as_path();
    let name = if input.is_file() || input.extension().is_some_and(|ext| ext == "csv") {
        input.file_stem()
    } else {
        input.file_name()
    };
    let name = name
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    input.with_file_name(format!("{name}{OUTPUT_SUFFIX}"))
}

/// The folder a command writes into: `<root>/serval_<command>`.
pub fn command_dir(root: &Path, command: &str) -> PathBuf {
    root.join(format!("serval_{}", command.replace(' ', "_")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_goes_next_to_the_input_and_stays_in_an_existing_root() {
        assert_eq!(
            default_root(Path::new("/data/2024_spring/")),
            Path::new("/data/2024_spring_serval_output")
        );
        assert_eq!(
            default_root(Path::new("/data/labels.csv")),
            Path::new("/data/labels_serval_output")
        );
        assert_eq!(
            default_root(Path::new(
                "/data/2024_spring_serval_output/serval_observe/tags.csv"
            )),
            Path::new("/data/2024_spring_serval_output")
        );
    }
}
