//! Read-only labeling progress for a task folder.
use crate::browse::review;
use crate::caracal::{self, Mode};
use crate::i18n::{count, t};
use crate::progress_cache;
use crate::screens::form::{Tone, card, help_line, pick_folder};
use crate::table::Csv;
use gpui_kit::component::{
    Disableable as _,
    button::{Button, ButtonVariants as _},
    h_flex, v_flex,
};
use gpui_kit::*;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, SystemTime};

#[derive(Default)]
struct Folder {
    files: usize,
    labeled: usize,
    children: BTreeMap<PathBuf, Folder>,
}

impl Folder {
    fn add(&mut self, path: &Path, labeled: bool) {
        let mut node = self;
        node.files += 1;
        node.labeled += usize::from(labeled);
        for component in path.components() {
            node = node
                .children
                .entry(PathBuf::from(component.as_os_str()))
                .or_default();
            node.files += 1;
            node.labeled += usize::from(labeled);
        }
    }
    fn summary(&self) -> String {
        let percent = if self.files == 0 {
            0.0
        } else {
            self.labeled as f64 * 100.0 / self.files as f64
        };
        format!(
            "{} / {} ({percent:.1}%) · {} {}",
            count(self.labeled),
            count(self.files),
            count(self.files - self.labeled),
            t("待标", "pending")
        )
    }
}

struct Results {
    root: PathBuf,
    table: PathBuf,
    csv: Csv,
    tree: Folder,
    changed: usize,
    removed: usize,
    scanned_at: SystemTime,
}
impl Results {
    fn read(
        root: PathBuf,
        table: PathBuf,
        changed: usize,
        removed: usize,
        scanned_at: SystemTime,
    ) -> Result<Self, String> {
        let csv = Csv::read(&table)?;
        let mut tree = Folder::default();
        let path_col = csv.column("path").ok_or("Missing path column")?;
        let species_col = csv.column("species").ok_or("Missing species column")?;
        let mut media = BTreeMap::<&Path, bool>::new();
        for row in &csv.rows {
            let path = Path::new(csv.cell(row, Some(path_col)));
            *media.entry(path).or_default() |= !csv.cell(row, Some(species_col)).is_empty();
        }
        for (path, labeled) in media {
            let folder = path.parent().unwrap_or(Path::new(""));
            let relative = folder.strip_prefix(&root).map_err(|_| {
                format!(
                    "{}: {}",
                    t("路径不在任务目录中", "Path outside task"),
                    folder.display()
                )
            })?;
            tree.add(relative, labeled);
        }
        Ok(Self {
            root,
            table,
            csv,
            tree,
            changed,
            removed,
            scanned_at,
        })
    }
    fn open(&self, folder: &Path, cx: &mut App) -> Result<(), String> {
        let table = if folder.as_os_str().is_empty() {
            self.table.clone()
        } else {
            let prefix = self.root.join(folder);
            let column = self.csv.column("path");
            let rows: Vec<_> = self
                .csv
                .rows
                .iter()
                .enumerate()
                .filter_map(|(i, row)| {
                    Path::new(self.csv.cell(row, column))
                        .starts_with(&prefix)
                        .then_some(i)
                })
                .collect();
            review::subset(
                &self.csv,
                &rows,
                &self.table.with_file_name("progress_selection"),
            )?
        };
        caracal::open(Mode::Species, &table, cx);
        Ok(())
    }
}

#[derive(Default)]
pub struct Progress {
    program: Option<PathBuf>,
    root: Option<PathBuf>,
    results: Option<Results>,
    expanded: BTreeSet<PathBuf>,
    busy: bool,
    status: String,
    error: Option<String>,
}
impl Progress {
    pub fn set_program(&mut self, path: Option<&Path>, supports: bool, cx: &mut Context<Self>) {
        self.program = path.filter(|_| supports).map(Path::to_owned);
        cx.notify();
    }
    pub fn example(&mut self, path: &Path, cx: &mut Context<Self>) {
        let root = PathBuf::from("/data");
        self.root = Some(root.clone());
        self.results = Results::read(root, path.to_owned(), 3, 0, SystemTime::now()).ok();
        cx.notify();
    }
    fn refresh(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if self.root.as_ref() != Some(&root) {
            self.results = None;
            self.expanded.clear();
        }
        self.root = Some(root.clone());
        let Some(program) = self.program.clone() else {
            self.error = Some(
                t(
                    "请在设置中选择支持 CSV 输入的 Serval。",
                    "Choose a Serval with CSV input support in Settings.",
                )
                .into(),
            );
            cx.notify();
            return;
        };
        self.error = None;
        self.busy = true;
        self.status = t("正在检查文件夹…", "Checking folder…").into();
        cx.notify();
        let (sender, receiver) = mpsc::channel();
        let displayed = self.results.as_ref().map(|r| r.table.clone());
        let worker = cx.background_executor().spawn(async move {
            let root = std::fs::canonicalize(&root).map_err(|e| e.to_string())?;
            let snapshot = progress_cache::refresh_with_progress(&root, &program, |n| {
                let _ = sender.send(n);
            })?;
            if displayed.as_ref() == Some(&snapshot.table) {
                return Ok((None, snapshot.scanned_at));
            }
            Results::read(
                root,
                snapshot.table,
                snapshot.changed,
                snapshot.removed,
                snapshot.scanned_at,
            )
            .map(|data| (Some(data), snapshot.scanned_at))
        });
        cx.spawn(async move |this, cx| {
            loop {
                match receiver.try_recv() {
                    Ok(n) => {
                        let _ = this.update(cx, |view, cx| {
                            view.status = format!("{} {} XMP", t("正在读取", "Reading"), count(n));
                            cx.notify();
                        });
                    }
                    Err(mpsc::TryRecvError::Disconnected) => break,
                    Err(mpsc::TryRecvError::Empty) => {}
                }
                cx.background_executor()
                    .timer(Duration::from_millis(80))
                    .await;
            }
            let result = worker.await;
            let _ = this.update(cx, |view, cx| {
                view.busy = false;
                match result {
                    Ok((Some(value), _)) => view.results = Some(value),
                    Ok((None, time)) => {
                        if let Some(data) = &mut view.results {
                            data.changed = 0;
                            data.removed = 0;
                            data.scanned_at = time;
                        }
                    }
                    Err(e) => view.error = Some(e),
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub fn cache_settings(&self, entity: Entity<Self>, cx: &App) -> Div {
        let mut row = card(cx).child(t("标注进度缓存", "Labeling progress cache"));
        if let Some(root) = self.root.clone() {
            row = row.child(root.display().to_string()).child(
                Button::new("clear-progress")
                    .outline()
                    .disabled(self.busy)
                    .label(t("清除该任务的缓存", "Clear this task’s cache"))
                    .on_click(move |_, _, cx| {
                        entity.update(cx, |view, cx| {
                            if view.busy {
                                return;
                            }
                            match progress_cache::clear(&root) {
                                Ok(()) => {
                                    view.results = None;
                                    view.error = None;
                                    view.root = None;
                                    view.expanded.clear();
                                }
                                Err(err) => view.error = Some(err),
                            }
                            cx.notify();
                        })
                    }),
            );
            if let Some(err) = &self.error {
                row = row.child(help_line(err.clone(), Tone::Danger, cx))
            }
        } else {
            row = row.child(help_line(
                t("未选择任务", "No task selected"),
                Tone::Quiet,
                cx,
            ));
        }
        row
    }
    fn tree_rows(
        &self,
        node: &Folder,
        path: &Path,
        depth: usize,
        out: &mut Vec<Div>,
        cx: &mut Context<Self>,
    ) {
        let idx = out.len();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| t("全部", "All").into());
        let mut line = h_flex().gap(px(8.)).pl(px(12. * depth as f32));
        if !node.children.is_empty() && depth != 0 {
            let folder = path.to_owned();
            line = line.child(
                Button::new(("progress-expand", idx))
                    .ghost()
                    .label(format!(
                        "{} {name}",
                        if self.expanded.contains(path) {
                            "▾"
                        } else {
                            "▸"
                        }
                    ))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.expanded.remove(&folder) {
                            this.expanded.insert(folder.clone());
                        }
                        cx.notify();
                    })),
            );
        } else {
            line = line.child(name);
        }
        let folder = path.to_owned();
        line = line.child(div().flex_1()).child(node.summary()).child(
            Button::new(("progress-caracal", idx))
                .ghost()
                .label(t("在 Caracal 中打开", "Open in Caracal"))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(results) = &this.results {
                        this.error = results.open(&folder, cx).err();
                        cx.notify();
                    }
                })),
        );
        out.push(line);
        if depth == 0 || self.expanded.contains(path) {
            for (name, folder) in &node.children {
                self.tree_rows(folder, &path.join(name), depth + 1, out, cx);
            }
        }
    }
}
impl Render for Progress {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut actions = h_flex().gap(px(8.)).child(
            Button::new("pick-task")
                .outline()
                .disabled(self.busy)
                .label(t("选择任务文件夹…", "Choose task folder…"))
                .on_click(cx.listener(|_, _, window, cx| {
                    pick_folder(window, cx, |view: &mut Self, path, _, cx| {
                        view.refresh(path, cx)
                    });
                })),
        );
        if let Some(root) = self.root.clone() {
            actions = actions.child(
                Button::new("refresh-task")
                    .outline()
                    .disabled(self.busy)
                    .label(t("刷新进度", "Refresh progress"))
                    .on_click(cx.listener(move |view, _, _, cx| view.refresh(root.clone(), cx))),
            );
        }
        let target = cx.entity();
        let mut page = v_flex().gap(px(12.)).child(
            card(cx)
                .child(actions)
                .child(
                    self.root
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| {
                            t("或将任务文件夹拖到这里", "Or drop a task folder here").into()
                        }),
                )
                .on_drop(move |drop: &ExternalPaths, _, cx| {
                    if let Some(path) = drop.paths().first() {
                        target.update(cx, |view, cx| view.refresh(path.to_owned(), cx));
                    }
                }),
        );
        if self.busy {
            page = page.child(self.status.clone());
        }
        if let Some(error) = &self.error {
            page = page.child(help_line(error.clone(), Tone::Danger, cx));
        }
        if let Some(results) = &self.results {
            let time: chrono::DateTime<chrono::Local> = results.scanned_at.into();
            let mut rows = vec![];
            self.tree_rows(&results.tree, Path::new(""), 0, &mut rows, cx);
            page = page.child(
                card(cx)
                    .child(format!(
                        "{} · {} {} · {} {}",
                        time.format("%Y-%m-%d %H:%M"),
                        results.changed,
                        t("个文件已读取", "files read"),
                        results.removed,
                        t("个文件已移除", "files removed")
                    ))
                    .children(rows),
            );
        }
        page
    }
}

#[cfg(test)]
mod tests {
    use super::{Folder, Results};
    use std::path::Path;
    #[test]
    fn parent_counts_include_direct_files_and_keep_same_named_branches_apart() {
        let mut tree = Folder::default();
        for (p, marked) in [
            ("", true),
            ("a/site", true),
            ("a/site", false),
            ("b/site", false),
        ] {
            tree.add(Path::new(p), marked);
        }
        assert_eq!((tree.files, tree.labeled), (4, 2));
        assert_eq!(
            tree.children[Path::new("a")].children[Path::new("site")].files,
            2
        );
        assert_eq!(tree.children[Path::new("b")].labeled, 0);
    }

    #[test]
    fn a_multi_species_file_counts_once_and_retains_all_source_rows() {
        let dir =
            std::env::temp_dir().join(format!("waxbill-progress-tree-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let table = dir.join("tags.csv");
        let mut writer = csv::Writer::from_path(&table).unwrap();
        writer.write_record(["path", "species"]).unwrap();
        for (name, species) in [("a.xmp", "Fox"), ("a.xmp", "Pika"), ("b.xmp", "")] {
            writer
                .write_record([dir.join(name).to_str().unwrap(), species])
                .unwrap();
        }
        writer.flush().unwrap();
        let data = Results::read(dir.clone(), table, 2, 0, std::time::SystemTime::now()).unwrap();
        assert_eq!(
            (data.tree.files, data.tree.labeled, data.csv.rows.len()),
            (2, 1, 3)
        );
        drop(writer);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
