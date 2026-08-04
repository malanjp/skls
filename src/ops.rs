//! Orchestrate add / delete / update across adapters and filesystem.

use crate::adapters::command::CommandRunner;
use crate::adapters::gh_skill::GhSkillCli;
use crate::adapters::npx_skills::NpxSkillsCli;
use crate::model::{Agent, InstallSource, Scope, SkillRecord};
use anyhow::{anyhow, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

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

    // Prefer filesystem paths from inventory. `npx skills remove` can hang on
    // prompts/network and freezes the TUI event loop if awaited first.
    if !plan.paths.is_empty() {
        for path in &plan.paths {
            match remove_skill_path(path) {
                Ok(()) => messages.push(format!("removed {}", path.display())),
                Err(err) => messages.push(format!("failed {}: {err}", path.display())),
            }
        }
        return Ok(messages);
    }

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
                }
            }
        }
    }

    if messages.is_empty() {
        return Err(anyhow!("nothing to delete for {}", plan.skill_name));
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

#[derive(Debug, Clone)]
pub struct UpdateJob {
    pub name: String,
    pub scope: Scope,
    pub dirs: Vec<PathBuf>,
}

pub fn execute_update(
    runner: &impl CommandRunner,
    backend: AddBackend,
    jobs: &[UpdateJob],
) -> Result<String> {
    match backend {
        AddBackend::GhSkill => execute_update_gh(runner, jobs),
        AddBackend::NpxSkills => execute_update_npx(runner, jobs),
    }
}

fn execute_update_npx(runner: &impl CommandRunner, jobs: &[UpdateJob]) -> Result<String> {
    let cli = NpxSkillsCli { runner };
    let mut msgs = Vec::new();
    for scope in [Scope::User, Scope::Project] {
        let names: Vec<&str> = jobs
            .iter()
            .filter(|j| j.scope == scope)
            .map(|j| j.name.as_str())
            .collect();
        if names.is_empty() {
            continue;
        }
        let out = cli.update(&names, scope)?;
        msgs.push(format!(
            "npx skills update ({scope})\n{}\n{}",
            out.stdout.trim(),
            out.stderr.trim()
        ));
    }
    if msgs.is_empty() {
        return Err(anyhow!("no skills to update via npx"));
    }
    Ok(msgs.join("\n\n"))
}

fn execute_update_gh(runner: &impl CommandRunner, jobs: &[UpdateJob]) -> Result<String> {
    let cli = GhSkillCli { runner };
    let mut msgs = Vec::new();
    for job in jobs {
        let mut attempts: Vec<String> = Vec::new();
        let mut try_dirs: Vec<Option<&Path>> =
            job.dirs.iter().map(|d| Some(d.as_path())).collect();
        if try_dirs.is_empty() {
            try_dirs.push(None);
        }

        let mut done = false;
        for dir in try_dirs {
            match cli.update(&job.name, dir) {
                Ok(out) => {
                    let dir_note = dir
                        .map(|d| format!("\n(dir: {})", d.display()))
                        .unwrap_or_default();
                    let body = format!(
                        "gh skill update {}{dir_note}\n{}\n{}",
                        job.name,
                        out.stdout.trim(),
                        out.stderr.trim()
                    );
                    let missing = body.contains("none of the specified skills are installed");
                    if missing {
                        attempts.push(body);
                        continue;
                    }
                    msgs.push(body);
                    done = true;
                    break;
                }
                Err(err) => {
                    attempts.push(format!(
                        "dir {}: {err}",
                        dir.map(|d| d.display().to_string())
                            .unwrap_or_else(|| "(default)".into())
                    ));
                }
            }
        }
        if !done {
            msgs.push(format!(
                "update {} failed after {} attempt(s):\n{}",
                job.name,
                attempts.len(),
                attempts.join("\n---\n")
            ));
        }
    }
    Ok(msgs.join("\n\n"))
}

/// Best-effort backend guess for the update picker.
pub fn suggested_update_backend(skill: &SkillRecord) -> Option<AddBackend> {
    if skill.source == InstallSource::Gh || skill_has_gh_metadata(skill) {
        return Some(AddBackend::GhSkill);
    }
    if skill.source == InstallSource::Npx {
        return Some(AddBackend::NpxSkills);
    }
    let only_agents = !skill.locations.is_empty()
        && skill.locations.iter().all(|l| {
            let p = l.resolved.as_ref().unwrap_or(&l.path);
            p.to_string_lossy().contains("/.agents/skills")
        });
    if only_agents {
        return Some(AddBackend::NpxSkills);
    }
    None
}

pub fn suggested_update_backend_for(skills: &[SkillRecord]) -> Option<AddBackend> {
    let mut iter = skills.iter().map(suggested_update_backend);
    let first = iter.next()??;
    for next in iter {
        match next {
            Some(b) if b == first => {}
            _ => return None,
        }
    }
    Some(first)
}

fn skill_has_gh_metadata(skill: &SkillRecord) -> bool {
    skill.locations.iter().any(|l| skill_path_has_github_metadata(&l.path))
}

/// Ordered skill-root dirs for `gh skill update --dir`, preferring copies
/// that already carry GitHub metadata in SKILL.md.
pub fn prefer_update_dirs(skill: &SkillRecord) -> Vec<PathBuf> {
    let score = |path: &Path| -> i32 {
        let s = path.to_string_lossy();
        let mut n = if s.contains("/.agents/skills") {
            0
        } else if s.contains("/.cursor/skills")
            || s.contains("/.claude/skills")
            || s.contains("/.codex/skills")
        {
            2
        } else {
            1
        };
        if skill_path_has_github_metadata(path) {
            n += 10;
        }
        n
    };
    let mut scored: Vec<(i32, PathBuf)> = skill
        .locations
        .iter()
        .filter_map(|l| {
            let parent = l.path.parent()?.to_path_buf();
            Some((score(&l.path), parent))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let mut out = Vec::new();
    for (_, dir) in scored {
        if !out.contains(&dir) {
            out.push(dir);
        }
    }
    out
}

fn skill_path_has_github_metadata(skill_dir: &Path) -> bool {
    let md = skill_dir.join("SKILL.md");
    let Ok(content) = fs::read_to_string(md) else {
        return false;
    };
    let lower = content.to_lowercase();
    lower.contains("github-repo:")
        || lower.contains("sourceurl:")
        || lower.contains("source_url:")
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

    #[test]
    fn suggested_backend_prefers_gh_metadata_over_npx_lock_source() {
        let tmp = tempfile::tempdir().unwrap();
        let cursor = tmp.path().join(".cursor/skills/tdd");
        fs::create_dir_all(&cursor).unwrap();
        fs::write(
            cursor.join("SKILL.md"),
            "---\nname: tdd\nmetadata:\n  github-repo: https://github.com/ex/skills\n  github-tree-sha: abc\n---\n",
        )
        .unwrap();
        let skill = SkillRecord {
            id: "tdd".into(),
            name: "tdd".into(),
            description: String::new(),
            scope: Scope::User,
            agents: vec![Agent::Cursor],
            locations: vec![SkillLocation {
                agent: Agent::Cursor,
                scope: Scope::User,
                path: cursor,
                kind: InstallKind::Copy,
                resolved: None,
            }],
            install_kind: InstallKind::Copy,
            source: InstallSource::Npx,
            source_url: Some("https://github.com/mattpocock/skills.git".into()),
            version: None,
            pinned: false,
            stats: SkillStats::default(),
        };
        assert_eq!(
            suggested_update_backend(&skill),
            Some(AddBackend::GhSkill)
        );
    }

    #[test]
    fn prefer_update_dirs_puts_metadata_host_first() {
        let tmp = tempfile::tempdir().unwrap();
        let claude = tmp.path().join(".claude/skills/tdd");
        let cursor = tmp.path().join(".cursor/skills/tdd");
        fs::create_dir_all(&claude).unwrap();
        fs::create_dir_all(&cursor).unwrap();
        fs::write(claude.join("SKILL.md"), "---\nname: tdd\n---\n").unwrap();
        fs::write(
            cursor.join("SKILL.md"),
            "---\nname: tdd\nmetadata:\n  github-repo: https://github.com/ex/skills\n---\n",
        )
        .unwrap();
        let skill = SkillRecord {
            id: "tdd".into(),
            name: "tdd".into(),
            description: String::new(),
            scope: Scope::User,
            agents: vec![Agent::ClaudeCode, Agent::Cursor],
            locations: vec![
                SkillLocation {
                    agent: Agent::ClaudeCode,
                    scope: Scope::User,
                    path: claude,
                    kind: InstallKind::Copy,
                    resolved: None,
                },
                SkillLocation {
                    agent: Agent::Cursor,
                    scope: Scope::User,
                    path: cursor.clone(),
                    kind: InstallKind::Copy,
                    resolved: None,
                },
            ],
            install_kind: InstallKind::Copy,
            source: InstallSource::Gh,
            source_url: Some("https://github.com/ex/skills".into()),
            version: None,
            pinned: false,
            stats: SkillStats::default(),
        };
        let dirs = prefer_update_dirs(&skill);
        assert_eq!(dirs[0], cursor.parent().unwrap());
    }
}
