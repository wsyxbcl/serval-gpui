//! Writing browse edits (Design/spec/browse.md §5): every edit changes the record at once and joins a queue; the
//! queue is written by one `serval xmp set` call when the user moves on, after 2 seconds without edits, or before
//! closing. Repeated edits of one file collapse, so each file is written once per flush; `expect` keeps what was
//! shown before the first edit, so a file changed elsewhere is reported instead of overwritten.

use crate::protocol::{EditLabels, FileEdit, FileState};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

pub const IDLE: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, PartialEq)]
pub enum SaveState {
    Saved,
    Writing(usize),
    Failed(String),
}

#[derive(Default)]
pub struct Queue {
    pending: BTreeMap<PathBuf, FileEdit>,
    last_edit: Option<Instant>,
}

pub enum Field {
    Species,
    Individuals,
}

impl Queue {
    fn entry(&mut self, sidecar: &Path) -> &mut FileEdit {
        self.last_edit = Some(Instant::now());
        self.pending
            .entry(sidecar.to_path_buf())
            .or_insert_with(|| FileEdit {
                path: sidecar.to_path_buf(),
                expect: EditLabels::default(),
                set: EditLabels::default(),
                add: Default::default(),
                remove: Default::default(),
            })
    }

    pub fn rating(&mut self, sidecar: &Path, rating: u8) {
        self.entry(sidecar).set.rating = Some(rating as i32);
    }

    /// The species or individual list as edited in the viewer, with the list shown before the edit.
    pub fn list(&mut self, sidecar: &Path, field: Field, before: &[String], after: Vec<String>) {
        let edit = self.entry(sidecar);
        let (expect, set) = match field {
            Field::Species => (&mut edit.expect.species, &mut edit.set.species),
            Field::Individuals => (&mut edit.expect.individuals, &mut edit.set.individuals),
        };
        if expect.is_none() {
            *expect = Some(before.to_vec());
        }
        *set = Some(after);
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Whether the queue should be written now (no edit for 2 seconds).
    pub fn idle(&self) -> bool {
        !self.pending.is_empty() && self.last_edit.is_some_and(|t| t.elapsed() >= IDLE)
    }

    pub fn take(&mut self) -> Vec<FileEdit> {
        self.last_edit = None;
        std::mem::take(&mut self.pending).into_values().collect()
    }
}

/// Run `serval xmp set` with the edits on stdin; returns one state per file. Blocking; run it in the background.
pub fn write(program: &Path, edits: &[FileEdit], log: &Path) -> Result<Vec<FileState>, String> {
    let mut child = crate::runner::background_command(program)
        .args([
            "xmp",
            "set",
            "--backup",
            "first",
            "--create-missing",
            "--log-to",
        ])
        .arg(log)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("{}: {err}", program.display()))?;
    {
        let mut stdin = child.stdin.take().unwrap();
        for edit in edits {
            let line = serde_json::to_string(edit).map_err(|e| e.to_string())?;
            writeln!(stdin, "{line}").map_err(|e| e.to_string())?;
        }
    }
    let states: Vec<FileState> = BufReader::new(child.stdout.take().unwrap())
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str(&line).ok())
        .collect();
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = stderr
            .lines()
            .rev()
            .find(|l| l.starts_with("Error"))
            .unwrap_or("serval xmp set failed");
        return Err(message.to_string());
    }
    Ok(states)
}

#[cfg(test)]
mod tests {
    use super::{Field, Queue, write};
    use crate::protocol::Outcome;

    /// Needs Serval 0.8.0: `SERVAL=/path/to/serval cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn edits_are_written_by_serval_and_stale_ones_conflict() {
        let serval = std::path::PathBuf::from(std::env::var("SERVAL").expect("SERVAL is set"));
        let dir = tempfile::tempdir().unwrap();
        let sidecar = dir.path().join("a.jpg.xmp");
        std::fs::write(
            &sidecar,
            r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
            <rdf:Description rdf:about="" xmlns:lr="http://ns.adobe.com/lightroom/1.0/">
            <lr:hierarchicalSubject><rdf:Bag><rdf:li>Species|Fox</rdf:li></rdf:Bag></lr:hierarchicalSubject>
            </rdf:Description></rdf:RDF></x:xmpmeta>"#,
        )
        .unwrap();
        let log = dir.path().join("session.log");
        let mut queue = Queue::default();
        queue.rating(&sidecar, 4);
        queue.list(
            &sidecar,
            Field::Species,
            &["Fox".into()],
            vec!["Red fox".into()],
        );
        let states = write(&serval, &queue.take(), &log).unwrap();
        assert_eq!(states[0].result, Some(Outcome::Written));
        let labels = states[0].labels.clone().unwrap();
        assert_eq!(
            (labels.rating, labels.species),
            (4, vec!["Red fox".to_string()])
        );

        // Shown as "Fox", but the file now says "Red fox": a conflict, nothing written.
        queue.list(
            &sidecar,
            Field::Species,
            &["Fox".into()],
            vec!["Wolf".into()],
        );
        let states = write(&serval, &queue.take(), &log).unwrap();
        assert_eq!(states[0].result, Some(Outcome::Conflict));
    }
}
