//! Repairing the metadata shape SMTC hands over.
//!
//! Apple Music for Windows leaves `AlbumTitle` empty and packs the album into
//! the artist field instead, as `<artist> — <album>` (U+2014, spaced). Left
//! alone that string becomes the whole artist row on the Discord card, and any
//! catalog search built from it matches nothing — the term would carry the
//! album twice.

/// The separator Apple Music for Windows uses, verified against a live SMTC
/// session: space, EM DASH, space. Deliberately not a plain hyphen — album
/// names routinely contain " - " (`Brand New - Single`), so matching one would
/// split in the wrong place.
const COMBINED_SEPARATOR: &str = " \u{2014} ";

/// Splits `<artist> — <album>` back apart, but only when there is nothing to
/// lose: a player that filled in the album is left untouched, and so is an
/// artist string that would leave either half empty.
pub fn split_combined_artist(artist: &str, album: &str) -> (String, String) {
    if !album.trim().is_empty() {
        return (artist.to_string(), album.to_string());
    }
    // First occurrence, not last: an album containing an em dash is more likely
    // than an artist name containing one, and the first split keeps the whole
    // remainder as the album.
    let Some(index) = artist.find(COMBINED_SEPARATOR) else {
        return (artist.to_string(), album.to_string());
    };
    let name = artist[..index].trim();
    let title = artist[index + COMBINED_SEPARATOR.len()..].trim();
    if name.is_empty() || title.is_empty() {
        return (artist.to_string(), album.to_string());
    }
    (name.to_string(), title.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_the_shape_apple_music_for_windows_reports() {
        let (artist, album) = split_combined_artist("Mrs. GREEN APPLE — Brand New - Single", "");
        assert_eq!(artist, "Mrs. GREEN APPLE");
        assert_eq!(album, "Brand New - Single");
    }

    #[test]
    fn leaves_a_filled_in_album_alone() {
        let (artist, album) = split_combined_artist("Some — Artist", "Real Album");
        assert_eq!(artist, "Some — Artist");
        assert_eq!(album, "Real Album");
    }

    #[test]
    fn keeps_the_remainder_whole_when_the_album_has_its_own_dash() {
        let (artist, album) = split_combined_artist("Artist — Album — Deluxe", "");
        assert_eq!(artist, "Artist");
        assert_eq!(album, "Album — Deluxe");
    }

    #[test]
    fn does_not_split_on_a_hyphen() {
        let (artist, album) = split_combined_artist("Brand New - Single", "");
        assert_eq!(artist, "Brand New - Single");
        assert_eq!(album, "");
    }

    #[test]
    fn refuses_a_split_that_would_empty_a_half() {
        let (artist, album) = split_combined_artist("— Album", "");
        assert_eq!(artist, "— Album");
        assert_eq!(album, "");
    }

    #[test]
    fn passes_through_an_ordinary_artist() {
        let (artist, album) = split_combined_artist("Mrs. GREEN APPLE", "");
        assert_eq!(artist, "Mrs. GREEN APPLE");
        assert_eq!(album, "");
    }
}
