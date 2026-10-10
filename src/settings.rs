//! Stored settings (Design/spec/settings.md §3): `settings.conf` in the platform config directory, `key=value`
//! lines. Keys Waxbill does not know are kept as they are, so older and newer versions can share the file.

use crate::i18n::Language;
use gpui_kit::*;
use std::collections::BTreeMap;
use std::path::PathBuf;

const CONFIG_DIR_NAME: &str = "io.github.wsyxbcl.waxbill";
const CONFIG_FILE_NAME: &str = "settings.conf";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Appearance {
    System,
    Light,
    Dark,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub language: Language,
    pub serval: Option<PathBuf>,
    pub appearance: Appearance,
    /// Every other key, including each form's last values (`form.<command>.<field>`).
    other: BTreeMap<String, String>,
}

impl Global for Settings {}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: Language::Chinese,
            serval: None,
            appearance: Appearance::System,
            other: BTreeMap::new(),
        }
    }
}

impl Settings {
    pub fn load() -> Settings {
        config_file()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|text| Settings::parse(&text))
            .unwrap_or_default()
    }

    fn parse(text: &str) -> Settings {
        let mut settings = Settings::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "version" => {}
                "language" => {
                    settings.language = if value == "en" {
                        Language::English
                    } else {
                        Language::Chinese
                    }
                }
                "serval_binary_path" => {
                    settings.serval = (!value.is_empty()).then(|| PathBuf::from(value))
                }
                "appearance" => {
                    settings.appearance = match value {
                        "light" => Appearance::Light,
                        "dark" => Appearance::Dark,
                        _ => Appearance::System,
                    }
                }
                key => {
                    settings.other.insert(key.to_string(), value.to_string());
                }
            }
        }
        settings
    }

    fn to_text(&self) -> String {
        let mut text = format!(
            "version=1\nlanguage={}\nserval_binary_path={}\nappearance={}\n",
            match self.language {
                Language::Chinese => "zh-CN",
                Language::English => "en",
            },
            self.serval
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            match self.appearance {
                Appearance::System => "system",
                Appearance::Light => "light",
                Appearance::Dark => "dark",
            }
        );
        for (key, value) in &self.other {
            text.push_str(&format!("{key}={value}\n"));
        }
        text
    }

    pub fn value(&self, key: &str) -> Option<&str> {
        self.other.get(key).map(String::as_str)
    }

    pub fn set_value(&mut self, key: &str, value: impl Into<String>) {
        let value: String = value.into();
        self.other
            .insert(key.to_string(), value.replace(['\n', '\r'], " "));
    }

    pub fn form_value(&self, command: &str, field: &str) -> Option<&str> {
        self.other
            .get(&format!("form.{command}.{field}"))
            .map(String::as_str)
    }

    pub fn set_form_value(&mut self, command: &str, field: &str, value: impl Into<String>) {
        let value: String = value.into();
        // One line per value: a newline would start another key.
        self.other.insert(
            format!("form.{command}.{field}"),
            value.replace(['\n', '\r'], " "),
        );
    }

    /// Change the settings and write them in the background (writing on the UI thread stalled Windows).
    pub fn update(cx: &mut App, change: impl FnOnce(&mut Settings)) {
        let settings = cx.default_global::<Settings>();
        change(settings);
        let text = settings.to_text();
        cx.background_executor()
            .spawn(async move {
                if let Some(path) = config_file() {
                    if let Some(dir) = path.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    if let Err(err) = std::fs::write(&path, text) {
                        eprintln!("Could not save {}: {err}", path.display());
                    }
                }
            })
            .detach();
    }
}

fn config_file() -> Option<PathBuf> {
    let base = if cfg!(target_os = "windows") {
        PathBuf::from(std::env::var_os("APPDATA")?)
    } else if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support")
    } else if let Some(base) = std::env::var_os("XDG_CONFIG_HOME") {
        PathBuf::from(base)
    } else {
        PathBuf::from(std::env::var_os("HOME")?).join(".config")
    };
    Some(base.join(CONFIG_DIR_NAME).join(CONFIG_FILE_NAME))
}

#[cfg(test)]
mod tests {
    use super::{Appearance, Settings};
    use crate::i18n::Language;

    #[test]
    fn the_previous_file_still_loads_and_unknown_keys_survive_a_save() {
        let old = "version=1\nlanguage=en\nserval_binary_path=/opt/serval\nfuture.key=1\n";
        let mut settings = Settings::parse(old);
        assert_eq!(settings.language, Language::English);
        assert_eq!(settings.appearance, Appearance::System);
        settings.set_form_value("observe", "input", "/data/a\nb");
        let again = Settings::parse(&settings.to_text());
        assert_eq!(again, settings);
        assert!(settings.to_text().contains("future.key=1\n"));
    }
}
