use crate::i18n::Language;
use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;

const CONFIG_DIR_NAME: &str = "io.github.wsyxbcl.waxbill";
const CONFIG_FILE_NAME: &str = "settings.conf";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SetupConfig {
    pub language: Language,
    pub serval_binary_path: Option<String>,
}

impl SetupConfig {
    pub fn load() -> io::Result<Self> {
        let path = config_file_path()?;
        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(err) => return Err(err),
        };

        Ok(Self::parse(&contents))
    }

    pub fn save(&self) -> io::Result<()> {
        let path = config_file_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, self.serialize())
    }

    fn parse(contents: &str) -> Self {
        let mut config = Self::default();

        for line in contents.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };

            match key.trim() {
                "language" => {
                    if let Some(language) = parse_language(value) {
                        config.language = language;
                    }
                }
                "serval_binary_path" => {
                    let trimmed = value.trim();
                    config.serval_binary_path = if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed.to_string())
                    };
                }
                _ => {}
            }
        }

        config
    }

    fn serialize(&self) -> String {
        let binary_path = self.serval_binary_path.as_deref().unwrap_or("");
        format!(
            "version=1\nlanguage={}\nserval_binary_path={binary_path}\n",
            language_config_value(self.language)
        )
    }
}

fn config_file_path() -> io::Result<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let base = env::var_os("APPDATA").ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "APPDATA is not set for Waxbill config",
            )
        })?;
        return Ok(PathBuf::from(base)
            .join(CONFIG_DIR_NAME)
            .join(CONFIG_FILE_NAME));
    }

    #[cfg(target_os = "macos")]
    {
        let home = env::var_os("HOME").ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "HOME is not set for Waxbill config",
            )
        })?;
        return Ok(PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join(CONFIG_DIR_NAME)
            .join(CONFIG_FILE_NAME));
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        if let Some(base) = env::var_os("XDG_CONFIG_HOME") {
            return Ok(PathBuf::from(base)
                .join(CONFIG_DIR_NAME)
                .join(CONFIG_FILE_NAME));
        }

        let home = env::var_os("HOME").ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "HOME is not set for Waxbill config",
            )
        })?;
        Ok(PathBuf::from(home)
            .join(".config")
            .join(CONFIG_DIR_NAME)
            .join(CONFIG_FILE_NAME))
    }
}

fn parse_language(value: &str) -> Option<Language> {
    match value.trim() {
        "en" => Some(Language::En),
        "zh-cn" | "zh_cn" | "zh-CN" => Some(Language::ZhCn),
        _ => None,
    }
}

fn language_config_value(language: Language) -> &'static str {
    match language {
        Language::En => "en",
        Language::ZhCn => "zh-CN",
    }
}

#[cfg(test)]
mod tests {
    use super::{Language, SetupConfig};

    #[test]
    fn parse_recovers_supported_setup_fields() {
        let config = SetupConfig::parse(
            "version=1\nlanguage=zh-CN\nserval_binary_path=/tmp/serval\nunknown=value\n",
        );

        assert_eq!(
            config,
            SetupConfig {
                language: Language::ZhCn,
                serval_binary_path: Some("/tmp/serval".to_string()),
            }
        );
    }

    #[test]
    fn serialize_writes_stable_key_value_format() {
        let config = SetupConfig {
            language: Language::En,
            serval_binary_path: Some("/opt/serval".to_string()),
        };

        assert_eq!(
            config.serialize(),
            "version=1\nlanguage=en\nserval_binary_path=/opt/serval\n"
        );
    }
}
