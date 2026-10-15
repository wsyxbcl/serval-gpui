//! The screens in the sidebar (Design/spec/shell.md §2) and what every command screen provides to the shell.

pub mod capture;
pub mod extract;
pub mod form;
pub mod observe;
pub mod simple;
pub mod translate;
pub mod update;

use crate::i18n::t;
use gpui_kit::assets::IconName;
use gpui_kit::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ScreenId {
    Rename,
    Align,
    XmpInit,
    Browse,
    Observe,
    Capture,
    Translate,
    XmpUpdate,
    XmpSync,
    Extract,
    XmpCopy,
    XmpRemove,
    Settings,
}

/// Sidebar groups, in order. Settings sits at the bottom, outside the groups.
pub fn groups() -> [(&'static str, &'static [ScreenId]); 5] {
    use ScreenId::*;
    [
        (t("数据分析", "Data analysis"), &[Observe, Capture, Browse]),
        (t("元数据", "Metadata"), &[XmpInit, XmpUpdate, XmpSync]),
        (t("文件导出", "Export"), &[Extract, XmpCopy]),
        (t("数据整理", "Data wrangling"), &[Rename, Align]),
        (t("工具", "Tools"), &[XmpRemove, Translate]),
    ]
}

impl ScreenId {
    const ALL: [ScreenId; 13] = [
        ScreenId::Rename,
        ScreenId::Align,
        ScreenId::XmpInit,
        ScreenId::Browse,
        ScreenId::Observe,
        ScreenId::Capture,
        ScreenId::Translate,
        ScreenId::XmpUpdate,
        ScreenId::XmpSync,
        ScreenId::Extract,
        ScreenId::XmpCopy,
        ScreenId::XmpRemove,
        ScreenId::Settings,
    ];

    /// The screen saved as `screen=<name>` in the settings.
    pub fn from_key(key: &str) -> Option<ScreenId> {
        Self::ALL.into_iter().find(|id| format!("{id:?}") == key)
    }

    pub fn label(self) -> &'static str {
        use ScreenId::*;
        match self {
            Rename => t("重命名", "Rename"),
            Align => t("路径对齐", "Align paths"),
            XmpInit => t("XMP 初始化", "Initialise XMP"),
            Browse => t("查看媒体", "View media"),
            Observe => t("标签获取", "Get tags"),
            Capture => t("独立捕获", "Temporal-independence events"),
            Translate => t("物种名替换", "Translate species"),
            XmpUpdate => t("XMP 更新", "Update XMP"),
            XmpSync => t("元数据同步", "Sync metadata"),
            Extract => t("媒体提取", "Copy media"),
            XmpCopy => t("XMP 导出", "Export XMP"),
            XmpRemove => t("XMP 删除", "Remove XMP"),
            Settings => t("设置", "Settings"),
        }
    }

    pub fn icon(self) -> IconName {
        use ScreenId::*;
        match self {
            Rename => IconName::Pencil,
            Align => IconName::ArrowRightLeft,
            XmpInit => IconName::Plus,
            Browse => IconName::LayoutGrid,
            Observe => IconName::ScanSearch,
            Capture => IconName::Timer,
            Translate => IconName::Languages,
            XmpUpdate => IconName::RefreshCw,
            XmpSync => IconName::ArrowLeftRight,
            Extract => IconName::Download,
            XmpCopy => IconName::Copy,
            XmpRemove => IconName::Trash,
            Settings => IconName::Settings,
        }
    }
}

/// What a command screen would run now: Serval's arguments, and either what the run will do (ready) or the first
/// thing still missing.
pub struct Plan {
    pub args: Vec<String>,
    /// Where Serval runs: the output root, or its nearest existing parent (run.md §5).
    pub cwd: std::path::PathBuf,
    pub ready: Result<String, String>,
}

/// A command screen's form, as the shell sees it.
#[derive(Clone)]
pub enum Form {
    Observe(Entity<observe::ObserveForm>),
    Capture(Entity<capture::CaptureForm>),
    Extract(Entity<extract::ExtractForm>),
    Simple(Entity<simple::SimpleForm>),
    Translate(Entity<translate::TranslateForm>),
    Update(Entity<update::UpdateForm>),
}

impl Form {
    pub fn summary(&self, cx: &App) -> &'static str {
        match self {
            Form::Simple(form) => form.read(cx).summary(),
            Form::Translate(_) => t(
                "按标签表把物种名换成另一种写法（例如中文名、学名）。",
                "Rewrites species names another way (e.g. Chinese or scientific names) from a taglist.",
            ),
            Form::Update(_) => t(
                "按表格修改 XMP 中的标签：按 xmp_update 列修改，或整表写入。",
                "Changes the tags in the XMP files from a table: by the xmp_update column, or the whole table.",
            ),
            Form::Observe(_) => t(
                "读取照片和视频中的标签，生成标签表（tags.csv）和物种统计。",
                "Reads species, individual and other tags from photos and videos into tags.csv and species counts.",
            ),
            Form::Extract(_) => t(
                "按标签筛选，把照片、视频和 XMP 一起复制到新文件夹。",
                "Copies the photos and videos you choose by tag, with their XMP, into a new folder.",
            ),
            Form::Capture(_) => t(
                "按时间间隔合并连续拍到的记录，统计每个位点、每个物种的独立捕获次数。",
                "Merges records close in time into independent captures and counts them per deployment and species.",
            ),
        }
    }

    pub fn plan(&self, cx: &App) -> Plan {
        match self {
            Form::Observe(form) => form.read(cx).plan(cx),
            Form::Capture(form) => form.read(cx).plan(cx),
            Form::Extract(form) => form.read(cx).plan(cx),
            Form::Simple(form) => form.read(cx).plan(cx),
            Form::Translate(form) => form.read(cx).plan(cx),
            Form::Update(form) => form.read(cx).plan(cx),
        }
    }

    /// Remember the form's values for the next launch (shell.md §3).
    pub fn remember(&self, cx: &mut App) {
        let (command, values) = match self {
            Form::Observe(form) => ("observe", form.read(cx).values(cx)),
            Form::Capture(form) => ("capture", form.read(cx).values(cx)),
            Form::Extract(form) => ("extract", form.read(cx).values(cx)),
            Form::Simple(form) => (form.read(cx).key(), form.read(cx).values(cx)),
            Form::Translate(form) => ("translate", form.read(cx).values(cx)),
            Form::Update(form) => ("xmp_update", form.read(cx).values(cx)),
        };
        crate::settings::Settings::update(cx, |settings| {
            for (field, value) in values {
                settings.set_form_value(command, field, value);
            }
        });
    }

    /// While its command runs, the form cannot be edited.
    pub fn set_locked(&self, locked: bool, cx: &mut App) {
        macro_rules! lock {
            ($form:expr) => {
                $form.update(cx, |form, cx| {
                    form.locked = locked;
                    cx.notify();
                })
            };
        }
        match self {
            Form::Observe(form) => lock!(form),
            Form::Capture(form) => lock!(form),
            Form::Extract(form) => lock!(form),
            Form::Simple(form) => lock!(form),
            Form::Translate(form) => lock!(form),
            Form::Update(form) => lock!(form),
        }
    }

    pub fn view(&self) -> AnyView {
        match self {
            Form::Observe(form) => form.clone().into(),
            Form::Capture(form) => form.clone().into(),
            Form::Extract(form) => form.clone().into(),
            Form::Simple(form) => form.clone().into(),
            Form::Translate(form) => form.clone().into(),
            Form::Update(form) => form.clone().into(),
        }
    }
}

/// A command line as a person would type it: arguments with spaces or quotes are quoted.
pub fn display_command(args: &[String]) -> String {
    std::iter::once("serval".to_string())
        .chain(args.iter().map(|arg| {
            if arg.contains('"') && !arg.contains('\'') {
                format!("'{arg}'")
            } else if arg.is_empty() || arg.contains([' ', '"', '\'']) {
                format!("\"{}\"", arg.replace('"', "\\\""))
            } else {
                arg.clone()
            }
        }))
        .collect::<Vec<_>>()
        .join(" ")
}
