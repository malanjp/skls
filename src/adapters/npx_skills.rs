//! Wrapper around `npx skills` CLI.

use crate::adapters::command::{CommandOutput, CommandRunner};
use crate::model::{Agent, Scope};
use anyhow::{Result, anyhow};

pub struct NpxSkillsCli<'a, R: CommandRunner> {
    pub runner: &'a R,
}

impl<'a, R: CommandRunner> NpxSkillsCli<'a, R> {
    pub fn add(
        &self,
        package: &str,
        skill: Option<&str>,
        agent: Agent,
        scope: Scope,
    ) -> Result<CommandOutput> {
        let mut args = vec!["skills", "add", package, "-y", "-a", agent.as_str()];
        if scope == Scope::User {
            args.push("-g");
        }
        let skill_owned;
        if let Some(skill) = skill {
            skill_owned = skill.to_string();
            args.push("-s");
            args.push(&skill_owned);
        }
        let out = self.runner.run("npx", &args)?;
        if !out.success() {
            return Err(anyhow!(
                "npx skills add failed ({}): {}",
                out.status,
                out.stderr.trim()
            ));
        }
        Ok(out)
    }

    pub fn remove(&self, skill: &str, agent: Agent, scope: Scope) -> Result<CommandOutput> {
        let mut args = vec!["skills", "remove", skill, "-y", "-a", agent.as_str()];
        if scope == Scope::User {
            args.push("-g");
        }
        let out = self.runner.run("npx", &args)?;
        if !out.success() {
            return Err(anyhow!(
                "npx skills remove failed ({}): {}",
                out.status,
                out.stderr.trim()
            ));
        }
        Ok(out)
    }

    pub fn update(&self, skills: &[&str], scope: Scope) -> Result<CommandOutput> {
        let mut args = vec!["skills".to_string(), "update".to_string()];
        for skill in skills {
            args.push((*skill).to_string());
        }
        args.push("-y".to_string());
        match scope {
            Scope::User => args.push("-g".to_string()),
            Scope::Project => args.push("-p".to_string()),
        }
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = self.runner.run("npx", &arg_refs)?;
        if !out.success() {
            return Err(anyhow!(
                "npx skills update failed ({}): {}",
                out.status,
                out.stderr.trim()
            ));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::command::{CommandOutput, FakeCommandRunner};

    #[test]
    fn remove_uses_global_flag_for_user_scope() {
        let runner = FakeCommandRunner::with_responses(vec![CommandOutput {
            status: 0,
            stdout: String::new(),
            stderr: String::new(),
        }]);
        let cli = NpxSkillsCli { runner: &runner };
        cli.remove("tdd", Agent::ClaudeCode, Scope::User).unwrap();
        let args = &runner.calls()[0].1;
        assert!(args.contains(&"-g".into()));
        assert!(args.contains(&"claude-code".into()));
    }

    #[test]
    fn update_uses_global_flag_for_user_scope() {
        let runner = FakeCommandRunner::with_responses(vec![CommandOutput {
            status: 0,
            stdout: "updated".into(),
            stderr: String::new(),
        }]);
        let cli = NpxSkillsCli { runner: &runner };
        cli.update(&["find-skills", "tdd"], Scope::User).unwrap();
        let args = &runner.calls()[0].1;
        assert_eq!(args[0], "skills");
        assert_eq!(args[1], "update");
        assert!(args.contains(&"find-skills".into()));
        assert!(args.contains(&"tdd".into()));
        assert!(args.contains(&"-g".into()));
        assert!(args.contains(&"-y".into()));
    }
}
