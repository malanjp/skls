//! TUI application state machine.

use crate::adapters::command::SystemCommandRunner;
use crate::adapters::gh_skill::{GhSkillCli, GhSkillSearchItem};
use crate::analytics::{analyze_logs, apply_scores, apply_stats, LogPaths};
use crate::inventory::{build_inventory, InventoryOptions};
use crate::model::{Agent, Scope, SkillFilters, SkillRecord, SortKey};
use crate::ops::{
    execute_add, execute_delete, execute_update, plan_delete, AddBackend, DeletePlan,
};
use anyhow::Result;
use chrono::Utc;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    List,
    Search,
    Filter,
    Help,
    AddBackend,
    AddQuery,
    AddResults,
    AddAgent,
    AddScope,
    DeleteConfirm,
    Message,
}

pub struct App {
    pub project_root: PathBuf,
    pub home: PathBuf,
    pub skills: Vec<SkillRecord>,
    pub filtered_indices: Vec<usize>,
    pub selected: usize,
    /// Keeps the skill list scrolled so `selected` stays visible.
    pub list_state: ListState,
    pub add_list_state: ListState,
    pub filters: SkillFilters,
    pub sort_key: SortKey,
    pub mode: Mode,
    pub input: String,
    pub status: String,
    pub warnings: Vec<String>,
    pub window_days: i64,
    pub gh_available: bool,
    pub npx_available: bool,
    pub add_backend: AddBackend,
    pub add_query: String,
    pub add_results: Vec<GhSkillSearchItem>,
    pub add_result_idx: usize,
    pub add_package: String,
    pub add_skill: String,
    pub add_agent: Agent,
    pub add_scope: Scope,
    pub delete_plan: Option<DeletePlan>,
    pub delete_agent_idx: usize,
    pub message: String,
    pub should_quit: bool,
}

impl App {
    pub fn new(project_root: PathBuf, home: PathBuf) -> Self {
        let gh_available = crate::adapters::command::which_ok("gh");
        let npx_available = crate::adapters::command::which_ok("npx");
        Self {
            project_root,
            home,
            skills: Vec::new(),
            filtered_indices: Vec::new(),
            selected: 0,
            list_state: {
                let mut state = ListState::default();
                state.select(Some(0));
                state
            },
            add_list_state: ListState::default(),
            filters: SkillFilters::default(),
            sort_key: SortKey::Score,
            mode: Mode::List,
            input: String::new(),
            status: "loading...".into(),
            warnings: Vec::new(),
            window_days: 30,
            gh_available,
            npx_available,
            add_backend: if gh_available {
                AddBackend::GhSkill
            } else {
                AddBackend::NpxSkills
            },
            add_query: String::new(),
            add_results: Vec::new(),
            add_result_idx: 0,
            add_package: String::new(),
            add_skill: String::new(),
            add_agent: Agent::Cursor,
            add_scope: Scope::User,
            delete_plan: None,
            delete_agent_idx: 0,
            message: String::new(),
            should_quit: false,
        }
    }

    pub fn reload(&mut self) -> Result<()> {
        let runner = SystemCommandRunner;
        let opts = InventoryOptions {
            use_gh: self.gh_available,
        };
        let (mut skills, warnings) =
            build_inventory(&self.project_root, &self.home, &runner, &opts)?;
        self.warnings = warnings;

        let log_paths = LogPaths::from_home(&self.home);
        match analyze_logs(&log_paths, self.window_days, Utc::now()) {
            Ok(index) => {
                apply_stats(&mut skills, &index);
                apply_scores(&mut skills, Utc::now());
            }
            Err(err) => self
                .warnings
                .push(format!("log analysis failed: {err}")),
        }

        self.skills = skills;
        self.recompute_view();
        self.status = format!(
            "{} skills | gh:{} npx:{} | {}",
            self.skills.len(),
            if self.gh_available { "ok" } else { "missing" },
            if self.npx_available { "ok" } else { "missing" },
            self.project_root.display()
        );
        Ok(())
    }

    pub fn recompute_view(&mut self) {
        let mut idxs: Vec<usize> = self
            .skills
            .iter()
            .enumerate()
            .filter(|(_, s)| self.filters.matches(s))
            .map(|(i, _)| i)
            .collect();

        idxs.sort_by(|&a, &b| {
            let sa = &self.skills[a];
            let sb = &self.skills[b];
            match self.sort_key {
                SortKey::Name => sa.name.cmp(&sb.name),
                SortKey::Rate => {
                    let ra = sa.stats.activation_rate.unwrap_or(-1.0);
                    let rb = sb.stats.activation_rate.unwrap_or(-1.0);
                    rb.partial_cmp(&ra).unwrap_or(std::cmp::Ordering::Equal)
                }
                SortKey::Score => sb
                    .stats
                    .delete_score
                    .partial_cmp(&sa.stats.delete_score)
                    .unwrap_or(std::cmp::Ordering::Equal),
                SortKey::LastHit => sb.stats.last_hit_at.cmp(&sa.stats.last_hit_at),
            }
        });

        self.filtered_indices = idxs;
        if self.selected >= self.filtered_indices.len() {
            self.selected = self.filtered_indices.len().saturating_sub(1);
        }
        self.sync_list_state();
    }

    pub fn sync_list_state(&mut self) {
        if self.filtered_indices.is_empty() {
            self.list_state.select(None);
        } else {
            self.list_state.select(Some(self.selected));
        }
    }

    pub fn selected_skill(&self) -> Option<&SkillRecord> {
        self.filtered_indices
            .get(self.selected)
            .and_then(|&i| self.skills.get(i))
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return Ok(());
        }

        match self.mode {
            Mode::List => self.handle_list_key(key)?,
            Mode::Search => self.handle_search_key(key),
            Mode::Filter => self.handle_filter_key(key),
            Mode::Help | Mode::Message => {
                if matches!(
                    key.code,
                    KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') | KeyCode::Char('?')
                ) {
                    self.mode = Mode::List;
                }
            }
            Mode::AddBackend => self.handle_add_backend_key(key),
            Mode::AddQuery => self.handle_add_query_key(key)?,
            Mode::AddResults => self.handle_add_results_key(key),
            Mode::AddAgent => self.handle_add_agent_key(key),
            Mode::AddScope => self.handle_add_scope_key(key)?,
            Mode::DeleteConfirm => self.handle_delete_key(key)?,
        }
        Ok(())
    }

    fn handle_list_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('j') | KeyCode::Down => {
                if !self.filtered_indices.is_empty() {
                    self.selected = (self.selected + 1) % self.filtered_indices.len();
                    self.sync_list_state();
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if !self.filtered_indices.is_empty() {
                    self.selected = if self.selected == 0 {
                        self.filtered_indices.len() - 1
                    } else {
                        self.selected - 1
                    };
                    self.sync_list_state();
                }
            }
            KeyCode::Char('/') => {
                self.mode = Mode::Search;
                self.input = self.filters.query.clone();
            }
            KeyCode::Char('f') => self.mode = Mode::Filter,
            KeyCode::Char('s') => {
                self.sort_key = self.sort_key.next();
                self.recompute_view();
                self.status = format!("sort: {}", self.sort_key.as_str());
            }
            KeyCode::Char('r') => {
                self.reload()?;
                self.status = format!("{} reloaded", self.status);
            }
            KeyCode::Char('a') => {
                self.mode = Mode::AddBackend;
            }
            KeyCode::Char('d') => self.begin_delete(),
            KeyCode::Char('u') => self.run_update()?,
            KeyCode::Char('?') => self.mode = Mode::Help,
            KeyCode::Enter => {}
            _ => {}
        }
        Ok(())
    }

    fn handle_search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::List;
            }
            KeyCode::Enter => {
                self.filters.query = self.input.clone();
                self.recompute_view();
                self.mode = Mode::List;
            }
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Char(c) => self.input.push(c),
            _ => {}
        }
    }

    fn handle_filter_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => self.mode = Mode::List,
            KeyCode::Char('p') => {
                self.filters.scope = Some(Scope::Project);
                self.recompute_view();
            }
            KeyCode::Char('u') => {
                self.filters.scope = Some(Scope::User);
                self.recompute_view();
            }
            KeyCode::Char('a') => {
                self.filters.scope = None;
                self.recompute_view();
            }
            KeyCode::Char('1') => {
                self.filters.agents = vec![Agent::Cursor];
                self.recompute_view();
            }
            KeyCode::Char('2') => {
                self.filters.agents = vec![Agent::ClaudeCode];
                self.recompute_view();
            }
            KeyCode::Char('3') => {
                self.filters.agents = vec![Agent::Codex];
                self.recompute_view();
            }
            KeyCode::Char('0') => {
                self.filters.agents.clear();
                self.recompute_view();
            }
            KeyCode::Char('c') => {
                self.filters = SkillFilters::default();
                self.recompute_view();
            }
            _ => {}
        }
    }

    fn handle_add_backend_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.mode = Mode::List,
            KeyCode::Char('1') | KeyCode::Char('g') => {
                self.add_backend = AddBackend::GhSkill;
                self.mode = Mode::AddQuery;
                self.input.clear();
            }
            KeyCode::Char('2') | KeyCode::Char('n') => {
                self.add_backend = AddBackend::NpxSkills;
                self.mode = Mode::AddQuery;
                self.input.clear();
            }
            _ => {}
        }
    }

    fn handle_add_query_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => self.mode = Mode::List,
            KeyCode::Enter => {
                self.add_query = self.input.clone();
                self.run_add_search()?;
            }
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Char(c) => self.input.push(c),
            _ => {}
        }
        Ok(())
    }

    fn run_add_search(&mut self) -> Result<()> {
        let runner = SystemCommandRunner;
        match self.add_backend {
            AddBackend::GhSkill => {
                if !self.gh_available {
                    self.show_message("gh not available".into());
                    return Ok(());
                }
                let cli = GhSkillCli { runner: &runner };
                match cli.search(&self.add_query, 15) {
                    Ok(items) => {
                        self.add_results = items;
                        self.add_result_idx = 0;
                        if self.add_results.is_empty() {
                            self.add_list_state.select(None);
                            self.show_message("no search results".into());
                        } else {
                            self.add_list_state.select(Some(0));
                            self.mode = Mode::AddResults;
                        }
                    }
                    Err(err) => self.show_message(format!("search failed: {err}")),
                }
            }
            AddBackend::NpxSkills => {
                // For npx, treat query as owner/repo[@skill] style package ref.
                let query = self.add_query.clone();
                if let Some((pkg, skill)) = query.split_once('@') {
                    self.add_package = pkg.to_string();
                    self.add_skill = skill.to_string();
                } else {
                    self.add_package = query;
                    self.add_skill = String::new();
                }
                self.mode = Mode::AddAgent;
                self.status =
                    "npx: enter package as owner/repo or owner/repo@skill, then pick agent"
                        .into();
            }
        }
        Ok(())
    }

    fn handle_add_results_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.mode = Mode::List,
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.add_results.is_empty() {
                    self.add_result_idx =
                        (self.add_result_idx + 1) % self.add_results.len();
                    self.add_list_state.select(Some(self.add_result_idx));
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if !self.add_results.is_empty() {
                    self.add_result_idx = if self.add_result_idx == 0 {
                        self.add_results.len() - 1
                    } else {
                        self.add_result_idx - 1
                    };
                    self.add_list_state.select(Some(self.add_result_idx));
                }
            }
            KeyCode::Enter => {
                if let Some(item) = self.add_results.get(self.add_result_idx) {
                    self.add_package = item.repo.clone();
                    self.add_skill = item.skill_name.clone();
                    self.mode = Mode::AddAgent;
                }
            }
            _ => {}
        }
    }

    fn handle_add_agent_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.mode = Mode::List,
            KeyCode::Char('1') => {
                self.add_agent = Agent::Cursor;
                self.mode = Mode::AddScope;
            }
            KeyCode::Char('2') => {
                self.add_agent = Agent::ClaudeCode;
                self.mode = Mode::AddScope;
            }
            KeyCode::Char('3') => {
                self.add_agent = Agent::Codex;
                self.mode = Mode::AddScope;
            }
            _ => {}
        }
    }

    fn handle_add_scope_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => self.mode = Mode::List,
            KeyCode::Char('p') => {
                self.add_scope = Scope::Project;
                self.finish_add()?;
            }
            KeyCode::Char('u') => {
                self.add_scope = Scope::User;
                self.finish_add()?;
            }
            _ => {}
        }
        Ok(())
    }

    fn finish_add(&mut self) -> Result<()> {
        let runner = SystemCommandRunner;
        let skill = if self.add_skill.is_empty() {
            "*".to_string()
        } else {
            self.add_skill.clone()
        };
        match execute_add(
            &runner,
            self.add_backend,
            &self.add_package,
            &skill,
            self.add_agent,
            self.add_scope,
        ) {
            Ok(msg) => {
                let _ = self.reload();
                self.show_message(msg);
            }
            Err(err) => self.show_message(format!("add failed: {err}")),
        }
        Ok(())
    }

    fn begin_delete(&mut self) {
        let Some(skill) = self.selected_skill().cloned() else {
            return;
        };
        let agents = skill.agents.clone();
        if agents.is_empty() {
            self.show_message("no agent locations to delete".into());
            return;
        }
        self.delete_plan = Some(plan_delete(&skill, &agents));
        self.delete_agent_idx = 0;
        self.mode = Mode::DeleteConfirm;
    }

    fn handle_delete_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('n') => {
                self.delete_plan = None;
                self.mode = Mode::List;
            }
            KeyCode::Char('y') => {
                if let Some(plan) = self.delete_plan.clone() {
                    let runner = SystemCommandRunner;
                    match execute_delete(&runner, &plan, self.npx_available) {
                        Ok(msgs) => {
                            let _ = self.reload();
                            self.show_message(msgs.join("\n"));
                        }
                        Err(err) => self.show_message(format!("delete failed: {err}")),
                    }
                }
                self.delete_plan = None;
            }
            KeyCode::Char('1') => self.delete_subset(&[Agent::Cursor]),
            KeyCode::Char('2') => self.delete_subset(&[Agent::ClaudeCode]),
            KeyCode::Char('3') => self.delete_subset(&[Agent::Codex]),
            _ => {}
        }
        Ok(())
    }

    fn delete_subset(&mut self, agents: &[Agent]) {
        if let Some(skill) = self.selected_skill().cloned() {
            self.delete_plan = Some(plan_delete(&skill, agents));
        }
    }

    fn run_update(&mut self) -> Result<()> {
        let Some(skill) = self.selected_skill().cloned() else {
            return Ok(());
        };
        if skill.source_url.is_none() {
            self.show_message("no gh provenance; cannot update".into());
            return Ok(());
        }
        let runner = SystemCommandRunner;
        match execute_update(&runner, &skill.name) {
            Ok(msg) => {
                let _ = self.reload();
                self.show_message(msg);
            }
            Err(err) => self.show_message(format!("update failed: {err}")),
        }
        Ok(())
    }

    fn show_message(&mut self, msg: String) {
        self.message = msg;
        self.mode = Mode::Message;
    }
}

/// Thin test helper for filter/sort transitions without terminal.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{InstallKind, InstallSource, SkillStats};

    fn sample_app() -> App {
        let mut app = App::new("/tmp/proj".into(), "/tmp/home".into());
        app.skills = vec![
            SkillRecord {
                id: "a".into(),
                name: "alpha".into(),
                description: "A".into(),
                scope: Scope::User,
                agents: vec![Agent::Cursor],
                locations: vec![],
                install_kind: InstallKind::Copy,
                source: InstallSource::Manual,
                source_url: None,
                version: None,
                pinned: false,
                stats: SkillStats {
                    hits: 0,
                    sessions_total: 10,
                    last_hit_at: None,
                    activation_rate: Some(0.0),
                    delete_score: 80.0,
                },
            },
            SkillRecord {
                id: "b".into(),
                name: "beta".into(),
                description: "B".into(),
                scope: Scope::Project,
                agents: vec![Agent::Codex],
                locations: vec![],
                install_kind: InstallKind::Symlink,
                source: InstallSource::Gh,
                source_url: Some("https://x".into()),
                version: Some("v1".into()),
                pinned: false,
                stats: SkillStats {
                    hits: 5,
                    sessions_total: 10,
                    last_hit_at: None,
                    activation_rate: Some(0.5),
                    delete_score: 10.0,
                },
            },
        ];
        app.recompute_view();
        app
    }

    #[test]
    fn filter_scope_project() {
        let mut app = sample_app();
        app.filters.scope = Some(Scope::Project);
        app.recompute_view();
        assert_eq!(app.filtered_indices.len(), 1);
        assert_eq!(app.selected_skill().unwrap().name, "beta");
    }

    #[test]
    fn sort_by_score_puts_high_first() {
        let mut app = sample_app();
        app.sort_key = SortKey::Score;
        app.recompute_view();
        assert_eq!(app.selected_skill().unwrap().name, "alpha");
    }
}
