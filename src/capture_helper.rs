#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CaptureHelperPromptKind {
    Minutes,
    CompareMode,
    AnalysisMode,
    Deployment,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptureHelperOption {
    pub value: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptureHelperPrompt {
    pub kind: CaptureHelperPromptKind,
    pub prompt: String,
    pub sample_path: Option<String>,
    pub options: Vec<CaptureHelperOption>,
}

#[derive(Clone, Debug, Default)]
pub struct CaptureHelperModel {
    buffer: String,
    prompt: Option<CaptureHelperPrompt>,
    last_submission: Option<String>,
}

impl CaptureHelperModel {
    pub fn reset(&mut self) -> bool {
        let had_state =
            !self.buffer.is_empty() || self.prompt.is_some() || self.last_submission.is_some();
        self.buffer.clear();
        self.prompt = None;
        self.last_submission = None;
        had_state
    }

    pub fn observe_output(&mut self, text: &str) -> bool {
        if text.trim().is_empty() {
            return false;
        }

        self.buffer.push_str(text);
        if let Some(prompt) = detect_prompt(&self.buffer) {
            let changed = self.prompt.as_ref() != Some(&prompt) || self.last_submission.is_some();
            self.prompt = Some(prompt);
            self.last_submission = None;
            return changed;
        }

        false
    }

    pub fn record_submission(&mut self, value: &str) -> bool {
        let value = value.to_string();
        let changed = self.last_submission.as_ref() != Some(&value) || self.prompt.is_some();
        self.last_submission = Some(value);
        self.prompt = None;
        self.buffer.clear();
        changed
    }

    pub fn prompt(&self) -> Option<&CaptureHelperPrompt> {
        self.prompt.as_ref()
    }

    pub fn last_submission(&self) -> Option<&str> {
        self.last_submission.as_deref()
    }
}

fn detect_prompt(buffer: &str) -> Option<CaptureHelperPrompt> {
    if buffer.contains("Select the number corresponding to the deployment:") {
        let sample_path = buffer.lines().find_map(|line| {
            let prefix = "Here is a sample of the file path (";
            line.strip_prefix(prefix)
                .and_then(|rest| rest.strip_suffix(')'))
                .map(ToString::to_string)
        });

        let options = buffer
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                let (number, label) = trimmed.split_once("): ")?;
                if number.chars().all(|ch| ch.is_ascii_digit()) {
                    Some(CaptureHelperOption {
                        value: number.to_string(),
                        label: label.to_string(),
                    })
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        return Some(CaptureHelperPrompt {
            kind: CaptureHelperPromptKind::Deployment,
            prompt: "Select the number corresponding to the deployment:".to_string(),
            sample_path,
            options,
        });
    }

    if buffer.contains("Perform analysis on") {
        return Some(CaptureHelperPrompt {
            kind: CaptureHelperPromptKind::AnalysisMode,
            prompt: "Perform analysis on".to_string(),
            sample_path: None,
            options: vec![
                CaptureHelperOption {
                    value: "1".to_string(),
                    label: "species".to_string(),
                },
                CaptureHelperOption {
                    value: "2".to_string(),
                    label: "individual".to_string(),
                },
            ],
        });
    }

    if buffer.contains("The Minimum Time Difference should be compared with?") {
        return Some(CaptureHelperPrompt {
            kind: CaptureHelperPromptKind::CompareMode,
            prompt: "The Minimum Time Difference should be compared with?".to_string(),
            sample_path: None,
            options: vec![
                CaptureHelperOption {
                    value: "1".to_string(),
                    label: "Last independent record".to_string(),
                },
                CaptureHelperOption {
                    value: "2".to_string(),
                    label: "Last record".to_string(),
                },
            ],
        });
    }

    if let Some(line) = buffer
        .lines()
        .rev()
        .find(|line| line.contains("Input the Minimum Time Difference"))
    {
        return Some(CaptureHelperPrompt {
            kind: CaptureHelperPromptKind::Minutes,
            prompt: line.trim().to_string(),
            sample_path: None,
            options: Vec::new(),
        });
    }

    None
}
