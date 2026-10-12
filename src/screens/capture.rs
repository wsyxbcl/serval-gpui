//! 独立捕获 Independent captures: `serval capture` (board: questions.html, capture-ready). Every question Serval
//! asks is a field here; the table is read first, so the deployment level is chosen on a real path.

use super::Plan;
use super::form::{
    Tone, accept_drop, card, field, help_line, level_picker, output_field, segmented,
};
use crate::i18n::{Language, count, language, t};
use crate::output;
use crate::repair::TableCheck;
use crate::table::{DEFAULT_EXCLUDED, Record, TagTable, detect_level};
use crate::theme::TokensExt;
use gpui_kit::component::button::Button;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::radio::Radio;
use gpui_kit::component::{ActiveTheme, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const DEFAULT_GAP: i64 = 30;
const ONE_WEEK_MINUTES: i64 = 10_080;

enum TableState {
    Nothing,
    Reading,
    Read(Arc<TagTable>),
    Failed(String),
}

pub struct CaptureForm {
    csv: Entity<InputState>,
    csv_check: Entity<TableCheck>,
    gap: Entity<InputState>,
    table: TableState,
    level: Option<usize>,
    detected: Option<usize>,
    from_last_independent: bool,
    by_individual: bool,
    event: bool,
    exclude: bool,
    camtrap_dp: bool,
    custom_output: Option<PathBuf>,
    /// The deployment count, recomputed when the level changes.
    deployments: usize,
    read_generation: usize,
    _subscriptions: Vec<Subscription>,
}

impl CaptureForm {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let csv = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t(
                "选择 tags.csv 或 Camtrap DP 的 observations.csv",
                "Choose tags.csv or a Camtrap DP observations.csv",
            ))
        });
        let gap = cx.new(|cx| InputState::new(window, cx).default_value(DEFAULT_GAP.to_string()));
        let subscriptions = vec![
            cx.subscribe_in(&csv, window, |this, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    this.csv_changed(cx);
                }
            }),
            cx.subscribe_in(&gap, window, |_, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
        ];
        CaptureForm {
            csv_check: cx.new(|cx| TableCheck::new(&csv, cx)),
            csv,
            gap,
            table: TableState::Nothing,
            level: None,
            detected: None,
            from_last_independent: false,
            by_individual: false,
            event: false,
            exclude: true,
            camtrap_dp: false,
            custom_output: None,
            deployments: 0,
            read_generation: 0,
            _subscriptions: subscriptions,
        }
    }

    fn csv_path(&self, cx: &App) -> Option<PathBuf> {
        let value = self.csv.read(cx).value();
        let value = value.trim();
        (!value.is_empty()).then(|| PathBuf::from(value))
    }

    fn gap_minutes(&self, cx: &App) -> Option<i64> {
        self.gap
            .read(cx)
            .value()
            .trim()
            .parse::<i64>()
            .ok()
            .filter(|&g| g >= 1)
    }

    pub fn set_csv(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        self.csv
            .update(cx, |input, cx| input.set_value(path, window, cx));
        self.csv_changed(cx);
    }

    /// Read the table in the background once the path names a file.
    fn csv_changed(&mut self, cx: &mut Context<Self>) {
        self.read_generation += 1;
        let generation = self.read_generation;
        let Some(path) = self.csv_path(cx).filter(|p| p.is_file()) else {
            self.table = TableState::Nothing;
            cx.notify();
            return;
        };
        self.table = TableState::Reading;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { TagTable::read(&path) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.read_generation == generation {
                    this.table_read(read, cx);
                }
            });
        })
        .detach();
    }

    fn table_read(&mut self, read: Result<TagTable, String>, cx: &mut Context<Self>) {
        match read {
            Ok(table) => {
                self.detected = (!table.camtrap_dp)
                    .then(|| detect_level(table.records.iter().map(|r| r.path.as_str())))
                    .flatten();
                self.level = self.detected;
                self.camtrap_dp = table.camtrap_dp;
                self.table = TableState::Read(Arc::new(table));
            }
            Err(err) => self.table = TableState::Failed(err),
        }
        self.refresh(cx);
    }

    /// Recompute what depends on the level, the target and the exclusions.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        if let TableState::Read(table) = &self.table {
            let level = if self.camtrap_dp { None } else { self.level };
            self.deployments = table.deployment_count(level);
        } else {
            self.deployments = 0;
        }
        cx.notify();
    }

    fn choose_csv(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
                let _ = this.update_in(cx, |this, window, cx| {
                    this.set_csv(path.to_string_lossy().to_string(), window, cx)
                });
            }
        })
        .detach();
    }

    fn output_root(&self, csv: &Path) -> PathBuf {
        self.custom_output
            .clone()
            .unwrap_or_else(|| output::default_root(csv))
    }

    pub fn plan(&self, cx: &App) -> Plan {
        let csv = self.csv_path(cx);
        let gap = self.gap_minutes(cx);
        let mut args = vec!["capture".to_string()];
        args.extend(["--min-gap".into(), gap.unwrap_or(DEFAULT_GAP).to_string()]);
        let from = if self.from_last_independent {
            "last-independent"
        } else {
            "last-record"
        };
        args.extend(["--measure-from".into(), from.into()]);
        args.extend([
            "--by".into(),
            if self.by_individual {
                "individual"
            } else {
                "species"
            }
            .into(),
        ]);
        if self.camtrap_dp {
            args.push("--camtrap-dp".into());
        } else if let Some(level) = self.level {
            args.extend(["--deployment-level".into(), level.to_string()]);
        }
        if self.event {
            args.push("--event".into());
        }
        if !self.exclude {
            args.push("--no-exclude".into());
        }
        args.extend(["--progress".into(), "json".into()]);
        if let Some(csv) = &csv {
            args.push("-o".into());
            args.push(
                output::command_dir(&self.output_root(csv), "capture")
                    .to_string_lossy()
                    .into(),
            );
            args.push(csv.to_string_lossy().into());
        }
        let ready = match (&csv, &self.table) {
            (None, _) => Err(t("选择标签表", "choose the tags table").to_string()),
            (Some(path), TableState::Nothing) if !path.is_file() => {
                Err(t("找不到这个表格", "the table does not exist").to_string())
            }
            (_, TableState::Reading | TableState::Nothing) => {
                Err(t("正在读取表格…", "reading the table…").to_string())
            }
            (_, TableState::Failed(err)) => Err(format!(
                "{} {err}",
                t("表格无法读取：", "cannot read the table:")
            )),
            _ if gap.is_none() => Err(t(
                "独立性间隔应为至少 1 分钟的整数",
                "the interval must be a whole number of minutes, at least 1",
            )
            .to_string()),
            _ if !self.camtrap_dp && self.level.is_none() => Err(t(
                "请在路径中选择部署所在的一层",
                "choose the deployment's level in the path",
            )
            .to_string()),
            _ => Ok(match language() {
                Language::Chinese => format!(
                    "{} 个部署 · {} 分钟 · {}",
                    count(self.deployments),
                    gap.unwrap_or(DEFAULT_GAP),
                    if self.by_individual {
                        "按个体"
                    } else {
                        "按物种"
                    }
                ),
                Language::English => format!(
                    "{} deployments · {} min · by {}",
                    count(self.deployments),
                    gap.unwrap_or(DEFAULT_GAP),
                    if self.by_individual {
                        "individual"
                    } else {
                        "species"
                    }
                ),
            }),
        };
        let cwd = csv
            .as_ref()
            .map(|csv| self.output_root(csv))
            .and_then(|root| {
                root.ancestors()
                    .find(|dir| dir.is_dir())
                    .map(Path::to_path_buf)
            })
            .unwrap_or_else(std::env::temp_dir);
        Plan { args, cwd, ready }
    }

    /// The values remembered for the next launch.
    pub fn values(&self, cx: &App) -> Vec<(&'static str, String)> {
        let flag = |b: bool| if b { "1" } else { "0" }.to_string();
        vec![
            ("csv", self.csv.read(cx).value().to_string()),
            ("min_gap", self.gap.read(cx).value().to_string()),
            ("last_independent", flag(self.from_last_independent)),
            ("individual", flag(self.by_individual)),
            ("event", flag(self.event)),
            ("exclude", flag(self.exclude)),
            (
                "output",
                self.custom_output
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            ),
        ]
    }

    pub fn restore(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let settings = cx.default_global::<crate::settings::Settings>().clone();
        let value = |field: &str| settings.form_value("capture", field);
        self.from_last_independent = value("last_independent") == Some("1");
        self.by_individual = value("individual") == Some("1");
        self.event = value("event") == Some("1");
        self.exclude = value("exclude") != Some("0");
        self.custom_output = value("output").filter(|v| !v.is_empty()).map(PathBuf::from);
        if let Some(gap) = value("min_gap").filter(|v| !v.is_empty()) {
            self.gap
                .update(cx, |input, cx| input.set_value(gap.to_string(), window, cx));
        }
        if let Some(csv) = value("csv").filter(|v| !v.is_empty()) {
            self.set_csv(csv.to_string(), window, cx);
        }
    }

    /// Fill the form for the capture-ready design state, with a table like the board's.
    pub fn set_example(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path = "/data/2024_spring_serval_output/serval_observe/tags_2024_spring.csv";
        self.csv.update(cx, |input, cx| {
            input.set_value(path.to_string(), window, cx)
        });
        self.event = true;
        self.read_generation += 1; // the example table is not read from disk
        self.table_read(Ok(example_table()), cx);
    }

    fn table_line(&self, cx: &App) -> Option<Div> {
        Some(match &self.table {
            TableState::Nothing => match self.csv_path(cx) {
                Some(path) if !path.is_file() => help_line(
                    t("找不到这个表格", "This table does not exist"),
                    Tone::Danger,
                    cx,
                ),
                _ => help_line(
                    t(
                        "也可以选择 Camtrap DP 的 observations.csv",
                        "A Camtrap DP observations.csv works too",
                    ),
                    Tone::Quiet,
                    cx,
                ),
            },
            TableState::Reading => help_line(t("正在读取…", "Reading…"), Tone::Quiet, cx),
            TableState::Failed(err) => help_line(
                format!("{} {err}", t("无法读取：", "Cannot read:")),
                Tone::Danger,
                cx,
            ),
            TableState::Read(table) => {
                let rows = count(table.records.len());
                let text = match (language(), table.camtrap_dp) {
                    (Language::Chinese, false) => {
                        format!("{rows} 行 · 也可以选择 Camtrap DP 的 observations.csv")
                    }
                    (Language::Chinese, true) => {
                        format!("{rows} 行 · Camtrap DP 观测表，部署来自 deploymentID")
                    }
                    (Language::English, false) => {
                        format!("{rows} rows · a Camtrap DP observations.csv works too")
                    }
                    (Language::English, true) => format!(
                        "{rows} rows · Camtrap DP observations; deployments from deploymentID"
                    ),
                };
                help_line(text, Tone::Quiet, cx)
            }
        })
    }

    fn render_level(&self, table: &TagTable, cx: &mut Context<Self>) -> Div {
        let sample = table.sample_path().unwrap_or("").to_string();
        let mut note = match (self.level, language()) {
            (None, Language::Chinese) => "点选路径中代表部署的一层".to_string(),
            (None, Language::English) => "Click the folder that is the deployment".to_string(),
            (Some(level), lang) => {
                let name = sample
                    .split(['/', '\\'])
                    .nth(level)
                    .unwrap_or("")
                    .to_string();
                let auto = self.detected == Some(level);
                match lang {
                    Language::Chinese => format!(
                        "点选路径中代表部署的一层。已选 {name}{} · 表中共有 {} 个部署",
                        if auto { "（自动识别）" } else { "" },
                        count(self.deployments)
                    ),
                    Language::English => format!(
                        "Click the folder that is the deployment. Chosen: {name}{} · {} deployments in the table",
                        if auto { " (detected)" } else { "" },
                        count(self.deployments)
                    ),
                }
            }
        };
        if table.mixed_depths() {
            note.push_str(t(
                "。路径深度不一致，按样例的层级计算",
                ". Paths have different depths; levels follow the sample",
            ));
        }
        field(t("部署所在的文件夹层级", "Deployment folder level"), cx)
            .child(level_picker(
                &sample,
                self.level,
                cx,
                |this: &mut Self, level, cx| {
                    this.level = Some(level);
                    this.refresh(cx);
                },
            ))
            .child(help_line(note, Tone::Quiet, cx))
    }
}

impl Render for CaptureForm {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.tokens().clone();
        let table = match &self.table {
            TableState::Read(table) => Some(table.clone()),
            _ => None,
        };
        let csv = self.csv_path(cx);
        let output_root = csv.as_ref().map(|csv| self.output_root(csv));
        let gap = self.gap_minutes(cx);

        let table_card =
            card(cx)
                .child(accept_drop(
                    field(t("标签表", "Tags table"), cx)
                        .child(
                            h_flex()
                                .gap(tokens.px("space-2"))
                                .child(div().flex_1().child(
                                    Input::new(&self.csv).large().bg(tokens.color("surface")),
                                ))
                                .child(
                                    Button::new("choose-csv")
                                        .label(t("选择…", "Choose…"))
                                        .outline()
                                        .large()
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.choose_csv(window, cx)
                                        })),
                                ),
                        )
                        .children(self.table_line(cx))
                        .child(self.csv_check.clone()),
                    &self.csv,
                    cx,
                ))
                .when_some(table.filter(|t| !t.camtrap_dp), |card, table| {
                    card.child(self.render_level(&table, cx))
                });

        let gap_field = field(t("独立性间隔", "Independence interval"), cx)
            .child(
                h_flex()
                    .gap(tokens.px("space-3"))
                    .child(
                        div().w(px(110.)).child(
                            Input::new(&self.gap)
                                .large()
                                .bg(tokens.color("surface"))
                                .suffix(div().text_color(cx.theme().muted_foreground).child(t("分钟", "min"))),
                        ),
                    )
                    .child(help_line(
                        t(
                            "同一部署、同一物种两条记录相隔不少于此时间，才算两次独立捕获。",
                            "Two records of one species at one deployment count as two captures only this far apart.",
                        ),
                        Tone::Quiet,
                        cx,
                    )),
            )
            .when(gap.is_none(), |f| {
                f.child(help_line(t("请输入至少 1 分钟的整数。", "Enter a whole number of minutes, at least 1."), Tone::Danger, cx))
            })
            .when(gap.is_some_and(|g| g > ONE_WEEK_MINUTES), |f| {
                f.child(help_line(t("超过一周（10,080 分钟），通常过大。", "More than a week (10,080 minutes) is unusually large."), Tone::Quiet, cx))
            });

        let measure = field(t("间隔从哪条记录算起", "Measure the interval from"), cx).children(
            [
                (
                    false,
                    t("上一条记录", "The previous record"),
                    t(
                        "每条记录都重新计时（默认）",
                        "Every record restarts the clock (default)",
                    ),
                ),
                (
                    true,
                    t("上一条独立记录", "The previous independent record"),
                    t(
                        "只在已计为独立的记录处重新计时",
                        "Only records already counted as independent restart it",
                    ),
                ),
            ]
            .into_iter()
            .map(|(value, label, help)| {
                v_flex()
                    .gap(px(2.))
                    .child(
                        Radio::new(if value {
                            "from-independent"
                        } else {
                            "from-record"
                        })
                        .label(label)
                        .checked(self.from_last_independent == value)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.from_last_independent = value;
                            cx.notify();
                        })),
                    )
                    .child(div().pl(px(24.)).child(help_line(help, Tone::Quiet, cx)))
            }),
        );

        let by = field(t("分析对象", "Analyse by"), cx).child(h_flex().child(segmented(
            "capture-by",
            &[t("物种", "Species"), t("个体", "Individual")],
            self.by_individual as usize,
            {
                let this = cx.entity().downgrade();
                move |index, _, cx| {
                    let _ = this.update(cx, |this, cx| {
                        this.by_individual = index == 1;
                        this.refresh(cx);
                    });
                }
            },
        )));

        let excluded = DEFAULT_EXCLUDED.join(t("、", ", "));
        let options = field(t("其他选项", "Other options"), cx)
            .child(
                Checkbox::new("event")
                    .label(t("生成事件编号", "Add event IDs"))
                    .checked(self.event)
                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                        this.event = *checked;
                        cx.notify();
                    })),
            )
            .child(
                v_flex()
                    .gap(px(2.))
                    .child(
                        Checkbox::new("exclude")
                            .label(t("排除默认标签", "Leave out the default tags"))
                            .checked(self.exclude)
                            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                this.exclude = *checked;
                                this.refresh(cx);
                            })),
                    )
                    .child(div().pl(px(24.)).child(help_line(
                        format!("{excluded} {}", t("不计入分析", "are not counted")),
                        Tone::Quiet,
                        cx,
                    ))),
            )
            .child(
                Checkbox::new("camtrap-dp")
                    .label(t(
                        "输入是 Camtrap DP 的观测表",
                        "The input is a Camtrap DP observations table",
                    ))
                    .checked(self.camtrap_dp)
                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                        this.camtrap_dp = *checked;
                        this.refresh(cx);
                    })),
            );

        let rule_card = card(cx)
            .child(gap_field)
            .child(measure)
            .child(by)
            .child(options);
        let output_card = card(cx).child(output_field(
            output_root.as_deref(),
            self.custom_output.is_some(),
            cx,
            |this: &mut Self, path, cx| {
                this.custom_output = path;
                cx.notify();
            },
        ));
        v_flex()
            .gap(tokens.px("space-4"))
            .child(table_card)
            .child(rule_card)
            .child(output_card)
    }
}

/// A table like the board's: 42 deployments, 14,106 rows, and one busy day of snow leopards at dep07.
fn example_table() -> TagTable {
    let day = |d: i64| (20_364 + d) * 86_400; // 2025-10-03 is day 20,364
    let mut records = Vec::new();
    let path = |dep: usize, i: usize| {
        let col = ["a", "b", "c"][dep % 3];
        format!(
            "/data/2024_spring/col_{col}/dep{:02}_col_{col}/100MEDIA/IMG_{:04}.JPG",
            dep + 1,
            i + 1
        )
    };
    for dep in 0..42 {
        for (s, species) in ["岩羊", "赤狐", "雪豹"].iter().enumerate() {
            for i in 0..111 {
                records.push(Record {
                    path: path(dep, s * 111 + i),
                    time: Some(day(i as i64 - 200) + 3600 * (6 + s as i64)),
                    species: species.to_string(),
                    individual: String::new(),
                    rating: String::new(),
                });
            }
        }
    }
    // dep07 snow leopards: 111 above plus 120 more, 9 of them on 2025-10-03.
    let busy = [
        (6, 12),
        (6, 15),
        (6, 29),
        (6, 43),
        (7, 33),
        (7, 45),
        (8, 59),
        (9, 5),
        (9, 31),
    ];
    for i in 0..120usize {
        let time = match busy.get(i) {
            Some((h, m)) => day(0) + h * 3600 + m * 60,
            None => day(i as i64 + 1) + 7 * 3600,
        };
        records.push(Record {
            path: path(6, 400 + i),
            time: Some(time),
            species: "雪豹".into(),
            individual: String::new(),
            rating: String::new(),
        });
    }
    TagTable {
        records,
        camtrap_dp: false,
    }
}
