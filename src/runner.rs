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
        ready: Option<u64>,
        already: Option<u64>,
        #[serde(default)]
        items: Vec<NeedItem>,
    },
    Unresolved {
        count: u64,
        #[serde(default)]
        species: Vec<UnresolvedSpecies>,
    },
    Error {
        message: String,
        path: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

/// A species in images whose individual IDs could not be given to one species (observe, `unresolved`).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct UnresolvedSpecies {
    pub name: String,
    pub images: u64,
    #[serde(default)]
    pub individuals: Vec<String>,
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
    /// Windows: the job Serval runs in, which ends it if Waxbill goes away (see `bound_to_waxbill`).
    #[cfg(windows)]
    _job: Option<job::Job>,
}

pub enum Poll {
    Running(Vec<Message>),
    Exited(Vec<Message>, ExitStatus),
}

impl Running {
    pub fn start(program: &Path, args: &[String], cwd: &Path) -> std::io::Result<Running> {
        let mut command = background_command(program);
        command
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        bound_to_waxbill(&mut command);
        let mut child = command.spawn()?;
        #[cfg(windows)]
        let job = job::Job::holding(&child);
        let (tx, messages) = channel();
        read_lines(child.stdout.take().unwrap(), tx.clone(), false);
        read_lines(child.stderr.take().unwrap(), tx, true);
        Ok(Running {
            child,
            messages,
            interrupted_at: None,
            #[cfg(windows)]
            _job: job,
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

/// Serval must not outlive Waxbill. A normal close asks and stops it; this covers Waxbill being killed or crashing.
/// Linux: the kernel sends Serval SIGTERM when the thread that started it ends (runs start on Waxbill's main
/// thread, which lives as long as Waxbill). Windows: see `job`. macOS has no such mechanism; Serval finishes there.
#[cfg(target_os = "linux")]
fn bound_to_waxbill(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    // SAFETY: prctl is async-signal-safe, as pre_exec requires.
    unsafe {
        command.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        });
    }
}

#[cfg(not(target_os = "linux"))]
fn bound_to_waxbill(_command: &mut Command) {}

/// Windows: Serval runs in a job object that ends its processes when the job's last handle closes; Waxbill holds
/// that handle, so the system closes it when Waxbill exits for any reason. Only Serval is in the job: files Waxbill
/// opens in other programs are not.
#[cfg(windows)]
mod job {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    };

    pub struct Job(HANDLE);

    impl Job {
        pub fn holding(child: &std::process::Child) -> Option<Job> {
            // SAFETY: plain Win32 calls on a handle we own and the child's live process handle.
            unsafe {
                let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if job.is_null() {
                    return None;
                }
                let job = Job(job);
                let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let set = SetInformationJobObject(
                    job.0,
                    JobObjectExtendedLimitInformation,
                    &limits as *const _ as *const std::ffi::c_void,
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );
                let assigned = AssignProcessToJobObject(job.0, child.as_raw_handle() as HANDLE);
                (set != 0 && assigned != 0).then_some(job)
            }
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            // SAFETY: the handle came from CreateJobObjectW and is closed once.
            unsafe {
                CloseHandle(self.0);
            }
        }
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

    /// What starts Serval going away takes Serval with it (Waxbill killed or crashed). The kernel watches the
    /// starting thread, so a thread that ends stands in for Waxbill.
    #[cfg(target_os = "linux")]
    #[test]
    fn serval_does_not_outlive_waxbill() {
        let child = std::thread::spawn(|| {
            let mut command = std::process::Command::new("sleep");
            command.arg("30");
            super::bound_to_waxbill(&mut command);
            command.spawn().unwrap()
        })
        .join()
        .unwrap();
        let mut child = child;
        std::thread::sleep(std::time::Duration::from_millis(300));
        let status = child.try_wait().unwrap();
        if status.is_none() {
            child.kill().unwrap();
        }
        assert!(status.is_some(), "the child kept running");
    }

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
            parse(r#"{"serval":"unresolved","kind":"id-species","count":6,"species":[{"name":"雪豹","images":6,"individuals":["SL01"]},{"name":"岩羊","images":5,"individuals":[]}],"paths":["a"]}"#),
            Event::Unresolved { count: 6, species } if species[0].individuals == ["SL01"] && species[1].individuals.is_empty()
        ));
        assert!(matches!(
            parse(r#"{"serval":"summary","written":2,"failed":0}"#),
            Event::Summary(map) if map["written"] == 2
        ));
    }
}
