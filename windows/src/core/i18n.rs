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
