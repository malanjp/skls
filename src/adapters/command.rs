//! Command runner abstraction for CLI adapters (testable).

use anyhow::{Context, Result, anyhow};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

impl CommandOutput {
    pub fn success(&self) -> bool {
        self.status == 0
    }
}

/// Strip CSI/OSC ANSI sequences so TUI modals stay readable.
pub fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('[') => {
                // CSI: ESC [ ... final byte in @..~
                for c2 in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&c2) {
                        break;
                    }
                }
            }
            Some(']') => {
                // OSC: ESC ] ... BEL or ST (ESC \)
                while let Some(c2) = chars.next() {
                    if c2 == '\u{07}' {
                        break;
                    }
                    if c2 == '\u{1b}' && chars.peek() == Some(&'\\') {
                        let _ = chars.next();
                        break;
                    }
                }
            }
            Some(_) | None => {}
        }
    }
    out
}

pub trait CommandRunner: Send + Sync {
    fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput>;
}

#[derive(Debug, Default)]
pub struct SystemCommandRunner;

impl CommandRunner for SystemCommandRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput> {
        let output = Command::new(program)
            .args(args)
            // Prefer plain text from CLIs that honor NO_COLOR.
            .env("NO_COLOR", "1")
            .env("FORCE_COLOR", "0")
            .env("CLICOLOR", "0")
            .output()
            .with_context(|| format!("failed to spawn `{program}`"))?;
        Ok(CommandOutput {
            status: output.status.code().unwrap_or(-1),
            stdout: strip_ansi(&String::from_utf8_lossy(&output.stdout)),
            stderr: strip_ansi(&String::from_utf8_lossy(&output.stderr)),
        })
    }
}

/// In-memory runner for tests: records calls and returns scripted outputs.
#[derive(Debug, Default)]
pub struct FakeCommandRunner {
    pub calls: std::sync::Mutex<Vec<(String, Vec<String>)>>,
    pub responses: std::sync::Mutex<Vec<Result<CommandOutput, String>>>,
}

impl FakeCommandRunner {
    pub fn with_responses(responses: Vec<CommandOutput>) -> Self {
        Self {
            calls: std::sync::Mutex::new(Vec::new()),
            responses: std::sync::Mutex::new(responses.into_iter().map(Ok).collect()),
        }
    }

    pub fn calls(&self) -> Vec<(String, Vec<String>)> {
        self.calls.lock().unwrap().clone()
    }
}

impl CommandRunner for FakeCommandRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput> {
        self.calls.lock().unwrap().push((
            program.to_string(),
            args.iter().map(|s| (*s).to_string()).collect(),
        ));
        let mut responses = self.responses.lock().unwrap();
        if responses.is_empty() {
            return Err(anyhow!("no scripted response for {program}"));
        }
        match responses.remove(0) {
            Ok(out) => Ok(out),
            Err(msg) => Err(anyhow!(msg)),
        }
    }
}

pub fn which_ok(program: &str) -> bool {
    Command::new("which")
        .arg(program)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_ansi_removes_csi_and_erase_line() {
        let raw = "\u{1b}[38;5;145mUpdating pi-prompting...\u{1b}[0m\n\
                   \u{1b}[38;5;102mChecking skills\u{1b}[0m\u{1b}[K\n\
                   \u{1b}[K\u{1b}[38;5;145m✔ All global skills are up to date\u{1b}[0m\n";
        let clean = strip_ansi(raw);
        assert_eq!(
            clean,
            "Updating pi-prompting...\nChecking skills\n✔ All global skills are up to date\n"
        );
        assert!(!clean.contains('['));
    }
}
