//! How the `verone` command prints results and errors.

use std::io::Write;

use serde::Serialize;
use verone_core::{Error, Result};

/// The form results are printed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Text for a person to read.
    Human,
    /// One JSON document for another program to read.
    Json,
}

impl Mode {
    /// Chooses the mode from the `--json` option.
    pub fn from_json_flag(json: bool) -> Self {
        if json { Mode::Json } else { Mode::Human }
    }
}

/// Writes a result to `out`: as JSON in JSON mode, otherwise as the text `human`
/// produces. A final newline is added if the text lacks one.
pub fn emit<T, W, F>(mode: Mode, out: &mut W, value: &T, human: F) -> Result<()>
where
    T: Serialize,
    W: Write,
    F: FnOnce(&T) -> String,
{
    let mut text = match mode {
        Mode::Json => serde_json::to_string_pretty(value)
            .map_err(|error| Error::failed(format!("could not produce JSON output: {error}")))?,
        Mode::Human => human(value),
    };
    if !text.ends_with('\n') {
        text.push('\n');
    }
    out.write_all(text.as_bytes())
        .map_err(|error| Error::failed(format!("could not write output: {error}")))
}

/// The text shown for an error: what went wrong, then what to do about it.
pub fn format_error(error: &Error) -> String {
    let mut text = format!("error: {}\n", error.message());
    if let Some(hint) = error.hint() {
        text.push_str(&format!("hint: {hint}\n"));
    }
    text
}

/// Prints an error to standard error.
pub fn print_error(error: &Error) {
    eprint!("{}", format_error(error));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize)]
    struct Sample {
        name: &'static str,
        running: bool,
    }

    const SAMPLE: Sample = Sample {
        name: "work",
        running: true,
    };

    #[test]
    fn human_mode_prints_the_text_and_adds_a_newline() {
        let mut out = Vec::new();
        emit(Mode::Human, &mut out, &SAMPLE, |sample| {
            format!("{} is running", sample.name)
        })
        .unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "work is running\n");
    }

    #[test]
    fn human_mode_does_not_double_a_newline() {
        let mut out = Vec::new();
        emit(Mode::Human, &mut out, &SAMPLE, |_| "done\n".to_string()).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "done\n");
    }

    #[test]
    fn json_mode_prints_parseable_json_and_ignores_the_text() {
        let mut out = Vec::new();
        emit(Mode::Json, &mut out, &SAMPLE, |_| "ignored".to_string()).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.ends_with('\n'));
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["name"], "work");
        assert_eq!(parsed["running"], true);
    }

    #[test]
    fn mode_follows_the_json_flag() {
        assert_eq!(Mode::from_json_flag(true), Mode::Json);
        assert_eq!(Mode::from_json_flag(false), Mode::Human);
    }

    #[test]
    fn an_error_shows_its_message_and_hint() {
        let error = Error::not_found("no phone named 'work'").with_hint("Run verone list.");
        assert_eq!(
            format_error(&error),
            "error: no phone named 'work'\nhint: Run verone list.\n"
        );
    }

    #[test]
    fn an_error_without_a_hint_shows_only_its_message() {
        assert_eq!(
            format_error(&Error::failed("it broke")),
            "error: it broke\n"
        );
    }
}
