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

/// Cap on the accumulated output scanned for prompts. Prompts always sit at
/// the tail of the stream, so older output can be dropped.
const MAX_BUFFER_BYTES: usize = 64 * 1024;

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
        self.trim_buffer();
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

    /// Keep only a tail window of the stream, dropping whole lines from the
    /// front so line-based prompt detection never sees a partial first line.
    fn trim_buffer(&mut self) {
        if self.buffer.len() <= MAX_BUFFER_BYTES {
            return;
        }

        let mut cut = self.buffer.len() - MAX_BUFFER_BYTES / 2;
        while !self.buffer.is_char_boundary(cut) {
            cut += 1;
        }
        if let Some(newline) = self.buffer[cut..].find('\n') {
            cut += newline + 1;
        }
        self.buffer.replace_range(..cut, "");
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
        .or_else(|| detect_xmp_init_info_prompt(buffer))
        .or_else(|| detect_analysis_prompt(buffer))
        .or_else(|| detect_compare_mode_prompt(buffer))
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

fn detect_xmp_init_info_prompt(buffer: &str) -> Option<InteractionHelperPrompt> {
    if !buffer.contains("Expect numeric input") {
        return None;
    }

    let sample_path = sample_path(buffer)?;
    let options = numbered_options(buffer);
    if options.is_empty() {
        return None;
    }

    Some(InteractionHelperPrompt {
        prompt: "Select the directory level to keep:".to_string(),
        sample_path: Some(sample_path),
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

    #[test]
    fn detects_xmp_init_info_numeric_prompt() {
        let prompt = prompt(
            "Here is a sample of the file path (/tmp/charton/assets/industry_echarts.png)\n\
             1): tmp\n\
             2): charton\n\
             3): assets\n\
              --< Expect numeric input",
        );

        assert_eq!(prompt.prompt, "Select the directory level to keep:");
        assert_eq!(
            prompt.sample_path.as_deref(),
            Some("/tmp/charton/assets/industry_echarts.png")
        );
        assert_eq!(prompt.options.len(), 3);
        assert_eq!(prompt.options[0].label, "tmp");
        assert_eq!(prompt.options[2].value, "3");
    }

    #[test]
    fn ignores_free_text_minimum_time_difference_prompt() {
        assert!(detect_prompt(
            "Input the Minimum Time Difference (when considering records as independent) in minutes (e.g. 30):"
        )
        .is_none());
    }
}
