//! Keys whose escape sequence the operating system delivered in pieces.
//!
//! F3 arrives as `ESC O R`, Page Down as `ESC [ 6 ~`. When the `ESC` byte reaches Den a
//! moment before the rest, crossterm reports a bare Esc followed by ordinary characters,
//! so F3 becomes "Back, O, R". Like vim's `ttimeout` and ncurses' `ESCDELAY`, a bare Esc
//! waits briefly: if the rest of a known sequence follows, it becomes that key.
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::{io, time::Duration};

/// How long a bare Esc waits for the rest of a sequence: ample for bytes split by a busy
/// system, unnoticeable when Esc is pressed on its own, and far shorter than a person can
/// type Esc then `[` (vim and tmux commonly wait 100 ms or more).
pub const ESC_WAIT: Duration = Duration::from_millis(50);
/// Longest sequence handled after `ESC`, such as `[14~`.
const LONGEST: usize = 4;

#[derive(Debug, PartialEq, Eq)]
enum Decode {
    Key(KeyCode, KeyModifiers),
    /// A known sequence could still follow.
    Partial,
    /// Not a sequence Den knows: the characters stay ordinary keys.
    Unknown,
}

/// The key named by the characters after `ESC`, for the keys Den uses.
fn decode(chars: &[char]) -> Decode {
    let none = KeyModifiers::NONE;
    let key = |code| Decode::Key(code, none);
    match chars {
        [] | ['['] | ['O'] => Decode::Partial,
        ['O', c] | ['[', c] if c.is_ascii_alphabetic() => match c {
            'A' => key(KeyCode::Up),
            'B' => key(KeyCode::Down),
            'C' => key(KeyCode::Right),
            'D' => key(KeyCode::Left),
            'H' => key(KeyCode::Home),
            'F' => key(KeyCode::End),
            'P' if chars[0] == 'O' => key(KeyCode::F(1)),
            'Q' if chars[0] == 'O' => key(KeyCode::F(2)),
            'R' if chars[0] == 'O' => key(KeyCode::F(3)),
            'S' if chars[0] == 'O' => key(KeyCode::F(4)),
            'Z' if chars[0] == '[' => Decode::Key(KeyCode::BackTab, KeyModifiers::SHIFT),
            _ => Decode::Unknown,
        },
        ['[', rest @ ..] if rest.len() < LONGEST && rest.iter().all(char::is_ascii_digit) => {
            Decode::Partial
        }
        ['[', digits @ .., '~'] if digits.iter().all(char::is_ascii_digit) => {
            match digits.iter().collect::<String>().as_str() {
                "1" | "7" => key(KeyCode::Home),
                "4" | "8" => key(KeyCode::End),
                "3" => key(KeyCode::Delete),
                "5" => key(KeyCode::PageUp),
                "6" => key(KeyCode::PageDown),
                "11" => key(KeyCode::F(1)),
                "12" => key(KeyCode::F(2)),
                "13" => key(KeyCode::F(3)),
                "14" => key(KeyCode::F(4)),
                _ => Decode::Unknown,
            }
        }
        _ => Decode::Unknown,
    }
}

fn bare_escape(key: &KeyEvent) -> bool {
    key.code == KeyCode::Esc && key.modifiers.is_empty() && key.kind != KeyEventKind::Release
}

/// The keys `key` stands for. After a bare Esc, `next` gives the next event that arrives
/// within [`ESC_WAIT`], or `None`; a completed sequence becomes one key, anything else
/// is delivered unchanged and in order.
pub fn complete(
    key: KeyEvent,
    mut next: impl FnMut() -> io::Result<Option<Event>>,
) -> io::Result<Vec<KeyEvent>> {
    if !bare_escape(&key) {
        return Ok(vec![key]);
    }
    let mut keys = vec![key];
    let mut chars = vec![];
    while let Some(event) = next()? {
        let Event::Key(follow) = event else {
            // Not a key (a resize, for example): the screen redraws on the next loop.
            break;
        };
        keys.push(follow);
        let KeyCode::Char(c) = follow.code else {
            break;
        };
        if follow.modifiers.difference(KeyModifiers::SHIFT) != KeyModifiers::NONE {
            break;
        }
        chars.push(c);
        match decode(&chars) {
            Decode::Key(code, modifiers) => return Ok(vec![KeyEvent::new(code, modifiers)]),
            Decode::Partial => {}
            Decode::Unknown => break,
        }
    }
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
    /// Feeds the characters after a bare Esc, then nothing more arrives.
    fn after_esc(rest: &str) -> Vec<KeyEvent> {
        let mut events = rest.chars().map(|c| Event::Key(key(KeyCode::Char(c))));
        complete(key(KeyCode::Esc), || Ok(events.next())).unwrap()
    }

    #[test]
    fn split_sequences_become_their_key() {
        assert_eq!(after_esc("OR"), [key(KeyCode::F(3))]);
        assert_eq!(after_esc("OQ"), [key(KeyCode::F(2))]);
        assert_eq!(after_esc("[6~"), [key(KeyCode::PageDown)]);
        assert_eq!(after_esc("[5~"), [key(KeyCode::PageUp)]);
        assert_eq!(after_esc("[B"), [key(KeyCode::Down)]);
        assert_eq!(after_esc("OA"), [key(KeyCode::Up)]);
        assert_eq!(after_esc("[H"), [key(KeyCode::Home)]);
        assert_eq!(after_esc("[4~"), [key(KeyCode::End)]);
        assert_eq!(after_esc("[13~"), [key(KeyCode::F(3))]);
        assert_eq!(
            after_esc("[Z"),
            [KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT)]
        );
    }

    #[test]
    fn a_real_escape_stays_an_escape() {
        // Nothing follows within the wait.
        assert_eq!(after_esc(""), [key(KeyCode::Esc)]);
        // Esc then ordinary keys, or an unknown sequence: delivered unchanged, in order.
        assert_eq!(after_esc("q"), [key(KeyCode::Esc), key(KeyCode::Char('q'))]);
        assert_eq!(
            after_esc("[9~"),
            ["\x1b", "[", "9", "~"]
                .map(|s| match s {
                    "\x1b" => key(KeyCode::Esc),
                    s => key(KeyCode::Char(s.chars().next().unwrap())),
                })
                .to_vec()
        );
        // An unfinished sequence that stops arriving is delivered as typed.
        assert_eq!(after_esc("["), [key(KeyCode::Esc), key(KeyCode::Char('['))]);
    }

    #[test]
    fn other_keys_pass_straight_through() {
        let mut calls = 0;
        let keys = complete(key(KeyCode::Char('j')), || {
            calls += 1;
            Ok(None)
        })
        .unwrap();
        assert_eq!(keys, [key(KeyCode::Char('j'))]);
        assert_eq!(calls, 0, "no wait unless the key is a bare Esc");
        let alt = KeyEvent::new(KeyCode::Esc, KeyModifiers::ALT);
        assert_eq!(complete(alt, || Ok(None)).unwrap(), [alt]);
    }

    #[test]
    fn a_long_digit_run_is_not_waited_on_forever() {
        assert_eq!(decode(&['[', '1', '2', '3', '4']), Decode::Unknown);
        assert_eq!(after_esc("[12345~").len(), 6);
    }
}
