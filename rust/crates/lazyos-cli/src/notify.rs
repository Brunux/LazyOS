//! `lazyos notify` - a card from the command line (spec 006).
//!
//! A direct command: it opens no window, because the card is the answer and a
//! `foot` started for a `notify-send` would take the focus. The shell's
//! `notify-send` call sites keep working; this is the same card with the house
//! rules built in (a picture from the theme, the group tag, the urgency).
//!
//! ```text
//! lazyos notify send TITLE [BODY] [--icon auto|none|NAME] [--urgency L] [--group TAG]
//! lazyos notify say  TITLE [BODY] [...]           the same card, for one result line
//! lazyos notify progress TAG PERCENT TITLE [BODY] [...]   one card, updated in place
//! ```
//!
//! `--icon auto` is the default and decides the picture from the body (see
//! `lazyos_ui::card::kind_of`); a name is a glyph name of the alphabet
//! (`image`) or a theme name (`network-wireless`). Exit codes are the shell's
//! contract: `0` sent (a picture that was not found is sent as text and the
//! reason goes to stderr), `2` usage, `3` the daemon did not take it and the
//! reason is on stderr.

use lazyos_ui::Glyph;
use lazyos_ui::card::{self, Card, Icon, Timeout, Urgency};

use crate::command::Resolved;

const EXIT_OK: i32 = 0;
const EXIT_USAGE: i32 = 2;
const EXIT_FAILED: i32 = 3;

/// `println!` panics when the reader closes the pipe; a card that could not be
/// printed is not worth a panic.
macro_rules! say {
    ($($arg:tt)*) => {{
        use std::io::Write as _;
        let _ = writeln!(std::io::stderr(), $($arg)*);
    }};
}

#[derive(Debug, Default)]
struct Options {
    words: Vec<String>,
    body: Option<String>,
    urgency: Option<Urgency>,
    icon: Option<String>,
    group: Option<String>,
    timeout: Option<Timeout>,
    progress: Option<u8>,
}

pub fn run(resolved: &Resolved) -> i32 {
    let args = resolved.args.as_slice();
    let Some((verb, rest)) = args.split_first() else {
        return usage();
    };
    match verb.as_str() {
        "send" | "say" => match parse(rest) {
            Ok(options) => card_from(&options, false),
            Err(message) => usage_error(&message),
        },
        "progress" => match parse(rest) {
            Ok(options) => card_from(&options, true),
            Err(message) => usage_error(&message),
        },
        "-h" | "--help" | "help" => usage(),
        other => usage_error(&format!(
            "'{other}' is not a notify verb (send, say, progress)"
        )),
    }
}

fn parse(args: &[String]) -> Result<Options, String> {
    let mut options = Options::default();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--body" => options.body = Some(value(&mut iter, "--body")?),
            "--urgency" => {
                let word = value(&mut iter, "--urgency")?;
                options.urgency = Some(Urgency::parse(&word).ok_or_else(|| {
                    format!("--urgency takes low, normal or critical, not '{word}'")
                })?);
            }
            "--icon" => options.icon = Some(value(&mut iter, "--icon")?),
            "--group" => options.group = Some(value(&mut iter, "--group")?),
            "--timeout" => {
                let word = value(&mut iter, "--timeout")?;
                options.timeout = Some(if word == "never" {
                    Timeout::Never
                } else {
                    Timeout::Millis(word.parse().map_err(|_| {
                        format!("--timeout takes milliseconds or 'never', not '{word}'")
                    })?)
                });
            }
            "--progress" => options.progress = Some(percent(&value(&mut iter, "--progress")?)?),
            flag if flag.starts_with("--") => return Err(format!("unknown option '{flag}'")),
            word => options.words.push(word.to_string()),
        }
    }
    Ok(options)
}

fn value<'a>(iter: &mut impl Iterator<Item = &'a String>, flag: &str) -> Result<String, String> {
    iter.next()
        .cloned()
        .ok_or_else(|| format!("{flag} needs a value"))
}

fn percent(word: &str) -> Result<u8, String> {
    match word.parse::<u8>() {
        Ok(value) if value <= 100 => Ok(value),
        _ => Err(format!("a percentage is 0 to 100, not '{word}'")),
    }
}

/// Build the card from the positional words and send it. `progress` takes the
/// group, the percentage, then the title and body; `send` and `say` take the
/// title and body.
fn card_from(options: &Options, progress: bool) -> i32 {
    let (title, body, group, percent) = if progress {
        let [tag, percent_word, title, rest @ ..] = options.words.as_slice() else {
            return usage_error("progress takes TAG PERCENT TITLE [BODY]");
        };
        if rest.len() > 1 {
            return usage_error("progress takes TAG PERCENT TITLE [BODY]");
        }
        let percent = match percent(percent_word) {
            Ok(value) => value,
            Err(message) => return usage_error(&message),
        };
        (
            title.as_str(),
            rest.first().map(String::as_str),
            Some(tag.as_str()),
            Some(percent),
        )
    } else {
        let (title, rest) = match options.words.as_slice() {
            [title] => (title.as_str(), None),
            [title, body] => (title.as_str(), Some(body.as_str())),
            _ => return usage_error("send takes TITLE [BODY]"),
        };
        (title, rest, options.group.as_deref(), options.progress)
    };
    let body = options.body.as_deref().or(body);
    let body = body.filter(|text| !text.is_empty());

    let urgency = options.urgency.unwrap_or(Urgency::Normal);
    let mut card = Card::new(title).urgency(urgency);
    if let Some(timeout) = options.timeout {
        card = card.timeout(timeout);
    }
    if let Some(text) = body {
        card = card.body(text);
    }
    card = match options.icon.as_deref() {
        None | Some("auto") => {
            let path = body.and_then(|text| std::fs::metadata(text).ok().map(|m| m.is_dir()));
            card.kind(card::kind_of(body.unwrap_or(""), urgency, path))
        }
        Some("none") => card.no_icon(),
        Some(name) => card.icon(icon_named(name)),
    };
    if let Some(tag) = group {
        card = card.group(tag);
    }
    if let Some(value) = percent {
        card = card.progress(value);
    }

    match card.send() {
        Ok(sent) => {
            if let Some(reason) = sent.fallback {
                say!("lazyos notify: no picture: {reason}; the card is text");
            }
            EXIT_OK
        }
        Err(refused) => {
            if let Some(reason) = refused.fallback {
                say!("lazyos notify: no picture: {reason}; the card is text");
            }
            say!("lazyos notify: {}", refused.reason);
            EXIT_FAILED
        }
    }
}

/// A glyph name of the alphabet first (`image`), then a theme name as given.
fn icon_named(name: &str) -> Icon {
    match Glyph::all().iter().find(|glyph| glyph.name() == name) {
        Some(glyph) => Icon::Glyph(*glyph),
        None => Icon::Name(name.to_string()),
    }
}

fn usage() -> i32 {
    say!("usage: lazyos notify send TITLE [BODY] [options]");
    say!("       lazyos notify say TITLE [BODY] [options]");
    say!("       lazyos notify progress TAG PERCENT TITLE [BODY] [options]");
    say!();
    say!("  --icon auto|none|NAME  auto (the default) from the body; a glyph or theme name");
    say!("  --urgency low|normal|critical");
    say!("  --group TAG            a card with the same tag replaces the last one");
    say!("  --timeout MS|never     how long it stays up");
    say!("  --progress N           a bar under the text, 0 to 100");
    say!("  --body TEXT            the body, when it is not the second word");
    EXIT_USAGE
}

fn usage_error(message: &str) -> i32 {
    say!("lazyos notify: {message}");
    usage()
}
