use crate::i18n::Language;

pub fn text_for_key(language: Language, key: &str) -> Option<&'static str> {
    match language {
        Language::En => en_text_for_key(key),
        Language::ZhCn => zh_text_for_key(key).or_else(|| en_text_for_key(key)),
    }
}

fn en_text_for_key(key: &str) -> Option<&'static str> {
    match key {
        "observe" => Some(OBSERVE_HELP),
        "capture" => Some(CAPTURE_HELP),
        "xmp" => Some(XMP_HELP),
        "xmp-copy" => Some(XMP_COPY_HELP),
        "xmp-init" => Some(XMP_INIT_HELP),
        "xmp-update" => Some(XMP_UPDATE_HELP),
        "xmp-remove" => Some(XMP_REMOVE_HELP),
        "xmp-sync" => Some(XMP_SYNC_HELP),
        "extract" => Some(EXTRACT_HELP),
        "translate" => Some(TRANSLATE_HELP),
        "observe|--xmp" => Some("Read tags from XMP sidecar files instead of embedded metadata."),
        "observe|--subject" => Some("Include Subject metadata in output."),
        "observe|--modified-time" => Some("Include filesystem modified time in output."),
        "observe|--video" => Some("Only process video files."),
        "observe|--image" => Some("Only process image files."),
        "observe|--debug" => Some("Enable debug output."),
        "observe|--independent" => Some("Run temporal independence analysis after metadata extraction."),
        "capture|--event" => Some("Create event IDs during temporal independence analysis."),
        "capture|--no-exclude" => Some(
            "Do not exclude default tags: Blank, Useless data, Unidentified, Human, Unknown, Blur.",
        ),
        "capture|--camtrap-dp" => Some("Use observation table from camtrap-dp data package."),
        "xmp-update|--datetime" => Some("Update datetime instead of tag fields."),
        "xmp-update|species" => Some("Set --tag-type to species."),
        "xmp-update|individual" => Some("Set --tag-type to individual."),
        "xmp-update|count" => Some("Set --tag-type to count."),
        "xmp-update|sex" => Some("Set --tag-type to sex."),
        "xmp-update|bodypart" => Some("Set --tag-type to bodypart."),
        "extract|--rename" => Some("Enable rename mode, including tags in filenames."),
        "extract|--skip-existing" => Some(
            "Skip copy if destination already exists (no auto-renaming).",
        ),
        "extract|--use-subdir" => Some("Use subdirectories to organize copied resources."),
        _ => None,
    }
}

fn zh_text_for_key(key: &str) -> Option<&'static str> {
    match key {
        "observe|--xmp" => Some("从 XMP 侧车文件读取标签，而不是从媒体嵌入元数据读取。"),
        "observe|--subject" => Some("在输出中包含 Subject 元数据。"),
        "observe|--modified-time" => Some("在输出中包含文件修改时间。"),
        "observe|--video" => Some("仅处理视频文件。"),
        "observe|--image" => Some("仅处理图像文件。"),
        "observe|--debug" => Some("启用调试输出。"),
        "observe|--independent" => Some("提取完成后执行时间独立性分析。"),
        "capture|--event" => Some("在时间独立性分析过程中生成事件 ID。"),
        "capture|--no-exclude" => Some(
            "不排除默认标签：Blank、Useless data、Unidentified、Human、Unknown、Blur。",
        ),
        "capture|--camtrap-dp" => Some("使用 camtrap-dp 数据包中的 observation 表。"),
        "xmp-update|--datetime" => Some("更新日期时间，而不是更新标签字段。"),
        "xmp-update|species" => Some("将 --tag-type 设置为 species。"),
        "xmp-update|individual" => Some("将 --tag-type 设置为 individual。"),
        "xmp-update|count" => Some("将 --tag-type 设置为 count。"),
        "xmp-update|sex" => Some("将 --tag-type 设置为 sex。"),
        "xmp-update|bodypart" => Some("将 --tag-type 设置为 bodypart。"),
        "extract|--rename" => Some("启用重命名模式（文件名中包含标签）。"),
        "extract|--skip-existing" => Some("如果目标文件已存在则跳过复制（不自动重命名）。"),
        "extract|--use-subdir" => Some("使用子目录组织复制结果。"),
        _ => None,
    }
}

const OBSERVE_HELP: &str = r#"Retrieve tags from media metadata
Usage: serval observe [OPTIONS] <MEDIA_DIR>

Arguments:
  <MEDIA_DIR>

Options:
  -o, --output <OUTPUT_DIR>  Output directory [default: ./serval_output/serval_observe]
  -x, --xmp                  Read from XMP files
  -s, --subject              Include Subject metadata
  -m, --modified-time        Include file modified time
      --video                Video only
      --image                Image only
  -d, --debug                Debug mode
  -i, --independent          Temporal independence analysis after retrieving
  -h, --help                 Print help"#;

const CAPTURE_HELP: &str = r#"Temporal independence analysis on a CSV file
Usage: serval capture [OPTIONS] <CSV_PATH>

Arguments:
  <CSV_PATH>  Path for tags.csv

Options:
      --event
      --no-exclude
      --camtrap-dp
  -o, --output <OUTPUT_DIR>  Output directory [default: ./serval_output/serval_capture]
  -h, --help                 Print help"#;

const XMP_HELP: &str = r#"XMP file operations
Usage: serval xmp <COMMAND>

Commands:
  copy    Copy XMP files to output directory
  init    Initialize XMP files for media files
  update  Update XMP files from CSV
  remove  Remove all XMP files recursively from a directory
  sync    Sync XMP metadata to corresponding media files"#;

const XMP_COPY_HELP: &str = r#"Copy XMP files to output directory
Usage: serval xmp copy <SOURCE_DIR> <OUTPUT_DIR>"#;

const XMP_INIT_HELP: &str = r#"Initialize XMP files for media files
Usage: serval xmp init <SOURCE_DIR>"#;

const XMP_UPDATE_HELP: &str = r#"Update XMP files from CSV
Usage: serval xmp update [OPTIONS] <CSV_PATH>

Options:
  -t, --tag-type <TYPE>  [possible values: species, individual, count, sex, bodypart]
      --datetime         Update datetime instead of tags"#;

const XMP_REMOVE_HELP: &str = r#"Remove all XMP files recursively from a directory
Usage: serval xmp remove <SOURCE_DIR>"#;

const XMP_SYNC_HELP: &str = r#"Sync XMP metadata to corresponding media files
Usage: serval xmp sync [OPTIONS] [DIR]

Options:
      --csv <CSV_PATH>  CSV file with paths to XMP files to sync"#;

const EXTRACT_HELP: &str = r#"Extract and copy resources by filtering target values (based on tags.csv)
Usage: serval extract [OPTIONS] --filter-type <FILTER> --value <VALUE> <CSV_PATH>

Options:
  -f, --filter-type <FILTER> [species, path, individual, rating, event, custom, advanced]
  -v, --value <VALUE>
      --rename
      --skip-existing
      --use-subdir
      --subdir-type <SUBDIR_TYPE> [default: species] [species, individual, rating, custom]
  -o, --output <OUTPUT_DIR> [default: ./serval_output/serval_extract]"#;

const TRANSLATE_HELP: &str = r#"Translate species column in csv according to taglist
Usage: serval translate [OPTIONS] --taglist-path <TAGLIST> --from <FROM> --to <TO> <CSV_PATH>

Options:
  -t, --taglist-path <TAGLIST>
      --from <FROM>
      --to <TO>
  -o, --output <OUTPUT_DIR> [default: ./serval_output/serval_translate]"#;
