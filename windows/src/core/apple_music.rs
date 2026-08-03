//! The only supported player, and the constants that belong to it.
//!
//! The macOS build keeps the same two values in `AppleMusic.swift`; anything
//! added here should be added there.

/// Shown in the status rows and the log. Not the name Discord displays — that
/// one belongs to the Application behind `DISCORD_CLIENT_ID`.
pub const DISPLAY_NAME: &str = "Apple Music";

/// Discord shows "Listening to <Application name>", and the name is a property
/// of the Application the client id identifies. This app's own Application is
/// named "Apple Music", so the id is fixed and the user never sets it.
pub const DISCORD_CLIENT_ID: &str = "1525381518258606130";
