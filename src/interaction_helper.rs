#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InteractionHelperOption {
    pub value: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InteractionHelperPrompt {
    pub prompt: String,
    pub sample_path: Option<String>,
    pub options: Vec<InteractionHelperOption>,
}

#[derive(Clone, Debug, Default)]
pub struct InteractionHelperModel {
    buffer: String,
    prompt: Option<InteractionHelperPrompt>,
    last_submission: Option<String>,
}

impl InteractionHelperModel {
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

    pub fn prompt(&self) -> Option<&InteractionHelperPrompt> {
        self.prompt.as_ref()
    }

    pub fn last_submission(&self) -> Option<&str> {
        self.last_submission.as_deref()
    }
}

fn detect_prompt(buffer: &str) -> Option<InteractionHelperPrompt> {
    detect_numbered_choice_prompt(buffer, "Select the number corresponding to the deployment:")
        .or_else(|| {
            detect_numbered_choice_prompt(buffer, "Select the top level directory to keep:")
        })
        .or_else(|| detect_analysis_prompt(buffer))
        .or_else(|| detect_compare_mode_prompt(buffer))
        .or_else(|| detect_minutes_prompt(buffer))
}

fn detect_numbered_choice_prompt(buffer: &str, prompt: &str) -> Option<InteractionHelperPrompt> {
    let prompt_line = prompt_line(buffer, prompt)?;
    let options = numbered_options(buffer);
    if options.is_empty() {
        return None;
    }

    Some(InteractionHelperPrompt {
        prompt: prompt_line,
        sample_path: sample_path(buffer),
        options,
    })
}

fn detect_analysis_prompt(buffer: &str) -> Option<InteractionHelperPrompt> {
    if has_prompt_prefix_line(buffer, "Perform analysis on") {
        return Some(InteractionHelperPrompt {
            prompt: "Perform analysis on".to_string(),
            sample_path: None,
            options: vec![
                InteractionHelperOption {
                    value: "1".to_string(),
                    label: "species".to_string(),
                },
                InteractionHelperOption {
                    value: "2".to_string(),
                    label: "individual".to_string(),
                },
            ],
        });
    }

    None
}

fn detect_compare_mode_prompt(buffer: &str) -> Option<InteractionHelperPrompt> {
    if prompt_line(
        buffer,
        "The Minimum Time Difference should be compared with?",
    )
    .is_some()
    {
        return Some(InteractionHelperPrompt {
            prompt: "The Minimum Time Difference should be compared with?".to_string(),
            sample_path: None,
            options: vec![
                InteractionHelperOption {
                    value: "1".to_string(),
                    label: "Last independent record".to_string(),
                },
                InteractionHelperOption {
                    value: "2".to_string(),
                    label: "Last record".to_string(),
                },
            ],
        });
    }

    None
}

fn detect_minutes_prompt(buffer: &str) -> Option<InteractionHelperPrompt> {
    if let Some(line) = buffer
        .lines()
        .rev()
        .find(|line| line.trim().contains("Input the Minimum Time Difference"))
    {
        return Some(InteractionHelperPrompt {
            prompt: line.trim().to_string(),
            sample_path: None,
            options: Vec::new(),
        });
    }

    None
}

fn prompt_line(buffer: &str, prompt: &str) -> Option<String> {
    buffer
        .lines()
        .find(|line| line.trim() == prompt)
        .map(|line| line.trim().to_string())
}

fn has_prompt_prefix_line(buffer: &str, prompt_prefix: &str) -> bool {
    buffer
        .lines()
        .any(|line| line.trim().starts_with(prompt_prefix))
}

fn sample_path(buffer: &str) -> Option<String> {
    let prefix = "Here is a sample of the file path (";
    buffer.lines().find_map(|line| {
        line.strip_prefix(prefix)
            .and_then(|rest| rest.strip_suffix(')'))
            .map(ToString::to_string)
    })
}

fn numbered_options(buffer: &str) -> Vec<InteractionHelperOption> {
    buffer
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let (number, label) = trimmed.split_once("): ")?;
            if number.chars().all(|ch| ch.is_ascii_digit()) {
                Some(InteractionHelperOption {
                    value: number.to_string(),
                    label: label.to_string(),
                })
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{detect_prompt, InteractionHelperPrompt};

    fn prompt(buffer: &str) -> InteractionHelperPrompt {
        detect_prompt(buffer).expect("prompt should be detected")
    }

    #[test]
    fn detects_capture_deployment_prompt() {
        let prompt = prompt(
            "Here is a sample of the file path (/tmp/camera/site/photo.jpg)\n\
             0): File Only (no directory)\n\
             1): /tmp/camera/site\n\
             2): /tmp/camera\n\
             Select the number corresponding to the deployment:",
        );

        assert_eq!(
            prompt.prompt,
            "Select the number corresponding to the deployment:"
        );
        assert_eq!(
            prompt.sample_path.as_deref(),
            Some("/tmp/camera/site/photo.jpg")
        );
        assert_eq!(prompt.options.len(), 3);
        assert_eq!(prompt.options[1].value, "1");
    }

    #[test]
    fn detects_extract_top_level_prompt() {
        let prompt = prompt(
            "Here is a sample of the file path (/run/media/data/project/animal/photo.jpg.xmp)\n\
             0): File Only (no directory)\n\
             1): /run/media/data/project/animal\n\
             2): /run/media/data/project\n\
             Select the top level directory to keep:",
        );

        assert_eq!(prompt.prompt, "Select the top level directory to keep:");
        assert_eq!(
            prompt.sample_path.as_deref(),
            Some("/run/media/data/project/animal/photo.jpg.xmp")
        );
        assert_eq!(prompt.options.len(), 3);
        assert_eq!(prompt.options[2].label, "/run/media/data/project");
    }

    #[test]
    fn ignores_numbered_prompt_after_answer_is_echoed() {
        assert!(detect_prompt(
            "Here is a sample of the file path (/tmp/camera/site/photo.jpg)\n\
             0): File Only (no directory)\n\
             1): /tmp/camera/site\n\
             Select the top level directory to keep: 0"
        )
        .is_none());
    }
}
