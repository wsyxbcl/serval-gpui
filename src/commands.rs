use std::path::{Path, PathBuf};

const OBSERVE_OUTPUT_DIR_NAME: &str = "serval_observe";
const CAPTURE_OUTPUT_DIR_NAME: &str = "serval_capture";
const EXTRACT_OUTPUT_DIR_NAME: &str = "serval_extract";
const TRANSLATE_OUTPUT_DIR_NAME: &str = "serval_translate";

fn is_shell_safe_char(ch: char) -> bool {
    matches!(
        ch,
        'a'..='z'
            | 'A'..='Z'
            | '0'..='9'
            | '_'
            | '-'
            | '.'
            | '/'
            | '\\'
            | ':'
            | '@'
            | '%'
            | '+'
            | '='
            | ','
    )
}

fn shell_quote_arg(arg: &str) -> String {
    if !arg.is_empty() && arg.chars().all(is_shell_safe_char) {
        return arg.to_string();
    }

    let mut quoted = String::with_capacity(arg.len() + 2);
    quoted.push('"');
    for ch in arg.chars() {
        match ch {
            '"' | '$' | '`' => {
                quoted.push('\\');
                quoted.push(ch);
            }
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            _ => quoted.push(ch),
        }
    }
    quoted.push('"');
    quoted
}

pub fn format_shell_command(program: &str, args: &[String]) -> String {
    std::iter::once(program)
        .chain(args.iter().map(String::as_str))
        .map(shell_quote_arg)
        .collect::<Vec<_>>()
        .join(" ")
}

fn explicit_output_dir(value: Option<&String>) -> Option<String> {
    value
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn default_output_dir(base_dir: PathBuf, command_dir_name: &str) -> String {
    base_dir
        .join("serval_output")
        .join(command_dir_name)
        .to_string_lossy()
        .to_string()
}

fn base_dir_from_directory_input(path: &str) -> Option<PathBuf> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(PathBuf::from(trimmed))
    }
}

fn base_dir_from_file_input(path: &str) -> Option<PathBuf> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return None;
    }

    let path = Path::new(trimmed);
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    if parent.as_os_str().is_empty() {
        Some(PathBuf::from("."))
    } else {
        Some(parent.to_path_buf())
    }
}

fn effective_observe_output_dir(input: &ObserveInput) -> Option<String> {
    explicit_output_dir(input.output_dir.as_ref()).or_else(|| {
        base_dir_from_directory_input(&input.media_dir)
            .map(|base_dir| default_output_dir(base_dir, OBSERVE_OUTPUT_DIR_NAME))
    })
}

fn effective_capture_output_dir(input: &CaptureInput) -> Option<String> {
    explicit_output_dir(input.output_dir.as_ref()).or_else(|| {
        base_dir_from_file_input(&input.csv_path)
            .map(|base_dir| default_output_dir(base_dir, CAPTURE_OUTPUT_DIR_NAME))
    })
}

fn effective_extract_output_dir(input: &ExtractInput) -> Option<String> {
    explicit_output_dir(input.output_dir.as_ref()).or_else(|| {
        base_dir_from_file_input(&input.csv_path)
            .map(|base_dir| default_output_dir(base_dir, EXTRACT_OUTPUT_DIR_NAME))
    })
}

fn effective_translate_output_dir(input: &TranslateInput) -> Option<String> {
    explicit_output_dir(input.output_dir.as_ref()).or_else(|| {
        base_dir_from_file_input(&input.csv_path)
            .map(|base_dir| default_output_dir(base_dir, TRANSLATE_OUTPUT_DIR_NAME))
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandKind {
    Observe,
    Capture,
    Xmp,
    Extract,
    Translate,
}

#[derive(Clone, Debug)]
pub struct ObserveInput {
    pub media_dir: String,
    pub output_dir: Option<String>,
    pub xmp: bool,
    pub subject: bool,
    pub modified_time: bool,
    pub video_only: bool,
    pub image_only: bool,
    pub debug: bool,
    pub independent: bool,
}

impl Default for ObserveInput {
    fn default() -> Self {
        Self {
            media_dir: String::new(),
            output_dir: None,
            xmp: false,
            subject: false,
            modified_time: false,
            video_only: false,
            image_only: false,
            debug: false,
            independent: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CaptureInput {
    pub csv_path: String,
    pub output_dir: Option<String>,
    pub event: bool,
    pub no_exclude: bool,
    pub camtrap_dp: bool,
}

impl Default for CaptureInput {
    fn default() -> Self {
        Self {
            csv_path: String::new(),
            output_dir: None,
            event: false,
            no_exclude: false,
            camtrap_dp: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CommandState {
    pub kind: CommandKind,
    pub observe: ObserveInput,
    pub capture: CaptureInput,
    pub xmp: XmpInput,
    pub extract: ExtractInput,
    pub translate: TranslateInput,
}

impl Default for CommandState {
    fn default() -> Self {
        Self {
            kind: CommandKind::Observe,
            observe: ObserveInput::default(),
            capture: CaptureInput::default(),
            xmp: XmpInput::default(),
            extract: ExtractInput::default(),
            translate: TranslateInput::default(),
        }
    }
}

impl CommandState {
    pub fn preview(&self) -> String {
        self.preview_with_program("serval")
    }

    pub fn preview_with_program(&self, program: &str) -> String {
        let args = match self.kind {
            CommandKind::Observe => preview_observe_args(&self.observe),
            CommandKind::Capture => preview_capture_args(&self.capture),
            CommandKind::Xmp => preview_xmp_args(&self.xmp),
            CommandKind::Extract => preview_extract_args(&self.extract),
            CommandKind::Translate => preview_translate_args(&self.translate),
        };
        format_shell_command(program, &args)
    }

    pub fn build_command(&self) -> Result<(String, Vec<String>), String> {
        match self.kind {
            CommandKind::Observe => build_observe(&self.observe),
            CommandKind::Capture => build_capture(&self.capture),
            CommandKind::Xmp => build_xmp(&self.xmp),
            CommandKind::Extract => build_extract(&self.extract),
            CommandKind::Translate => build_translate(&self.translate),
        }
    }

    pub fn effective_output_dir(&self) -> Option<String> {
        match self.kind {
            CommandKind::Observe => effective_observe_output_dir(&self.observe),
            CommandKind::Capture => effective_capture_output_dir(&self.capture),
            CommandKind::Xmp => {
                if self.xmp.subcommand == XmpSubcommand::Copy {
                    let trimmed = self.xmp.output_dir.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed.to_string())
                    }
                } else {
                    None
                }
            }
            CommandKind::Extract => effective_extract_output_dir(&self.extract),
            CommandKind::Translate => effective_translate_output_dir(&self.translate),
        }
    }

    pub fn auto_output_dir_hint(&self) -> Option<String> {
        match self.kind {
            CommandKind::Observe
                if explicit_output_dir(self.observe.output_dir.as_ref()).is_none() =>
            {
                effective_observe_output_dir(&self.observe)
            }
            CommandKind::Capture
                if explicit_output_dir(self.capture.output_dir.as_ref()).is_none() =>
            {
                effective_capture_output_dir(&self.capture)
            }
            CommandKind::Extract
                if explicit_output_dir(self.extract.output_dir.as_ref()).is_none() =>
            {
                effective_extract_output_dir(&self.extract)
            }
            CommandKind::Translate
                if explicit_output_dir(self.translate.output_dir.as_ref()).is_none() =>
            {
                effective_translate_output_dir(&self.translate)
            }
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ExtractInput {
    pub csv_path: String,
    pub filter_type: String,
    pub value: String,
    pub rename: bool,
    pub skip_existing: bool,
    pub use_subdir: bool,
    pub subdir_type: Option<String>,
    pub output_dir: Option<String>,
}

impl Default for ExtractInput {
    fn default() -> Self {
        Self {
            csv_path: String::new(),
            filter_type: "species".to_string(),
            value: String::new(),
            rename: false,
            skip_existing: false,
            use_subdir: false,
            subdir_type: Some("species".to_string()),
            output_dir: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TranslateInput {
    pub csv_path: String,
    pub taglist_path: String,
    pub from: String,
    pub to: String,
    pub output_dir: Option<String>,
}

impl Default for TranslateInput {
    fn default() -> Self {
        Self {
            csv_path: String::new(),
            taglist_path: String::new(),
            from: String::new(),
            to: String::new(),
            output_dir: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XmpSubcommand {
    Copy,
    Init,
    Update,
    Remove,
    Sync,
}

#[derive(Clone, Debug)]
pub struct XmpInput {
    pub subcommand: XmpSubcommand,
    pub source_dir: String,
    pub output_dir: String,
    pub csv_path: String,
    pub dir: String,
    pub tag_type: Option<String>,
    pub datetime: bool,
}

impl Default for XmpInput {
    fn default() -> Self {
        Self {
            subcommand: XmpSubcommand::Copy,
            source_dir: String::new(),
            output_dir: String::new(),
            csv_path: String::new(),
            dir: String::new(),
            tag_type: None,
            datetime: false,
        }
    }
}

fn preview_observe_args(input: &ObserveInput) -> Vec<String> {
    let mut parts = vec!["observe".to_string()];

    if let Some(output) = effective_observe_output_dir(input) {
        parts.push("-o".to_string());
        parts.push(output);
    }
    if input.xmp {
        parts.push("--xmp".to_string());
    }
    if input.subject {
        parts.push("--subject".to_string());
    }
    if input.modified_time {
        parts.push("--modified-time".to_string());
    }
    if input.video_only {
        parts.push("--video".to_string());
    }
    if input.image_only {
        parts.push("--image".to_string());
    }
    if input.debug {
        parts.push("--debug".to_string());
    }
    if input.independent {
        parts.push("--independent".to_string());
    }

    let media_dir = if input.media_dir.is_empty() {
        "<MEDIA_DIR>"
    } else {
        input.media_dir.as_str()
    };
    parts.push(media_dir.to_string());
    parts
}

fn preview_capture_args(input: &CaptureInput) -> Vec<String> {
    let mut parts = vec!["capture".to_string()];

    if input.event {
        parts.push("--event".to_string());
    }
    if input.no_exclude {
        parts.push("--no-exclude".to_string());
    }
    if input.camtrap_dp {
        parts.push("--camtrap-dp".to_string());
    }
    if let Some(output) = effective_capture_output_dir(input) {
        parts.push("-o".to_string());
        parts.push(output);
    }

    let csv_path = if input.csv_path.is_empty() {
        "<CSV_PATH>"
    } else {
        input.csv_path.as_str()
    };
    parts.push(csv_path.to_string());
    parts
}

fn preview_xmp_args(input: &XmpInput) -> Vec<String> {
    let mut parts = vec!["xmp".to_string()];

    match input.subcommand {
        XmpSubcommand::Copy => {
            parts.push("copy".to_string());
            parts.push(if input.source_dir.is_empty() {
                "<SOURCE_DIR>".to_string()
            } else {
                input.source_dir.clone()
            });
            parts.push(if input.output_dir.is_empty() {
                "<OUTPUT_DIR>".to_string()
            } else {
                input.output_dir.clone()
            });
        }
        XmpSubcommand::Init => {
            parts.push("init".to_string());
            parts.push(if input.source_dir.is_empty() {
                "<SOURCE_DIR>".to_string()
            } else {
                input.source_dir.clone()
            });
        }
        XmpSubcommand::Remove => {
            parts.push("remove".to_string());
            parts.push(if input.source_dir.is_empty() {
                "<SOURCE_DIR>".to_string()
            } else {
                input.source_dir.clone()
            });
        }
        XmpSubcommand::Update => {
            parts.push("update".to_string());
            if input.datetime {
                parts.push("--datetime".to_string());
            } else if let Some(tag_type) = input.tag_type.as_ref().filter(|t| !t.is_empty()) {
                parts.push("--tag-type".to_string());
                parts.push(tag_type.clone());
            }
            parts.push(if input.csv_path.is_empty() {
                "<CSV_PATH>".to_string()
            } else {
                input.csv_path.clone()
            });
        }
        XmpSubcommand::Sync => {
            parts.push("sync".to_string());
            if !input.csv_path.is_empty() {
                parts.push("--csv".to_string());
                parts.push(input.csv_path.clone());
            }
            if !input.dir.is_empty() {
                parts.push(input.dir.clone());
            }
        }
    }

    parts
}

fn preview_extract_args(input: &ExtractInput) -> Vec<String> {
    let mut parts = vec!["extract".to_string()];
    parts.push("--filter-type".to_string());
    parts.push(input.filter_type.clone());
    parts.push("--value".to_string());
    parts.push(if input.value.is_empty() {
        "<VALUE>".to_string()
    } else {
        input.value.clone()
    });
    if input.rename {
        parts.push("--rename".to_string());
    }
    if input.skip_existing {
        parts.push("--skip-existing".to_string());
    }
    if input.use_subdir {
        parts.push("--use-subdir".to_string());
    }
    if let Some(subdir_type) = input.subdir_type.as_ref().filter(|s| !s.is_empty()) {
        parts.push("--subdir-type".to_string());
        parts.push(subdir_type.clone());
    }
    if let Some(output_dir) = effective_extract_output_dir(input) {
        parts.push("-o".to_string());
        parts.push(output_dir);
    }
    parts.push(if input.csv_path.is_empty() {
        "<CSV_PATH>".to_string()
    } else {
        input.csv_path.clone()
    });
    parts
}

fn preview_translate_args(input: &TranslateInput) -> Vec<String> {
    let mut parts = vec!["translate".to_string()];
    parts.push("--taglist-path".to_string());
    parts.push(if input.taglist_path.is_empty() {
        "<TAGLIST>".to_string()
    } else {
        input.taglist_path.clone()
    });
    parts.push("--from".to_string());
    parts.push(if input.from.is_empty() {
        "<FROM>".to_string()
    } else {
        input.from.clone()
    });
    parts.push("--to".to_string());
    parts.push(if input.to.is_empty() {
        "<TO>".to_string()
    } else {
        input.to.clone()
    });
    if let Some(output_dir) = effective_translate_output_dir(input) {
        parts.push("-o".to_string());
        parts.push(output_dir);
    }
    parts.push(if input.csv_path.is_empty() {
        "<CSV_PATH>".to_string()
    } else {
        input.csv_path.clone()
    });
    parts
}

fn build_observe(input: &ObserveInput) -> Result<(String, Vec<String>), String> {
    if input.media_dir.trim().is_empty() {
        return Err("MEDIA_DIR is required.".to_string());
    }

    let mut args = vec!["observe".to_string()];

    if let Some(output) = effective_observe_output_dir(input) {
        args.push("-o".to_string());
        args.push(output);
    }
    if input.xmp {
        args.push("--xmp".to_string());
    }
    if input.subject {
        args.push("--subject".to_string());
    }
    if input.modified_time {
        args.push("--modified-time".to_string());
    }
    if input.video_only {
        args.push("--video".to_string());
    }
    if input.image_only {
        args.push("--image".to_string());
    }
    if input.debug {
        args.push("--debug".to_string());
    }
    if input.independent {
        args.push("--independent".to_string());
    }

    args.push(input.media_dir.clone());

    Ok(("serval".to_string(), args))
}

fn build_capture(input: &CaptureInput) -> Result<(String, Vec<String>), String> {
    if input.csv_path.trim().is_empty() {
        return Err("CSV_PATH is required.".to_string());
    }

    let mut args = vec!["capture".to_string()];

    if input.event {
        args.push("--event".to_string());
    }
    if input.no_exclude {
        args.push("--no-exclude".to_string());
    }
    if input.camtrap_dp {
        args.push("--camtrap-dp".to_string());
    }
    if let Some(output) = effective_capture_output_dir(input) {
        args.push("-o".to_string());
        args.push(output);
    }

    args.push(input.csv_path.clone());

    Ok(("serval".to_string(), args))
}

fn build_xmp(input: &XmpInput) -> Result<(String, Vec<String>), String> {
    let mut args = vec!["xmp".to_string()];

    match input.subcommand {
        XmpSubcommand::Copy => {
            if input.source_dir.trim().is_empty() {
                return Err("XMP copy requires SOURCE_DIR.".to_string());
            }
            if input.output_dir.trim().is_empty() {
                return Err("XMP copy requires OUTPUT_DIR.".to_string());
            }
            args.push("copy".to_string());
            args.push(input.source_dir.clone());
            args.push(input.output_dir.clone());
        }
        XmpSubcommand::Init => {
            if input.source_dir.trim().is_empty() {
                return Err("XMP init requires SOURCE_DIR.".to_string());
            }
            args.push("init".to_string());
            args.push(input.source_dir.clone());
        }
        XmpSubcommand::Remove => {
            if input.source_dir.trim().is_empty() {
                return Err("XMP remove requires SOURCE_DIR.".to_string());
            }
            args.push("remove".to_string());
            args.push(input.source_dir.clone());
        }
        XmpSubcommand::Update => {
            if input.csv_path.trim().is_empty() {
                return Err("XMP update requires CSV_PATH.".to_string());
            }
            if !input.datetime && input.tag_type.as_deref().unwrap_or("").trim().is_empty() {
                return Err("XMP update requires --datetime or --tag-type.".to_string());
            }
            args.push("update".to_string());
            if input.datetime {
                args.push("--datetime".to_string());
            } else if let Some(tag_type) = input.tag_type.as_ref().filter(|t| !t.is_empty()) {
                args.push("--tag-type".to_string());
                args.push(tag_type.clone());
            }
            args.push(input.csv_path.clone());
        }
        XmpSubcommand::Sync => {
            args.push("sync".to_string());
            if !input.csv_path.trim().is_empty() {
                args.push("--csv".to_string());
                args.push(input.csv_path.clone());
            }
            if !input.dir.trim().is_empty() {
                args.push(input.dir.clone());
            }
        }
    }

    Ok(("serval".to_string(), args))
}

fn build_extract(input: &ExtractInput) -> Result<(String, Vec<String>), String> {
    if input.csv_path.trim().is_empty() {
        return Err("Extract requires CSV_PATH.".to_string());
    }
    if input.filter_type.trim().is_empty() {
        return Err("Extract requires filter type.".to_string());
    }
    if input.value.trim().is_empty() {
        return Err("Extract requires value.".to_string());
    }

    let mut args = vec![
        "extract".to_string(),
        "--filter-type".to_string(),
        input.filter_type.clone(),
        "--value".to_string(),
        input.value.clone(),
    ];

    if input.rename {
        args.push("--rename".to_string());
    }
    if input.skip_existing {
        args.push("--skip-existing".to_string());
    }
    if input.use_subdir {
        args.push("--use-subdir".to_string());
    }
    if let Some(subdir_type) = input.subdir_type.as_ref().filter(|s| !s.is_empty()) {
        args.push("--subdir-type".to_string());
        args.push(subdir_type.clone());
    }
    if let Some(output_dir) = effective_extract_output_dir(input) {
        args.push("-o".to_string());
        args.push(output_dir);
    }
    args.push(input.csv_path.clone());

    Ok(("serval".to_string(), args))
}

fn build_translate(input: &TranslateInput) -> Result<(String, Vec<String>), String> {
    if input.csv_path.trim().is_empty() {
        return Err("Translate requires CSV_PATH.".to_string());
    }
    if input.taglist_path.trim().is_empty() {
        return Err("Translate requires TAGLIST_PATH.".to_string());
    }
    if input.from.trim().is_empty() {
        return Err("Translate requires FROM column.".to_string());
    }
    if input.to.trim().is_empty() {
        return Err("Translate requires TO column.".to_string());
    }

    let mut args = vec![
        "translate".to_string(),
        "--taglist-path".to_string(),
        input.taglist_path.clone(),
        "--from".to_string(),
        input.from.clone(),
        "--to".to_string(),
        input.to.clone(),
    ];

    if let Some(output_dir) = effective_translate_output_dir(input) {
        args.push("-o".to_string());
        args.push(output_dir);
    }

    args.push(input.csv_path.clone());

    Ok(("serval".to_string(), args))
}

#[cfg(test)]
mod tests {
    use super::{
        format_shell_command, CommandKind, CommandState, ExtractInput, ObserveInput, TranslateInput,
    };

    #[test]
    fn shell_command_quotes_spaced_arguments() {
        assert_eq!(
            format_shell_command(
                "serval",
                &[
                    "extract".to_string(),
                    "--value".to_string(),
                    "Snow leopard".to_string(),
                ]
            ),
            r#"serval extract --value "Snow leopard""#
        );
    }

    #[test]
    fn shell_command_quotes_program_paths_and_placeholder_tokens() {
        assert_eq!(
            format_shell_command(
                "/tmp/serval bin",
                &["capture".to_string(), "<CSV_PATH>".to_string()]
            ),
            r#""/tmp/serval bin" capture "<CSV_PATH>""#
        );
    }

    #[test]
    fn extract_preview_quotes_spaced_filter_values() {
        let mut state = CommandState::default();
        state.kind = CommandKind::Extract;
        state.extract = ExtractInput {
            csv_path: "/tmp/records.csv".to_string(),
            value: "Snow leopard".to_string(),
            subdir_type: None,
            ..ExtractInput::default()
        };

        assert_eq!(
            state.preview(),
            r#"serval extract --filter-type species --value "Snow leopard" -o /tmp/serval_output/serval_extract /tmp/records.csv"#
        );
    }

    #[test]
    fn observe_build_uses_input_directory_for_default_output() {
        let input = ObserveInput {
            media_dir: "/tmp/media".to_string(),
            ..ObserveInput::default()
        };

        let (_, args) = super::build_observe(&input).expect("observe command should build");

        assert_eq!(
            args,
            vec![
                "observe",
                "-o",
                "/tmp/media/serval_output/serval_observe",
                "/tmp/media",
            ]
        );
    }

    #[test]
    fn translate_hint_uses_csv_parent_when_output_dir_is_blank() {
        let mut state = CommandState::default();
        state.kind = CommandKind::Translate;
        state.translate = TranslateInput {
            csv_path: "/tmp/jobs/tags.csv".to_string(),
            taglist_path: "/tmp/jobs/taglist.csv".to_string(),
            from: "species".to_string(),
            to: "species_cn".to_string(),
            ..TranslateInput::default()
        };

        assert_eq!(
            state.auto_output_dir_hint().as_deref(),
            Some("/tmp/jobs/serval_output/serval_translate")
        );
    }

    #[test]
    fn explicit_output_dir_disables_auto_hint() {
        let mut state = CommandState::default();
        state.kind = CommandKind::Extract;
        state.extract = ExtractInput {
            csv_path: "/tmp/records.csv".to_string(),
            value: "Snow leopard".to_string(),
            output_dir: Some("/tmp/custom-output".to_string()),
            ..ExtractInput::default()
        };

        assert_eq!(state.auto_output_dir_hint(), None);
        assert_eq!(
            state.effective_output_dir().as_deref(),
            Some("/tmp/custom-output")
        );
    }
}
