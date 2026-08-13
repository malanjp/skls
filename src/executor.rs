//! Deferred-operation executor.
//!
//! Owns the busy/message composition and reload policy for pending operations
//! (add / delete / update / analyze). Key handlers only submit
//! [`PendingAction`]s; this module decides how to run them and what to tell
//! the user. TUI specifics stay out — the composition root supplies a `redraw`
//! closure so phases can be painted, and tests can pass a no-op.

use crate::adapters::command::CommandRunner;
use crate::app::{App, PendingAction};
use crate::model::Agent;
use crate::model::Scope;
use crate::ops::{
    AddBackend, DeletePlan, PluginDeletePlan, UpdateJob, execute_add, execute_delete,
    execute_plugin_add, execute_plugin_delete, execute_plugin_update, execute_update,
};
use anyhow::Result;

/// Run a queued operation, redrawing between busy phases via `redraw`.
pub fn run_pending_action(
    app: &mut App,
    action: PendingAction,
    runner: &impl CommandRunner,
    redraw: &mut dyn FnMut(&mut App) -> Result<()>,
) -> Result<()> {
    match action {
        PendingAction::Delete(plans) => run_delete(app, plans, runner, redraw),
        PendingAction::Update { backend, jobs } => run_update(app, backend, jobs, runner, redraw),
        PendingAction::Add {
            backend,
            package,
            skill,
            agents,
            scope,
        } => run_add(
            app,
            AddRequest {
                backend,
                package,
                skill,
                agents,
                scope,
            },
            runner,
            redraw,
        ),
        PendingAction::PluginAdd {
            spec,
            agents,
            scope,
        } => run_plugin_add(app, spec, agents, scope, runner, redraw),
        PendingAction::PluginUpdate { plugins, agents } => {
            run_plugin_update(app, plugins, agents, runner, redraw)
        }
        PendingAction::PluginDelete(plans) => run_plugin_delete(app, plans, runner, redraw),
        PendingAction::AnalyzeActivations => {
            app.set_busy("Analyzing activations (recent sessions) …");
            redraw(app)?;
            app.analyze_activations()
        }
    }
}

/// What a pending add wants executed, bundled to keep the executor interface
/// narrow.
struct AddRequest {
    backend: AddBackend,
    package: String,
    skill: String,
    agents: Vec<Agent>,
    scope: Scope,
}

fn run_delete(
    app: &mut App,
    plans: Vec<DeletePlan>,
    runner: &impl CommandRunner,
    redraw: &mut dyn FnMut(&mut App) -> Result<()>,
) -> Result<()> {
    let label = if plans.len() == 1 {
        format!("Deleting '{}' …", plans[0].skill_name)
    } else {
        format!("Deleting {} skills …", plans.len())
    };
    app.set_busy(label);
    redraw(app)?;

    let mut msgs = Vec::new();
    let mut errors = Vec::new();
    for plan in &plans {
        match execute_delete(runner, plan, app.npx_available, &app.project_root) {
            Ok(m) => msgs.extend(m),
            Err(err) => errors.push(format!("{}: {err}", plan.skill_name)),
        }
    }
    // Inventory ids are skill directory names (== skill_name).
    for plan in &plans {
        app.checked
            .remove(&(plan.skill_name.clone(), plan.scope, plan.project.clone()));
    }

    app.set_busy("Refreshing skill list …");
    redraw(app)?;

    let refresh_result = app.reload_light();

    let mut body = msgs.join("\n");
    if !errors.is_empty() {
        if !body.is_empty() {
            body.push_str("\n\n");
        }
        body.push_str(&errors.join("\n"));
    }
    match refresh_result {
        Ok(()) => app.show_message(format!("{body}\n\n(Press R to recompute activation stats)")),
        Err(err) => app.show_message(format!("{body}\n\nrefresh failed: {err}")),
    }
    Ok(())
}

fn run_update(
    app: &mut App,
    backend: AddBackend,
    jobs: Vec<UpdateJob>,
    runner: &impl CommandRunner,
    redraw: &mut dyn FnMut(&mut App) -> Result<()>,
) -> Result<()> {
    let label = if jobs.len() == 1 {
        format!("Updating '{}' via {} …", jobs[0].name, backend.as_str())
    } else {
        format!("Updating {} skills via {} …", jobs.len(), backend.as_str())
    };
    app.set_busy(label);
    redraw(app)?;

    let msg = match execute_update(runner, backend, &jobs, &app.project_root) {
        Ok(m) => m,
        Err(err) => format!("update failed: {err}"),
    };

    app.set_busy("Refreshing skill list …");
    redraw(app)?;

    let msg = match app.reload_light() {
        Ok(()) => msg,
        Err(err) => format!("{msg}\n\nrefresh failed: {err}"),
    };
    app.show_message(msg);
    Ok(())
}

fn run_add(
    app: &mut App,
    req: AddRequest,
    runner: &impl CommandRunner,
    redraw: &mut dyn FnMut(&mut App) -> Result<()>,
) -> Result<()> {
    let label = if req.agents.len() == 1 {
        format!("Adding '{}' to {} …", req.skill, req.agents[0])
    } else {
        format!("Adding '{}' to {} agents …", req.skill, req.agents.len())
    };
    app.set_busy(label);
    redraw(app)?;

    match execute_add(
        runner,
        req.backend,
        &req.package,
        &req.skill,
        &req.agents,
        req.scope,
    ) {
        Ok(msg) => {
            app.set_busy("Refreshing skill list …");
            redraw(app)?;
            let msg = match app.reload() {
                Ok(()) => msg,
                Err(err) => format!("{msg}\n\nrefresh failed: {err}"),
            };
            app.show_message(msg);
        }
        Err(err) => app.show_message(format!("add failed: {err}")),
    }
    Ok(())
}

fn run_plugin_add(
    app: &mut App,
    spec: String,
    agents: Vec<Agent>,
    scope: Scope,
    runner: &impl CommandRunner,
    redraw: &mut dyn FnMut(&mut App) -> Result<()>,
) -> Result<()> {
    app.set_busy(format!("Installing plugin {spec} …"));
    redraw(app)?;
    match execute_plugin_add(runner, &spec, &agents, scope) {
        Ok(msg) => {
            app.set_busy("Refreshing inventory …");
            redraw(app)?;
            let msg = match app.reload_light() {
                Ok(()) => msg,
                Err(err) => format!("{msg}\n\nrefresh failed: {err}"),
            };
            app.show_message(msg);
        }
        Err(err) => app.show_message(format!("plugin add failed: {err}")),
    }
    Ok(())
}

fn run_plugin_update(
    app: &mut App,
    plugins: Vec<crate::model::PluginRecord>,
    agents: Vec<Agent>,
    runner: &impl CommandRunner,
    redraw: &mut dyn FnMut(&mut App) -> Result<()>,
) -> Result<()> {
    app.set_busy(format!("Updating {} plugin(s) …", plugins.len()));
    redraw(app)?;
    match execute_plugin_update(runner, &plugins, &agents) {
        Ok(msg) => {
            app.set_busy("Refreshing inventory …");
            redraw(app)?;
            let msg = match app.reload_light() {
                Ok(()) => msg,
                Err(err) => format!("{msg}\n\nrefresh failed: {err}"),
            };
            app.show_message(msg);
        }
        Err(err) => app.show_message(format!("plugin update failed: {err}")),
    }
    Ok(())
}

fn run_plugin_delete(
    app: &mut App,
    plans: Vec<PluginDeletePlan>,
    runner: &impl CommandRunner,
    redraw: &mut dyn FnMut(&mut App) -> Result<()>,
) -> Result<()> {
    app.set_busy(format!("Uninstalling {} plugin(s) …", plans.len()));
    redraw(app)?;
    let mut msgs = Vec::new();
    let mut errors = Vec::new();
    for plan in &plans {
        match execute_plugin_delete(runner, plan) {
            Ok(m) => msgs.extend(m),
            Err(err) => errors.push(format!("{}: {err}", plan.name)),
        }
        app.checked_plugins.remove(&(plan.name.clone(), plan.scope));
    }
    app.set_busy("Refreshing inventory …");
    redraw(app)?;
    let refresh_result = app.reload_light();
    let mut body = msgs.join("\n");
    if !errors.is_empty() {
        if !body.is_empty() {
            body.push_str("\n\n");
        }
        body.push_str(&errors.join("\n"));
    }
    match refresh_result {
        Ok(()) => app.show_message(body),
        Err(err) => app.show_message(format!("{body}\n\nrefresh failed: {err}")),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::command::{CommandOutput, FakeCommandRunner};
    use crate::model::{
        InstallKind, InstallSource, SkillKey, SkillLocation, SkillRecord, SkillStats,
    };
    use std::path::PathBuf;

    fn no_redraw(_app: &mut App) -> Result<()> {
        Ok(())
    }

    fn skill_record(name: &str, path: PathBuf) -> SkillRecord {
        SkillRecord {
            id: name.into(),
            name: name.into(),
            description: String::new(),
            scope: Scope::User,
            project: None,
            agents: vec![Agent::Cursor],
            locations: vec![SkillLocation {
                agent: Agent::Cursor,
                scope: Scope::User,
                path,
                kind: InstallKind::Copy,
                resolved: None,
            }],
            install_kind: InstallKind::Copy,
            source: InstallSource::Manual,
            source_url: None,
            author: None,
            version: None,
            pinned: false,
            stats: SkillStats::default(),
        }
    }

    #[test]
    fn delete_removes_paths_and_cleans_checked() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("alpha");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "---\nname: alpha\n---\n").unwrap();

        let mut app = App::new(tmp.path().join("proj"), tmp.path().to_path_buf());
        app.gh_available = false;
        app.npx_available = false;
        let rec = skill_record("alpha", skill_dir.clone());
        let key: SkillKey = rec.key();
        app.skills = vec![rec.clone()];
        app.checked.insert(key);
        app.recompute_view();

        let plan = crate::ops::plan_delete(&rec, &[Agent::Cursor]);
        let runner = FakeCommandRunner::default();
        run_pending_action(
            &mut app,
            PendingAction::Delete(vec![plan]),
            &runner,
            &mut no_redraw,
        )
        .unwrap();

        assert!(!skill_dir.exists());
        assert!(app.checked.is_empty());
        assert!(app.message.contains("removed"));
        assert_eq!(app.mode, crate::app::Mode::Message);
    }

    #[test]
    fn delete_unchecks_project_scoped_row() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("proj");
        let skill_dir = project.join(".agents/skills/alpha");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "---\nname: alpha\n---\n").unwrap();

        let mut app = App::new(project.clone(), tmp.path().join("home"));
        app.gh_available = false;
        app.npx_available = false;
        let mut rec = skill_record("alpha", skill_dir.clone());
        rec.scope = Scope::Project;
        rec.project = Some(project.clone());
        rec.locations[0].scope = Scope::Project;
        let key: SkillKey = rec.key();
        app.skills = vec![rec.clone()];
        app.checked.insert(key);
        app.recompute_view();

        let plan = crate::ops::plan_delete(&rec, &[Agent::Cursor]);
        let runner = FakeCommandRunner::default();
        let mut checked_during_refresh = None;
        run_pending_action(
            &mut app,
            PendingAction::Delete(vec![plan]),
            &runner,
            &mut |app| {
                if app.busy_message.contains("Refreshing") {
                    checked_during_refresh = Some(app.checked.clone());
                }
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(
            checked_during_refresh.as_ref().map(|c| c.is_empty()),
            Some(true),
            "project row must be unchecked before reload"
        );
        assert!(app.checked.is_empty());
    }

    #[test]
    fn add_runs_install_and_reloads() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = App::new(tmp.path().join("proj"), tmp.path().join("home"));
        app.gh_available = false;
        app.npx_available = false;
        let runner = FakeCommandRunner::with_responses(vec![CommandOutput {
            status: 0,
            stdout: "installed".into(),
            stderr: String::new(),
        }]);
        run_pending_action(
            &mut app,
            PendingAction::Add {
                backend: AddBackend::GhSkill,
                package: "mattpocock/skills".into(),
                skill: "*".into(),
                agents: vec![Agent::Cursor],
                scope: Scope::User,
            },
            &runner,
            &mut no_redraw,
        )
        .unwrap();
        let calls = runner.calls();
        assert_eq!(calls[0].0, "gh");
        assert_eq!(calls[0].1[0], "skill");
        assert_eq!(calls[0].1[1], "install");
        assert!(app.message.contains("installed"));
    }

    #[test]
    fn add_failure_reports_without_reload() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = App::new(tmp.path().join("proj"), tmp.path().join("home"));
        app.gh_available = false;
        app.npx_available = false;
        let runner = FakeCommandRunner::with_responses(vec![CommandOutput {
            status: 1,
            stdout: String::new(),
            stderr: "boom".into(),
        }]);
        run_pending_action(
            &mut app,
            PendingAction::Add {
                backend: AddBackend::GhSkill,
                package: "x/y".into(),
                skill: "*".into(),
                agents: vec![Agent::Cursor],
                scope: Scope::User,
            },
            &runner,
            &mut no_redraw,
        )
        .unwrap();
        assert!(app.message.contains("add failed"));
    }

    #[test]
    fn plugin_add_runs_catalog_cli() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = App::new(tmp.path().join("proj"), tmp.path().join("home"));
        let runner = FakeCommandRunner::with_responses(vec![CommandOutput {
            status: 0,
            stdout: "installed plugin".into(),
            stderr: String::new(),
        }]);
        run_pending_action(
            &mut app,
            PendingAction::PluginAdd {
                spec: "fmt@claude-plugins-official".into(),
                agents: vec![Agent::ClaudeCode],
                scope: Scope::User,
            },
            &runner,
            &mut no_redraw,
        )
        .unwrap();
        let calls = runner.calls();
        assert_eq!(calls[0].0, "claude");
        assert!(calls[0].1.contains(&"install".into()));
        assert!(app.message.contains("installed plugin"));
    }
}
