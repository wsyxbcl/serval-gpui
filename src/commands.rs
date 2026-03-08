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
        match self.kind {
            CommandKind::Observe => preview_observe(&self.observe),
            CommandKind::Capture => preview_capture(&self.capture),
            CommandKind::Xmp => preview_xmp(&self.xmp),
            CommandKind::Extract => preview_extract(&self.extract),
            CommandKind::Translate => preview_translate(&self.translate),
        }
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

fn preview_observe(input: &ObserveInput) -> String {
    let mut parts = vec!["serval".to_string(), "observe".to_string()];

    if let Some(output) = input.output_dir.as_ref().filter(|s| !s.is_empty()) {
        parts.push("-o".to_string());
        parts.push(output.clone());
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

    parts.join(" ")
}

fn preview_capture(input: &CaptureInput) -> String {
    let mut parts = vec!["serval".to_string(), "capture".to_string()];

    if input.event {
        parts.push("--event".to_string());
    }
    if input.no_exclude {
        parts.push("--no-exclude".to_string());
    }
    if input.camtrap_dp {
        parts.push("--camtrap-dp".to_string());
    }
    if let Some(output) = input.output_dir.as_ref().filter(|s| !s.is_empty()) {
        parts.push("-o".to_string());
        parts.push(output.clone());
    }

    let csv_path = if input.csv_path.is_empty() {
        "<CSV_PATH>"
    } else {
        input.csv_path.as_str()
    };
    parts.push(csv_path.to_string());

    parts.join(" ")
}

fn preview_xmp(input: &XmpInput) -> String {
    let mut parts = vec!["serval".to_string(), "xmp".to_string()];

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

    parts.join(" ")
}

fn preview_extract(input: &ExtractInput) -> String {
    let mut parts = vec!["serval".to_string(), "extract".to_string()];
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
    if let Some(output_dir) = input.output_dir.as_ref().filter(|s| !s.is_empty()) {
        parts.push("-o".to_string());
        parts.push(output_dir.clone());
    }
    parts.push(if input.csv_path.is_empty() {
        "<CSV_PATH>".to_string()
    } else {
        input.csv_path.clone()
    });
    parts.join(" ")
}

fn preview_translate(input: &TranslateInput) -> String {
    let mut parts = vec!["serval".to_string(), "translate".to_string()];
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
    if let Some(output_dir) = input.output_dir.as_ref().filter(|s| !s.is_empty()) {
        parts.push("-o".to_string());
        parts.push(output_dir.clone());
    }
    parts.push(if input.csv_path.is_empty() {
        "<CSV_PATH>".to_string()
    } else {
        input.csv_path.clone()
    });
    parts.join(" ")
}

fn build_observe(input: &ObserveInput) -> Result<(String, Vec<String>), String> {
    if input.media_dir.trim().is_empty() {
        return Err("MEDIA_DIR is required.".to_string());
    }

    let mut args = vec!["observe".to_string()];

    if let Some(output) = input.output_dir.as_ref().filter(|s| !s.is_empty()) {
        args.push("-o".to_string());
        args.push(output.clone());
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
    if let Some(output) = input.output_dir.as_ref().filter(|s| !s.is_empty()) {
        args.push("-o".to_string());
        args.push(output.clone());
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
    if let Some(output_dir) = input.output_dir.as_ref().filter(|s| !s.is_empty()) {
        args.push("-o".to_string());
        args.push(output_dir.clone());
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

    if let Some(output_dir) = input.output_dir.as_ref().filter(|s| !s.is_empty()) {
        args.push("-o".to_string());
        args.push(output_dir.clone());
    }

    args.push(input.csv_path.clone());

    Ok(("serval".to_string(), args))
}
