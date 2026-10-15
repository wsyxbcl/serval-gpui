//! The Waxbill theme, built from `Design/boards/tokens.css`, the single source for colours and sizes. The file is
//! embedded at compile time: gpui-component's theme keys are filled from it, and the tokens gpui-component has no
//! slot for (soft status fills, the accent marker, layout sizes) are read with [`Tokens`].

use gpui_kit::component::theme::{Theme, ThemeMode, ThemeRegistry};
use gpui_kit::*;
use std::collections::HashMap;

const TOKENS_CSS: &str = include_str!("../Design/boards/tokens.css");

/// gpui-component theme key → token name. Keys without an entry keep gpui-component's defaults.
const THEME_KEYS: &[(&str, &str)] = &[
    ("background", "background"),
    ("foreground", "foreground"),
    ("border", "border"),
    ("input.border", "input-border"),
    ("ring", "ring"),
    ("caret", "foreground"),
    ("muted.background", "muted"),
    ("muted.foreground", "muted-foreground"),
    ("primary.background", "primary"),
    ("primary.hover.background", "primary-hover"),
    ("primary.active.background", "primary-hover"),
    ("primary.foreground", "primary-foreground"),
    ("secondary.background", "surface"),
    ("secondary.hover.background", "muted"),
    ("secondary.active.background", "muted"),
    ("secondary.foreground", "foreground"),
    ("accent.background", "muted"),
    ("accent.foreground", "foreground"),
    ("link", "accent"),
    ("scrollbar.thumb.background", "scrollbar-thumb"),
    ("scrollbar.thumb.hover.background", "scrollbar-thumb-hover"),
    ("link.hover", "accent"),
    ("link.active", "accent"),
    ("popover.background", "surface"),
    ("popover.foreground", "foreground"),
    ("list.background", "surface"),
    ("sidebar.background", "sidebar"),
    ("sidebar.foreground", "sidebar-foreground"),
    ("sidebar.accent.background", "sidebar-active"),
    ("sidebar.accent.foreground", "foreground"),
    ("sidebar.border", "border"),
    ("title_bar.background", "background"),
    ("title_bar.border", "border"),
    ("success.background", "success"),
    ("warning.background", "warning"),
    ("danger.background", "danger"),
    ("progress.bar.background", "progress-bar"),
    ("overlay", "overlay"),
    ("window.border", "border"),
];

/// Token values for one appearance. Colours are `#rrggbb`, `#rrggbbaa` (from `rgba()`), sizes `px` numbers.
#[derive(Clone)]
pub struct Tokens {
    values: HashMap<String, String>,
}

impl Tokens {
    /// The tokens of the light (`:root`) or dark (`[data-appearance="dark"]`, over `:root`) appearance.
    pub fn parse(dark: bool) -> Tokens {
        let mut values = HashMap::new();
        let mut block = "";
        for line in TOKENS_CSS.lines() {
            let line = line.trim();
            if line.starts_with(":root") {
                block = "light";
            } else if line.starts_with("[data-appearance=\"dark\"]") {
                block = "dark";
            } else if let Some(rest) = line.strip_prefix("--")
                && (block == "light" || (dark && block == "dark"))
                && let Some((name, value)) = rest.split_once(':')
            {
                let value = value
                    .split("/*")
                    .next()
                    .unwrap()
                    .trim()
                    .trim_end_matches(';');
                values.insert(name.trim().to_string(), value.trim().to_string());
            }
        }
        let mut tokens = Tokens { values };
        // Resolve `var(--name)` references and normalise colours to hex.
        let names: Vec<String> = tokens.values.keys().cloned().collect();
        for name in names {
            let value = tokens.resolve(&name);
            tokens.values.insert(name, value);
        }
        tokens
    }

    fn resolve(&self, name: &str) -> String {
        let value = self
            .values
            .get(name)
            .unwrap_or_else(|| panic!("token --{name} is not in tokens.css"));
        if let Some(reference) = value.strip_prefix("var(--") {
            return self.resolve(reference.trim_end_matches(')'));
        }
        if let Some(args) = value.strip_prefix("rgba(") {
            let parts: Vec<f32> = args
                .trim_end_matches(')')
                .split(',')
                .map(|p| p.trim().parse().expect("rgba() component"))
                .collect();
            return format!(
                "#{:02x}{:02x}{:02x}{:02x}",
                parts[0] as u8,
                parts[1] as u8,
                parts[2] as u8,
                (parts[3] * 255.0).round() as u8
            );
        }
        value.clone()
    }

    pub fn color(&self, name: &str) -> Hsla {
        let hex = self.resolve(name);
        let digits = hex.trim_start_matches('#');
        let value = u32::from_str_radix(digits, 16)
            .unwrap_or_else(|_| panic!("token --{name} is not a colour: {hex}"));
        match digits.len() {
            6 => rgb(value).into(),
            8 => rgba(value).into(),
            _ => panic!("token --{name} is not a colour: {hex}"),
        }
    }

    pub fn px(&self, name: &str) -> Pixels {
        let value = self.resolve(name);
        px(value
            .trim_end_matches("px")
            .parse()
            .unwrap_or_else(|_| panic!("token --{name} is not a size: {value}")))
    }

    /// The gpui-component theme for this appearance, as gpui-component's theme JSON.
    fn theme_json(&self, name: &str, dark: bool, font: Option<&str>) -> serde_json::Value {
        let colors: serde_json::Map<String, serde_json::Value> = THEME_KEYS
            .iter()
            .map(|(key, token)| (key.to_string(), self.resolve(token).into()))
            .collect();
        let mut theme = serde_json::json!({
            "name": name,
            "mode": if dark { "dark" } else { "light" },
            "font.size": self.px("text-md").as_f32(),
            "radius": self.px("radius-sm").as_f32() as usize,
            "radius.lg": self.px("radius-lg").as_f32() as usize,
            "shadow": false,
            "colors": colors,
        });
        if let Some(font) = font {
            theme["font.family"] = font.into();
        }
        theme
    }
}

/// The tokens of the current appearance, for drawing what gpui-component does not cover.
pub struct ActiveTokens(pub Tokens);

impl Global for ActiveTokens {}

pub trait TokensExt {
    fn tokens(&self) -> &Tokens;
}

impl TokensExt for App {
    fn tokens(&self) -> &Tokens {
        &self.global::<ActiveTokens>().0
    }
}

/// Register the Waxbill light and dark themes and switch to `mode`.
/// Simplified-Chinese UI families as Linux distributions name them, in order of preference. Each also covers Latin.
const LINUX_UI_FONTS: &[&str] = &[
    "Noto Sans CJK SC",
    "Source Han Sans SC",
    "Source Han Sans CN",
    "WenQuanYi Micro Hei",
    "WenQuanYi Zen Hei",
];

/// The UI font to name on the theme. On Linux the system UI font (Noto Sans, Cantarell…) has no Chinese, and
/// every Chinese character then falls back on its own, sometimes to a Japanese or Korean face, so one word can mix
/// fonts; naming an installed Simplified-Chinese family keeps a line in one font. Windows and macOS fall back to
/// Microsoft YaHei and PingFang, which is right.
fn ui_font(cx: &App) -> Option<String> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let installed = cx.text_system().all_font_names();
    LINUX_UI_FONTS
        .iter()
        .find(|family| installed.iter().any(|name| name == *family))
        .map(|family| family.to_string())
}

pub fn init(mode: ThemeMode, cx: &mut App) {
    let light = Tokens::parse(false);
    let dark = Tokens::parse(true);
    let font = ui_font(cx);
    let set = serde_json::json!({
        "name": "Waxbill",
        "themes": [
            light.theme_json("Waxbill Light", false, font.as_deref()),
            dark.theme_json("Waxbill Dark", true, font.as_deref()),
        ],
    });
    ThemeRegistry::global_mut(cx)
        .load_themes_from_str(&set.to_string())
        .expect("the Waxbill theme loads");
    let themes = ThemeRegistry::global(cx).themes().clone();
    let theme = Theme::global_mut(cx);
    theme.light_theme = themes["Waxbill Light"].clone();
    theme.dark_theme = themes["Waxbill Dark"].clone();
    // A scrollbar wherever something does not fit (page, sidebar, log), not only while scrolling: a long screen
    // shows that there is more below. Areas that fit show none.
    theme.scrollbar_mode = gpui_kit::component::scroll::ScrollbarMode::Always;
    set_mode(mode, cx);
}

pub fn set_mode(mode: ThemeMode, cx: &mut App) {
    cx.set_global(ActiveTokens(Tokens::parse(mode.is_dark())));
    Theme::change(mode, None, cx);
}

#[cfg(test)]
mod tests {
    // Not `super::*`: GPUI's glob export includes its own `test` macro.
    use super::{THEME_KEYS, Tokens};

    #[test]
    fn every_theme_key_has_a_colour_in_both_appearances() {
        for dark in [false, true] {
            let tokens = Tokens::parse(dark);
            for (_, token) in THEME_KEYS {
                tokens.color(token);
            }
        }
        assert_ne!(
            Tokens::parse(false).resolve("primary"),
            Tokens::parse(true).resolve("primary")
        );
    }
}
