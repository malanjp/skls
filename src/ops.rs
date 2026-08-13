//! Orchestrate add / delete / update across adapters and filesystem.

use crate::adapters::command::CommandRunner;
use crate::adapters::gh_skill::GhSkillCli;
use crate::adapters::npx_skills::NpxSkillsCli;
use crate::adapters::plugin_cli::PluginCli;
use crate::model::{
    Agent, InstallSource, PluginBackend, PluginRecord, Scope, SkillRecord, plugin_cli_agents,
};
use anyhow::{Context, Result, anyhow};
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
    pub project: Option<PathBuf>,
    pub agents: Vec<Agent>,
    pub paths: Vec<std::path::PathBuf>,
    pub source: InstallSource,
    pub shared_warning: Option<String>,
    pub plugin_warning: Option<String>,
}

/// Whether a path lives inside an agent plugin cache / store.
pub(crate) fn is_plugin_path(path: &Path) -> bool {
    let s = path.to_string_lossy();
    s.contains("/plugins/")
}

/// Shared store used by `npx skills` (and gh `universal`).
pub(crate) fn is_agents_skills_path(path: &Path) -> bool {
    let s = path.to_string_lossy();
    s.contains("/.agents/skills/") || s.ends_with("/.agents/skills")
}

pub fn plan_delete(skill: &SkillRecord, agents: &[Agent]) -> DeletePlan {
    let selected: Vec<_> = skill
        .locations
        .iter()
        .filter(|l| agents.contains(&l.agent))
        .cloned()
        .collect();
    let mut paths: Vec<_> = selected.iter().map(|l| l.path.clone()).collect();
    paths.sort();
    paths.dedup();
    let mut shared_warning = None;
    let mut plugin_warning = None;
    for loc in &selected {
        let candidates = [Some(loc.path.as_path()), loc.resolved.as_deref()];
        for path in candidates.into_iter().flatten() {
            let s = path.to_string_lossy();
            if s.contains("/.agents/skills/") {
                shared_warning = Some(format!(
                    "Removing shared store path may affect other agents: {s}"
                ));
                break;
            }
            if is_plugin_path(path) {
                plugin_warning = Some(format!(
                    "This path is inside an agent plugin; removing it may break the plugin install: {s}"
                ));
                break;
            }
        }
        if shared_warning.is_some() {
            break;
        }
    }
    DeletePlan {
        skill_name: skill.name.clone(),
        scope: skill.scope,
        project: skill.project.clone(),
        agents: agents.to_vec(),
        paths,
        source: skill.source,
        shared_warning,
        plugin_warning,
    }
}

pub fn execute_delete(
    runner: &impl CommandRunner,
    plan: &DeletePlan,
    npx_available: bool,
    active_root: &Path,
) -> Result<Vec<String>> {
    let mut messages = Vec::new();

    // Remove inventory paths first so the TUI stays responsive even if the
    // subsequent `npx skills remove` is slow.
    for path in &plan.paths {
        match remove_skill_path(path) {
            Ok(()) => messages.push(format!("removed {}", path.display())),
            Err(err) => messages.push(format!("failed {}: {err}", path.display())),
        }
    }

    // For npx-sourced skills, always run `npx skills remove` too so the lockfile
    // / shared store stay consistent (not only when inventory paths are empty).
    // Project-scope npx without `-g` mutates process cwd — skip when the plan
    // belongs to a different project than the active root.
    if npx_available && plan.source == InstallSource::Npx {
        match (plan.scope, plan.project.as_deref()) {
            (Scope::Project, Some(project))
                if !crate::config::paths_eq_canonical(project, active_root) =>
            {
                messages.push(format!(
                    "npx skills remove skipped for {}: project {} is not the active root",
                    plan.skill_name,
                    project.display()
                ));
            }
            _ if plan.agents.is_empty() => {
                messages.push(format!(
                    "npx skills remove skipped for {}: no agents selected",
                    plan.skill_name
                ));
            }
            _ => {
                let cli = NpxSkillsCli { runner };
                for agent in &plan.agents {
                    match cli.remove(&plan.skill_name, *agent, plan.scope) {
                        Ok(out) => {
                            let detail = out.stdout.trim();
                            if detail.is_empty() {
                                messages.push(format!(
                                    "npx skills remove {} @{}",
                                    plan.skill_name, agent
                                ));
                            } else {
                                messages.push(format!(
                                    "npx skills remove {} @{}: {detail}",
                                    plan.skill_name, agent
                                ));
                            }
                        }
                        Err(err) => {
                            messages.push(format!("npx remove failed for {agent}: {err}"));
                        }
                    }
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
    let meta = fs::symlink_metadata(path).with_context(|| format!("stat {}", path.display()))?;
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
    agents: &[Agent],
    scope: Scope,
) -> Result<String> {
    if agents.is_empty() {
        return Err(anyhow!("no agents selected"));
    }
    let mut msgs = Vec::new();
    for &agent in agents {
        let msg = match backend {
            AddBackend::GhSkill => {
                let cli = GhSkillCli { runner };
                let out = cli.install(package_or_repo, skill, agent, scope)?;
                format!(
                    "gh skill install ok ({agent})\n{}\n{}",
                    out.stdout.trim(),
                    out.stderr.trim()
                )
            }
            AddBackend::NpxSkills => {
                let cli = NpxSkillsCli { runner };
                let out = cli.add(package_or_repo, Some(skill), agent, scope)?;
                format!(
                    "npx skills add ok ({agent})\n{}\n{}",
                    out.stdout.trim(),
                    out.stderr.trim()
                )
            }
        };
        msgs.push(msg);
    }
    Ok(msgs.join("\n\n"))
}

#[derive(Debug, Clone)]
pub struct PluginDeletePlan {
    pub name: String,
    pub spec: String,
    pub scope: Scope,
    pub agents: Vec<Agent>,
    pub paths: Vec<PathBuf>,
}

pub fn plan_plugin_delete(plugin: &PluginRecord, agents: &[Agent]) -> PluginDeletePlan {
    let selected: Vec<_> = plugin
        .locations
        .iter()
        .filter(|l| agents.contains(&l.agent))
        .cloned()
        .collect();
    let mut paths: Vec<_> = selected.iter().map(|l| l.path.clone()).collect();
    paths.sort();
    paths.dedup();
    PluginDeletePlan {
        name: plugin.name.clone(),
        spec: plugin.spec.clone(),
        scope: plugin.scope,
        agents: agents.to_vec(),
        paths,
    }
}

pub fn execute_plugin_add(
    runner: &impl CommandRunner,
    spec: &str,
    agents: &[Agent],
    scope: Scope,
) -> Result<String> {
    if spec.trim().is_empty() {
        return Err(anyhow!("plugin spec is empty (use name@marketplace)"));
    }
    if agents.is_empty() {
        return Err(anyhow!("no agents selected"));
    }
    let cli = PluginCli { runner };
    let mut msgs = Vec::new();
    for &agent in agents {
        let Some(backend) = PluginBackend::from_agent(agent) else {
            msgs.push(format!(
                "{agent}: no plugin catalog CLI (install from the host marketplace)"
            ));
            continue;
        };
        match cli.install(backend, spec, scope) {
            Ok(out) => msgs.push(format!(
                "{} plugin install {spec} ok ({agent})\n{}\n{}",
                backend.as_str(),
                out.stdout.trim(),
                out.stderr.trim()
            )),
            Err(err) => msgs.push(format!("{agent}: {err}")),
        }
    }
    if msgs.is_empty() {
        return Err(anyhow!("no plugin CLI available for selected agents"));
    }
    Ok(msgs.join("\n\n"))
}

pub fn execute_plugin_update(
    runner: &impl CommandRunner,
    plugins: &[PluginRecord],
    agents: &[Agent],
) -> Result<String> {
    let cli = PluginCli { runner };
    let mut msgs = Vec::new();
    for plugin in plugins {
        let targets: Vec<Agent> = plugin
            .agents
            .iter()
            .copied()
            .filter(|a| agents.contains(a))
            .collect();
        if targets.is_empty() {
            continue;
        }
        for agent in targets {
            let Some(backend) = PluginBackend::from_agent(agent) else {
                msgs.push(format!("{} @ {agent}: no plugin catalog CLI", plugin.name));
                continue;
            };
            match cli.update(backend, &plugin.spec, plugin.scope) {
                Ok(out) => msgs.push(format!(
                    "{} plugin update {} ok ({agent})\n{}\n{}",
                    backend.as_str(),
                    plugin.spec,
                    out.stdout.trim(),
                    out.stderr.trim()
                )),
                Err(err) => msgs.push(format!("{} @ {agent}: {err}", plugin.name)),
            }
        }
    }
    if msgs.is_empty() {
        return Err(anyhow!("nothing to update"));
    }
    Ok(msgs.join("\n\n"))
}

pub fn execute_plugin_delete(
    runner: &impl CommandRunner,
    plan: &PluginDeletePlan,
) -> Result<Vec<String>> {
    let cli = PluginCli { runner };
    let mut messages = Vec::new();
    let mut cli_ok = false;
    for &agent in &plan.agents {
        let Some(backend) = PluginBackend::from_agent(agent) else {
            messages.push(format!(
                "{agent}: no plugin catalog CLI; skipped (path left in place)"
            ));
            continue;
        };
        match cli.uninstall(backend, &plan.spec, plan.scope) {
            Ok(out) => {
                cli_ok = true;
                messages.push(format!(
                    "{} plugin uninstall {} ok ({agent})\n{}\n{}",
                    backend.as_str(),
                    plan.spec,
                    out.stdout.trim(),
                    out.stderr.trim()
                ));
            }
            Err(err) => messages.push(format!("{agent}: {err}")),
        }
    }
    if !cli_ok {
        for path in &plan.paths {
            match remove_skill_path(path) {
                Ok(()) => messages.push(format!("removed {}", path.display())),
                Err(err) => messages.push(format!("failed {}: {err}", path.display())),
            }
        }
    }
    if messages.is_empty() {
        return Err(anyhow!("nothing to delete for {}", plan.name));
    }
    Ok(messages)
}

pub fn plugin_add_default_agents(claude: bool, copilot: bool, codex: bool) -> Vec<Agent> {
    plugin_cli_agents()
        .iter()
        .copied()
        .filter(|a| match a {
            Agent::ClaudeCode => claude,
            Agent::GitHubCopilot => copilot,
            Agent::Codex => codex,
            _ => false,
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct UpdateJob {
    pub name: String,
    pub scope: Scope,
    pub dirs: Vec<PathBuf>,
    pub project: Option<PathBuf>,
}

pub fn execute_update(
    runner: &impl CommandRunner,
    backend: AddBackend,
    jobs: &[UpdateJob],
    active_root: &Path,
) -> Result<String> {
    match backend {
        AddBackend::GhSkill => execute_update_gh(runner, jobs),
        AddBackend::NpxSkills => execute_update_npx(runner, jobs, active_root),
    }
}

fn execute_update_npx(
    runner: &impl CommandRunner,
    jobs: &[UpdateJob],
    active_root: &Path,
) -> Result<String> {
    let cli = NpxSkillsCli { runner };
    let mut msgs = Vec::new();
    let mut runnable = Vec::new();
    for job in jobs {
        if let (Scope::Project, Some(project)) = (job.scope, job.project.as_deref()) {
            if !crate::config::paths_eq_canonical(project, active_root) {
                msgs.push(format!(
                    "npx skills update skipped for {}: project {} is not the active root",
                    job.name,
                    project.display()
                ));
                continue;
            }
        }
        runnable.push(job);
    }
    for scope in [Scope::User, Scope::Project] {
        let names: Vec<&str> = runnable
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
        let mut try_dirs: Vec<Option<&Path>> = job.dirs.iter().map(|d| Some(d.as_path())).collect();
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
    if skill.source == InstallSource::Plugin {
        return None;
    }
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
    skill
        .locations
        .iter()
        .any(|l| crate::adapters::fs::skill_path_has_github_metadata(&l.path))
}

/// Ordered skill-root dirs for `gh skill update --dir`, preferring copies
/// that already carry GitHub metadata in SKILL.md.
///
/// Only locations whose agent is in `agents` are considered.
pub fn prefer_update_dirs(skill: &SkillRecord, agents: &[Agent]) -> Vec<PathBuf> {
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
        if crate::adapters::fs::skill_path_has_github_metadata(path) {
            n += 10;
        }
        n
    };
    let mut scored: Vec<(i32, PathBuf)> = skill
        .locations
        .iter()
        .filter(|l| agents.contains(&l.agent))
        .filter(|l| !is_plugin_path(&l.path))
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
            project: None,
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
            author: None,
            version: None,
            pinned: false,
            stats: SkillStats::default(),
        };
        let plan = plan_delete(&skill, &[Agent::Cursor]);
        assert!(plan.shared_warning.is_some());
        assert_eq!(plan.source, InstallSource::Manual);
    }

    #[test]
    fn execute_delete_calls_npx_even_when_paths_exist() {
        use crate::adapters::command::{CommandOutput, FakeCommandRunner};

        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("find-skills");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "---\nname: find-skills\n---\n").unwrap();

        let plan = DeletePlan {
            skill_name: "find-skills".into(),
            scope: Scope::User,
            project: None,
            agents: vec![Agent::Cursor, Agent::ClaudeCode],
            paths: vec![skill_dir.clone()],
            source: InstallSource::Npx,
            shared_warning: None,
            plugin_warning: None,
        };
        let runner = FakeCommandRunner::with_responses(vec![
            CommandOutput {
                status: 0,
                stdout: "ok".into(),
                stderr: String::new(),
            },
            CommandOutput {
                status: 0,
                stdout: "ok".into(),
                stderr: String::new(),
            },
        ]);

        let msgs = execute_delete(&runner, &plan, true, tmp.path()).unwrap();
        assert!(msgs.iter().any(|m| m.contains("removed")));
        assert!(!skill_dir.exists());

        let calls = runner.calls();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].0, "npx");
        assert!(calls[0].1.contains(&"remove".into()));
        assert!(calls[0].1.contains(&"find-skills".into()));
        assert!(calls[0].1.contains(&"-y".into()));
        assert!(calls[0].1.contains(&"cursor".into()));
        assert!(calls[1].1.contains(&"claude-code".into()));
    }

    #[test]
    fn execute_delete_skips_npx_for_non_npx_source() {
        use crate::adapters::command::{CommandOutput, FakeCommandRunner};

        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("manual");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "---\nname: manual\n---\n").unwrap();

        let plan = DeletePlan {
            skill_name: "manual".into(),
            scope: Scope::User,
            project: None,
            agents: vec![Agent::Cursor],
            paths: vec![skill_dir],
            source: InstallSource::Manual,
            shared_warning: None,
            plugin_warning: None,
        };
        let runner = FakeCommandRunner::with_responses(vec![CommandOutput {
            status: 0,
            stdout: String::new(),
            stderr: String::new(),
        }]);

        execute_delete(&runner, &plan, true, tmp.path()).unwrap();
        assert!(runner.calls().is_empty());
    }

    #[test]
    fn execute_delete_skips_npx_when_project_is_not_active_root() {
        use crate::adapters::command::FakeCommandRunner;

        let active = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let skill_dir = other.path().join("find-skills");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "---\nname: find-skills\n---\n").unwrap();

        let plan = DeletePlan {
            skill_name: "find-skills".into(),
            scope: Scope::Project,
            project: Some(other.path().to_path_buf()),
            agents: vec![Agent::Cursor],
            paths: vec![skill_dir.clone()],
            source: InstallSource::Npx,
            shared_warning: None,
            plugin_warning: None,
        };
        let runner =
            FakeCommandRunner::with_responses(vec![crate::adapters::command::CommandOutput {
                status: 0,
                stdout: "should-not-run".into(),
                stderr: String::new(),
            }]);

        let msgs = execute_delete(&runner, &plan, true, active.path()).unwrap();
        assert!(!skill_dir.exists());
        assert!(
            runner.calls().iter().all(|c| c.0 != "npx"),
            "npx must not run against the process cwd for another project"
        );
        assert!(msgs.iter().any(|m| m.contains("npx skills remove skipped")
            && m.contains(&other.path().display().to_string())));
    }

    #[test]
    fn execute_delete_calls_npx_when_project_matches_active_root() {
        use crate::adapters::command::{CommandOutput, FakeCommandRunner};

        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("find-skills");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "---\nname: find-skills\n---\n").unwrap();

        let plan = DeletePlan {
            skill_name: "find-skills".into(),
            scope: Scope::Project,
            project: Some(tmp.path().to_path_buf()),
            agents: vec![Agent::Cursor],
            paths: vec![skill_dir.clone()],
            source: InstallSource::Npx,
            shared_warning: None,
            plugin_warning: None,
        };
        let runner = FakeCommandRunner::with_responses(vec![CommandOutput {
            status: 0,
            stdout: "ok".into(),
            stderr: String::new(),
        }]);

        execute_delete(&runner, &plan, true, tmp.path()).unwrap();
        let calls = runner.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "npx");
        assert!(calls[0].1.contains(&"remove".into()));
        assert!(calls[0].1.contains(&"find-skills".into()));
        assert!(!skill_dir.exists());
    }

    #[test]
    fn execute_update_npx_skips_jobs_for_other_projects() {
        use crate::adapters::command::FakeCommandRunner;

        let active = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let jobs = vec![UpdateJob {
            name: "find-skills".into(),
            scope: Scope::Project,
            dirs: vec![],
            project: Some(other.path().to_path_buf()),
        }];
        let runner =
            FakeCommandRunner::with_responses(vec![crate::adapters::command::CommandOutput {
                status: 0,
                stdout: "should-not-run".into(),
                stderr: String::new(),
            }]);
        let msg = execute_update(&runner, AddBackend::NpxSkills, &jobs, active.path()).unwrap();
        assert!(
            runner.calls().iter().all(|c| c.0 != "npx"),
            "npx must not update the process cwd for another project"
        );
        assert!(msg.contains("skipped"));
        assert!(msg.contains(&other.path().display().to_string()));
    }

    #[test]
    fn execute_update_npx_runs_when_project_matches_active_root() {
        use crate::adapters::command::{CommandOutput, FakeCommandRunner};

        let tmp = tempfile::tempdir().unwrap();
        let jobs = vec![UpdateJob {
            name: "find-skills".into(),
            scope: Scope::Project,
            dirs: vec![],
            project: Some(tmp.path().to_path_buf()),
        }];
        let runner = FakeCommandRunner::with_responses(vec![CommandOutput {
            status: 0,
            stdout: "updated".into(),
            stderr: String::new(),
        }]);
        execute_update(&runner, AddBackend::NpxSkills, &jobs, tmp.path()).unwrap();
        let calls = runner.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "npx");
        assert!(calls[0].1.contains(&"update".into()));
        assert!(calls[0].1.contains(&"find-skills".into()));
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
            project: None,
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
            author: None,
            version: None,
            pinned: false,
            stats: SkillStats::default(),
        };
        assert_eq!(suggested_update_backend(&skill), Some(AddBackend::GhSkill));
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
            project: None,
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
            author: None,
            version: None,
            pinned: false,
            stats: SkillStats::default(),
        };
        let dirs = prefer_update_dirs(&skill, Agent::all());
        assert_eq!(dirs[0], cursor.parent().unwrap());
    }

    #[test]
    fn prefer_update_dirs_filters_by_agent() {
        let tmp = tempfile::tempdir().unwrap();
        let claude = tmp.path().join(".claude/skills/tdd");
        let cursor = tmp.path().join(".cursor/skills/tdd");
        fs::create_dir_all(&claude).unwrap();
        fs::create_dir_all(&cursor).unwrap();
        fs::write(claude.join("SKILL.md"), "---\nname: tdd\n---\n").unwrap();
        fs::write(cursor.join("SKILL.md"), "---\nname: tdd\n---\n").unwrap();
        let skill = SkillRecord {
            id: "tdd".into(),
            name: "tdd".into(),
            description: String::new(),
            scope: Scope::User,
            project: None,
            agents: vec![Agent::ClaudeCode, Agent::Cursor],
            locations: vec![
                SkillLocation {
                    agent: Agent::ClaudeCode,
                    scope: Scope::User,
                    path: claude.clone(),
                    kind: InstallKind::Copy,
                    resolved: None,
                },
                SkillLocation {
                    agent: Agent::Cursor,
                    scope: Scope::User,
                    path: cursor,
                    kind: InstallKind::Copy,
                    resolved: None,
                },
            ],
            install_kind: InstallKind::Copy,
            source: InstallSource::Gh,
            source_url: None,
            author: None,
            version: None,
            pinned: false,
            stats: SkillStats::default(),
        };
        let dirs = prefer_update_dirs(&skill, &[Agent::ClaudeCode]);
        assert_eq!(dirs, vec![claude.parent().unwrap().to_path_buf()]);
    }

    fn plugin_skill(source: InstallSource) -> SkillRecord {
        SkillRecord {
            id: "x".into(),
            name: "x".into(),
            description: String::new(),
            scope: Scope::User,
            project: None,
            agents: vec![Agent::ClaudeCode],
            locations: vec![SkillLocation {
                agent: Agent::ClaudeCode,
                scope: Scope::User,
                path: PathBuf::from(
                    "/home/.claude/plugins/cache/claude-plugins-official/sp/1.0.0/skills/x",
                ),
                kind: InstallKind::Copy,
                resolved: None,
            }],
            install_kind: InstallKind::Copy,
            source,
            source_url: None,
            author: None,
            version: None,
            pinned: false,
            stats: SkillStats::default(),
        }
    }

    #[test]
    fn plan_delete_warns_on_plugin_paths() {
        let plan = plan_delete(&plugin_skill(InstallSource::Plugin), &[Agent::ClaudeCode]);
        assert!(plan.plugin_warning.is_some());
        assert!(plan.plugin_warning.as_deref().unwrap().contains("plugin"));
    }

    #[test]
    fn plan_delete_does_not_warn_on_plugin_for_manual_path() {
        let plan = plan_delete(&plugin_skill(InstallSource::Manual), &[Agent::ClaudeCode]);
        assert!(
            plan.plugin_warning.is_some(),
            "detects by path regardless of source"
        );
    }

    #[test]
    fn suggested_backend_none_for_plugin_source() {
        assert_eq!(
            suggested_update_backend(&plugin_skill(InstallSource::Plugin)),
            None
        );
    }

    #[test]
    fn prefer_update_dirs_excludes_plugin_cache_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let claude = tmp.path().join(".claude/skills/x");
        fs::create_dir_all(&claude).unwrap();
        fs::write(claude.join("SKILL.md"), "---\nname: x\n---\n").unwrap();
        let mut skill = plugin_skill(InstallSource::Plugin);
        skill.locations.push(SkillLocation {
            agent: Agent::ClaudeCode,
            scope: Scope::User,
            path: claude.clone(),
            kind: InstallKind::Copy,
            resolved: None,
        });
        let dirs = prefer_update_dirs(&skill, &[Agent::ClaudeCode]);
        assert_eq!(dirs, vec![claude.parent().unwrap().to_path_buf()]);
    }

    #[test]
    fn execute_plugin_add_runs_claude_and_copilot() {
        use crate::adapters::command::{CommandOutput, FakeCommandRunner};
        let runner = FakeCommandRunner::with_responses(vec![
            CommandOutput {
                status: 0,
                stdout: "installed".into(),
                stderr: String::new(),
            },
            CommandOutput {
                status: 0,
                stdout: "ok".into(),
                stderr: String::new(),
            },
        ]);
        execute_plugin_add(
            &runner,
            "fmt@claude-plugins-official",
            &[Agent::ClaudeCode, Agent::GitHubCopilot],
            Scope::User,
        )
        .unwrap();
        let calls = runner.calls();
        assert_eq!(calls[0].0, "claude");
        assert_eq!(calls[1].0, "copilot");
    }

    #[test]
    fn execute_plugin_add_skips_cursor_without_cli() {
        use crate::adapters::command::FakeCommandRunner;
        let runner = FakeCommandRunner::default();
        let msg = execute_plugin_add(&runner, "x@m", &[Agent::Cursor], Scope::User).unwrap();
        assert!(msg.contains("no plugin catalog CLI"));
        assert!(runner.calls().is_empty());
    }

    fn sample_plugin_record(path: PathBuf) -> PluginRecord {
        PluginRecord {
            id: "fmt".into(),
            name: "fmt".into(),
            description: String::new(),
            version: None,
            author: None,
            marketplace: Some("m".into()),
            spec: "fmt@m".into(),
            agents: vec![Agent::ClaudeCode],
            locations: vec![SkillLocation {
                agent: Agent::ClaudeCode,
                scope: Scope::User,
                path,
                kind: InstallKind::Copy,
                resolved: None,
            }],
            skill_names: Vec::new(),
            mcp_names: Vec::new(),
            source_url: None,
            scope: Scope::User,
        }
    }

    #[test]
    fn execute_plugin_delete_prefers_cli_over_path() {
        use crate::adapters::command::{CommandOutput, FakeCommandRunner};
        let tmp = tempfile::tempdir().unwrap();
        let plugin_dir = tmp.path().join("fmt");
        fs::create_dir_all(&plugin_dir).unwrap();
        let plugin = sample_plugin_record(plugin_dir.clone());
        let plan = plan_plugin_delete(&plugin, &[Agent::ClaudeCode]);
        let runner = FakeCommandRunner::with_responses(vec![CommandOutput {
            status: 0,
            stdout: "uninstalled".into(),
            stderr: String::new(),
        }]);
        let msgs = execute_plugin_delete(&runner, &plan).unwrap();
        assert!(msgs.iter().any(|m| m.contains("uninstall")));
        assert!(plugin_dir.exists(), "CLI success should leave the path");
        assert_eq!(runner.calls()[0].0, "claude");
    }

    #[test]
    fn execute_plugin_delete_falls_back_to_path_when_cli_fails() {
        use crate::adapters::command::{CommandOutput, FakeCommandRunner};
        let tmp = tempfile::tempdir().unwrap();
        let plugin_dir = tmp.path().join("fmt");
        fs::create_dir_all(&plugin_dir).unwrap();
        let plugin = sample_plugin_record(plugin_dir.clone());
        let plan = plan_plugin_delete(&plugin, &[Agent::ClaudeCode]);
        let runner = FakeCommandRunner::with_responses(vec![CommandOutput {
            status: 1,
            stdout: String::new(),
            stderr: "nope".into(),
        }]);
        let msgs = execute_plugin_delete(&runner, &plan).unwrap();
        assert!(msgs.iter().any(|m| m.contains("removed")));
        assert!(!plugin_dir.exists());
    }
}
