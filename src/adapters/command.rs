//! Command runner abstraction for CLI adapters (testable).

use anyhow::{anyhow, Context, Result};
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

pub trait CommandRunner: Send + Sync {
    fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput>;
}

#[derive(Debug, Default)]
pub struct SystemCommandRunner;

impl CommandRunner for SystemCommandRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput> {
        let output = Command::new(program)
            .args(args)
            .output()
            .with_context(|| format!("failed to spawn `{program}`"))?;
        Ok(CommandOutput {
            status: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
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
            responses: std::sync::Mutex::new(
                responses.into_iter().map(Ok).collect(),
            ),
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
