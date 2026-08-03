use serde::{Deserialize, Serialize};

/// Display language of the menu. Discord-facing text is always English.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppLanguage {
    #[default]
    En,
    Ja,
}

impl AppLanguage {
    pub const ALL: [AppLanguage; 2] = [AppLanguage::En, AppLanguage::Ja];

    pub fn display_name(self) -> &'static str {
        match self {
            AppLanguage::En => "English",
            AppLanguage::Ja => "日本語",
        }
    }
}

/// Picks the menu string for the current language.
pub fn t(lang: AppLanguage, en: &str, ja: &str) -> String {
    match lang {
        AppLanguage::En => en.to_string(),
        AppLanguage::Ja => ja.to_string(),
    }
}

/// As [`t`], for strings that interpolate.
///
/// Named apart from `t` on purpose: the two used to be `t` and `t!`, imported
/// side by side from different paths, and telling which one a call site meant
/// took a second look every time.
///
/// `t(lang, &format!(…), &format!(…))` builds both sides, throws one away, then
/// copies the survivor — three allocations to return one, on a path that runs
/// several times a second while a player is active. This formats only the arm
/// that is actually used.
#[macro_export]
macro_rules! t_fmt {
    ($lang:expr, $en:literal, $ja:literal $(,)?) => {
        match $lang {
            $crate::core::i18n::AppLanguage::En => format!($en),
            $crate::core::i18n::AppLanguage::Ja => format!($ja),
        }
    };
}
