//! Wrapper around `gh skill` CLI.

use crate::adapters::command::{CommandOutput, CommandRunner};
use crate::model::{Agent, Scope};
use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GhSkillListItem {
    pub skill_name: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub scope: String,
    /// gh emits `sourceURL` (URL capitalized), not camelCase `sourceUrl`.
    #[serde(default, rename = "sourceURL", alias = "sourceUrl")]
    pub source_url: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub agent_hosts: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GhSkillSearchItem {
    pub skill_name: String,
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub stars: u64,
}

pub struct GhSkillCli<'a, R: CommandRunner> {
    pub runner: &'a R,
}

impl<'a, R: CommandRunner> GhSkillCli<'a, R> {
    pub fn list(&self, scope: Option<Scope>, agent: Option<Agent>) -> Result<Vec<GhSkillListItem>> {
        let mut args = vec![
            "skill",
            "list",
            "--json",
            "skillName,path,scope,sourceURL,version,pinned,agentHosts",
        ];
        let scope_s;
        if let Some(scope) = scope {
            scope_s = scope.as_str().to_string();
            args.push("--scope");
            args.push(&scope_s);
        }
        let agent_s;
        if let Some(agent) = agent {
            agent_s = agent.as_str().to_string();
            args.push("--agent");
            args.push(&agent_s);
        }
        let out = self.runner.run("gh", &args)?;
        if !out.success() {
            return Err(anyhow!(
                "gh skill list failed ({}): {}",
                out.status,
                out.stderr.trim()
            ));
        }
        parse_list_json(&out.stdout)
    }

    pub fn search(&self, query: &str, limit: u32) -> Result<Vec<GhSkillSearchItem>> {
        let limit_s = limit.to_string();
        let out = self.runner.run(
            "gh",
            &[
                "skill",
                "search",
                query,
                "--limit",
                &limit_s,
                "--json",
                "skillName,repo,description,path,stars",
            ],
        )?;
        if !out.success() {
            return Err(anyhow!(
                "gh skill search failed ({}): {}",
                out.status,
                out.stderr.trim()
            ));
        }
        serde_json::from_str(out.stdout.trim())
            .with_context(|| format!("invalid gh skill search json: {}", out.stdout))
    }

    pub fn install(
        &self,
        repo: &str,
        skill: &str,
        agent: Agent,
        scope: Scope,
    ) -> Result<CommandOutput> {
        let agent_s = agent.as_str();
        let scope_s = scope.as_str();
        let out = self.runner.run(
            "gh",
            &[
                "skill",
                "install",
                repo,
                skill,
                "--agent",
                agent_s,
                "--scope",
                scope_s,
            ],
        )?;
        if !out.success() {
            return Err(anyhow!(
                "gh skill install failed ({}): {}",
                out.status,
                out.stderr.trim()
            ));
        }
        Ok(out)
    }

    pub fn update(&self, skill: &str, dir: Option<&std::path::Path>) -> Result<CommandOutput> {
        let mut args = vec![
            "skill".to_string(),
            "update".to_string(),
            skill.to_string(),
            "--all".to_string(),
        ];
        if let Some(dir) = dir {
            // Scope the scan so we hit the host copy that has GitHub metadata,
            // not a sibling install under ~/.agents/skills without provenance.
            args.push("--dir".to_string());
            args.push(dir.display().to_string());
        }
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = self.runner.run("gh", &arg_refs)?;
        if !out.success() {
            return Err(anyhow!(
                "gh skill update failed ({}): {}",
                out.status,
                out.stderr.trim()
            ));
        }
        Ok(out)
    }
}

fn parse_list_json(stdout: &str) -> Result<Vec<GhSkillListItem>> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(trimmed)
        .with_context(|| format!("invalid gh skill list json: {trimmed}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::command::{CommandOutput, FakeCommandRunner};

    #[test]
    fn list_parses_json_and_passes_flags() {
        let json = r#"[{"skillName":"tdd","path":"/tmp/tdd","scope":"user","sourceURL":"https://x","version":"v1","pinned":false,"agentHosts":["cursor"]}]"#;
        let runner = FakeCommandRunner::with_responses(vec![CommandOutput {
            status: 0,
            stdout: json.into(),
            stderr: String::new(),
        }]);
        let cli = GhSkillCli { runner: &runner };
        let items = cli.list(Some(Scope::User), Some(Agent::Cursor)).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].skill_name, "tdd");
        assert_eq!(items[0].source_url, "https://x");
        let calls = runner.calls();
        assert_eq!(calls[0].0, "gh");
        assert!(calls[0].1.contains(&"--scope".into()));
        assert!(calls[0].1.contains(&"user".into()));
    }
}
