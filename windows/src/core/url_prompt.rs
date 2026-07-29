//! Asking for a URL until it is one.
//!
//! macOS shows the error and gives up, losing whatever was typed. Re-asking
//! with the text still in the box is kinder and costs nothing, and keeping the
//! loop here — as control flow over two closures — means it can be tested
//! without a dialog ever appearing.

use super::activity_builder::is_valid_button_url;

/// Prompts, validates, and re-prompts until the answer is usable or the user
/// gives up. `None` means they cancelled.
///
/// `allow_empty` distinguishes the two macOS behaviours: a blank custom URL
/// clears the setting, whereas the repository URL has no sensible blank state.
pub fn prompt_valid_url(
    mut show_prompt: impl FnMut(&str) -> Option<String>,
    mut show_error: impl FnMut(),
    initial: &str,
    allow_empty: bool,
) -> Option<String> {
    let mut current = initial.to_string();
    loop {
        let entered = show_prompt(&current)?;
        if (entered.is_empty() && allow_empty) || is_valid_button_url(&entered) {
            return Some(entered);
        }
        show_error();
        // Re-offered rather than discarded: a typo is easier to fix than to
        // retype.
        current = entered;
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    /// Feeds scripted answers and counts how many times the error was shown.
    fn run(answers: &[Option<&str>], initial: &str, allow_empty: bool) -> (Option<String>, u32) {
        let next = Cell::new(0usize);
        let errors = Cell::new(0u32);
        let result = prompt_valid_url(
            |_| {
                let index = next.get();
                next.set(index + 1);
                answers[index].map(str::to_string)
            },
            || errors.set(errors.get() + 1),
            initial,
            allow_empty,
        );
        (result, errors.get())
    }

    #[test]
    fn a_valid_url_is_accepted_first_time() {
        let (result, errors) = run(&[Some("https://example.com")], "", false);
        assert_eq!(result.as_deref(), Some("https://example.com"));
        assert_eq!(errors, 0);
    }

    #[test]
    fn an_invalid_url_is_reported_and_asked_again() {
        let (result, errors) = run(&[Some("nonsense"), Some("https://example.com")], "", false);
        assert_eq!(result.as_deref(), Some("https://example.com"));
        assert_eq!(errors, 1, "the user should have been told once");
    }

    #[test]
    fn the_rejected_text_is_offered_back_for_editing() {
        let seen: Cell<Option<String>> = Cell::new(None);
        let mut answers = [Some("htp://typo.example"), Some("http://typo.example")].into_iter();
        prompt_valid_url(
            |current| {
                seen.set(Some(current.to_string()));
                answers.next().flatten().map(str::to_string)
            },
            || {},
            "",
            false,
        );
        assert_eq!(seen.into_inner().as_deref(), Some("htp://typo.example"));
    }

    #[test]
    fn cancelling_changes_nothing() {
        let (result, errors) = run(&[None], "https://example.com", false);
        assert_eq!(result, None);
        assert_eq!(errors, 0);
    }

    #[test]
    fn cancelling_after_an_error_still_changes_nothing() {
        let (result, errors) = run(&[Some("nope"), None], "", false);
        assert_eq!(result, None);
        assert_eq!(errors, 1);
    }

    #[test]
    fn blank_clears_the_setting_only_where_that_makes_sense() {
        // The custom URL: blank means "no custom URL".
        let (result, errors) = run(&[Some("")], "https://old.example", true);
        assert_eq!(result.as_deref(), Some(""));
        assert_eq!(errors, 0);

        // The repository URL: blank is just an invalid URL.
        let (result, errors) = run(&[Some(""), Some("https://example.com")], "", false);
        assert_eq!(result.as_deref(), Some("https://example.com"));
        assert_eq!(errors, 1);
    }

    #[test]
    fn a_non_http_scheme_is_refused() {
        // Discord only renders http(s) buttons, and `file:` under someone's
        // presence would be a broken link at best.
        let (result, errors) = run(&[Some("file:///c:/secret"), None], "", false);
        assert_eq!(result, None);
        assert_eq!(errors, 1);
    }
}
