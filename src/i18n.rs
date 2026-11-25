//! Interface language: follow the macOS preferred language, allow a manual override.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};

/// Interface languages the app ships.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Zh,
    En,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::Zh => "zh",
            Lang::En => "en",
        }
    }

    /// Value for the `lang` attribute of the report document.
    pub fn html(self) -> &'static str {
        match self {
            Lang::Zh => "zh-CN",
            Lang::En => "en",
        }
    }

    /// Locale used by `Intl.NumberFormat` inside the report document.
    pub fn locale(self) -> &'static str {
        match self {
            Lang::Zh => "zh-CN",
            Lang::En => "en-US",
        }
    }

    /// Reads a stored value such as `zh` or `en-US`.
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim().to_ascii_lowercase();
        if value.starts_with("zh") {
            Some(Lang::Zh)
        } else if value.starts_with("en") {
            Some(Lang::En)
        } else {
            None
        }
    }

    fn from_byte(value: u8) -> Self {
        if value == Lang::En as u8 {
            Lang::En
        } else {
            Lang::Zh
        }
    }
}

static LANG: AtomicU8 = AtomicU8::new(Lang::Zh as u8);

pub fn lang() -> Lang {
    Lang::from_byte(LANG.load(Ordering::Relaxed))
}

pub fn set_lang(lang: Lang) {
    LANG.store(lang as u8, Ordering::Relaxed);
}

/// Returns the string that belongs to the active interface language.
pub fn pick<'a>(zh: &'a str, en: &'a str) -> &'a str {
    match lang() {
        Lang::Zh => zh,
        Lang::En => en,
    }
}

fn path(home: &Path) -> PathBuf {
    home.join("Library/Application Support/Malema/language")
}

/// Stored preference, falling back to the system language on first launch.
pub fn load(home: &Path) -> Lang {
    fs::read_to_string(path(home))
        .ok()
        .and_then(|value| Lang::parse(&value))
        .unwrap_or_else(system)
}

pub fn save(home: &Path, lang: Lang) -> std::io::Result<()> {
    let file = path(home);
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&file, lang.code())
}

pub fn system() -> Lang {
    system_preference().unwrap_or(Lang::En)
}

#[cfg(target_os = "macos")]
fn system_preference() -> Option<Lang> {
    use objc2_foundation::NSLocale;

    let languages = NSLocale::preferredLanguages();
    Lang::parse(&languages.firstObject()?.to_string())
}

#[cfg(not(target_os = "macos"))]
fn system_preference() -> Option<Lang> {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .filter_map(|key| std::env::var(key).ok())
        .find_map(|value| Lang::parse(&value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_language_tags_we_store() {
        assert_eq!(Lang::parse("zh"), Some(Lang::Zh));
        assert_eq!(Lang::parse(" zh-Hans-CN "), Some(Lang::Zh));
        assert_eq!(Lang::parse("en"), Some(Lang::En));
        assert_eq!(Lang::parse("en-GB"), Some(Lang::En));
        assert_eq!(Lang::parse("de"), None);
        assert_eq!(Lang::parse(""), None);
    }

    #[test]
    fn picks_the_string_for_the_active_language() {
        set_lang(Lang::Zh);
        assert_eq!(pick("总览", "Overview"), "总览");
        set_lang(Lang::En);
        assert_eq!(pick("总览", "Overview"), "Overview");
        set_lang(Lang::Zh);
    }
}
