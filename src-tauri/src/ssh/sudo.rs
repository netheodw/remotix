//! Remembers the sudo password of a terminal session.
//!
//! [`SudoAssist`] watches a terminal's output for a `[sudo] password for ...:`
//! prompt. With a saved password the terminal thread answers the prompt
//! itself. Without one, the keystrokes the user types at the prompt are
//! captured and, if sudo does not reject them within [`VERIFY_WINDOW`],
//! handed back to be saved. A saved password that sudo rejects is reported so
//! the caller can delete it and let the user type the new one.
//!
//! This module is pure (no I/O, time passed in) so it can be unit-tested;
//! `ssh/terminal.rs` wires it to the channel and `SudoRepository`.

use std::time::{Duration, Instant};

use zeroize::Zeroizing;

/// How long after a password is submitted sudo has to reject it. PAM delays
/// a failure by about two seconds, so this leaves room for a slow link.
pub const VERIFY_WINDOW: Duration = Duration::from_secs(4);
const MAX_LINE: usize = 512;
const MAX_PASSWORD: usize = 1024;

/// Lower-cased text sudo prints when a password is wrong. A re-prompt also
/// counts as a failure, which covers translations not listed here.
const FAILURE_MARKERS: &[&str] = &[
    "sorry, try again",
    "incorrect password",
    "authentication failure",
    "not in the sudoers",
    "λυπάμαι",
];

#[derive(Debug, Default, PartialEq, Eq)]
pub struct OutputSignals {
    /// A sudo prompt is waiting for a password. The assist is now capturing
    /// keystrokes; call [`SudoAssist::autofilled`] after answering it instead.
    pub prompt: bool,
    /// The saved password was just rejected and should be deleted.
    pub saved_rejected: bool,
}

#[derive(Clone, Copy)]
enum Escape {
    None,
    Esc,
    Csi,
    Osc,
    OscEsc,
}

enum Phase {
    Idle,
    Capturing(Zeroizing<Vec<u8>>),
    Verifying { candidate: Zeroizing<Vec<u8>>, since: Instant },
    Autofilled { since: Instant },
}

pub struct SudoAssist {
    escape: Escape,
    line: String,
    prompt_on_line: bool,
    phase: Phase,
}

impl Default for SudoAssist {
    fn default() -> Self {
        Self::new()
    }
}

impl SudoAssist {
    pub fn new() -> Self {
        Self { escape: Escape::None, line: String::new(), prompt_on_line: false, phase: Phase::Idle }
    }

    /// Feeds bytes the remote side printed.
    pub fn on_output(&mut self, data: &[u8]) -> OutputSignals {
        let mut signals = OutputSignals::default();
        let text = String::from_utf8_lossy(data);
        for ch in text.chars() {
            self.escape = match self.escape {
                Escape::None => match ch {
                    '\x1b' => Escape::Esc,
                    '\n' => {
                        self.end_line(&mut signals);
                        Escape::None
                    }
                    '\x08' => {
                        self.line.pop();
                        Escape::None
                    }
                    // `\r` included: "text\r\n" must still end the line with its text.
                    c if (c as u32) < 0x20 => Escape::None,
                    c => {
                        if self.line.len() < MAX_LINE {
                            self.line.push(c);
                        }
                        Escape::None
                    }
                },
                Escape::Esc => match ch {
                    '[' => Escape::Csi,
                    ']' => Escape::Osc,
                    _ => Escape::None,
                },
                Escape::Csi if ('\x40'..='\x7e').contains(&ch) => Escape::None,
                Escape::Csi => Escape::Csi,
                Escape::Osc if ch == '\x07' => Escape::None,
                Escape::Osc if ch == '\x1b' => Escape::OscEsc,
                Escape::Osc => Escape::Osc,
                Escape::OscEsc => Escape::None,
            };
        }
        if !self.prompt_on_line && is_sudo_prompt(&self.line) {
            self.prompt_on_line = true;
            if matches!(self.phase, Phase::Autofilled { .. }) {
                signals.saved_rejected = true;
            }
            self.phase = Phase::Capturing(Zeroizing::new(Vec::with_capacity(MAX_PASSWORD)));
            signals.prompt = true;
        }
        signals
    }

    /// The caller answered the current prompt with the saved password.
    pub fn autofilled(&mut self, now: Instant) {
        self.phase = Phase::Autofilled { since: now };
    }

    /// Feeds bytes the user typed. Only matters while a prompt is open.
    pub fn on_input(&mut self, data: &[u8], now: Instant) {
        let Phase::Capturing(buf) = &mut self.phase else { return };
        for &b in data {
            match b {
                b'\r' | b'\n' => {
                    let candidate = std::mem::replace(buf, Zeroizing::new(Vec::new()));
                    self.phase = if candidate.is_empty() {
                        Phase::Idle
                    } else {
                        Phase::Verifying { candidate, since: now }
                    };
                    return;
                }
                0x7f | 0x08 => {
                    // Remove one UTF-8 character: continuation bytes, then its lead byte.
                    while let Some(last) = buf.pop() {
                        if last & 0xC0 != 0x80 {
                            break;
                        }
                    }
                }
                0x15 => buf.clear(),
                // Escape sequences (arrow keys), Ctrl+C, Ctrl+D, Ctrl+W and the
                // like: what reaches sudo is no longer certain, so don't save.
                b if b < 0x20 => {
                    self.phase = Phase::Idle;
                    return;
                }
                b => {
                    if buf.len() >= MAX_PASSWORD {
                        self.phase = Phase::Idle;
                        return;
                    }
                    buf.push(b);
                }
            }
        }
    }

    /// Returns a typed password once sudo has had [`VERIFY_WINDOW`] to reject
    /// it and did not.
    pub fn poll(&mut self, now: Instant) -> Option<Zeroizing<Vec<u8>>> {
        match &self.phase {
            Phase::Verifying { since, .. } if now.duration_since(*since) >= VERIFY_WINDOW => {
                match std::mem::replace(&mut self.phase, Phase::Idle) {
                    Phase::Verifying { candidate, .. } => Some(candidate),
                    _ => None,
                }
            }
            Phase::Autofilled { since } if now.duration_since(*since) >= VERIFY_WINDOW => {
                self.phase = Phase::Idle;
                None
            }
            _ => None,
        }
    }

    fn end_line(&mut self, signals: &mut OutputSignals) {
        let lower = self.line.to_lowercase();
        let failed = FAILURE_MARKERS.iter().any(|m| lower.contains(m));
        match self.phase {
            Phase::Autofilled { .. } if failed => {
                signals.saved_rejected = true;
                self.phase = Phase::Idle;
            }
            Phase::Verifying { .. } if failed => self.phase = Phase::Idle,
            // Password prompts don't echo, so a new line while capturing means
            // the prompt is gone (timed out, interrupted).
            Phase::Capturing(_) => self.phase = Phase::Idle,
            _ => {}
        }
        self.line.clear();
        self.prompt_on_line = false;
    }
}

/// `[sudo] password for alice:` in any language, and variants such as
/// `[sudo: authenticate] Password:`.
fn is_sudo_prompt(line: &str) -> bool {
    let t = line.trim();
    t.starts_with("[sudo") && t.ends_with(':')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> Instant {
        Instant::now()
    }

    #[test]
    fn detects_prompt_through_colour_codes() {
        let mut a = SudoAssist::new();
        let s = a.on_output(b"\x1b[01;32muser@host\x1b[0m:~$ sudo ls\r\n\x1b]0;title\x07[sudo] password for user: ");
        assert_eq!(s, OutputSignals { prompt: true, saved_rejected: false });
        // The same prompt is not reported twice.
        assert_eq!(a.on_output(b""), OutputSignals::default());
    }

    #[test]
    fn detects_localized_prompt_split_across_chunks() {
        let mut a = SudoAssist::new();
        assert!(!a.on_output("[sudo] κωδικός πρόσβασης".as_bytes()).prompt);
        assert!(a.on_output(" για user: ".as_bytes()).prompt);
    }

    #[test]
    fn ignores_ordinary_output() {
        let mut a = SudoAssist::new();
        assert!(!a.on_output(b"Password: ").prompt);
        assert!(!a.on_output(b"user@host:~$ echo [sudo] done\r\n").prompt);
    }

    #[test]
    fn typed_password_is_returned_after_the_window() {
        let mut a = SudoAssist::new();
        let now = t0();
        a.on_output(b"[sudo] password for user: ");
        a.on_input(b"hunte", now);
        a.on_input(b"x\x7fr2", now);
        a.on_input(b"\r", now);
        a.on_output(b"\r\n");
        assert!(a.poll(now + Duration::from_secs(1)).is_none());
        assert_eq!(&a.poll(now + VERIFY_WINDOW).unwrap()[..], b"hunter2");
        assert!(a.poll(now + VERIFY_WINDOW * 2).is_none());
    }

    #[test]
    fn rejected_password_is_not_returned() {
        let mut a = SudoAssist::new();
        let now = t0();
        a.on_output(b"[sudo] password for user: ");
        a.on_input(b"wrong\r", now);
        a.on_output(b"\r\n");
        let s = a.on_output(b"Sorry, try again.\r\n[sudo] password for user: ");
        assert!(s.prompt && !s.saved_rejected);
        a.on_input(b"right\r", now);
        assert_eq!(&a.poll(now + VERIFY_WINDOW).unwrap()[..], b"right");
    }

    #[test]
    fn reprompt_without_a_known_message_counts_as_failure() {
        let mut a = SudoAssist::new();
        let now = t0();
        a.on_output(b"[sudo] password for user: ");
        a.on_input(b"wrong\r", now);
        a.on_output(b"\r\nDesole.\r\n[sudo] password for user: ");
        assert!(a.poll(now + VERIFY_WINDOW).is_none());
    }

    #[test]
    fn interrupted_entry_is_not_saved() {
        let mut a = SudoAssist::new();
        let now = t0();
        a.on_output(b"[sudo] password for user: ");
        a.on_input(b"abc\x03", now);
        a.on_input(b"\r", now);
        assert!(a.poll(now + VERIFY_WINDOW).is_none());

        a.on_output(b"\r\n[sudo] password for user: ");
        a.on_input(b"abc\x1b[A\r", now);
        assert!(a.poll(now + VERIFY_WINDOW).is_none());
    }

    #[test]
    fn typing_outside_a_prompt_is_ignored() {
        let mut a = SudoAssist::new();
        let now = t0();
        a.on_input(b"ls -la\r", now);
        assert!(a.poll(now + VERIFY_WINDOW).is_none());
    }

    #[test]
    fn saved_password_rejected_by_message_or_reprompt() {
        let now = t0();
        let mut a = SudoAssist::new();
        a.on_output(b"[sudo] password for user: ");
        a.autofilled(now);
        assert!(a.on_output(b"\r\nsudo: 1 incorrect password attempt\r\n").saved_rejected);

        let mut b = SudoAssist::new();
        b.on_output(b"[sudo] password for user: ");
        b.autofilled(now);
        let s = b.on_output(b"\r\n[sudo] password for user: ");
        assert!(s.saved_rejected && s.prompt);
        // After the rejection the user's own typing is captured.
        b.on_input(b"new\r", now);
        assert_eq!(&b.poll(now + VERIFY_WINDOW).unwrap()[..], b"new");
    }

    #[test]
    fn accepted_saved_password_is_not_returned_again() {
        let now = t0();
        let mut a = SudoAssist::new();
        a.on_output(b"[sudo] password for user: ");
        a.autofilled(now);
        a.on_output(b"\r\nfile1 file2\r\n");
        assert!(a.poll(now + VERIFY_WINDOW).is_none());
        assert!(!a.on_output(b"more output\r\n").saved_rejected);
    }
}
