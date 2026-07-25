use serde::{Deserialize, Serialize};

use super::i18n::{t, AppLanguage};
use super::models::MusicSourceId;

pub const DEFAULT_REPOSITORY_URL: &str = "https://github.com/Narcissus-tazetta/Ceyrad";

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

    /// Label a button shows until the user edits it. Only the song link varies
    /// by source; `None` means "for menu display", which reads as Apple Music.
    pub fn default_label(self, source: Option<MusicSourceId>) -> &'static str {
        match self {
            LinkType::Song => match source {
                Some(MusicSourceId::Spotify) => "Play on Spotify",
                _ => "Play on Apple Music",
            },
            LinkType::Artist => "View Artist",
            LinkType::Album => "View Album",
            LinkType::Custom => "Open Link",
            LinkType::Repository => "About This App",
            LinkType::Disabled => "",
        }
    }

    /// Every string treated as a default for this link type, used to decide
    /// whether the user has customized the label.
    pub fn default_labels(self) -> &'static [&'static str] {
        match self {
            LinkType::Song => &["Play on Apple Music", "Play on Spotify"],
            LinkType::Artist => &["View Artist"],
            LinkType::Album => &["View Album"],
            LinkType::Custom => &["Open Link"],
            LinkType::Repository => &["About This App"],
            LinkType::Disabled => &[""],
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

fn default_true() -> bool {
    true
}

fn default_repository_url() -> String {
    DEFAULT_REPOSITORY_URL.to_string()
}

fn default_button2_type() -> LinkType {
    LinkType::Repository
}

fn default_pause_hide_minutes() -> i32 {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub button1_type: LinkType,
    /// `None` means the label follows `button1_type`'s default.
    button1_label: Option<String>,
    #[serde(default = "default_button2_type")]
    pub button2_type: LinkType,
    button2_label: Option<String>,
    pub custom_url: String,
    #[serde(default = "default_repository_url")]
    pub repository_url: String,
    pub badge_label: BadgeLabelType,
    #[serde(default = "default_pause_hide_minutes")]
    pub pause_hide_minutes: i32,
    pub language: AppLanguage,
    #[serde(default = "default_true")]
    pub apple_music_enabled: bool,
    /// Off by default: Discord ships its own Spotify integration, so Ceyrad
    /// only drives Spotify when the user explicitly asks for it.
    pub spotify_enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            button1_type: LinkType::Song,
            button1_label: None,
            button2_type: LinkType::Repository,
            button2_label: None,
            custom_url: String::new(),
            repository_url: default_repository_url(),
            badge_label: BadgeLabelType::Artist,
            pause_hide_minutes: default_pause_hide_minutes(),
            language: AppLanguage::En,
            apple_music_enabled: true,
            spotify_enabled: false,
        }
    }
}

impl Settings {
    pub fn button1_label(&self, source: Option<MusicSourceId>) -> String {
        label_for(&self.button1_label, self.button1_type, source)
    }

    pub fn button2_label(&self, source: Option<MusicSourceId>) -> String {
        label_for(&self.button2_label, self.button2_type, source)
    }

    pub fn set_button1_type(&mut self, new_value: LinkType) {
        set_type(&mut self.button1_label, self.button1_type);
        self.button1_type = new_value;
    }

    pub fn set_button2_type(&mut self, new_value: LinkType) {
        set_type(&mut self.button2_label, self.button2_type);
        self.button2_type = new_value;
    }

    pub fn set_button1_label(&mut self, new_value: &str) {
        set_label(&mut self.button1_label, self.button1_type, new_value);
    }

    pub fn set_button2_label(&mut self, new_value: &str) {
        set_label(&mut self.button2_label, self.button2_type, new_value);
    }

    pub fn is_source_enabled(&self, source: MusicSourceId) -> bool {
        match source {
            MusicSourceId::AppleMusic => self.apple_music_enabled,
            MusicSourceId::Spotify => self.spotify_enabled,
        }
    }

    pub fn set_source_enabled(&mut self, source: MusicSourceId, enabled: bool) {
        match source {
            MusicSourceId::AppleMusic => self.apple_music_enabled = enabled,
            MusicSourceId::Spotify => self.spotify_enabled = enabled,
        }
    }
}

fn label_for(
    stored: &Option<String>,
    link_type: LinkType,
    source: Option<MusicSourceId>,
) -> String {
    match stored {
        Some(label) => label.clone(),
        None => link_type.default_label(source).to_string(),
    }
}

/// Drops the stored label when it is still one of the old type's defaults, so
/// an uncustomized label keeps following the link type. A label the user
/// actually chose survives the change.
fn set_type(stored: &mut Option<String>, old: LinkType) {
    if let Some(label) = stored.as_deref() {
        if old.default_labels().contains(&label) {
            *stored = None;
        }
    }
}

/// An empty label, or one equal to the current type's default, is not a
/// customization — clearing it keeps the label following the type.
fn set_label(stored: &mut Option<String>, link_type: LinkType, new_value: &str) {
    let value: String = new_value.chars().take(MAX_LABEL_CHARS).collect();
    if value.is_empty() || link_type.default_labels().contains(&value.as_str()) {
        *stored = None;
    } else {
        *stored = Some(value);
    }
}
