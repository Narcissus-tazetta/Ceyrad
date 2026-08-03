use serde::{Deserialize, Serialize};

use super::i18n::{t, AppLanguage};
use super::text;

pub const DEFAULT_REPOSITORY_URL: &str = "https://github.com/Narcissus-tazetta/Ceyrad";

/// Which of the two configurable Discord buttons something is about.
///
/// The two are entirely symmetrical, so carrying the slot as a value is what
/// keeps every rule about them from being written down twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonSlot {
    One,
    Two,
}

impl ButtonSlot {
    pub const ALL: [ButtonSlot; 2] = [ButtonSlot::One, ButtonSlot::Two];

    pub fn number(self) -> u8 {
        match self {
            ButtonSlot::One => 1,
            ButtonSlot::Two => 2,
        }
    }

    /// The link type a fresh install starts with: the song page and the repo.
    fn default_type(self) -> LinkType {
        match self {
            ButtonSlot::One => LinkType::Song,
            ButtonSlot::Two => LinkType::Repository,
        }
    }
}

/// Minutes of continuous pause before the status is cleared.
/// `-1` never clears, `0` clears immediately.
pub const PAUSE_HIDE_CHOICES: [i32; 6] = [-1, 0, 1, 3, 5, 10];

pub const MAX_LABEL_CHARS: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkType {
    #[default]
    Song,
    Artist,
    Album,
    Custom,
    Repository,
    Disabled,
}

impl LinkType {
    pub const SELECTABLE: [LinkType; 6] = [
        LinkType::Song,
        LinkType::Artist,
        LinkType::Album,
        LinkType::Custom,
        LinkType::Repository,
        LinkType::Disabled,
    ];

    pub fn display_name(self, lang: AppLanguage) -> String {
        match self {
            LinkType::Song => t(lang, "Song Page", "曲ページ"),
            LinkType::Artist => t(lang, "Artist Page", "アーティストページ"),
            LinkType::Album => t(lang, "Album Page", "アルバムページ"),
            LinkType::Custom => t(lang, "Custom URL", "カスタムURL"),
            LinkType::Repository => t(lang, "Repository", "リポジトリ"),
            LinkType::Disabled => t(lang, "Off", "オフ"),
        }
    }

    /// Label a button shows until the user edits it. Discord-facing, so never
    /// localized. Also the value that decides whether a stored label counts as
    /// a customization.
    pub fn default_label(self) -> &'static str {
        match self {
            LinkType::Song => "Play on Apple Music",
            LinkType::Artist => "View Artist",
            LinkType::Album => "View Album",
            LinkType::Custom => "Open Link",
            LinkType::Repository => "About This App",
            LinkType::Disabled => "",
        }
    }
}

/// What the compact "Listening to …" badge shows. Values match Discord's
/// `status_display_type` (0 = name, 1 = state, 2 = details).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BadgeLabelType {
    AppName,
    #[default]
    Artist,
    Track,
}

impl BadgeLabelType {
    pub const ALL: [BadgeLabelType; 3] = [
        BadgeLabelType::AppName,
        BadgeLabelType::Artist,
        BadgeLabelType::Track,
    ];

    pub fn raw_value(self) -> i64 {
        match self {
            BadgeLabelType::AppName => 0,
            BadgeLabelType::Artist => 1,
            BadgeLabelType::Track => 2,
        }
    }

    pub fn display_name(self, lang: AppLanguage) -> String {
        match self {
            // The Discord-side name depends on which client is connected.
            BadgeLabelType::AppName => t(lang, "App Name", "アプリ名"),
            BadgeLabelType::Artist => t(lang, "Artist Name", "アーティスト名"),
            BadgeLabelType::Track => t(lang, "Track Name", "曲名"),
        }
    }
}

/// Container-level `#[serde(default)]` fills every missing key from
/// `Settings::default()`, so a hand-edited or partial file still loads and
/// there is exactly one place a default is written down.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    // The four button fields are private on purpose: every rule about them —
    // which default applies, when a label stops following its type — lives in
    // the `ButtonSlot` accessors below, and a direct write would sidestep all
    // of it. The names stay `button1_*` because they are the keys in the
    // settings file on disk.
    button1_type: LinkType,
    /// `None` means the label follows `button1_type`'s default.
    button1_label: Option<String>,
    button2_type: LinkType,
    button2_label: Option<String>,
    pub custom_url: String,
    pub repository_url: String,
    pub badge_label: BadgeLabelType,
    pub pause_hide_minutes: i32,
    pub language: AppLanguage,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            button1_type: ButtonSlot::One.default_type(),
            button1_label: None,
            button2_type: ButtonSlot::Two.default_type(),
            button2_label: None,
            custom_url: String::new(),
            repository_url: DEFAULT_REPOSITORY_URL.to_string(),
            badge_label: BadgeLabelType::Artist,
            pause_hide_minutes: 5,
            language: AppLanguage::En,
        }
    }
}

impl Settings {
    pub fn button_type(&self, slot: ButtonSlot) -> LinkType {
        match slot {
            ButtonSlot::One => self.button1_type,
            ButtonSlot::Two => self.button2_type,
        }
    }

    pub fn button_label(&self, slot: ButtonSlot) -> String {
        match self.stored_label(slot) {
            Some(label) => label.clone(),
            None => self.button_type(slot).default_label().to_string(),
        }
    }

    /// Changes where the button points.
    ///
    /// A label that is still the old type's default is dropped, so an
    /// uncustomized label keeps following the destination. A label the user
    /// actually chose survives the change.
    pub fn set_button_type(&mut self, slot: ButtonSlot, new_value: LinkType) {
        let old_default = self.button_type(slot).default_label();
        let stored = self.stored_label_mut(slot);
        if stored.as_deref() == Some(old_default) {
            *stored = None;
        }
        match slot {
            ButtonSlot::One => self.button1_type = new_value,
            ButtonSlot::Two => self.button2_type = new_value,
        }
    }

    /// An empty label, or one equal to the current type's default, is not a
    /// customization — clearing it keeps the label following the type.
    pub fn set_button_label(&mut self, slot: ButtonSlot, new_value: &str) {
        // Cut the way `activity_builder` cuts the fields it sends, so a label
        // that ends in an emoji does not come back from the dialog as half of
        // one.
        let value = text::truncate(new_value, MAX_LABEL_CHARS);
        let is_default = value.is_empty() || value == self.button_type(slot).default_label();
        let value = value.to_string();
        let stored = self.stored_label_mut(slot);
        *stored = if is_default { None } else { Some(value) };
    }

    fn stored_label(&self, slot: ButtonSlot) -> Option<&String> {
        match slot {
            ButtonSlot::One => self.button1_label.as_ref(),
            ButtonSlot::Two => self.button2_label.as_ref(),
        }
    }

    fn stored_label_mut(&mut self, slot: ButtonSlot) -> &mut Option<String> {
        match slot {
            ButtonSlot::One => &mut self.button1_label,
            ButtonSlot::Two => &mut self.button2_label,
        }
    }
}
