//! Cutting a string to a length without cutting a character in half.
//!
//! Swift's `prefix` counts grapheme clusters and never splits one, so the macOS
//! build gets this for free wherever it truncates. Rust counts scalars, and a
//! cut at a fixed scalar count can land inside a character — a family emoji is
//! seven scalars joined by ZWJ — leaving a dangling joiner or a bare combining
//! mark for Discord to render. This reaches the same guarantee from the other
//! side, by dropping whatever the cut left incomplete.

/// Whether `c` only exists as part of the character before it.
///
/// Deliberately a hand-written table rather than the real `Grapheme_Extend`
/// property, which is thousands of ranges and a Unicode crate to keep them up
/// to date. It covers what turns up in track titles and button labels; a mark it
/// misses costs one wrong-looking character at the end of a string that was
/// already being cut short.
///
/// Regional indicators are deliberately absent. A flag is a *pair* of them and
/// no per-character test can tell the second half of a complete flag from the
/// first half of a broken one, so treating them as continuations would break
/// the flag that happens to end exactly on the limit.
fn is_continuation(c: char) -> bool {
    matches!(c,
        '\u{200D}'                  // zero-width joiner
        | '\u{FE00}'..='\u{FE0F}'   // variation selectors
        | '\u{1F3FB}'..='\u{1F3FF}' // skin-tone modifiers
        | '\u{0300}'..='\u{036F}'   // combining diacritical marks
        | '\u{1AB0}'..='\u{1AFF}'   // …extended
        | '\u{1DC0}'..='\u{1DFF}'   // …supplement
        | '\u{20D0}'..='\u{20F0}'   // combining marks for symbols
        | '\u{FE20}'..='\u{FE2F}'   // combining half marks
        | '\u{E0020}'..='\u{E007F}' // tag characters (flag sequences)
        | '\u{E0100}'..='\u{E01EF}' // variation selectors supplement
    )
}

const ZWJ: char = '\u{200D}';

/// `s` cut to at most `max` characters, ending on a character boundary in the
/// sense a reader would recognise.
///
/// Borrowed, not owned, and measured in one walk that stops at the limit: `nth`
/// yields the byte offset the cut falls on and, by yielding nothing, says the
/// string was short enough to hand back untouched. A title well under the limit
/// is never walked to its end and never copied.
pub fn truncate(s: &str, max: usize) -> &str {
    let Some((end, first_dropped)) = s.char_indices().nth(max) else {
        return s;
    };
    let mut head = &s[..end];

    // Whether the cut split a character is decided by what it dropped, not by
    // what it kept: a mark that still has everything it belongs to is a whole
    // character sitting on the limit and stays. Only when the *next* thing was
    // part of the same character has the tail lost its meaning, and then it
    // goes — leaving whole characters, which is what Swift's cluster-counting
    // `prefix` arrives at from the other side.
    if is_continuation(first_dropped) {
        while let Some(last) = head.chars().next_back() {
            if !is_continuation(last) {
                break;
            }
            head = &head[..head.len() - last.len_utf8()];
        }
    }

    // A joiner is never the end of anything. What it joined to is on the far
    // side of the cut whatever that was, so it goes either way.
    while head.ends_with(ZWJ) {
        head = &head[..head.len() - ZWJ.len_utf8()];
    }

    head
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_string_within_the_limit_is_handed_back_as_it_is() {
        assert_eq!(truncate("Brand New", 32), "Brand New");
        // Exactly on the limit is not over it — the off-by-one that would show
        // up as a title losing its last character for no reason.
        assert_eq!(truncate("abcd", 4), "abcd");
    }

    #[test]
    fn the_limit_counts_characters_not_bytes() {
        assert_eq!(truncate("あいうえお", 3), "あいう");
    }

    #[test]
    fn a_whole_character_sitting_on_the_limit_keeps_its_marks() {
        // "e" + combining acute, complete, with the cut falling after it.
        assert_eq!(truncate("ae\u{0301}i", 3), "ae\u{0301}");
    }

    #[test]
    fn the_leftovers_of_a_split_character_go_with_it() {
        // "e" + acute + grave, cut between the two marks: what is left is not
        // the character that was there, so it goes back to the last whole one.
        assert_eq!(truncate("ae\u{0301}\u{0300}i", 3), "ae");
    }

    #[test]
    fn a_dangling_joiner_is_not_left_at_the_end() {
        // A family emoji cut in the middle.
        let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
        assert_eq!(truncate(family, 2), "\u{1F468}");
    }

    #[test]
    fn a_string_of_nothing_but_marks_truncates_to_empty() {
        // Degenerate, but it must not panic or slice mid-character.
        assert_eq!(truncate(&"\u{0301}".repeat(10), 4), "");
    }

    #[test]
    fn a_complete_flag_on_the_limit_survives() {
        // Two regional indicators, and the pair is the character. Testing a
        // per-character continuation rule would eat both.
        let flag = "\u{1F1EF}\u{1F1F5}";
        assert_eq!(truncate(&format!("{flag}x"), 2), flag);
    }
}
