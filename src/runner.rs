//! Running Serval (Design/spec/serval-protocol.md §4): a plain process with every answer as a flag, `--progress
//! json` events on stderr, Serval's ordinary output on stdout. Lines on stderr that are not events (an error
//! message, a panic) go to the log with stdout, so nothing is lost.

use serde::Deserialize;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::time::{Duration, Instant};

/// An event Serval writes with `--progress json`. Fields Waxbill does not know are ignored, and so are event kinds
/// it does not know, so a newer Serval can add both.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(tag = "serval", rename_all = "lowercase")]
pub enum Event {
    Hello {
        protocol: u32,
        version: String,
    },
    Progress {
        phase: String,
        done: u64,
        total: Option<u64>,
    },
    Warning {
        message: String,
        path: Option<String>,
    },
    Output {
        kind: String,
        path: PathBuf,
        rows: Option<u64>,
    },
    Summary(serde_json::Map<String, serde_json::Value>),
    Needs {
        count: u64,
        #[serde(default)]
        items: Vec<NeedItem>,
    },
    Unresolved(serde_json::Map<String, serde_json::Value>),
    Error {
        message: String,
        path: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct NeedItem {
    pub target: String,
    #[serde(default)]
    pub reasons: Vec<String>,
}

pub enum Message {
    Event(Event),
    /// A line of Serval's ordinary output, or a stderr line that is not an event.
    Line(String),
}

/// Exit codes (serval-protocol §4).
pub const EXIT_NEEDS_DECISION: i32 = 3;

pub struct Running {
    child: Child,
    messages: Receiver<Message>,
    interrupted_at: Option<Instant>,
}

pub enum Poll {
    Running(Vec<Message>),
    Exited(Vec<Message>, ExitStatus),
}

impl Running {
    pub fn start(program: &Path, args: &[String], cwd: &Path) -> std::io::Result<Running> {
        let mut child = background_command(program)
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let (tx, messages) = channel();
        read_lines(child.stdout.take().unwrap(), tx.clone(), false);
        read_lines(child.stderr.take().unwrap(), tx, true);
        Ok(Running {
            child,
            messages,
            interrupted_at: None,
        })
    }

    /// Take what Serval wrote since the last poll. The run is over when the process has exited and both streams
    /// are read to the end.
    pub fn poll(&mut self) -> Poll {
        let mut batch = Vec::new();
        let mut disconnected = false;
        loop {
            match self.messages.try_recv() {
                Ok(message) => batch.push(message),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }
        if let Some(at) = self.interrupted_at
            && at.elapsed() > Duration::from_secs(3)
        {
            let _ = self.child.kill();
        }
        match self.child.try_wait() {
            Ok(Some(status)) if disconnected => Poll::Exited(batch, status),
            _ => Poll::Running(batch),
        }
    }

    /// Stop: an interrupt first, so Serval can finish the file it is writing and its log; ended after 3 seconds
    /// (run.md §6). Serval's writes are complete or absent, so nothing needs cleaning up.
    pub fn stop(&mut self) {
        if self.interrupted_at.is_some() {
            return;
        }
        self.interrupted_at = Some(Instant::now());
        #[cfg(unix)]
        {
            let interrupted = Command::new("kill")
                .args(["-INT", &self.child.id().to_string()])
                .status()
                .is_ok_and(|s| s.success());
            if interrupted {
                return;
            }
        }
        let _ = self.child.kill();
    }
}

/// A command for a program Waxbill runs in the background: on Windows, without a console window flashing up.
pub fn background_command(program: &Path) -> Command {
    #[allow(unused_mut)]
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

fn read_lines(stream: impl Read + Send + 'static, tx: Sender<Message>, events: bool) {
    std::thread::spawn(move || {
        for line in BufReader::new(stream).lines() {
            let Ok(line) = line else { break };
            let message = if events && line.starts_with('{') {
                match serde_json::from_str::<Event>(&line) {
                    Ok(event) => Message::Event(event),
                    Err(_) => Message::Line(line),
                }
            } else {
                Message::Line(line)
            };
            if tx.send(message).is_err() {
                break;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::Event;

    #[test]
    fn events_from_a_newer_serval_still_parse() {
        let parse = |line: &str| serde_json::from_str::<Event>(line).unwrap();
        assert_eq!(
            parse(r#"{"serval":"progress","phase":"copy","done":3,"total":9,"eta":1}"#),
            Event::Progress {
                phase: "copy".into(),
                done: 3,
                total: Some(9)
            }
        );
        assert_eq!(parse(r#"{"serval":"later-kind","x":1}"#), Event::Unknown);
        assert!(matches!(
            parse(r#"{"serval":"summary","written":2,"failed":0}"#),
            Event::Summary(map) if map["written"] == 2
        ));
    }
}
