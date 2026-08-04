//! Orchestrate add / delete / update across adapters and filesystem.

use crate::adapters::command::CommandRunner;
use crate::adapters::gh_skill::GhSkillCli;
use crate::adapters::npx_skills::NpxSkillsCli;
use crate::model::{Agent, Scope, SkillRecord};
use anyhow::{anyhow, Context, Result};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddBackend {
    GhSkill,
    NpxSkills,
}

impl AddBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            AddBackend::GhSkill => "gh skill",
            AddBackend::NpxSkills => "npx skills",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DeletePlan {
    pub skill_name: String,
    pub scope: Scope,
    pub agents: Vec<Agent>,
    pub paths: Vec<std::path::PathBuf>,
    pub shared_warning: Option<String>,
}

pub fn plan_delete(skill: &SkillRecord, agents: &[Agent]) -> DeletePlan {
    let selected: Vec<_> = skill
        .locations
        .iter()
        .filter(|l| agents.contains(&l.agent))
        .cloned()
        .collect();
    let paths: Vec<_> = selected.iter().map(|l| l.path.clone()).collect();
    let mut shared_warning = None;
    for loc in &selected {
        if let Some(resolved) = &loc.resolved {
            let s = resolved.to_string_lossy();
            if s.contains("/.agents/skills/") {
                shared_warning = Some(format!(
                    "Removing symlink target may affect other agents sharing {s}"
                ));
                break;
            }
        }
    }
    DeletePlan {
        skill_name: skill.name.clone(),
        scope: skill.scope,
        agents: agents.to_vec(),
        paths,
        shared_warning,
    }
}

pub fn execute_delete(
    runner: &impl CommandRunner,
    plan: &DeletePlan,
    prefer_npx: bool,
) -> Result<Vec<String>> {
    let mut messages = Vec::new();
    let mut npx_ok = prefer_npx;

    if prefer_npx {
        let cli = NpxSkillsCli { runner };
        for agent in &plan.agents {
            match cli.remove(&plan.skill_name, *agent, plan.scope) {
                Ok(out) => {
                    messages.push(format!(
                        "npx skills remove {} @{}: {}",
                        plan.skill_name,
                        agent,
                        out.stdout.trim()
                    ));
                }
                Err(err) => {
                    messages.push(format!("npx remove failed for {agent}: {err}"));
                    npx_ok = false;
                }
            }
        }
    }

    if !npx_ok || !prefer_npx {
        for path in &plan.paths {
            remove_skill_path(path)?;
            messages.push(format!("removed {}", path.display()));
        }
    }
    Ok(messages)
}

pub fn remove_skill_path(path: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(path)
        .with_context(|| format!("stat {}", path.display()))?;
    if meta.file_type().is_symlink() {
        fs::remove_file(path).with_context(|| format!("unlink {}", path.display()))?;
    } else if meta.is_dir() {
        fs::remove_dir_all(path).with_context(|| format!("rm -rf {}", path.display()))?;
    } else {
        fs::remove_file(path).with_context(|| format!("rm {}", path.display()))?;
    }
    Ok(())
}

pub fn execute_add(
    runner: &impl CommandRunner,
    backend: AddBackend,
    package_or_repo: &str,
    skill: &str,
    agent: Agent,
    scope: Scope,
) -> Result<String> {
    match backend {
        AddBackend::GhSkill => {
            let cli = GhSkillCli { runner };
            let out = cli.install(package_or_repo, skill, agent, scope)?;
            Ok(format!(
                "gh skill install ok\n{}\n{}",
                out.stdout.trim(),
                out.stderr.trim()
            ))
        }
        AddBackend::NpxSkills => {
            let cli = NpxSkillsCli { runner };
            let out = cli.add(package_or_repo, Some(skill), agent, scope)?;
            Ok(format!(
                "npx skills add ok\n{}\n{}",
                out.stdout.trim(),
                out.stderr.trim()
            ))
        }
    }
}

pub fn execute_update(runner: &impl CommandRunner, skill: &str) -> Result<String> {
    let cli = GhSkillCli { runner };
    let out = cli.update(skill)?;
    Ok(format!(
        "gh skill update ok\n{}\n{}",
        out.stdout.trim(),
        out.stderr.trim()
    ))
}

pub fn require_cli(name: &str) -> Result<()> {
    if crate::adapters::command::which_ok(name) {
        Ok(())
    } else {
        Err(anyhow!("{name} not found on PATH"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{InstallKind, InstallSource, SkillLocation, SkillStats};
    use std::path::PathBuf;

    #[test]
    fn plan_delete_warns_on_shared_agents_target() {
        let skill = SkillRecord {
            id: "x".into(),
            name: "x".into(),
            description: String::new(),
            scope: Scope::User,
            agents: vec![Agent::Cursor],
            locations: vec![SkillLocation {
                agent: Agent::Cursor,
                scope: Scope::User,
                path: PathBuf::from("/home/.cursor/skills/x"),
                kind: InstallKind::Symlink,
                resolved: Some(PathBuf::from("/home/.agents/skills/x")),
            }],
            install_kind: InstallKind::Symlink,
            source: InstallSource::Manual,
            source_url: None,
            version: None,
            pinned: false,
            stats: SkillStats::default(),
        };
        let plan = plan_delete(&skill, &[Agent::Cursor]);
        assert!(plan.shared_warning.is_some());
    }

    #[test]
    fn remove_symlink_only() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("target");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("SKILL.md"), "---\nname: x\n---\n").unwrap();
        let link = tmp.path().join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        remove_skill_path(&link).unwrap();
        assert!(!link.exists());
        assert!(target.exists());
    }
}
