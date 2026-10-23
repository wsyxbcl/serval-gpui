//! Screens that are only paths, checkboxes and short choices (XMP 初始化, 重命名, 路径对齐, XMP 导出, XMP 删除,
//! 元数据同步): each is a [`Spec`] (its fields) and a function that turns the values into Serval's arguments.
//! The shared form draws the fields, counts input folders, and remembers the values.

use super::Plan;
use super::form::{
    FolderCount, Tone, accept_drop, card, count_folder, field, help_line, output_field,
    pick_folder, segmented,
};
use crate::i18n::{Language, count, language, t};
use crate::output;
use crate::theme::TokensExt;
use gpui_kit::component::button::Button;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Chinese and English.
pub type Text = (&'static str, &'static str);

fn tr(text: Text) -> &'static str {
    t(text.0, text.1)
}

#[derive(Clone, Copy, PartialEq)]
pub enum PathKind {
    Folder,
    /// A file with this extension (lowercase), e.g. "csv".
    File(&'static str),
}

pub struct PathSpec {
    pub key: &'static str,
    pub label: Text,
    pub kind: PathKind,
    pub optional: bool,
    pub help: Option<Text>,
}

pub struct FlagSpec {
    pub key: &'static str,
    pub label: Text,
    pub help: Option<Text>,
    pub default: bool,
}

pub struct ChoiceSpec {
    pub key: &'static str,
    pub label: Text,
    /// Label and the value given to `build`.
    pub options: &'static [(Text, &'static str)],
}

/// Serval's arguments, and what the run will do or the first thing missing.
pub type Build = fn(&Values) -> (Vec<String>, Result<String, String>);

pub struct Spec {
    /// The key for remembered values (`form.<key>.*`).
    pub key: &'static str,
    pub summary: Text,
    pub paths: &'static [PathSpec],
    pub choices: &'static [ChoiceSpec],
    pub flags: &'static [FlagSpec],
    /// The folder name under the output root (`serval_<name>`), when the command writes there.
    pub output: Option<&'static str>,
    /// The first path, which the default output location is derived from.
    pub output_from: Option<&'static str>,
    pub build: Build,
}

/// The form's values as `build` sees them.
pub struct Values<'a> {
    paths: HashMap<&'static str, Option<PathBuf>>,
    flags: &'a HashMap<&'static str, bool>,
    choices: HashMap<&'static str, &'static str>,
    counts: &'a HashMap<&'static str, FolderCount>,
    pub output_dir: Option<PathBuf>,
}

impl Values<'_> {
    pub fn path(&self, key: &str) -> Option<&Path> {
        self.paths.get(key).and_then(|p| p.as_deref())
    }

    pub fn path_arg(&self, key: &str) -> Option<String> {
        self.path(key).map(|p| p.to_string_lossy().into_owned())
    }

    pub fn flag(&self, key: &str) -> bool {
        self.flags.get(key).copied().unwrap_or(false)
    }

    pub fn choice(&self, key: &str) -> &'static str {
        self.choices.get(key).copied().unwrap_or("")
    }

    pub fn count(&self, key: &str) -> Option<FolderCount> {
        self.counts.get(key).copied()
    }

    /// The first missing or wrong path, for "还不能运行".
    pub fn check_path(&self, spec: &PathSpec) -> Result<(), String> {
        match self.path(spec.key) {
            None if spec.optional => Ok(()),
            None => Err(format!("{}{}", t("选择", "choose "), tr(spec.label))),
            Some(path) => match spec.kind {
                PathKind::Folder if !path.is_dir() => Err(format!(
                    "{}{}",
                    tr(spec.label),
                    t("不存在", " does not exist")
                )),
                PathKind::File(_) if !path.is_file() => Err(format!(
                    "{}{}",
                    tr(spec.label),
                    t("不存在", " does not exist")
                )),
                _ => Ok(()),
            },
        }
    }
}

pub struct SimpleForm {
    spec: &'static Spec,
    inputs: HashMap<&'static str, Entity<InputState>>,
    flags: HashMap<&'static str, bool>,
    choices: HashMap<&'static str, usize>,
    counts: HashMap<&'static str, FolderCount>,
    custom_output: Option<PathBuf>,
    generation: usize,
    /// Its command is running: the inputs are disabled (the app also blocks the mouse).
    pub locked: bool,
    _subscriptions: Vec<Subscription>,
}

impl SimpleForm {
    pub fn new(spec: &'static Spec, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut inputs = HashMap::new();
        let mut subscriptions = Vec::new();
        for path in spec.paths {
            let placeholder = match path.kind {
                PathKind::Folder => t("选择或拖入一个文件夹", "Choose or drop a folder"),
                PathKind::File(_) => t("选择或拖入一个文件", "Choose or drop a file"),
            };
            let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
            let key = path.key;
            subscriptions.push(cx.subscribe_in(
                &input,
                window,
                move |this, _, event: &InputEvent, _, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.path_changed(key, cx);
                    }
                },
            ));
            inputs.insert(key, input);
        }
        SimpleForm {
            spec,
            inputs,
            flags: spec.flags.iter().map(|f| (f.key, f.default)).collect(),
            choices: spec.choices.iter().map(|c| (c.key, 0)).collect(),
            counts: HashMap::new(),
            custom_output: None,
            generation: 0,
            locked: false,
            _subscriptions: subscriptions,
        }
    }

    pub fn key(&self) -> &'static str {
        self.spec.key
    }

    pub fn summary(&self) -> &'static str {
        tr(self.spec.summary)
    }

    fn path(&self, key: &str, cx: &App) -> Option<PathBuf> {
        let value = self.inputs.get(key)?.read(cx).value();
        let value = value.trim();
        (!value.is_empty()).then(|| PathBuf::from(value))
    }

    pub fn set_path(
        &mut self,
        key: &'static str,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(input) = self.inputs.get(key) {
            input.update(cx, |input, cx| input.set_value(path, window, cx));
            self.path_changed(key, cx);
        }
    }

    /// Count a folder input a moment after typing stops.
    fn path_changed(&mut self, key: &'static str, cx: &mut Context<Self>) {
        self.counts.remove(key);
        cx.notify();
        let is_folder = self
            .spec
            .paths
            .iter()
            .any(|p| p.key == key && p.kind == PathKind::Folder);
        let Some(path) = self.path(key, cx).filter(|p| is_folder && p.is_dir()) else {
            return;
        };
        self.generation += 1;
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(300))
                .await;
            let counted = cx
                .background_executor()
                .spawn(async move { count_folder(&path) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation == generation {
                    this.counts.insert(key, counted);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn output_root(&self, cx: &App) -> Option<PathBuf> {
        if let Some(custom) = &self.custom_output {
            return Some(custom.clone());
        }
        self.path(self.spec.output_from?, cx)
            .map(|p| output::default_root(&p))
    }

    pub fn plan(&self, cx: &App) -> Plan {
        let values = Values {
            paths: self
                .spec
                .paths
                .iter()
                .map(|p| (p.key, self.path(p.key, cx)))
                .collect(),
            flags: &self.flags,
            choices: self
                .spec
                .choices
                .iter()
                .map(|c| (c.key, c.options[self.choices[c.key]].1))
                .collect(),
            counts: &self.counts,
            output_dir: self.spec.output.and_then(|name| {
                self.output_root(cx)
                    .map(|root| output::command_dir(&root, name))
            }),
        };
        let checked = self
            .spec
            .paths
            .iter()
            .try_for_each(|p| values.check_path(p));
        let (args, ready) = (self.spec.build)(&values);
        let cwd = self
            .output_root(cx)
            .or_else(|| self.spec.paths.first().and_then(|p| self.path(p.key, cx)))
            .and_then(|p| p.ancestors().find(|d| d.is_dir()).map(Path::to_path_buf))
            .unwrap_or_else(std::env::temp_dir);
        Plan {
            args,
            cwd,
            ready: checked.and(ready),
        }
    }

    pub fn values(&self, _cx: &App) -> Vec<(&'static str, String)> {
        let mut values: Vec<(&'static str, String)> = Vec::new();
        values.extend(
            self.flags
                .iter()
                // Confirmations and the preview are asked afresh each time.
                .filter(|(k, _)| !matches!(**k, "confirm" | "dryrun"))
                .map(|(k, v)| (*k, if *v { "1" } else { "0" }.to_string())),
        );
        values.extend(self.choices.iter().map(|(k, v)| (*k, v.to_string())));
        values
    }

    pub fn restore(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let settings = cx.default_global::<crate::settings::Settings>().clone();
        let value = |field: &str| {
            settings
                .form_value(self.spec.key, field)
                .map(str::to_string)
        };
        for flag in self
            .spec
            .flags
            .iter()
            .filter(|f| !matches!(f.key, "confirm" | "dryrun"))
        {
            if let Some(v) = value(flag.key) {
                self.flags.insert(flag.key, v == "1");
            }
        }
        for choice in self.spec.choices {
            if let Some(i) = value(choice.key)
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|&i| i < choice.options.len())
            {
                self.choices.insert(choice.key, i);
            }
        }
    }

    fn found_line(&self, spec: &PathSpec, cx: &App) -> Option<Div> {
        if let Some(help) = spec.help
            && self.path(spec.key, cx).is_none()
        {
            return Some(help_line(tr(help), Tone::Quiet, cx));
        }
        let path = self.path(spec.key, cx)?;
        let missing = match spec.kind {
            PathKind::Folder => !path.is_dir(),
            PathKind::File(_) => !path.is_file(),
        };
        if missing {
            return Some(help_line(
                t("找不到这个位置", "This location does not exist"),
                Tone::Danger,
                cx,
            ));
        }
        let found = self.counts.get(spec.key)?;
        Some(help_line(
            match language() {
                Language::Chinese => format!(
                    "找到 {} 张图片、{} 个视频、{} 个 XMP",
                    count(found.images),
                    count(found.videos),
                    count(found.sidecars)
                ),
                Language::English => format!(
                    "Found {} images, {} videos and {} XMP sidecars",
                    count(found.images),
                    count(found.videos),
                    count(found.sidecars)
                ),
            },
            Tone::Quiet,
            cx,
        ))
    }
}

impl Render for SimpleForm {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.tokens().clone();
        let spec = self.spec;
        let mut inputs = card(cx);
        for (i, path) in spec.paths.iter().enumerate() {
            let key = path.key;
            let kind = path.kind;
            let mut label = tr(path.label).to_string();
            if path.optional {
                label.push_str(t("（可选）", " (optional)"));
            }
            inputs = inputs.child(accept_drop(
                field_owned(label, cx)
                    .child(
                        h_flex()
                            .gap(tokens.px("space-2"))
                            .child(
                                div().flex_1().child(
                                    Input::new(&self.inputs[key])
                                        .disabled(self.locked)
                                        .large()
                                        .bg(tokens.color("surface")),
                                ),
                            )
                            .child(
                                Button::new(("choose", i))
                                    .label(t("选择…", "Choose…"))
                                    .outline()
                                    .large()
                                    .on_click(cx.listener(move |_, _, window, cx| match kind {
                                        PathKind::Folder => pick_folder(
                                            window,
                                            cx,
                                            move |this: &mut Self, path, window, cx| {
                                                this.set_path(
                                                    key,
                                                    path.to_string_lossy().to_string(),
                                                    window,
                                                    cx,
                                                )
                                            },
                                        ),
                                        PathKind::File(_) => pick_file(
                                            window,
                                            cx,
                                            move |this: &mut Self, path, window, cx| {
                                                this.set_path(
                                                    key,
                                                    path.to_string_lossy().to_string(),
                                                    window,
                                                    cx,
                                                )
                                            },
                                        ),
                                    })),
                            ),
                    )
                    .children(self.found_line(path, cx)),
                &self.inputs[key],
                cx,
            ));
        }
        for choice in spec.choices {
            let key = choice.key;
            let labels: Vec<&'static str> =
                choice.options.iter().map(|(label, _)| tr(*label)).collect();
            let this = cx.entity().downgrade();
            inputs = inputs.child(field(tr(choice.label), cx).child(h_flex().child(segmented(
                key,
                &labels,
                self.choices[key],
                move |index, _, cx| {
                    let _ = this.update(cx, |this, cx| {
                        this.choices.insert(key, index);
                        cx.notify();
                    });
                },
            ))));
        }
        if !spec.flags.is_empty() {
            let mut flags = v_flex().gap(tokens.px("space-2"));
            for flag in spec.flags {
                let key = flag.key;
                flags = flags.child(
                    v_flex()
                        .gap(px(2.))
                        .child(
                            Checkbox::new(key)
                                .label(tr(flag.label))
                                .checked(self.flags[key])
                                .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                                    this.flags.insert(key, *checked);
                                    cx.notify();
                                })),
                        )
                        .when_some(flag.help, |row, help| {
                            row.child(
                                div()
                                    .pl(px(24.))
                                    .child(help_line(tr(help), Tone::Quiet, cx)),
                            )
                        }),
                );
            }
            inputs = inputs.child(field(t("选项", "Options"), cx).child(flags));
        }
        let output_card = spec.output.map(|_| {
            let root = self.output_root(cx);
            card(cx).child(output_field(
                root.as_deref(),
                self.custom_output.is_some(),
                cx,
                |this: &mut Self, path, cx| {
                    this.custom_output = path;
                    cx.notify();
                },
            ))
        });
        v_flex()
            .gap(tokens.px("space-4"))
            .child(inputs)
            .children(output_card)
    }
}

fn field_owned(label: String, cx: &App) -> Div {
    use gpui_kit::component::StyledExt;
    v_flex().gap(cx.tokens().px("space-2")).child(
        div()
            .text_size(cx.tokens().px("text-md"))
            .font_medium()
            .child(label),
    )
}

fn pick_file<V: 'static>(
    window: &mut Window,
    cx: &mut Context<V>,
    apply: impl FnOnce(&mut V, PathBuf, &mut Window, &mut Context<V>) + 'static,
) {
    let paths = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: false,
        multiple: false,
        prompt: None,
    });
    cx.spawn_in(window, async move |this, cx| {
        if let Ok(Ok(Some(paths))) = paths.await
            && let Some(path) = paths.into_iter().next()
        {
            let _ = this.update_in(cx, |this, window, cx| apply(this, path, window, cx));
        }
    })
    .detach();
}

/// `--progress json` before the positional arguments.
fn progress(args: &mut Vec<String>) {
    args.extend(["--progress".into(), "json".into()]);
}

fn files_line(found: Option<FolderCount>, sidecars: bool) -> String {
    let Some(found) = found else {
        return String::new();
    };
    let n = if sidecars {
        found.sidecars
    } else {
        found.images + found.videos
    };
    match (language(), sidecars) {
        (Language::Chinese, false) => format!("{} 个媒体文件", count(n)),
        (Language::Chinese, true) => format!("{} 个 XMP 文件", count(n)),
        (Language::English, false) => format!("{} media files", count(n)),
        (Language::English, true) => format!("{} XMP files", count(n)),
    }
}

pub static XMP_INIT: Spec = Spec {
    key: "xmp_init",
    summary: (
        "为媒体文件建立 XMP sidecar（已有的不改动），生成信息表单（用于数据检查）。",
        "Creates XMP sidecars for media files (existing ones are kept) and a table for checking the data.",
    ),
    paths: &[PathSpec {
        key: "dir",
        label: ("媒体文件夹", "Media folder"),
        kind: PathKind::Folder,
        optional: false,
        help: None,
    }],
    choices: &[],
    flags: &[],
    output: Some("init"),
    output_from: Some("dir"),
    build: |v| {
        let mut args = vec!["xmp".into(), "init".into()];
        progress(&mut args);
        if let Some(out) = &v.output_dir {
            args.extend(["-o".into(), out.to_string_lossy().into()]);
        }
        args.extend(v.path_arg("dir"));
        let found = v.count("dir");
        let ready = Ok(match (found, language()) {
            (Some(f), Language::Chinese) => format!(
                "{} 个媒体文件，已有 {} 个 XMP",
                count(f.images + f.videos),
                count(f.sidecars)
            ),
            (Some(f), Language::English) => format!(
                "{} media files, {} XMP already",
                count(f.images + f.videos),
                count(f.sidecars)
            ),
            (None, _) => String::new(),
        });
        (args, ready)
    },
};

pub static RENAME: Spec = Spec {
    key: "rename",
    summary: (
        "把项目中的位点文件夹统一改名为「位点_<collection>」（小写），与位点表中的 deploymentID 一致。",
        "Renames the deployment folders in a project to <deployment>_<collection> in lower case, matching deploymentID in the deployments table.",
    ),
    paths: &[PathSpec {
        key: "dir",
        label: ("项目文件夹", "Project folder"),
        kind: PathKind::Folder,
        optional: false,
        help: Some((
            "项目文件夹：其下是各<collection>文件夹，<collection>下是位点文件夹。",
            "The project folder: a folder per collection, with the deployment folders inside.",
        )),
    }],
    choices: &[],
    flags: &[FlagSpec {
        key: "dryrun",
        label: ("先预览，不改名", "Preview only, rename nothing"),
        help: Some((
            "在运行日志里列出将要做的改名。",
            "Lists the renames in the run log.",
        )),
        default: true,
    }],
    output: None,
    output_from: None,
    build: |v| {
        let mut args = vec!["rename".into()];
        if v.flag("dryrun") {
            args.push("--dryrun".into());
        }
        progress(&mut args);
        args.extend(v.path_arg("dir"));
        let ready = Ok(if v.flag("dryrun") {
            t("预览改名", "preview the renames")
        } else {
            t("将直接改名", "renames the folders")
        }
        .to_string());
        (args, ready)
    },
};

pub static ALIGN: Spec = Spec {
    key: "align",
    summary: (
        "有位点表时，把各位点的文件整理到统一的目录；没有时，把文件夹中的文件摊平到一个目录。",
        "With a deployments table, arranges each deployment's files into one layout; without, flattens a folder into one.",
    ),
    paths: &[
        PathSpec {
            key: "dir",
            label: ("项目或文件夹", "Project or folder"),
            kind: PathKind::Folder,
            optional: false,
            help: None,
        },
        PathSpec {
            key: "deploy_table",
            label: ("位点表", "Deployments table"),
            kind: PathKind::File("csv"),
            optional: true,
            help: Some((
                "提供 deployments.csv 时按位点对齐，否则摊平文件夹。",
                "With deployments.csv, aligns deployments; without, flattens the folder.",
            )),
        },
    ],
    choices: &[ChoiceSpec {
        key: "type",
        label: ("文件类型", "File types"),
        options: &[
            (("媒体", "Media"), "media"),
            (("图片", "Images"), "image"),
            (("视频", "Videos"), "video"),
            (("XMP", "XMP"), "xmp"),
        ],
    }],
    flags: &[
        FlagSpec {
            key: "move",
            label: ("移动而不是复制", "Move instead of copy"),
            help: None,
            default: false,
        },
        FlagSpec {
            key: "keep_first_subdir",
            label: (
                "保留第一层子文件夹（摊平时）",
                "Keep the first subfolder (when flattening)",
            ),
            help: None,
            default: false,
        },
        FlagSpec {
            key: "dryrun",
            label: ("先预览，不复制", "Preview only, copy nothing"),
            help: None,
            default: true,
        },
    ],
    output: Some("align"),
    output_from: Some("dir"),
    build: |v| {
        let mut args = vec!["align".into(), "-t".into(), v.choice("type").into()];
        if let Some(table) = v.path_arg("deploy_table") {
            args.extend(["-d".into(), table]);
        }
        for (flag, arg) in [
            ("move", "--move-mode"),
            ("keep_first_subdir", "--keep-first-subdir"),
            ("dryrun", "--dryrun"),
        ] {
            if v.flag(flag) {
                args.push(arg.into());
            }
        }
        progress(&mut args);
        if let Some(out) = &v.output_dir {
            args.extend(["-o".into(), out.to_string_lossy().into()]);
        }
        args.extend(v.path_arg("dir"));
        (args, Ok(files_line(v.count("dir"), false)))
    },
};

pub static XMP_COPY: Spec = Spec {
    key: "xmp_copy",
    summary: (
        "按原目录结构复制文件夹中的 XMP sidecar，用于备份或交给他人。",
        "Copies a folder's XMP sidecars, keeping the folder structure, for a backup or to hand over.",
    ),
    paths: &[PathSpec {
        key: "dir",
        label: ("媒体文件夹", "Media folder"),
        kind: PathKind::Folder,
        optional: false,
        help: None,
    }],
    choices: &[],
    flags: &[],
    output: Some("xmp_copy"),
    output_from: Some("dir"),
    build: |v| {
        let mut args = vec!["xmp".into(), "copy".into()];
        progress(&mut args);
        args.extend(v.path_arg("dir"));
        if let Some(out) = &v.output_dir {
            args.push(out.to_string_lossy().into());
        }
        (args, Ok(files_line(v.count("dir"), true)))
    },
};

pub static XMP_REMOVE: Spec = Spec {
    key: "xmp_remove",
    summary: (
        "删除文件夹（含子文件夹）中所有的 XMP sidecar。",
        "Deletes every XMP sidecar in a folder and its subfolders.",
    ),
    paths: &[PathSpec {
        key: "dir",
        label: ("文件夹", "Folder"),
        kind: PathKind::Folder,
        optional: false,
        help: None,
    }],
    choices: &[],
    flags: &[FlagSpec {
        key: "confirm",
        label: ("确认删除这些 XMP 文件", "Delete these XMP files"),
        help: Some((
            "元数据保存在 XMP 里，删除后无法恢复；可先用「XMP 导出」备份。",
            "The metadata lives in the XMP files and cannot be recovered; back it up with Export XMP first.",
        )),
        default: false,
    }],
    output: None,
    output_from: None,
    build: |v| {
        let mut args = vec!["xmp".into(), "remove".into()];
        progress(&mut args);
        args.extend(v.path_arg("dir"));
        let ready = if v.count("dir").is_some_and(|c| c.sidecars == 0) {
            Err(t(
                "这个文件夹里没有 XMP 文件",
                "there are no XMP files in this folder",
            )
            .to_string())
        } else if !v.flag("confirm") {
            Err(t("请先确认删除", "confirm the deletion first").to_string())
        } else {
            Ok(match language() {
                Language::Chinese => format!("将删除 {}", files_line(v.count("dir"), true)),
                Language::English => format!("deletes {}", files_line(v.count("dir"), true)),
            })
        };
        (args, ready)
    },
};

pub static XMP_SYNC: Spec = Spec {
    key: "xmp_sync",
    summary: (
        "把 XMP sidecar 中的标签写入媒体文件，让不读取 XMP 的软件也能看到元数据。",
        "Writes the tags in the XMP sidecars into the media files, so software that ignores XMP sees the metadata too.",
    ),
    paths: &[
        PathSpec {
            key: "dir",
            label: ("文件夹", "Folder"),
            kind: PathKind::Folder,
            optional: true,
            help: Some((
                "同步文件夹中所有的 XMP，或用下面的表格只同步一部分。",
                "Syncs every XMP in the folder; or use a table below to sync some of them.",
            )),
        },
        PathSpec {
            key: "csv",
            label: ("表格", "Table"),
            kind: PathKind::File("csv"),
            optional: true,
            help: Some((
                "只同步表格 path 列里的文件。",
                "Syncs only the files in the table's path column.",
            )),
        },
    ],
    choices: &[],
    flags: &[],
    output: None,
    output_from: None,
    build: |v| {
        let mut args = vec!["xmp".into(), "sync".into()];
        progress(&mut args);
        let ready = match (v.path_arg("dir"), v.path_arg("csv")) {
            (Some(_), Some(_)) => Err(t(
                "文件夹和表格只选一个",
                "choose a folder or a table, not both",
            )
            .to_string()),
            (None, None) => Err(t("选择文件夹或表格", "choose a folder or a table").to_string()),
            (Some(dir), None) => {
                args.push(dir);
                Ok(files_line(v.count("dir"), true))
            }
            (None, Some(csv)) => {
                args.extend(["--csv".into(), csv]);
                Ok(String::new())
            }
        };
        (args, ready)
    },
};
