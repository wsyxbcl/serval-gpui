//! Interface language. Chinese is the default and English is complete: every string is written with both at the
//! place it is used, so a missing translation cannot exist.

use std::sync::atomic::{AtomicBool, Ordering};

static ENGLISH: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    Chinese,
    English,
}

pub fn set_language(language: Language) {
    ENGLISH.store(language == Language::English, Ordering::Relaxed);
}

pub fn language() -> Language {
    if ENGLISH.load(Ordering::Relaxed) {
        Language::English
    } else {
        Language::Chinese
    }
}

/// The string for the current language.
pub fn t(zh: &'static str, en: &'static str) -> &'static str {
    match language() {
        Language::Chinese => zh,
        Language::English => en,
    }
}

/// A number with thousands separators ("13,782").
pub fn count(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}
