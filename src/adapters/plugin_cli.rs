//! Plugin catalog CLIs: `claude plugin`, `codex plugin`, `copilot plugin`.
//!
//! Listing still works without these binaries (filesystem scan). They are only
//! used for add / update / delete.

use crate::adapters::command::{CommandOutput, CommandRunner};
use crate::model::{PluginBackend, Scope};
use anyhow::{Result, anyhow};

pub struct PluginCli<'a, R: CommandRunner> {
    pub runner: &'a R,
}

impl<'a, R: CommandRunner> PluginCli<'a, R> {
    pub fn install(
        &self,
        backend: PluginBackend,
        spec: &str,
        scope: Scope,
    ) -> Result<CommandOutput> {
        let out = match backend {
            PluginBackend::Claude => self.runner.run(
                "claude",
                &["plugin", "install", spec, "--scope", claude_scope(scope)],
            )?,
            PluginBackend::Codex => self.runner.run("codex", &["plugin", "add", spec])?,
            PluginBackend::Copilot => self.runner.run("copilot", &["plugin", "install", spec])?,
        };
        fail_unless_ok(backend, "install", spec, &out)?;
        Ok(out)
    }

    pub fn uninstall(
        &self,
        backend: PluginBackend,
        spec: &str,
        scope: Scope,
    ) -> Result<CommandOutput> {
        let name = spec.split_once('@').map(|(n, _)| n).unwrap_or(spec);
        let out = match backend {
            PluginBackend::Claude => self.runner.run(
                "claude",
                &["plugin", "uninstall", spec, "--scope", claude_scope(scope)],
            )?,
            PluginBackend::Codex => self.runner.run("codex", &["plugin", "remove", name])?,
            PluginBackend::Copilot => self.runner.run("copilot", &["plugin", "uninstall", name])?,
        };
        fail_unless_ok(backend, "uninstall", spec, &out)?;
        Ok(out)
    }

    pub fn update(
        &self,
        backend: PluginBackend,
        spec: &str,
        scope: Scope,
    ) -> Result<CommandOutput> {
        let name = spec.split_once('@').map(|(n, _)| n).unwrap_or(spec);
        let out = match backend {
            PluginBackend::Claude => self.runner.run(
                "claude",
                &["plugin", "update", spec, "--scope", claude_scope(scope)],
            )?,
            PluginBackend::Codex => self.runner.run("codex", &["plugin", "add", spec])?,
            PluginBackend::Copilot => self.runner.run("copilot", &["plugin", "update", name])?,
        };
        fail_unless_ok(backend, "update", spec, &out)?;
        Ok(out)
    }
}

fn claude_scope(scope: Scope) -> &'static str {
    match scope {
        Scope::User => "user",
        Scope::Project => "project",
    }
}

fn fail_unless_ok(backend: PluginBackend, op: &str, spec: &str, out: &CommandOutput) -> Result<()> {
    if out.success() {
        return Ok(());
    }
    Err(anyhow!(
        "{} plugin {op} {spec} failed ({}): {}",
        backend.as_str(),
        out.status,
        out.stderr.trim()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::command::{CommandOutput, FakeCommandRunner};

    fn ok() -> CommandOutput {
        CommandOutput {
            status: 0,
            stdout: "ok".into(),
            stderr: String::new(),
        }
    }

    #[test]
    fn claude_install_uses_scope_flag() {
        let runner = FakeCommandRunner::with_responses(vec![ok()]);
        let cli = PluginCli { runner: &runner };
        cli.install(
            PluginBackend::Claude,
            "fmt@claude-plugins-official",
            Scope::User,
        )
        .unwrap();
        assert_eq!(
            runner.calls()[0],
            (
                "claude".into(),
                vec![
                    "plugin".into(),
                    "install".into(),
                    "fmt@claude-plugins-official".into(),
                    "--scope".into(),
                    "user".into(),
                ]
            )
        );
    }

    #[test]
    fn copilot_uninstall_uses_bare_name() {
        let runner = FakeCommandRunner::with_responses(vec![ok()]);
        let cli = PluginCli { runner: &runner };
        cli.uninstall(
            PluginBackend::Copilot,
            "frontend@awesome-copilot",
            Scope::User,
        )
        .unwrap();
        assert_eq!(
            runner.calls()[0],
            (
                "copilot".into(),
                vec!["plugin".into(), "uninstall".into(), "frontend".into(),]
            )
        );
    }

    #[test]
    fn codex_update_reinstalls_via_add() {
        let runner = FakeCommandRunner::with_responses(vec![ok()]);
        let cli = PluginCli { runner: &runner };
        cli.update(PluginBackend::Codex, "linear@openai-curated", Scope::User)
            .unwrap();
        assert_eq!(
            runner.calls()[0],
            (
                "codex".into(),
                vec![
                    "plugin".into(),
                    "add".into(),
                    "linear@openai-curated".into(),
                ]
            )
        );
    }
}
