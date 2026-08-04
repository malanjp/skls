//! TUI application state machine.

use crate::adapters::command::SystemCommandRunner;
use crate::adapters::gh_skill::{GhSkillCli, GhSkillSearchItem};
use crate::analytics::{
    analyze_logs_with_limits, apply_scores, apply_stats, AnalyzeLimits, LogPaths,
};
use crate::inventory::{build_inventory, InventoryOptions};
use crate::model::{Agent, Scope, SkillFilters, SkillRecord, SortKey};
use crate::ops::{
    execute_add, plan_delete, prefer_update_dirs, suggested_update_backend_for, AddBackend,
    DeletePlan, UpdateJob,
};
use anyhow::Result;
use chrono::Utc;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;
use std::collections::HashSet;
use std::path::PathBuf;

/// Unique skill identity in the inventory (name id + scope).
pub type SkillKey = (String, Scope);

pub fn skill_key(skill: &SkillRecord) -> SkillKey {
    (skill.id.clone(), skill.scope)
}

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
    UpdateAgents,
    UpdateBackend,
    DeleteConfirm,
    Message,
    Busy,
}

#[derive(Debug, Clone)]
pub enum PendingAction {
    Delete(Vec<DeletePlan>),
    Update {
        backend: AddBackend,
        jobs: Vec<UpdateJob>,
    },
    /// Compute activation stats after the inventory is already on screen.
    AnalyzeActivations,
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
    pub analyze_limits: AnalyzeLimits,
    pub gh_available: bool,
    pub npx_available: bool,
    pub add_backend: AddBackend,
    /// Skills waiting for update agent / backend selection.
    pub update_skills: Vec<SkillRecord>,
    /// Targets waiting for update-backend selection.
    pub update_jobs: Vec<UpdateJob>,
    pub update_suggested: Option<AddBackend>,
    /// Agents selected for the pending update.
    pub update_agents: Vec<Agent>,
    pub add_query: String,
    pub add_results: Vec<GhSkillSearchItem>,
    pub add_result_idx: usize,
    pub add_package: String,
    pub add_skill: String,
    /// Agents selected for the pending add.
    pub add_agents: Vec<Agent>,
    pub add_scope: Scope,
    /// Skills queued for delete confirmation (supports multi-select).
    pub delete_skills: Vec<SkillRecord>,
    /// Agents selected for the pending delete (toggle; empty = nothing).
    pub delete_agents: Vec<Agent>,
    /// Cursor into the available-agents list for j/k + Space toggles.
    pub agent_focus: usize,
    /// Multi-select marks keyed by (id, scope).
    pub checked: HashSet<SkillKey>,
    pub message: String,
    pub busy_message: String,
    pub should_quit: bool,
    /// Work deferred until after the next redraw (keeps the TUI responsive).
    pub pending_action: Option<PendingAction>,
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
            analyze_limits: AnalyzeLimits::default(),
            gh_available,
            npx_available,
            add_backend: if gh_available {
                AddBackend::GhSkill
            } else {
                AddBackend::NpxSkills
            },
            update_skills: Vec::new(),
            update_jobs: Vec::new(),
            update_suggested: None,
            update_agents: Vec::new(),
            add_query: String::new(),
            add_results: Vec::new(),
            add_result_idx: 0,
            add_package: String::new(),
            add_skill: String::new(),
            add_agents: Agent::all().to_vec(),
            add_scope: Scope::User,
            delete_skills: Vec::new(),
            delete_agents: Vec::new(),
            agent_focus: 0,
            checked: HashSet::new(),
            message: String::new(),
            busy_message: String::new(),
            should_quit: false,
            pending_action: None,
        }
    }

    pub fn set_busy(&mut self, message: impl Into<String>) {
        self.busy_message = message.into();
        self.mode = Mode::Busy;
        self.status = self.busy_message.clone();
    }

    pub fn cancel_delete(&mut self) {
        self.delete_skills.clear();
        self.delete_agents.clear();
        self.mode = Mode::List;
        self.status = "delete cancelled".into();
    }

    pub fn delete_available_agents(&self) -> Vec<Agent> {
        union_agents(self.delete_skills.iter())
    }

    pub fn update_available_agents(&self) -> Vec<Agent> {
        union_agents(self.update_skills.iter())
    }

    pub fn is_checked(&self, skill: &SkillRecord) -> bool {
        self.checked.contains(&skill_key(skill))
    }

    pub fn checked_count(&self) -> usize {
        self.checked.len()
    }

    /// Skills targeted by delete/update: multi-select if any, else the cursor row.
    pub fn operation_targets(&self) -> Vec<SkillRecord> {
        if self.checked.is_empty() {
            return self.selected_skill().cloned().into_iter().collect();
        }
        self.skills
            .iter()
            .filter(|s| self.checked.contains(&skill_key(s)))
            .cloned()
            .collect()
    }

    pub fn delete_plans(&self) -> Vec<DeletePlan> {
        self.delete_skills
            .iter()
            .filter_map(|skill| {
                let agents: Vec<Agent> = skill
                    .agents
                    .iter()
                    .copied()
                    .filter(|a| self.delete_agents.contains(a))
                    .collect();
                if agents.is_empty() {
                    return None;
                }
                Some(plan_delete(skill, &agents))
            })
            .collect()
    }

    fn prune_checked(&mut self) {
        let live: HashSet<SkillKey> = self.skills.iter().map(skill_key).collect();
        self.checked.retain(|k| live.contains(k));
    }

    fn toggle_check_current(&mut self) {
        let Some(skill) = self.selected_skill() else {
            return;
        };
        let key = skill_key(skill);
        if !self.checked.remove(&key) {
            self.checked.insert(key);
        }
        self.status = format!("{} selected", self.checked.len());
    }

    fn select_all_visible(&mut self) {
        let keys: Vec<SkillKey> = self
            .filtered_indices
            .iter()
            .filter_map(|&i| self.skills.get(i).map(skill_key))
            .collect();
        let all_selected =
            !keys.is_empty() && keys.iter().all(|k| self.checked.contains(k));
        if all_selected {
            for k in keys {
                self.checked.remove(&k);
            }
            self.status = "selection cleared (visible)".into();
        } else {
            for k in keys {
                self.checked.insert(k);
            }
            self.status = format!("{} selected", self.checked.len());
        }
    }

    fn clear_checked(&mut self) {
        self.checked.clear();
        self.status = "selection cleared".into();
    }

    /// Full inventory + activation analysis (slow on large transcript trees).
    pub fn reload(&mut self) -> Result<()> {
        self.reload_with_options(true)
    }

    /// Fast refresh: rescan inventory, keep prior activation stats.
    pub fn reload_light(&mut self) -> Result<()> {
        self.reload_with_options(false)
    }

    /// Inventory only, then queue background-style activation analysis.
    pub fn bootstrap_fast(&mut self) -> Result<()> {
        self.reload_light()?;
        self.set_busy("Analyzing activations (recent sessions) …");
        self.pending_action = Some(PendingAction::AnalyzeActivations);
        Ok(())
    }

    pub fn analyze_activations(&mut self) -> Result<()> {
        let log_paths = LogPaths::from_home(&self.home);
        match analyze_logs_with_limits(
            &log_paths,
            self.window_days,
            Utc::now(),
            self.analyze_limits,
        ) {
            Ok(index) => {
                apply_stats(&mut self.skills, &index);
                apply_scores(&mut self.skills, Utc::now());
                self.recompute_view();
                let trunc = if index.truncated_files > 0 {
                    format!(" | sampled (-{} older)", index.truncated_files)
                } else {
                    String::new()
                };
                self.status = format!(
                    "{} skills | gh:{} npx:{} | activations ready{} | {}",
                    self.skills.len(),
                    if self.gh_available { "ok" } else { "missing" },
                    if self.npx_available { "ok" } else { "missing" },
                    trunc,
                    self.project_root.display()
                );
            }
            Err(err) => {
                self.warnings
                    .push(format!("log analysis failed: {err}"));
                self.status = format!("activation analysis failed: {err}");
            }
        }
        self.mode = Mode::List;
        Ok(())
    }

    fn reload_with_options(&mut self, analyze: bool) -> Result<()> {
        let previous_stats: std::collections::HashMap<(String, Scope), crate::model::SkillStats> =
            self.skills
                .iter()
                .map(|s| ((s.id.clone(), s.scope), s.stats.clone()))
                .collect();

        let runner = SystemCommandRunner;
        let opts = InventoryOptions {
            use_gh: self.gh_available,
        };
        let (mut skills, warnings) =
            build_inventory(&self.project_root, &self.home, &runner, &opts)?;
        self.warnings = warnings;

        if analyze {
            let log_paths = LogPaths::from_home(&self.home);
            match analyze_logs_with_limits(
                &log_paths,
                self.window_days,
                Utc::now(),
                self.analyze_limits,
            ) {
                Ok(index) => {
                    apply_stats(&mut skills, &index);
                    apply_scores(&mut skills, Utc::now());
                    if index.truncated_files > 0 {
                        self.warnings.push(format!(
                            "activation sample: skipped {} older session files",
                            index.truncated_files
                        ));
                    }
                }
                Err(err) => self
                    .warnings
                    .push(format!("log analysis failed: {err}")),
            }
        } else {
            for skill in &mut skills {
                if let Some(stats) = previous_stats.get(&(skill.id.clone(), skill.scope)) {
                    skill.stats = stats.clone();
                }
            }
            apply_scores(&mut skills, Utc::now());
        }

        self.skills = skills;
        self.prune_checked();
        self.recompute_view();
        let sel = if self.checked.is_empty() {
            String::new()
        } else {
            format!(" | {} selected", self.checked.len())
        };
        self.status = format!(
            "{} skills | gh:{} npx:{}{} | {}",
            self.skills.len(),
            if self.gh_available { "ok" } else { "missing" },
            if self.npx_available { "ok" } else { "missing" },
            sel,
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
            Mode::Busy => {
                // Ignore input while a blocking operation runs.
            }
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
            Mode::UpdateAgents => self.handle_update_agents_key(key),
            Mode::UpdateBackend => self.handle_update_backend_key(key),
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
                self.set_busy("Refreshing skill list …");
                self.reload_light()?;
                self.mode = Mode::List;
                self.status = format!("{} (light refresh; R = recompute activations)", self.status);
            }
            KeyCode::Char('R') => {
                self.set_busy("Recomputing activations …");
                self.pending_action = Some(PendingAction::AnalyzeActivations);
            }
            KeyCode::Char('a') => {
                self.mode = Mode::AddBackend;
            }
            KeyCode::Char(' ') => self.toggle_check_current(),
            KeyCode::Char('*') => self.select_all_visible(),
            KeyCode::Char('x') => self.clear_checked(),
            KeyCode::Char('d') => self.begin_delete(),
            KeyCode::Char('u') => self.begin_update(),
            KeyCode::Char('?') => self.mode = Mode::Help,
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

    fn cancel_add(&mut self) {
        self.input.clear();
        self.add_query.clear();
        self.add_results.clear();
        self.add_package.clear();
        self.add_skill.clear();
        self.add_agents = Agent::all().to_vec();
        self.mode = Mode::List;
        self.status = "add cancelled".into();
    }

    fn enter_add_agent(&mut self) {
        self.add_agents = Agent::all().to_vec();
        self.agent_focus = 0;
        self.mode = Mode::AddAgent;
    }

    fn handle_add_backend_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.cancel_add(),
            KeyCode::Char('1') | KeyCode::Char('g') => {
                if !self.gh_available {
                    self.status = "gh not available".into();
                    return;
                }
                self.add_backend = AddBackend::GhSkill;
                self.mode = Mode::AddQuery;
                self.input.clear();
            }
            KeyCode::Char('2') | KeyCode::Char('n') => {
                if !self.npx_available {
                    self.status = "npx not available".into();
                    return;
                }
                self.add_backend = AddBackend::NpxSkills;
                self.mode = Mode::AddQuery;
                self.input.clear();
            }
            _ => {}
        }
    }

    fn handle_add_query_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Char('q') if self.input.is_empty() => self.cancel_add(),
            KeyCode::Esc => {
                self.input.clear();
                self.mode = Mode::AddBackend;
            }
            KeyCode::Enter => {
                if self.input.trim().is_empty() {
                    self.status = "enter a source / query first".into();
                    return Ok(());
                }
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
                self.enter_add_agent();
                self.status = format!("追加中: {} → エージェント選択", self.add_package);
            }
        }
        Ok(())
    }

    fn handle_add_results_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.cancel_add(),
            KeyCode::Esc => {
                self.mode = Mode::AddQuery;
                self.input = self.add_query.clone();
            }
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
                    self.enter_add_agent();
                }
            }
            _ => {}
        }
    }

    fn handle_add_agent_key(&mut self, key: KeyEvent) {
        if handle_agent_list_keys(
            key,
            &mut self.add_agents,
            Agent::all(),
            &mut self.agent_focus,
        ) {
            return;
        }
        match key.code {
            KeyCode::Char('q') => self.cancel_add(),
            KeyCode::Esc => {
                // gh flow has a results step; npx goes straight from query.
                self.mode = if self.add_backend == AddBackend::GhSkill
                    && !self.add_results.is_empty()
                {
                    Mode::AddResults
                } else {
                    Mode::AddQuery
                };
                if self.mode == Mode::AddQuery {
                    self.input = self.add_query.clone();
                }
            }
            KeyCode::Enter => {
                if self.add_agents.is_empty() {
                    self.status = "select at least one agent".into();
                } else {
                    self.mode = Mode::AddScope;
                }
            }
            _ => {}
        }
    }

    fn handle_add_scope_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Char('q') => self.cancel_add(),
            KeyCode::Esc => self.mode = Mode::AddAgent,
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
            &self.add_agents,
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
        let targets = self.operation_targets();
        if targets.is_empty() {
            return;
        }
        let deletable: Vec<SkillRecord> = targets
            .into_iter()
            .filter(|s| !s.agents.is_empty())
            .collect();
        if deletable.is_empty() {
            self.show_message("no agent locations to delete".into());
            return;
        }
        self.delete_agents = union_agents(deletable.iter());
        self.delete_skills = deletable;
        self.agent_focus = 0;
        self.mode = Mode::DeleteConfirm;
    }

    fn handle_delete_key(&mut self, key: KeyEvent) -> Result<()> {
        let available = self.delete_available_agents();
        if handle_agent_list_keys(
            key,
            &mut self.delete_agents,
            &available,
            &mut self.agent_focus,
        ) {
            return Ok(());
        }
        match key.code {
            // Footer still advertises `q`; accept it as cancel too.
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Char('q') => {
                self.cancel_delete();
            }
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                let plans = self.delete_plans();
                self.delete_skills.clear();
                self.delete_agents.clear();
                if plans.is_empty() {
                    self.show_message("nothing to delete for selected agents".into());
                } else {
                    let label = if plans.len() == 1 {
                        format!("Deleting '{}' …", plans[0].skill_name)
                    } else {
                        format!("Deleting {} skills …", plans.len())
                    };
                    self.set_busy(label);
                    self.pending_action = Some(PendingAction::Delete(plans));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn begin_update(&mut self) {
        if !self.gh_available && !self.npx_available {
            self.show_message("neither gh nor npx available on PATH".into());
            return;
        }
        let targets = self.operation_targets();
        if targets.is_empty() {
            return;
        }
        let with_agents: Vec<SkillRecord> = targets
            .into_iter()
            .filter(|s| !s.agents.is_empty())
            .collect();
        if with_agents.is_empty() {
            self.show_message("no agent locations to update".into());
            return;
        }
        self.update_suggested = suggested_update_backend_for(&with_agents);
        self.update_agents = union_agents(with_agents.iter());
        self.update_skills = with_agents;
        self.update_jobs.clear();
        self.agent_focus = 0;
        self.mode = Mode::UpdateAgents;
    }

    fn cancel_update(&mut self) {
        self.update_skills.clear();
        self.update_jobs.clear();
        self.update_agents.clear();
        self.update_suggested = None;
        self.mode = Mode::List;
        self.status = "update cancelled".into();
    }

    fn handle_update_agents_key(&mut self, key: KeyEvent) {
        let available = self.update_available_agents();
        if handle_agent_list_keys(
            key,
            &mut self.update_agents,
            &available,
            &mut self.agent_focus,
        ) {
            return;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.cancel_update(),
            KeyCode::Enter => self.confirm_update_agents(),
            _ => {}
        }
    }

    fn confirm_update_agents(&mut self) {
        if self.update_agents.is_empty() {
            self.status = "select at least one agent".into();
            return;
        }
        let agents = self.update_agents.clone();
        self.update_jobs = self
            .update_skills
            .iter()
            .filter(|s| s.agents.iter().any(|a| agents.contains(a)))
            .map(|s| UpdateJob {
                name: s.name.clone(),
                scope: s.scope,
                dirs: prefer_update_dirs(s, &agents),
            })
            .collect();
        if self.update_jobs.is_empty() {
            self.status = "nothing to update for selected agents".into();
            return;
        }
        // If only one backend is installed, skip the picker.
        match (self.gh_available, self.npx_available) {
            (true, false) => self.queue_update(AddBackend::GhSkill),
            (false, true) => self.queue_update(AddBackend::NpxSkills),
            _ => self.mode = Mode::UpdateBackend,
        }
    }

    fn handle_update_backend_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.update_jobs.clear();
                self.mode = Mode::UpdateAgents;
            }
            KeyCode::Char('q') => self.cancel_update(),
            KeyCode::Char('1') | KeyCode::Char('g') => {
                if self.gh_available {
                    self.queue_update(AddBackend::GhSkill);
                } else {
                    self.status = "gh not available".into();
                }
            }
            KeyCode::Char('2') | KeyCode::Char('n') => {
                if self.npx_available {
                    self.queue_update(AddBackend::NpxSkills);
                } else {
                    self.status = "npx not available".into();
                }
            }
            KeyCode::Enter => {
                if let Some(backend) = self.update_suggested {
                    let ok = match backend {
                        AddBackend::GhSkill => self.gh_available,
                        AddBackend::NpxSkills => self.npx_available,
                    };
                    if ok {
                        self.queue_update(backend);
                    } else {
                        self.status = format!("{} not available", backend.as_str());
                    }
                }
            }
            _ => {}
        }
    }

    fn queue_update(&mut self, backend: AddBackend) {
        let jobs = std::mem::take(&mut self.update_jobs);
        self.update_skills.clear();
        self.update_agents.clear();
        self.update_suggested = None;
        if jobs.is_empty() {
            self.mode = Mode::List;
            return;
        }
        let label = if jobs.len() == 1 {
            format!("Updating '{}' via {} …", jobs[0].name, backend.as_str())
        } else {
            format!("Updating {} skills via {} …", jobs.len(), backend.as_str())
        };
        self.set_busy(label);
        self.pending_action = Some(PendingAction::Update { backend, jobs });
    }

    pub fn show_message(&mut self, msg: String) {
        self.message = crate::adapters::command::strip_ansi(&msg);
        self.mode = Mode::Message;
    }
}

fn toggle_agent(selected: &mut Vec<Agent>, agent: Agent) {
    if let Some(pos) = selected.iter().position(|a| *a == agent) {
        selected.remove(pos);
    } else {
        selected.push(agent);
        selected.sort_by_key(|a| a.as_str());
    }
}

fn select_all_agents(selected: &mut Vec<Agent>, available: &[Agent]) {
    selected.clear();
    selected.extend(available.iter().copied());
}

fn clear_all_agents(selected: &mut Vec<Agent>) {
    selected.clear();
}

fn clamp_agent_focus(focus: &mut usize, available: &[Agent]) {
    if available.is_empty() {
        *focus = 0;
    } else if *focus >= available.len() {
        *focus = available.len() - 1;
    }
}

fn move_agent_focus(focus: &mut usize, available: &[Agent], down: bool) {
    if available.is_empty() {
        *focus = 0;
        return;
    }
    clamp_agent_focus(focus, available);
    if down {
        *focus = (*focus + 1) % available.len();
    } else {
        *focus = if *focus == 0 {
            available.len() - 1
        } else {
            *focus - 1
        };
    }
}

/// Shared j/k / Space / * / x handling for agent checkbox lists.
/// Returns true when the key was consumed.
fn handle_agent_list_keys(
    key: KeyEvent,
    selected: &mut Vec<Agent>,
    available: &[Agent],
    focus: &mut usize,
) -> bool {
    clamp_agent_focus(focus, available);
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            move_agent_focus(focus, available, true);
            true
        }
        KeyCode::Char('k') | KeyCode::Up => {
            move_agent_focus(focus, available, false);
            true
        }
        KeyCode::Char(' ') => {
            if let Some(agent) = available.get(*focus).copied() {
                toggle_agent(selected, agent);
            }
            true
        }
        KeyCode::Char('*') => {
            select_all_agents(selected, available);
            true
        }
        KeyCode::Char('x') => {
            clear_all_agents(selected);
            true
        }
        _ => false,
    }
}

fn union_agents<'a>(skills: impl Iterator<Item = &'a SkillRecord>) -> Vec<Agent> {
    let mut agents = Vec::new();
    for skill in skills {
        for agent in &skill.agents {
            if !agents.contains(agent) {
                agents.push(*agent);
            }
        }
    }
    // Stable display order from Agent::all().
    Agent::all()
        .iter()
        .copied()
        .filter(|a| agents.contains(a))
        .collect()
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

    #[test]
    fn delete_modal_cancels_with_q() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = sample_app();
        app.skills[0].agents = vec![Agent::Cursor];
        app.skills[0].locations = vec![crate::model::SkillLocation {
            agent: Agent::Cursor,
            scope: Scope::User,
            path: std::path::PathBuf::from("/tmp/alpha"),
            kind: InstallKind::Copy,
            resolved: None,
        }];
        app.recompute_view();
        app.begin_delete();
        assert_eq!(app.mode, Mode::DeleteConfirm);
        app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.mode, Mode::List);
        assert!(app.delete_skills.is_empty());
    }

    #[test]
    fn delete_confirm_enters_busy() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = sample_app();
        app.skills[0].agents = vec![Agent::Cursor];
        app.skills[0].locations = vec![crate::model::SkillLocation {
            agent: Agent::Cursor,
            scope: Scope::User,
            path: std::path::PathBuf::from("/tmp/alpha-missing"),
            kind: InstallKind::Copy,
            resolved: None,
        }];
        app.recompute_view();
        app.begin_delete();
        app.handle_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE))
            .unwrap();
        assert!(matches!(app.pending_action, Some(PendingAction::Delete(_))));
        assert_eq!(app.mode, Mode::Busy);
        assert!(app.busy_message.contains("Deleting"));
    }

    #[test]
    fn multi_select_targets_checked_skills() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = sample_app();
        // Score sort puts alpha first.
        assert_eq!(app.selected_skill().unwrap().name, "alpha");
        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
            .unwrap();
        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
            .unwrap();
        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.checked_count(), 2);
        let targets = app.operation_targets();
        assert_eq!(targets.len(), 2);
        app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.checked_count(), 0);
        assert_eq!(app.operation_targets().len(), 1);
    }

    #[test]
    fn star_selects_all_visible() {
        let mut app = sample_app();
        app.select_all_visible();
        assert_eq!(app.checked_count(), 2);
        app.select_all_visible();
        assert_eq!(app.checked_count(), 0);
    }

    #[test]
    fn toggle_agent_adds_and_removes() {
        let mut selected = vec![Agent::Cursor];
        toggle_agent(&mut selected, Agent::Codex);
        assert_eq!(selected, vec![Agent::Codex, Agent::Cursor]);
        toggle_agent(&mut selected, Agent::Cursor);
        assert_eq!(selected, vec![Agent::Codex]);
    }

    #[test]
    fn delete_agent_toggle_narrows_plans() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = sample_app();
        app.skills[0].agents = vec![Agent::Cursor, Agent::ClaudeCode];
        app.skills[0].locations = vec![
            crate::model::SkillLocation {
                agent: Agent::Cursor,
                scope: Scope::User,
                path: std::path::PathBuf::from("/tmp/alpha-cursor"),
                kind: InstallKind::Copy,
                resolved: None,
            },
            crate::model::SkillLocation {
                agent: Agent::ClaudeCode,
                scope: Scope::User,
                path: std::path::PathBuf::from("/tmp/alpha-claude"),
                kind: InstallKind::Copy,
                resolved: None,
            },
        ];
        app.recompute_view();
        app.begin_delete();
        assert_eq!(app.delete_agents, vec![Agent::Cursor, Agent::ClaudeCode]);
        assert_eq!(app.delete_plans()[0].paths.len(), 2);
        // j moves to claude-code, Space unchecks it.
        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.agent_focus, 1);
        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.delete_agents, vec![Agent::Cursor]);
        let plan = &app.delete_plans()[0];
        assert_eq!(plan.agents, vec![Agent::Cursor]);
        assert_eq!(plan.paths.len(), 1);
    }

    #[test]
    fn add_agent_enter_requires_selection() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = sample_app();
        app.mode = Mode::AddAgent;
        app.add_agents.clear();
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.mode, Mode::AddAgent);
        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
            .unwrap();
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.mode, Mode::AddScope);
        assert_eq!(app.add_agents, vec![Agent::Cursor]);
    }

    #[test]
    fn agent_star_and_x_select_clear_all() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = sample_app();
        app.mode = Mode::AddAgent;
        app.add_agents.clear();
        app.handle_key(KeyEvent::new(KeyCode::Char('*'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.add_agents, Agent::all().to_vec());
        app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE))
            .unwrap();
        assert!(app.add_agents.is_empty());
    }

    #[test]
    fn update_starts_at_agent_selection() {
        let mut app = sample_app();
        app.gh_available = true;
        app.npx_available = true;
        app.skills[0].agents = vec![Agent::Cursor, Agent::Codex];
        app.skills[0].locations = vec![crate::model::SkillLocation {
            agent: Agent::Cursor,
            scope: Scope::User,
            path: std::path::PathBuf::from("/tmp/.cursor/skills/alpha"),
            kind: InstallKind::Copy,
            resolved: None,
        }];
        app.recompute_view();
        app.begin_update();
        assert_eq!(app.mode, Mode::UpdateAgents);
        assert_eq!(app.update_agents, vec![Agent::Cursor, Agent::Codex]);
        assert_eq!(app.agent_focus, 0);
    }

    #[test]
    fn agent_jk_space_toggles_focused() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = sample_app();
        app.mode = Mode::AddAgent;
        app.add_agents = Agent::all().to_vec();
        app.agent_focus = 0;
        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.agent_focus, 1);
        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
            .unwrap();
        assert!(!app.add_agents.contains(&Agent::ClaudeCode));
        app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.agent_focus, 0);
    }
}
