//! One run of a command and how it is described (Design/spec/run.md): the run bar's line in each state, the
//! result banner's sentence, warnings, produced files and the log.

use crate::i18n::{Language, count, language, t};
use crate::runner::{
    EXIT_NEEDS_DECISION, Event, Message, NeedItem, Poll, Running, UnresolvedSpecies,
};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Running,
    Finished,
    NeedsDecision,
    Failed,
    Stopped,
}

pub struct Progress {
    pub phase: String,
    pub done: u64,
    pub total: Option<u64>,
    pub phase_started: Instant,
}

/// Targets that already hold different files (serval-protocol §4, `needs`).
pub struct Needs {
    pub count: u64,
    pub ready: u64,
    pub already: u64,
    pub items: Vec<NeedItem>,
}

pub struct OutputFile {
    pub kind: String,
    pub path: PathBuf,
    pub rows: Option<u64>,
    /// A table's column names, read from its header when Serval reports it (which tool can open it).
    pub columns: Vec<String>,
}

/// The column names in a CSV's first line (BOM and case removed); empty when it cannot be read.
fn header_columns(path: &std::path::Path) -> Vec<String> {
    use std::io::BufRead;
    let Ok(file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let mut line = String::new();
    let _ = std::io::BufReader::new(file).read_line(&mut line);
    line.trim_start_matches('\u{feff}')
        .trim_end()
        .split(',')
        .map(|c| c.trim().trim_matches('"').to_lowercase())
        .collect()
}

pub struct Run {
    /// Distinguishes this run from a later one on the same screen, for work that finishes after a while.
    pub id: u64,
    pub command: String,
    /// The form's command when the run started, without decisions: once the form says something else, the run
    /// bar is about the new command again.
    pub form_command: String,
    /// The arguments it ran with (the form's, for real after 按此执行, with any answers): continuing, retrying or
    /// running it again repeats them while the form is unchanged.
    pub args: Vec<String>,
    /// The output folder this run (and the paused runs it continues) created, offered for deletion on cancel when
    /// the command only copies into it.
    pub created_output: Option<crate::run_dialogs::CreatedOutput>,
    /// The folder the command writes into (its `-o`), for 打开输出文件夹.
    pub output_dir: Option<PathBuf>,
    pub started: Instant,
    pub elapsed: Duration,
    pub outcome: Outcome,
    pub progress: Option<Progress>,
    pub log: Vec<String>,
    pub warnings: Vec<(String, Option<String>)>,
    pub errors: Vec<(String, Option<String>)>,
    pub outputs: Vec<OutputFile>,
    pub summary: serde_json::Map<String, serde_json::Value>,
    pub needs: Option<Needs>,
    /// Observe: images whose individual IDs could not be given to a species (questions.md §4).
    pub unresolved: Option<(u64, Vec<UnresolvedSpecies>)>,
    pub log_open: bool,
    /// Where the log is scrolled, kept across redraws (results.rs).
    pub log_scroll: gpui_kit::ScrollHandle,
    stop_requested: bool,
    process: Option<Running>,
}

impl Run {
    pub fn new(command: String, process: Running) -> Run {
        // A preview's result is the plan Serval prints: the log is open from the start.
        let preview = is_preview(&command);
        Run {
            log: vec![format!("$ {command}")],
            command,
            form_command: String::new(),
            args: Vec::new(),
            id: 0,
            created_output: None,
            output_dir: None,
            started: Instant::now(),
            elapsed: Duration::ZERO,
            outcome: Outcome::Running,
            progress: None,
            warnings: Vec::new(),
            errors: Vec::new(),
            outputs: Vec::new(),
            summary: Default::default(),
            needs: None,
            unresolved: None,
            log_open: preview,
            log_scroll: gpui_kit::ScrollHandle::new(),
            stop_requested: false,
            process: Some(process),
        }
    }

    /// The board's extract-conflicts state: Serval stopped before writing, 37 targets differ.
    pub fn example_conflicts() -> Run {
        let mut run = Run::failed_to_start(String::new(), String::new());
        run.errors.clear();
        run.log_open = false;
        run.outcome = Outcome::NeedsDecision;
        let item = |target: &str, reason: &str| NeedItem {
            target: format!("/data/2024_spring_serval_output/serval_extract/{target}"),
            reasons: vec![reason.into()],
        };
        run.needs = Some(Needs {
            count: 37,
            ready: 1_463,
            already: 0,
            items: vec![
                item("dep01/IMG_0042.JPG", "sidecar-differs"),
                item("dep01/IMG_0043.JPG", "file-differs"),
                item("dep03/IMG_0108.JPG", "unrelated-sidecar"),
            ],
        });
        run
    }

    /// The board's observe-id-species-after-scan state: a finished observe with 6 unresolved images.
    pub fn example_unresolved() -> Run {
        let mut run = Run::failed_to_start(String::new(), String::new());
        run.errors.clear();
        run.log_open = false;
        run.outcome = Outcome::Finished;
        run.elapsed = Duration::from_secs(124);
        let species = |name: &str, images, individuals: &[&str]| UnresolvedSpecies {
            name: name.into(),
            images,
            individuals: individuals.iter().map(|s| s.to_string()).collect(),
        };
        run.unresolved = Some((
            6,
            vec![
                species("雪豹", 6, &["SL01", "SL03", "SL04", "SL07"]),
                species("岩羊", 5, &[]),
                species("赤狐", 1, &[]),
            ],
        ));
        run
    }

    /// A run that could not start (e.g. the program is gone).
    pub fn failed_to_start(command: String, error: String) -> Run {
        let mut run = Run {
            log: vec![format!("$ {command}"), error.clone()],
            command,
            form_command: String::new(),
            args: Vec::new(),
            id: 0,
            created_output: None,
            output_dir: None,
            started: Instant::now(),
            elapsed: Duration::ZERO,
            outcome: Outcome::Failed,
            progress: None,
            warnings: Vec::new(),
            errors: vec![(error, None)],
            outputs: Vec::new(),
            summary: Default::default(),
            needs: None,
            unresolved: None,
            log_open: true,
            log_scroll: gpui_kit::ScrollHandle::new(),
            stop_requested: false,
            process: None,
        };
        run.elapsed = run.started.elapsed();
        run
    }

    pub fn is_running(&self) -> bool {
        self.outcome == Outcome::Running
    }

    /// A preview (`--dryrun`): nothing was changed, and the same command can be run for real.
    pub fn is_preview(&self) -> bool {
        is_preview(&self.command)
    }

    /// Pause was pressed and Serval is finishing what it is writing.
    pub fn stopping(&self) -> bool {
        self.stop_requested && self.is_running()
    }

    pub fn stop(&mut self) {
        if let Some(process) = &mut self.process {
            self.stop_requested = true;
            process.stop();
        }
    }

    /// Take in what Serval wrote; returns whether anything changed.
    pub fn poll(&mut self) -> bool {
        let Some(process) = &mut self.process else {
            return false;
        };
        let (messages, exit) = match process.poll() {
            Poll::Running(messages) => (messages, None),
            Poll::Exited(messages, status) => (messages, Some(status)),
        };
        let changed = !messages.is_empty() || exit.is_some();
        for message in messages {
            self.take(message);
        }
        if let Some(status) = exit {
            self.process = None;
            self.elapsed = self.started.elapsed();
            self.outcome = match status.code() {
                Some(0) => Outcome::Finished,
                Some(EXIT_NEEDS_DECISION) => Outcome::NeedsDecision,
                _ if self.stop_requested => Outcome::Stopped,
                _ => Outcome::Failed,
            };
            if self.outcome == Outcome::Failed {
                self.log_open = true;
            }
        }
        changed
    }

    fn take(&mut self, message: Message) {
        match message {
            Message::Line(line) => self.log.push(line),
            Message::Event(event) => match event {
                Event::Progress { phase, done, total } => {
                    let same_phase = self.progress.as_ref().is_some_and(|p| p.phase == phase);
                    let phase_started = match &self.progress {
                        Some(p) if same_phase => p.phase_started,
                        _ => Instant::now(),
                    };
                    self.progress = Some(Progress {
                        phase,
                        done,
                        total,
                        phase_started,
                    });
                }
                Event::Warning { message, path } => self.warnings.push((message, path)),
                Event::Error { message, path } => self.errors.push((message, path)),
                Event::Output { kind, path, rows } => {
                    let columns = if kind == "csv" {
                        header_columns(&path)
                    } else {
                        Vec::new()
                    };
                    self.outputs.push(OutputFile {
                        kind,
                        path,
                        rows,
                        columns,
                    })
                }
                Event::Summary(summary) => self.summary = summary,
                Event::Needs {
                    count,
                    ready,
                    already,
                    items,
                } => {
                    self.needs = Some(Needs {
                        count,
                        ready: ready.unwrap_or(0),
                        already: already.unwrap_or(0),
                        items,
                    })
                }
                Event::Unresolved { count, species } => self.unresolved = Some((count, species)),
                Event::Hello { .. } | Event::Unknown => {}
            },
        }
    }

    /// "正在复制 · 1,204 / 1,500 个文件 · 剩余约 0:42"
    pub fn progress_line(&self) -> String {
        let Some(p) = &self.progress else {
            return t("正在启动…", "Starting…").to_string();
        };
        let mut line = phase_words(&p.phase).to_string();
        // Every phase counts files; saying so keeps a check over all files from reading as that many writes.
        let done = count(p.done as usize);
        line.push_str(&match (p.total, language()) {
            (Some(total), Language::Chinese) => {
                format!(" · {done} / {} 个文件", count(total as usize))
            }
            (Some(total), Language::English) => {
                format!(" · {done} / {} files", count(total as usize))
            }
            (None, Language::Chinese) => format!(" · {done} 个文件"),
            (None, Language::English) => format!(" · {done} files"),
        });
        let running_for = p.phase_started.elapsed();
        if self.outcome == Outcome::Running
            && let Some(total) = p.total
            && p.done > 0
            && running_for > Duration::from_secs(5)
        {
            let remaining =
                running_for.mul_f64((total.saturating_sub(p.done)) as f64 / p.done as f64);
            line.push_str(&format!(" · {} {}", t("剩余约", "about"), clock(remaining)));
            if language() == Language::English {
                line.push_str(" left");
            }
        }
        line
    }

    pub fn fraction(&self) -> Option<f32> {
        let p = self.progress.as_ref()?;
        let total = p.total.filter(|&t| t > 0)?;
        Some((p.done as f32 / total as f32).clamp(0.0, 1.0))
    }

    /// What happened, in one sentence: what was done, how many, how long.
    pub fn result_sentence(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        for (key, zh, en) in SUMMARY_WORDS {
            if let Some(n) = self.summary.get(*key).and_then(|v| v.as_u64())
                && n > 0
            {
                parts.push(match language() {
                    Language::Chinese => format!("{zh} {}", count(n as usize)),
                    Language::English => format!("{} {en}", count(n as usize)),
                });
            }
        }
        if parts.is_empty() && !self.outputs.is_empty() {
            parts.push(match language() {
                Language::Chinese => format!("生成 {} 个结果", self.outputs.len()),
                Language::English => format!("{} results written", self.outputs.len()),
            });
        }
        let took = match language() {
            Language::Chinese => format!("用时 {}", took(self.elapsed)),
            Language::English => format!("took {}", took(self.elapsed)),
        };
        parts.push(took);
        parts.join(t("，", ", "))
    }

    /// Where the results are: the `-o` folder, else the folder a result was written to.
    pub fn output_folder(&self) -> Option<PathBuf> {
        self.output_dir
            .clone()
            .filter(|dir| dir.is_dir())
            .or_else(|| {
                self.outputs
                    .iter()
                    .find(|o| o.kind == "folder")
                    .map(|o| o.path.clone())
            })
            .or_else(|| {
                self.outputs
                    .first()
                    .and_then(|o| o.path.parent())
                    .map(|p| p.to_path_buf())
            })
    }

    /// The first error, for a failed run.
    pub fn failure(&self) -> String {
        self.errors
            .iter()
            .rev()
            .find(|(_, path)| path.is_none())
            .or(self.errors.first())
            .map(|(message, _)| message.clone())
            .or_else(|| {
                self.log
                    .iter()
                    .rev()
                    .find(|l| l.starts_with("Error"))
                    .cloned()
            })
            .unwrap_or_else(|| t("Serval 意外退出", "Serval exited unexpectedly").to_string())
    }
}

/// The commonest Serval errors in one sentence with what to do (the raw message stays beside it and in the log).
/// `None` for anything else: then the raw message is the best there is.
pub fn plain_error(raw: &str) -> Option<&'static str> {
    let raw = raw.to_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| raw.contains(n));
    Some(if has(&["os error 32", "being used by another process"]) {
        t(
            "文件正被其他程序占用（例如在 Excel 中打开）。关闭后重新运行。",
            "A file is in use by another program (e.g. open in Excel). Close it and run again.",
        )
    } else if has(&[
        "os error 28",
        "os error 112",
        "no space left",
        "not enough space",
    ]) {
        t(
            "磁盘空间不足。清理空间或更换输出位置后重新运行。",
            "The disk is full. Free some space or choose another output location, then run again.",
        )
    } else if has(&["os error 13", "permission denied", "access is denied"]) {
        t(
            "没有权限读写这个位置。检查文件夹权限，或更换输出位置。",
            "No permission to read or write this location. Check the folder's permissions, or choose another output location.",
        )
    } else if has(&[
        "os error 2",
        "os error 3",
        "no such file or directory",
        "cannot find the",
    ]) {
        t(
            "找不到文件或文件夹。检查路径是否正确，移动硬盘或网络位置是否已连接。",
            "A file or folder was not found. Check the path, and that the drive or network location is connected.",
        )
    } else if has(&["datetime column"]) {
        t(
            "表格中的时间有空值或无法读取。检查 datetime 列。",
            "The table's times are empty or cannot be read. Check the datetime column.",
        )
    } else if has(&["deploymentid"]) {
        t(
            "位点表中的 deploymentID 有误，应为「位点_<collection>」。可先用「重命名」统一文件夹名。",
            "A deploymentID in the deployments table is wrong; it should be <deployment>_<collection>. Rename can make the folder names match first.",
        )
    } else if has(&["columnnotfound", "unable to find column"]) || raw.starts_with("not found: \"")
    {
        t(
            "表格缺少需要的列。检查列名（见下方原文）。",
            "The table is missing a column it needs. Check the column names (see the message below).",
        )
    } else {
        return None;
    })
}

const SUMMARY_WORDS: &[(&str, &str, &str)] = &[
    ("written", "写入", "written"),
    ("updated", "更新", "updated"),
    ("created", "新建", "created"),
    ("changed", "修改", "changed"),
    ("done", "完成", "done"),
    ("replaced", "替换", "replaced"),
    ("already", "已是最新", "already up to date"),
    ("existing", "已存在", "existing"),
    ("skipped", "跳过", "skipped"),
    ("failed", "失败", "failed"),
];

fn phase_words(phase: &str) -> &'static str {
    match phase {
        "scan" => t("正在扫描", "Scanning"),
        "read" => t("正在读取", "Reading"),
        "plan" => t("正在检查", "Checking"),
        "copy" => t("正在复制", "Copying"),
        "move" => t("正在移动", "Moving"),
        "write" => t("正在写入", "Writing"),
        "remove" => t("正在删除", "Removing"),
        "analyse" => t("正在分析", "Analysing"),
        _ => t("运行中", "Running"),
    }
}

/// m:ss
pub fn clock(duration: Duration) -> String {
    let seconds = duration.as_secs();
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// How long a run took: "3.4 秒" under a minute, m:ss above.
pub fn took(duration: Duration) -> String {
    if duration.as_secs_f64() < 59.95 {
        let seconds = format!("{:.1}", duration.as_secs_f64());
        match language() {
            Language::Chinese => format!("{seconds} 秒"),
            Language::English => format!("{seconds} s"),
        }
    } else {
        clock(duration)
    }
}

fn is_preview(command: &str) -> bool {
    command.split(' ').any(|arg| arg == "--dryrun")
}

#[cfg(test)]
mod tests {
    use super::plain_error;

    #[test]
    fn common_serval_errors_get_a_plain_sentence() {
        // Messages as Serval writes them (collected 2026-10-16), and Windows' wording.
        for (raw, starts) in [
            ("Permission denied (os error 13)", "没有权限"),
            ("not found: \"datetime\" not found", "表格缺少"),
            (
                "Datetime column contains empty values, please fill them before proceeding.",
                "表格中的时间",
            ),
            (
                "The process cannot access the file because it is being used by another process. (os error 32)",
                "文件正被",
            ),
            ("No such file or directory (os error 2)", "找不到"),
        ] {
            let plain = plain_error(raw).unwrap_or_default();
            assert!(plain.starts_with(starts), "{raw} → {plain}");
        }
        assert_eq!(plain_error("something Serval has never said"), None);
    }
}
