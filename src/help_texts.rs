use crate::i18n::Language;

#[cfg(test)]
const HELP_KEYS: &[&str] = &[
    "observe",
    "capture",
    "xmp",
    "xmp-copy",
    "xmp-init",
    "xmp-update",
    "xmp-remove",
    "xmp-sync",
    "extract",
    "translate",
    "observe|--xmp",
    "observe|--video",
    "observe|--image",
    "observe|--debug",
    "capture|--event",
    "capture|--no-exclude",
    "capture|--camtrap-dp",
    "xmp-init|--info",
    "xmp-update|--datetime",
    "xmp-update|species",
    "xmp-update|individual",
    "xmp-update|count",
    "xmp-update|sex",
    "xmp-update|bodypart",
    "extract|--rename",
    "extract|--skip-existing",
    "extract|--use-subdir",
];

pub fn text_for_key(language: Language, key: &str) -> Option<&'static str> {
    match language {
        Language::En => en_text_for_key(key),
        Language::ZhCn => zh_text_for_key(key).or_else(|| en_text_for_key(key)),
    }
}

fn en_text_for_key(key: &str) -> Option<&'static str> {
    match key {
        "observe" => Some(OBSERVE_HELP_EN),
        "capture" => Some(CAPTURE_HELP_EN),
        "xmp" => Some(XMP_HELP_EN),
        "xmp-copy" => Some(XMP_COPY_HELP_EN),
        "xmp-init" => Some(XMP_INIT_HELP_EN),
        "xmp-update" => Some(XMP_UPDATE_HELP_EN),
        "xmp-remove" => Some(XMP_REMOVE_HELP_EN),
        "xmp-sync" => Some(XMP_SYNC_HELP_EN),
        "extract" => Some(EXTRACT_HELP_EN),
        "translate" => Some(TRANSLATE_HELP_EN),
        "observe|--xmp" => Some("Read tags from XMP files."),
        "observe|--video" => Some("Only process video files."),
        "observe|--image" => Some("Only process image files."),
        "observe|--debug" => Some("Enable debug output."),
        "capture|--event" => Some("Create event ID."),
        "capture|--no-exclude" => Some(
            "Do not exclude default tags (Blank, Useless data, Unidentified, Unknown, Blur) from temporal independence analysis.",
        ),
        "capture|--camtrap-dp" => Some("Use observation table from camtrap-dp data package."),
        "xmp-init|--info" => Some("Enable info mode and write an XMP init datetime CSV."),
        "xmp-update|--datetime" => {
            Some("Use datetime mode (reads `xmp_update_datetime` instead of xmp_update).")
        }
        "xmp-update|species" => Some("Set --tag-type to species."),
        "xmp-update|individual" => Some("Set --tag-type to individual."),
        "xmp-update|count" => Some("Set --tag-type to count."),
        "xmp-update|sex" => Some("Set --tag-type to sex."),
        "xmp-update|bodypart" => Some("Set --tag-type to bodypart."),
        "extract|--rename" => Some("Enable rename mode (including tags in filenames)."),
        "extract|--skip-existing" => {
            Some("Skip the copy when the destination file already exists (no auto-renaming).")
        }
        "extract|--use-subdir" => Some("Use subdirectories to organize resources."),
        _ => None,
    }
}

const OBSERVE_HELP_EN: &str = r#"Retrieve tags from media metadata

Usage: serval observe [OPTIONS] <MEDIA_DIR>

Arguments:
  <MEDIA_DIR>

Options:
  -o, --output <OUTPUT_DIR>  Output directory [default: ./serval_output/serval_observe]
  -x, --xmp                  Read from XMP files
      --video                Video only
      --image                Image only
  -d, --debug                Debug mode
  -h, --help                 Print help"#;

const CAPTURE_HELP_EN: &str = r#"Temporal independence analysis on a CSV file

Usage: serval capture [OPTIONS] <CSV_PATH>

Arguments:
  <CSV_PATH>  Path for tags.csv

Options:
      --event                Create event ID
      --no-exclude           Do not exclude default tags (Blank, Useless data, Unidentified, Unknown, Blur) from temporal independence analysis
      --camtrap-dp           Use observation table from camtrap-dp data package
  -o, --output <OUTPUT_DIR>  Output directory [default: ./serval_output/serval_capture]
  -h, --help                 Print help"#;

const XMP_HELP_EN: &str = r#"XMP file operations

Usage: serval xmp <COMMAND>

Commands:
  copy    Copy XMP files to output directory
  init    Initialize XMP files for media files
  update  Update XMP files from CSV. Tag mode uses: `xmp_update`, plus `species` or `individual` according to `--tag-type`. Datetime mode (`--datetime`) uses: `xmp_update_datetime` (format: yyyy-MM-dd HH:mm:ss)
  remove  Remove all XMP files recursively from a directory
  sync    Sync XMP metadata to corresponding media files
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help"#;

const XMP_COPY_HELP_EN: &str = r#"Copy XMP files to output directory

Usage: serval xmp copy <SOURCE_DIR> <OUTPUT_DIR>

Arguments:
  <SOURCE_DIR>
  <OUTPUT_DIR>

Options:
  -h, --help  Print help"#;

const XMP_INIT_HELP_EN: &str = r#"Initialize XMP files for media files

Usage: serval xmp init [OPTIONS] <SOURCE_DIR>

Arguments:
  <SOURCE_DIR>

Options:
  -i, --info  Enable info mode and write an XMP init datetime CSV
  -h, --help  Print help"#;

const XMP_UPDATE_HELP_EN: &str = r#"Update XMP files from CSV. Tag mode uses: `xmp_update`, plus `species` or `individual` according to `--tag-type`. Datetime mode (`--datetime`) uses: `xmp_update_datetime` (format: yyyy-MM-dd HH:mm:ss)

Usage: serval xmp update [OPTIONS] <CSV_PATH>

Arguments:
  <CSV_PATH>

Options:
  -t, --tag-type <TYPE>  Tag type for tag mode (`species` or `individual`) [possible values: species, individual, count, sex, bodypart]
      --datetime         Use datetime mode (reads `xmp_update_datetime` instead of xmp_update)
  -h, --help             Print help"#;

const XMP_REMOVE_HELP_EN: &str = r#"Remove all XMP files recursively from a directory

Usage: serval xmp remove <SOURCE_DIR>

Arguments:
  <SOURCE_DIR>

Options:
  -h, --help  Print help"#;

const XMP_SYNC_HELP_EN: &str = r#"Sync XMP metadata to corresponding media files

Usage: serval xmp sync [OPTIONS] [DIR]

Arguments:
  [DIR]  Directory containing XMP files to sync

Options:
      --csv <CSV_PATH>  CSV file with paths to XMP files to sync
  -h, --help            Print help"#;

const EXTRACT_HELP_EN: &str = r#"Extract and copy resources by filtering target values (based on tags.csv)

# Basic Filtering
Use simple filter types for single-field queries:
serval extract tags.csv -f species -v "Snow leopard"
serval extract tags.csv -f rating -v "4-5"

# Advanced Filtering
Use `-f advanced` for complex multi-field queries with logical operators:

Same Species AND (images with BOTH species):
-f advanced -v "species:Blue sheep and species:Snow leopard"

AND conditions:
-f advanced -v "species:Serval and rating:4-5"

OR conditions:
-f advanced -v "species:Serval or species:White-lipped deer"

Complex combinations:
-f advanced -v "(species:Serval and rating:4-5) or (species:Snow leopard and rating:5)"

# Field Aliases
species: sp, s  |  individual: ind, i  |  rating: rate, r
path: p  |  event: e  |  custom: c

# Operators
Exact match:     species:Fox
Range:           rating:3-5
Comparisons:     rating:>=4, rating:>4, rating:<5, rating:<=5

Usage: serval extract [OPTIONS] --filter-type <FILTER> --value <VALUE> <CSV_PATH>

Arguments:
  <CSV_PATH>
          Path for tags.csv

Options:
  -f, --filter-type <FILTER>
          Specify the filter type

          [possible values: species, path, individual, rating, event, custom, advanced]

  -v, --value <VALUE>
          The target value (or substring for the path filter), use "ALL_VALUES" for all non-empty values

      --rename
          Enable rename rename mode (including tags in filenames)

      --skip-existing
          Skip the copy when the destination file already exists (no auto-renaming)

      --use-subdir
          Use subdirectories to organize resources

      --subdir-type <SUBDIR_TYPE>
          Specify the type used when creating subdirectories

          [default: species]
          [possible values: species, individual, rating, custom]

  -o, --output <OUTPUT_DIR>
          Set the output directory

          [default: ./serval_output/serval_extract]

  -h, --help
          Print help (see a summary with '-h')"#;

const TRANSLATE_HELP_EN: &str = r#"Translate species column in csv according to taglist

Usage: serval translate [OPTIONS] --taglist-path <TAGLIST> --from <FROM> --to <TO> <CSV_PATH>

Arguments:
  <CSV_PATH>  Path for tags.csv

Options:
  -t, --taglist-path <TAGLIST>  Path for the taglist csv file
  -o, --output <OUTPUT_DIR>     Output directory [default: ./serval_output/serval_translate]
      --from <FROM>             Column name (in taglist) to translate from
      --to <TO>                 Column name (in taglist) to translate to
  -h, --help                    Print help"#;
fn zh_text_for_key(key: &str) -> Option<&'static str> {
    match key {
        "observe" => Some(OBSERVE_HELP_ZH),
        "capture" => Some(CAPTURE_HELP_ZH),
        "xmp" => Some(XMP_HELP_ZH),
        "xmp-copy" => Some(XMP_COPY_HELP_ZH),
        "xmp-init" => Some(XMP_INIT_HELP_ZH),
        "xmp-update" => Some(XMP_UPDATE_HELP_ZH),
        "xmp-remove" => Some(XMP_REMOVE_HELP_ZH),
        "xmp-sync" => Some(XMP_SYNC_HELP_ZH),
        "extract" => Some(EXTRACT_HELP_ZH),
        "translate" => Some(TRANSLATE_HELP_ZH),
        "observe|--xmp" => Some("从 XMP 文件读取标签。"),
        "observe|--video" => Some("仅处理视频文件。"),
        "observe|--image" => Some("仅处理图像文件。"),
        "observe|--debug" => Some("启用调试输出。"),
        "capture|--event" => Some("对每一个独立捕获事件创建一个 ID，并输出在表格中。"),
        "capture|--no-exclude" => Some(
            "在独立捕获统计中不自动去除这些标签：
Blank 无动物
Useless data 不可用数据
Unidentified 不确定物种
Blur（仅用于个体识别）",
        ),
        "capture|--camtrap-dp" => Some("使用 camtrap-dp 数据包中的 observation 表来进行统计，此时 CSV 路径应为 observation.csv 的路径。"),
        "xmp-init|--info" => Some("启用信息汇总模式，在初始化 XMP 的同时，读取媒体所有可参考的时间信息，汇总生成一个 CSV 用于时间的观察，并给出 `xmp_update_datetime` 列便于修正。"),
        "xmp-update|--datetime" => {
            Some("使用时间模式对时间信息进行更新（读取 CSV 表格中的 `xmp_update_datetime` 列）。")
        }
        "xmp-update|species" => Some(
            "使用标签模式对物种信息进行更新（读取 CSV 表格中的 `xmp_update` 列）。",
        ),
        "xmp-update|individual" => Some(
            "使用标签模式对个体信息进行更新（读取 CSV 表格中的 `xmp_update` 列）。",
        ),
        "xmp-update|count" => Some(
            "使用标签模式对数量信息进行更新（读取 CSV 表格中的 `xmp_update` 列）。",
        ),
        "xmp-update|sex" => Some(
            "使用标签模式对性别信息进行更新（读取 CSV 表格中的 `xmp_update` 列）。",
        ),
        "xmp-update|bodypart" => Some(
            "使用标签模式对身体部位信息进行更新（读取 CSV 表格中的 `xmp_update` 列）。",
        ),
        "extract|--rename" => Some("给提取出来的媒体加上标签前缀，以便查看，格式为：物种名-个体名-原文件名。"),
        "extract|--skip-existing" => Some("如果目标文件已存在，则跳过复制（一般不勾选，主要用于断点续传）"),
        "extract|--use-subdir" => Some("使用子目录来组织提取出来的媒体，如物种、个体、星级等。"),
        _ => None,
    }
}

const OBSERVE_HELP_ZH: &str = r#"将媒体的元数据信息（包括媒体路径、媒体类型、时间、标签等）提取至一个 CSV 表格，便于后续分析。
在这个命令中，你可以指定：
· 观察媒体本身还是观察它的附属 XMP 文件；
· 只观察照片或只观察视频；
· 或直接进入调试模式进行更多操作。"#;

const CAPTURE_HELP_ZH: &str = r#"使用“媒体信息观察”得到的 tags.csv，或 camtrap-dp 中的 observation.csv，进行独立捕获统计。
你需要在下方命令行中根据你的需求声明：
· 独立捕获的时间间隔；
· 是与该物种上一次独立捕获的首条媒体比较时间间隔（Last independent record）还是与该物种上一条记录比较时间间隔（Last record）；
· 是对物种还是个体进行独立捕获统计；
· 给出的选项中，哪一个代表了相机位点的存储层级。

独立捕获统计默认去除这些标签：无动物、不可用数据、不确定物种，和个体识别标签中的模糊。对于鸟类、狐类、小型啮齿类等未定义到物种级别的标签，没有在这一步中去除，需要在后续统计时予以关注。"#;

const XMP_HELP_ZH: &str = r#"这里是一系列与 XMP 文件有关的子命令，具体帮助请点击子命令进行查看。"#;

const XMP_COPY_HELP_ZH: &str = r#"递归查找路径中的所有 XMP 文件，并按照存储结构将其复制到输出目录。可以用来进行 XMP 的备份操作，或在不需要拷贝媒体原文件的情况下传递媒体的元数据信息。"#;

const XMP_INIT_HELP_ZH: &str = r#"为媒体文件初始化 XMP 文件，将媒体文件已有的时间和标签信息写入 XMP 文件中。

其中，启用信息汇总模式，可以在初始化时，读取媒体所有可参考的时间信息并写入一个 CSV 文件，用于时间的观察与修正。"#;

const XMP_UPDATE_HELP_ZH: &str = r#"根据 CSV 更新 XMP 文件。在 tags.csv 的基础上，可以新建 xmp_update 列（用于更新物种、个体等标签）或 xmp_update_datetime 列（用于更新时间）来进行信息的更新。

注意：
· 更改的标签填入新建的列中，不要进行原位修改；
· 保存 CSV 表格时，格式选择为 CSV UTF-8 编码"#;

const XMP_REMOVE_HELP_ZH: &str = r#"递归删除目录中的所有 XMP 文件"#;

const XMP_SYNC_HELP_ZH: &str = r#"将 XMP 元数据同步到对应的媒体文件，可以选择对路径下所有文件进行同步，也可以通过 CSV 表格指定需要同步的媒体。"#;

const EXTRACT_HELP_ZH: &str = r#"根据目标值筛选媒体，并复制到输出路径。可以实现如下功能：

# 基础筛选：对单字段查询进行简单的筛选
示例 1：提取“物种”为“雪豹”的所有媒体
筛选类别：物种筛选
值：雪豹
示例 2：提取“星级”为“4-5”的所有媒体
筛选类别：星级筛选
值：4-5

# 高级筛选：包含逻辑运算符的复杂多字段筛选（AND/OR）
示例 1：提取所有既有“岩羊”又有“雪豹”的媒体
筛选类别：高级筛选
值：species:岩羊 and species:雪豹

# 也可以在 tags.csv 中新建 custom 列，手动筛选并赋值后，再用筛选类别中的“自定义筛选”进行提取

提取时，可以以物种、个体、星级等存储方式创建子路径，与此同时还可以选择是否保留相机位点名称。"#;

const TRANSLATE_HELP_ZH: &str = r#"实现 CSV 中物种列的中文名、英文名、拉丁名等的相互转换"#;

#[cfg(test)]
mod tests {
    use super::{en_text_for_key, zh_text_for_key, HELP_KEYS};

    #[test]
    fn english_help_covers_all_registered_keys() {
        for key in HELP_KEYS {
            assert!(
                en_text_for_key(key).is_some(),
                "missing English help for key: {key}"
            );
        }
    }

    #[test]
    fn chinese_help_covers_all_registered_keys() {
        for key in HELP_KEYS {
            assert!(
                zh_text_for_key(key).is_some(),
                "missing Chinese help for key: {key}"
            );
        }
    }
}
