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
        "capture|--event" => Some("创建事件 ID。"),
        "capture|--no-exclude" => {
            Some("在独立捕获统计中不自动去除这些标签：Blank 无动物、Useless data 不可用数据、Unidentified 不确定物种、Blur（仅在个体识别中使用）。")
        }
        "capture|--camtrap-dp" => Some("使用 camtrap-dp 数据包中的 observation 表。"),
        "xmp-init|--info" => Some("启用 info 模式，并写出一个 XMP 初始化时间 CSV。"),
        "xmp-update|--datetime" => {
            Some("使用 datetime 模式（读取 `xmp_update_datetime`，而不是 xmp_update）。")
        }
        "xmp-update|species" => Some("将 --tag-type 设置为 species。"),
        "xmp-update|individual" => Some("将 --tag-type 设置为 individual。"),
        "xmp-update|count" => Some("将 --tag-type 设置为 count。"),
        "xmp-update|sex" => Some("将 --tag-type 设置为 sex。"),
        "xmp-update|bodypart" => Some("将 --tag-type 设置为 bodypart。"),
        "extract|--rename" => Some("启用重命名模式（文件名中包含标签）。"),
        "extract|--skip-existing" => Some("如果目标文件已存在则跳过复制（不自动重命名）。"),
        "extract|--use-subdir" => Some("使用子目录组织资源。"),
        _ => None,
    }
}

const OBSERVE_HELP_ZH: &str = r#"从媒体元数据中提取标签

用法: serval observe [OPTIONS] <MEDIA_DIR>

参数:
  <MEDIA_DIR>

选项:
  -o, --output <OUTPUT_DIR>  输出目录 [默认: ./serval_output/serval_observe]
  -x, --xmp                  从 XMP 文件读取
      --video                仅处理视频
      --image                仅处理图像
  -d, --debug                调试模式
  -h, --help                 打印帮助"#;

const CAPTURE_HELP_ZH: &str = r#"对 CSV 文件进行独立捕获统计

用法: serval capture [OPTIONS] <CSV_PATH>

参数:
  <CSV_PATH>  tags.csv 路径

选项:
      --event                创建独立事件 ID
      --no-exclude           在独立捕获统计中不自动去除这些标签：Blank 无动物、Useless data 不可用数据、Unidentified 不确定物种、Blur（仅在个体识别中使用）
      --camtrap-dp           使用 camtrap-dp 数据包中的 observation 表
  -o, --output <OUTPUT_DIR>  输出目录 [默认: ./serval_output/serval_capture]
  -h, --help                 打印帮助"#;

const XMP_HELP_ZH: &str = r#"XMP 文件操作

用法: serval xmp <COMMAND>

子命令:
  copy    递归查找路径中的所有 XMP 文件并复制到输出目录
  init    为媒体文件初始化 XMP 文件
  update  根据 CSV 更新 XMP 文件。标签模式：使用 CSV 中的 `xmp_update` 列，并根据 `--tag-type` 标签类别选择更新 `species`（物种）或 `individual`（个体）。时间模式：使用 CSV 中的 `xmp_update_datetime` 列（格式：yyyy-MM-dd HH:mm:ss）更新 XMP 中的时间
  remove  递归删除目录中的所有 XMP 文件
  sync    将 XMP 元数据同步到对应的媒体文件
  help    打印此消息或指定子命令的帮助

选项:
  -h, --help  打印帮助"#;

const XMP_COPY_HELP_ZH: &str = r#"递归查找路径中的所有 XMP 文件并复制到输出目录

用法: serval xmp copy <SOURCE_DIR> <OUTPUT_DIR>

参数:
  <SOURCE_DIR>
  <OUTPUT_DIR>

选项:
  -h, --help  打印帮助"#;

const XMP_INIT_HELP_ZH: &str = r#"为媒体文件初始化 XMP 文件

用法: serval xmp init [OPTIONS] <SOURCE_DIR>

参数:
  <SOURCE_DIR>

选项:
  -i, --info  启用 info 模式，并写出一个 XMP 初始化时间 CSV
  -h, --help  打印帮助"#;

const XMP_UPDATE_HELP_ZH: &str = r#"根据 CSV 更新 XMP 文件。标签模式使用 `xmp_update`，并根据 `--tag-type` 选择 `species` 或 `individual`。时间模式（`--datetime`）使用 `xmp_update_datetime`（格式: yyyy-MM-dd HH:mm:ss）

用法: serval xmp update [OPTIONS] <CSV_PATH>

参数:
  <CSV_PATH>

选项:
  -t, --tag-type <TYPE>  标签模式下的标签类别：`species`（物种）或 `individual`（个体 ID） [其它可选值: count, sex, bodypart]
      --datetime         使用时间更新模式（读取 CSV 中的 `xmp_update_datetime` 列，而不是 `xmp_update` 列）
  -h, --help             打印帮助"#;

const XMP_REMOVE_HELP_ZH: &str = r#"递归删除目录中的所有 XMP 文件

用法: serval xmp remove <SOURCE_DIR>

参数:
  <SOURCE_DIR>

选项:
  -h, --help  打印帮助"#;

const XMP_SYNC_HELP_ZH: &str = r#"将 XMP 元数据同步到对应的媒体文件

用法: serval xmp sync [OPTIONS] [DIR]

参数:
  [DIR]  包含待同步 XMP 文件的目录

选项:
      --csv <CSV_PATH>  包含待同步 XMP 文件路径的 CSV 文件
  -h, --help            打印帮助"#;

const EXTRACT_HELP_ZH: &str = r#"根据目标值筛选并提取（复制）资源（基于 tags.csv）

# 基础筛选
对单字段查询进行简单的筛选：
serval extract tags.csv -f species -v "Snow leopard" （提取“物种”为“雪豹”的所有媒体）
serval extract tags.csv -f rating -v "4-5" （提取“星级”为“4到5”的所有媒体）

# 高级筛选
对包含逻辑运算符的复杂多字段查询使用 `-f advanced`：

多物种 AND（同时包含多物种的图片）:
-f advanced -v "species:Blue sheep and species:Snow leopard" （提取所有既有“岩羊”又有“雪豹”的媒体）

AND 条件:
-f advanced -v "species:Serval and rating:4-5" （提取所有星级为“4到5”的“薮猫”媒体）
OR 条件:
-f advanced -v "species:Serval or species:White-lipped deer" （提取所有“物种”为“薮猫”或“白唇鹿”的媒体）

复杂组合:
-f advanced -v "(species:Serval and rating:4-5) or (species:Snow leopard and rating:5)" （提取所有“4到5星”的“薮猫”媒体和所有“5星”的“雪豹”媒体）

# 字段别名
species: sp, s  |  individual: ind, i  |  rating: rate, r
path: p  |  event: e  |  custom: c

# 运算符
精确匹配:     species:Fox
范围:         rating:3-5
比较:         rating:>=4, rating:>4, rating:<5, rating:<=5

用法: serval extract [OPTIONS] --filter-type <FILTER> --value <VALUE> <CSV_PATH>

参数:
  <CSV_PATH>
          tags.csv 路径

选项:
  -f, --filter-type <FILTER>
          指定筛选类型

          [可选值: species 物种, path 路径, individual 个体, rating 星级, event 独立捕获事件ID, custom 自定义, advanced 逻辑]

  -v, --value <VALUE>
          目标值（对于 path 过滤器则为子串），使用 "ALL_VALUES" 表示所有非空值

      --rename
          启用重命名模式（文件名中包含标签）

      --skip-existing
          如果目标文件已存在则跳过复制（不自动重命名）

      --use-subdir
          使用子目录组织资源

      --subdir-type <SUBDIR_TYPE>
          指定创建子目录时使用的类型

          [默认: species 物种]
          [可选值: species 物种, individual 个体, rating 星级, custom 自定义]

  -o, --output <OUTPUT_DIR>
          设置输出目录

          [默认: ./serval_output/serval_extract]

  -h, --help
          打印帮助（使用 '-h' 可查看摘要）"#;

const TRANSLATE_HELP_ZH: &str = r#"根据 taglist 转换 csv 中的物种列

用法: serval translate [OPTIONS] --taglist-path <TAGLIST> --from <FROM> --to <TO> <CSV_PATH>

参数:
  <CSV_PATH>  tags.csv 路径

选项:
  -t, --taglist-path <TAGLIST>  taglist csv 文件路径
  -o, --output <OUTPUT_DIR>     输出目录 [默认: ./serval_output/serval_translate]
      --from <FROM>             要作为源进行转换的列名（位于 taglist 中）
      --to <TO>                 要转换到的目标列名（位于 taglist 中）
  -h, --help                    打印帮助"#;

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
