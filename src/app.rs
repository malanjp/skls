//! TUI application state machine.

use crate::adapters::command::SystemCommandRunner;
use crate::adapters::gh_skill::{GhSkillCli, GhSkillSearchItem};
use crate::analytics::{
    AnalyzeLimits, LogPaths, analyze_logs_with_limits, apply_scores, apply_stats,
};
use crate::inventory::{InventoryOptions, build_inventory};
use crate::model::{
    Agent, InstallSource, ListView, McpServerRecord, NavItem, PluginRecord, Scope, SkillFilters,
    SkillKey, SkillRecord, SortDir, SortKey, plugin_cli_agents,
};
use crate::ops::{
    AddBackend, DeletePlan, PluginDeletePlan, UpdateJob, plan_delete, plan_plugin_delete,
    plugin_add_default_agents, prefer_update_dirs, suggested_update_backend_for,
};
use anyhow::Result;
use chrono::Utc;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;
use std::collections::HashSet;
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
    Add {
        backend: AddBackend,
        package: String,
        skill: String,
        agents: Vec<Agent>,
        scope: Scope,
    },
    PluginAdd {
        spec: String,
        agents: Vec<Agent>,
        scope: Scope,
    },
    PluginUpdate {
        plugins: Vec<PluginRecord>,
        agents: Vec<Agent>,
    },
    PluginDelete(Vec<PluginDeletePlan>),
    /// Compute activation stats after the inventory is already on screen.
    AnalyzeActivations,
}

pub struct App {
    pub project_root: PathBuf,
    pub home: PathBuf,
    /// Project-scope scan roots (config list + active cwd, home excluded).
    pub scan_roots: Vec<PathBuf>,
    /// Number of projects listed in config (status `projects:N` uses scan_roots.len()).
    pub config_project_count: usize,
    /// Config/resolve warnings kept across inventory reloads.
    pub config_warnings: Vec<String>,
    pub skills: Vec<SkillRecord>,
    pub plugins: Vec<PluginRecord>,
    pub mcp_servers: Vec<McpServerRecord>,
    pub list_view: ListView,
    /// Left-sidebar category (manual / gh / npx / plugins / mcp).
    pub nav: NavItem,
    /// Which pane receives j/k: sidebar vs the item list.
    pub focus: FocusPane,
    pub filtered_indices: Vec<usize>,
    pub selected: usize,
    /// Keeps the skill list scrolled so `selected` stays visible.
    pub list_state: ListState,
    pub sidebar_state: ListState,
    pub add_list_state: ListState,
    pub filters: SkillFilters,
    pub sort_key: SortKey,
    pub sort_dir: SortDir,
    /// Visible list rows (inner height). Used by Ctrl+F / Ctrl+B paging.
    pub list_page_rows: usize,
    pub mode: Mode,
    pub input: String,
    pub status: String,
    pub warnings: Vec<String>,
    pub window_days: i64,
    pub analyze_limits: AnalyzeLimits,
    pub gh_available: bool,
    pub npx_available: bool,
    pub claude_available: bool,
    pub copilot_available: bool,
    pub codex_available: bool,
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
    /// Cached `DeletePlan`s for the confirm modal, recomputed on transition.
    pub delete_plans_cache: Vec<DeletePlan>,
    /// Cursor into the available-agents list for j/k + Space toggles.
    pub agent_focus: usize,
    /// Multi-select marks keyed by (id, scope, project).
    pub checked: HashSet<SkillKey>,
    pub checked_plugins: HashSet<(String, Scope)>,
    pub checked_mcp: HashSet<(String, Scope, String)>,
    pub delete_plugins: Vec<PluginRecord>,
    pub plugin_delete_plans_cache: Vec<PluginDeletePlan>,
    pub update_plugins: Vec<PluginRecord>,
    /// When true, the add flow installs a plugin catalog spec instead of a skill.
    pub add_plugin: bool,
    pub message: String,
    pub busy_message: String,
    pub should_quit: bool,
    /// Work deferred until after the next redraw (keeps the TUI responsive).
    pub pending_action: Option<PendingAction>,
    /// First `g` of a `gg` jump-to-top chord.
    pending_g: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusPane {
    Sidebar,
    List,
}

impl App {
    pub fn new(project_root: PathBuf, home: PathBuf) -> Self {
        let gh_available = crate::adapters::command::which_ok("gh");
        let npx_available = crate::adapters::command::which_ok("npx");
        Self {
            project_root,
            home,
            scan_roots: Vec::new(),
            config_project_count: 0,
            config_warnings: Vec::new(),
            skills: Vec::new(),
            plugins: Vec::new(),
            mcp_servers: Vec::new(),
            list_view: ListView::Skills,
            nav: NavItem::Manual,
            focus: FocusPane::List,
            filtered_indices: Vec::new(),
            selected: 0,
            list_state: {
                let mut state = ListState::default();
                state.select(Some(0));
                state
            },
            sidebar_state: {
                let mut state = ListState::default();
                state.select(Some(0));
                state
            },
            add_list_state: ListState::default(),
            filters: SkillFilters::default(),
            sort_key: SortKey::Score,
            sort_dir: SortDir::Desc,
            list_page_rows: 20,
            mode: Mode::List,
            input: String::new(),
            status: "loading...".into(),
            warnings: Vec::new(),
            window_days: 30,
            analyze_limits: AnalyzeLimits::default(),
            gh_available,
            npx_available,
            claude_available: crate::adapters::command::which_ok("claude"),
            copilot_available: crate::adapters::command::which_ok("copilot"),
            codex_available: crate::adapters::command::which_ok("codex"),
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
            add_agents: Agent::primary().to_vec(),
            add_scope: Scope::User,
            delete_skills: Vec::new(),
            delete_agents: Vec::new(),
            delete_plans_cache: Vec::new(),
            agent_focus: 0,
            checked: HashSet::new(),
            checked_plugins: HashSet::new(),
            checked_mcp: HashSet::new(),
            delete_plugins: Vec::new(),
            plugin_delete_plans_cache: Vec::new(),
            update_plugins: Vec::new(),
            add_plugin: false,
            message: String::new(),
            busy_message: String::new(),
            should_quit: false,
            pending_action: None,
            pending_g: false,
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
        self.delete_plans_cache.clear();
        self.delete_plugins.clear();
        self.plugin_delete_plans_cache.clear();
        self.mode = Mode::List;
        self.status = "delete cancelled".into();
    }

    pub fn delete_available_agents(&self) -> Vec<Agent> {
        if !self.delete_plugins.is_empty() {
            union_plugin_agents(self.delete_plugins.iter())
        } else {
            union_agents(self.delete_skills.iter())
        }
    }

    pub fn update_available_agents(&self) -> Vec<Agent> {
        if !self.update_plugins.is_empty() {
            union_plugin_agents(self.update_plugins.iter())
        } else {
            union_agents(self.update_skills.iter())
        }
    }

    pub fn is_checked(&self, skill: &SkillRecord) -> bool {
        self.checked.contains(&skill.key())
    }

    pub fn is_plugin_checked(&self, plugin: &PluginRecord) -> bool {
        self.checked_plugins.contains(&plugin.key())
    }

    pub fn is_mcp_checked(&self, mcp: &McpServerRecord) -> bool {
        self.checked_mcp.contains(&mcp.key())
    }

    pub fn checked_count(&self) -> usize {
        match self.list_view {
            ListView::Skills => self.checked.len(),
            ListView::Plugins => self.checked_plugins.len(),
            ListView::Mcp => self.checked_mcp.len(),
        }
    }

    /// Skills targeted by delete/update: multi-select if any, else the cursor row.
    pub fn operation_targets(&self) -> Vec<SkillRecord> {
        if self.checked.is_empty() {
            return self.selected_skill().cloned().into_iter().collect();
        }
        self.skills
            .iter()
            .filter(|s| self.checked.contains(&s.key()))
            .cloned()
            .collect()
    }

    /// Cached delete plans for the confirm modal (recomputed on state change).
    pub fn delete_plans(&self) -> &[DeletePlan] {
        &self.delete_plans_cache
    }

    fn refresh_delete_plans(&mut self) {
        if !self.delete_plugins.is_empty() {
            self.plugin_delete_plans_cache = self
                .delete_plugins
                .iter()
                .filter_map(|plugin| {
                    let agents: Vec<Agent> = plugin
                        .agents
                        .iter()
                        .copied()
                        .filter(|a| self.delete_agents.contains(a))
                        .collect();
                    if agents.is_empty() {
                        return None;
                    }
                    Some(plan_plugin_delete(plugin, &agents))
                })
                .collect();
            self.delete_plans_cache.clear();
            return;
        }
        self.delete_plans_cache = self
            .delete_skills
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
            .collect();
    }

    fn prune_checked(&mut self) {
        let live: HashSet<SkillKey> = self.skills.iter().map(SkillRecord::key).collect();
        self.checked.retain(|k| live.contains(k));
        let live_p: HashSet<(String, Scope)> = self.plugins.iter().map(PluginRecord::key).collect();
        self.checked_plugins.retain(|k| live_p.contains(k));
        let live_m: HashSet<(String, Scope, String)> =
            self.mcp_servers.iter().map(McpServerRecord::key).collect();
        self.checked_mcp.retain(|k| live_m.contains(k));
    }

    fn toggle_check_current(&mut self) {
        match self.list_view {
            ListView::Skills => {
                let Some(skill) = self.selected_skill() else {
                    return;
                };
                let key = skill.key();
                if !self.checked.remove(&key) {
                    self.checked.insert(key);
                }
            }
            ListView::Plugins => {
                let Some(plugin) = self.selected_plugin() else {
                    return;
                };
                let key = plugin.key();
                if !self.checked_plugins.remove(&key) {
                    self.checked_plugins.insert(key);
                }
            }
            ListView::Mcp => {
                let Some(mcp) = self.selected_mcp() else {
                    return;
                };
                let key = mcp.key();
                if !self.checked_mcp.remove(&key) {
                    self.checked_mcp.insert(key);
                }
            }
        }
        self.status = format!("{} selected", self.checked_count());
    }

    fn select_all_visible(&mut self) {
        match self.list_view {
            ListView::Skills => {
                let keys: Vec<SkillKey> = self
                    .filtered_indices
                    .iter()
                    .filter_map(|&i| self.skills.get(i).map(SkillRecord::key))
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
            ListView::Plugins => {
                let keys: Vec<(String, Scope)> = self
                    .filtered_indices
                    .iter()
                    .filter_map(|&i| self.plugins.get(i).map(PluginRecord::key))
                    .collect();
                let all_selected =
                    !keys.is_empty() && keys.iter().all(|k| self.checked_plugins.contains(k));
                if all_selected {
                    for k in keys {
                        self.checked_plugins.remove(&k);
                    }
                    self.status = "selection cleared (visible)".into();
                } else {
                    for k in keys {
                        self.checked_plugins.insert(k);
                    }
                    self.status = format!("{} selected", self.checked_plugins.len());
                }
            }
            ListView::Mcp => {
                let keys: Vec<(String, Scope, String)> = self
                    .filtered_indices
                    .iter()
                    .filter_map(|&i| self.mcp_servers.get(i).map(McpServerRecord::key))
                    .collect();
                let all_selected =
                    !keys.is_empty() && keys.iter().all(|k| self.checked_mcp.contains(k));
                if all_selected {
                    for k in keys {
                        self.checked_mcp.remove(&k);
                    }
                    self.status = "selection cleared (visible)".into();
                } else {
                    for k in keys {
                        self.checked_mcp.insert(k);
                    }
                    self.status = format!("{} selected", self.checked_mcp.len());
                }
            }
        }
    }

    fn clear_checked(&mut self) {
        match self.list_view {
            ListView::Skills => self.checked.clear(),
            ListView::Plugins => self.checked_plugins.clear(),
            ListView::Mcp => self.checked_mcp.clear(),
        }
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
                    "{} skills | gh:{} npx:{} | activations ready{} | {}{}",
                    self.skills.len(),
                    if self.gh_available { "ok" } else { "missing" },
                    if self.npx_available { "ok" } else { "missing" },
                    trunc,
                    self.project_root.display(),
                    self.projects_status_suffix()
                );
            }
            Err(err) => {
                self.warnings.push(format!("log analysis failed: {err}"));
                self.status = format!("activation analysis failed: {err}");
            }
        }
        self.mode = Mode::List;
        Ok(())
    }

    fn reload_with_options(&mut self, analyze: bool) -> Result<()> {
        let previous_stats: std::collections::HashMap<SkillKey, crate::model::SkillStats> = self
            .skills
            .iter()
            .map(|s| (s.key(), s.stats.clone()))
            .collect();

        let runner = SystemCommandRunner;
        let opts = InventoryOptions {
            use_gh: self.gh_available,
        };
        let inventory = build_inventory(&self.scan_roots, &self.home, &runner, &opts)?;
        let mut skills = inventory.skills;
        self.warnings = self.config_warnings.clone();
        self.warnings.extend(inventory.warnings);
        self.plugins = inventory.plugins;
        self.mcp_servers = inventory.mcp;

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
                Err(err) => self.warnings.push(format!("log analysis failed: {err}")),
            }
        } else {
            for skill in &mut skills {
                if let Some(stats) = previous_stats.get(&skill.key()) {
                    skill.stats = stats.clone();
                }
            }
            apply_scores(&mut skills, Utc::now());
        }

        self.skills = skills;
        self.prune_checked();
        self.recompute_view();
        let sel = if self.checked.is_empty() && self.checked_plugins.is_empty() {
            String::new()
        } else {
            format!(
                " | {} selected",
                self.checked.len() + self.checked_plugins.len()
            )
        };
        self.status = format!(
            "{} {} | {} plugins | {} mcp | gh:{} npx:{} claude:{} copilot:{} codex:{}{} | {}{}",
            self.skills.len(),
            self.nav.as_str(),
            self.plugins.len(),
            self.mcp_servers.len(),
            if self.gh_available { "ok" } else { "missing" },
            if self.npx_available { "ok" } else { "missing" },
            if self.claude_available {
                "ok"
            } else {
                "missing"
            },
            if self.copilot_available {
                "ok"
            } else {
                "missing"
            },
            if self.codex_available {
                "ok"
            } else {
                "missing"
            },
            sel,
            self.project_root.display(),
            self.projects_status_suffix()
        );
        Ok(())
    }

    fn projects_status_suffix(&self) -> String {
        if self.config_project_count > 0 {
            format!(" | projects:{}", self.scan_roots.len())
        } else {
            String::new()
        }
    }

    pub fn recompute_view(&mut self) {
        self.list_view = self.nav.list_view();
        self.sidebar_state.select(Some(self.nav.index()));
        let mut idxs: Vec<usize> = match self.list_view {
            ListView::Skills => self
                .skills
                .iter()
                .enumerate()
                .filter(|(_, s)| self.nav.matches_skill(s) && self.filters.matches(s))
                .map(|(i, _)| i)
                .collect(),
            ListView::Plugins => self
                .plugins
                .iter()
                .enumerate()
                .filter(|(_, p)| self.plugin_matches(p))
                .map(|(i, _)| i)
                .collect(),
            ListView::Mcp => self
                .mcp_servers
                .iter()
                .enumerate()
                .filter(|(_, m)| self.mcp_matches(m))
                .map(|(i, _)| i)
                .collect(),
        };

        match self.list_view {
            ListView::Skills => {
                idxs.sort_by(|&a, &b| {
                    let sa = &self.skills[a];
                    let sb = &self.skills[b];
                    let order = match self.sort_key {
                        SortKey::Name => sa.name.cmp(&sb.name),
                        SortKey::Rate => {
                            let ra = sa.stats.activation_rate.unwrap_or(-1.0);
                            let rb = sb.stats.activation_rate.unwrap_or(-1.0);
                            ra.partial_cmp(&rb).unwrap_or(std::cmp::Ordering::Equal)
                        }
                        SortKey::Score => sa
                            .stats
                            .delete_score
                            .partial_cmp(&sb.stats.delete_score)
                            .unwrap_or(std::cmp::Ordering::Equal),
                        SortKey::LastHit => sa.stats.last_hit_at.cmp(&sb.stats.last_hit_at),
                        SortKey::Author => return self.cmp_author(sa, sb),
                        SortKey::Source => Self::source_rank(sa.source)
                            .cmp(&Self::source_rank(sb.source))
                            .then_with(|| sa.name.cmp(&sb.name)),
                    };
                    self.apply_sort_dir(order)
                });
            }
            ListView::Plugins => {
                idxs.sort_by(|&a, &b| self.plugins[a].name.cmp(&self.plugins[b].name));
            }
            ListView::Mcp => {
                idxs.sort_by(|&a, &b| self.mcp_servers[a].name.cmp(&self.mcp_servers[b].name));
            }
        }

        self.filtered_indices = idxs;
        if self.selected >= self.filtered_indices.len() {
            self.selected = self.filtered_indices.len().saturating_sub(1);
        }
        self.sync_list_state();
    }

    fn plugin_matches(&self, plugin: &PluginRecord) -> bool {
        if let Some(scope) = self.filters.scope
            && plugin.scope != scope
        {
            return false;
        }
        if !self.filters.agents.is_empty()
            && !plugin
                .agents
                .iter()
                .any(|a| self.filters.agents.contains(a))
        {
            return false;
        }
        if !self.filters.query.is_empty() {
            let q = self.filters.query.to_lowercase();
            let hay = format!(
                "{} {} {}",
                plugin.name.to_lowercase(),
                plugin.description.to_lowercase(),
                plugin.spec.to_lowercase()
            );
            if !hay.contains(&q) {
                return false;
            }
        }
        true
    }

    fn mcp_matches(&self, mcp: &McpServerRecord) -> bool {
        if let Some(scope) = self.filters.scope
            && mcp.scope != scope
        {
            return false;
        }
        if !self.filters.agents.is_empty()
            && !mcp.agents.iter().any(|a| self.filters.agents.contains(a))
        {
            return false;
        }
        if !self.filters.query.is_empty() {
            let q = self.filters.query.to_lowercase();
            let hay = format!(
                "{} {} {}",
                mcp.name.to_lowercase(),
                mcp.plugin.as_deref().unwrap_or("").to_lowercase(),
                mcp.endpoint_label().to_lowercase()
            );
            if !hay.contains(&q) {
                return false;
            }
        }
        true
    }

    /// author sort keeps `None` (unknown) at the end in both directions.
    /// Direction applies only to the author name among known authors.
    fn cmp_author(&self, a: &SkillRecord, b: &SkillRecord) -> std::cmp::Ordering {
        match (&a.author, &b.author) {
            (Some(x), Some(y)) => self
                .apply_sort_dir(x.cmp(y))
                .then_with(|| a.name.cmp(&b.name)),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.name.cmp(&b.name),
        }
    }

    /// source sort order: managed installs first (gh → npx → plugin → manual).
    fn source_rank(s: InstallSource) -> u8 {
        match s {
            InstallSource::Gh => 0,
            InstallSource::Npx => 1,
            InstallSource::Plugin => 2,
            InstallSource::Manual => 3,
        }
    }

    fn apply_sort_dir(&self, asc: std::cmp::Ordering) -> std::cmp::Ordering {
        match self.sort_dir {
            SortDir::Asc => asc,
            SortDir::Desc => asc.reverse(),
        }
    }

    /// Header / status sort label. Plugins and MCP are always name ascending.
    pub fn displayed_sort(&self) -> (SortKey, SortDir) {
        match self.nav.list_view() {
            ListView::Skills => (self.sort_key, self.sort_dir),
            ListView::Plugins | ListView::Mcp => (SortKey::Name, SortDir::Asc),
        }
    }

    pub fn nav_count(&self, item: NavItem) -> usize {
        match item {
            NavItem::Manual | NavItem::Gh | NavItem::Npx => self
                .skills
                .iter()
                .filter(|s| item.matches_skill(s) && self.filters.matches(s))
                .count(),
            NavItem::Plugins => self
                .plugins
                .iter()
                .filter(|p| self.plugin_matches(p))
                .count(),
            NavItem::Mcp => self
                .mcp_servers
                .iter()
                .filter(|m| self.mcp_matches(m))
                .count(),
        }
    }

    pub fn apply_nav(&mut self, nav: NavItem) {
        self.nav = nav;
        self.list_view = nav.list_view();
        self.selected = 0;
        self.recompute_view();
        self.status = format!("view: {}", nav.as_str());
    }

    fn move_nav(&mut self, dir: i32) {
        let len = NavItem::ALL.len() as i32;
        let next = (self.nav.index() as i32 + dir).rem_euclid(len) as usize;
        self.apply_nav(NavItem::from_index(next));
    }

    fn cycle_nav(&mut self) {
        self.apply_nav(self.nav.next());
    }

    pub fn sync_list_state(&mut self) {
        if self.filtered_indices.is_empty() {
            self.list_state.select(None);
        } else {
            self.list_state.select(Some(self.selected));
        }
    }

    fn page_list(&mut self, dir: i32) {
        let len = self.filtered_indices.len();
        if len == 0 {
            return;
        }
        let step = self.list_page_rows.saturating_sub(1).max(1);
        let next = if dir > 0 {
            self.selected.saturating_add(step).min(len - 1)
        } else {
            self.selected.saturating_sub(step)
        };
        self.selected = next;
        self.sync_list_state();
    }

    fn jump_list_home(&mut self) {
        if self.filtered_indices.is_empty() {
            return;
        }
        self.selected = 0;
        self.sync_list_state();
    }

    fn jump_list_end(&mut self) {
        let len = self.filtered_indices.len();
        if len == 0 {
            return;
        }
        self.selected = len - 1;
        self.sync_list_state();
    }

    fn cycle_sort_key(&mut self) {
        if self.list_view != ListView::Skills {
            self.status = "sort key applies to skills view".into();
            return;
        }
        self.sort_key = self.sort_key.next();
        self.sort_dir = self.sort_key.default_dir();
        self.recompute_view();
        self.status = format!(
            "sort: {} {}",
            self.sort_key.as_str(),
            self.sort_dir.as_str()
        );
    }

    fn toggle_sort_dir(&mut self) {
        if self.list_view != ListView::Skills {
            self.status = "sort direction applies to skills view".into();
            return;
        }
        self.sort_dir = self.sort_dir.toggle();
        self.recompute_view();
        self.status = format!(
            "sort: {} {}",
            self.sort_key.as_str(),
            self.sort_dir.as_str()
        );
    }

    pub fn selected_skill(&self) -> Option<&SkillRecord> {
        if self.list_view != ListView::Skills {
            return None;
        }
        self.filtered_indices
            .get(self.selected)
            .and_then(|&i| self.skills.get(i))
    }

    pub fn selected_plugin(&self) -> Option<&PluginRecord> {
        if self.list_view != ListView::Plugins {
            return None;
        }
        self.filtered_indices
            .get(self.selected)
            .and_then(|&i| self.plugins.get(i))
    }

    pub fn selected_mcp(&self) -> Option<&McpServerRecord> {
        if self.list_view != ListView::Mcp {
            return None;
        }
        self.filtered_indices
            .get(self.selected)
            .and_then(|&i| self.mcp_servers.get(i))
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
            Mode::AddScope => self.handle_add_scope_key(key),
            Mode::UpdateAgents => self.handle_update_agents_key(key),
            Mode::UpdateBackend => self.handle_update_backend_key(key),
            Mode::DeleteConfirm => self.handle_delete_key(key)?,
        }
        Ok(())
    }

    fn handle_list_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            self.pending_g = false;
            if self.focus == FocusPane::Sidebar {
                return Ok(());
            }
            match key.code {
                KeyCode::Char('f' | 'F') => self.page_list(1),
                KeyCode::Char('b' | 'B') => self.page_list(-1),
                KeyCode::Char('l' | 'L') => self.jump_list_end(),
                _ => {}
            }
            return Ok(());
        }
        if matches!(key.code, KeyCode::Tab | KeyCode::BackTab) {
            self.pending_g = false;
            self.focus = match self.focus {
                FocusPane::Sidebar => FocusPane::List,
                FocusPane::List => FocusPane::Sidebar,
            };
            return Ok(());
        }
        if self.focus == FocusPane::Sidebar {
            return self.handle_sidebar_key(key);
        }
        if matches!(key.code, KeyCode::Char('g')) && !key.modifiers.contains(KeyModifiers::SHIFT) {
            if self.pending_g {
                self.pending_g = false;
                self.jump_list_home();
            } else {
                self.pending_g = true;
            }
            return Ok(());
        }
        self.pending_g = false;
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('h') | KeyCode::Left => {
                self.focus = FocusPane::Sidebar;
            }
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
            KeyCode::PageDown => self.page_list(1),
            KeyCode::PageUp => self.page_list(-1),
            KeyCode::Home => self.jump_list_home(),
            KeyCode::End => self.jump_list_end(),
            KeyCode::Char('L') => self.jump_list_end(),
            KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.jump_list_end();
            }
            KeyCode::Char('/') => {
                self.mode = Mode::Search;
                self.input = self.filters.query.clone();
            }
            KeyCode::Char('f') => self.mode = Mode::Filter,
            KeyCode::Char('S') => self.toggle_sort_dir(),
            KeyCode::Char('s') => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    self.toggle_sort_dir();
                } else {
                    self.cycle_sort_key();
                }
            }
            KeyCode::Char('r') => {
                self.set_busy("Refreshing skill list …");
                self.reload_light()?;
                self.mode = Mode::List;
                self.status = format!("{} (light refresh; R = recompute activations)", self.status);
            }
            KeyCode::Char('R') => {
                self.pending_action = Some(PendingAction::AnalyzeActivations);
            }
            KeyCode::Char('t') => self.cycle_nav(),
            KeyCode::Char(' ') => self.toggle_check_current(),
            KeyCode::Char('*') => self.select_all_visible(),
            KeyCode::Char('x') => self.clear_checked(),
            KeyCode::Char('a') => self.begin_add(),
            KeyCode::Char('d') => self.begin_delete(),
            KeyCode::Char('u') => self.begin_update(),
            KeyCode::Char('?') => self.mode = Mode::Help,
            _ => {}
        }
        Ok(())
    }

    fn handle_sidebar_key(&mut self, key: KeyEvent) -> Result<()> {
        if matches!(key.code, KeyCode::Char('g')) && !key.modifiers.contains(KeyModifiers::SHIFT) {
            if self.pending_g {
                self.pending_g = false;
                self.apply_nav(NavItem::Manual);
            } else {
                self.pending_g = true;
            }
            return Ok(());
        }
        self.pending_g = false;
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('j') | KeyCode::Down => self.move_nav(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_nav(-1),
            KeyCode::Char('t') => self.cycle_nav(),
            KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => {
                self.focus = FocusPane::List;
            }
            KeyCode::Char('h') | KeyCode::Left => {}
            KeyCode::Home => self.apply_nav(NavItem::Manual),
            KeyCode::End | KeyCode::Char('L') => self.apply_nav(NavItem::Mcp),
            KeyCode::Char('/') => {
                self.mode = Mode::Search;
                self.input = self.filters.query.clone();
            }
            KeyCode::Char('f') => self.mode = Mode::Filter,
            KeyCode::Char('S') => self.toggle_sort_dir(),
            KeyCode::Char('s') => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    self.toggle_sort_dir();
                } else {
                    self.cycle_sort_key();
                }
            }
            KeyCode::Char('r') => {
                self.set_busy("Refreshing skill list …");
                self.reload_light()?;
                self.mode = Mode::List;
                self.status = format!("{} (light refresh; R = recompute activations)", self.status);
            }
            KeyCode::Char('R') => {
                self.pending_action = Some(PendingAction::AnalyzeActivations);
            }
            KeyCode::Char('a') => self.begin_add(),
            KeyCode::Char('d') => self.begin_delete(),
            KeyCode::Char('u') => self.begin_update(),
            KeyCode::Char('*') => self.select_all_visible(),
            KeyCode::Char('x') => self.clear_checked(),
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
            KeyCode::Char('0') | KeyCode::Char('*') => {
                // Empty agents filter = show all.
                self.filters.agents.clear();
                self.agent_focus = 0;
                self.recompute_view();
            }
            KeyCode::Char('j') | KeyCode::Down => {
                move_agent_focus(&mut self.agent_focus, Agent::all(), true);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                move_agent_focus(&mut self.agent_focus, Agent::all(), false);
            }
            KeyCode::Char(' ') => {
                let available = Agent::all();
                clamp_agent_focus(&mut self.agent_focus, available);
                let Some(agent) = available.get(self.agent_focus).copied() else {
                    return;
                };
                if self.filters.agents.is_empty() {
                    // Narrow from "all" to the focused agent.
                    self.filters.agents = vec![agent];
                } else {
                    toggle_agent(&mut self.filters.agents, agent);
                    if self.filters.agents.is_empty()
                        || self.filters.agents.len() == available.len()
                    {
                        self.filters.agents.clear();
                    }
                }
                self.recompute_view();
            }
            KeyCode::Char('x') => {
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
        self.add_plugin = false;
        self.add_agents = Agent::primary().to_vec();
        self.mode = Mode::List;
        self.status = "add cancelled".into();
    }

    fn begin_add(&mut self) {
        match self.list_view {
            ListView::Mcp => {
                self.show_message(
                    "MCP servers are bundled in plugins. Select plugins in the sidebar to add from a catalog.".into(),
                );
            }
            ListView::Plugins => {
                if !self.claude_available && !self.copilot_available && !self.codex_available {
                    self.show_message(
                        "no plugin catalog CLI on PATH (claude / copilot / codex)".into(),
                    );
                    return;
                }
                self.add_plugin = true;
                self.add_agents = plugin_add_default_agents(
                    self.claude_available,
                    self.copilot_available,
                    self.codex_available,
                );
                self.mode = Mode::AddQuery;
                self.input.clear();
                self.status = "plugin spec: name@marketplace".into();
            }
            ListView::Skills => {
                self.add_plugin = false;
                match self.nav {
                    NavItem::Gh if self.gh_available => {
                        self.add_backend = AddBackend::GhSkill;
                        self.mode = Mode::AddQuery;
                        self.input.clear();
                        self.status = "gh skill: search keywords".into();
                    }
                    NavItem::Npx if self.npx_available => {
                        self.add_backend = AddBackend::NpxSkills;
                        self.mode = Mode::AddQuery;
                        self.input.clear();
                        self.status = "npx skills: owner/repo or owner/repo@skill".into();
                    }
                    _ => {
                        self.mode = Mode::AddBackend;
                    }
                }
            }
        }
    }

    fn enter_add_agent(&mut self) {
        if self.add_plugin {
            self.add_agents = plugin_add_default_agents(
                self.claude_available,
                self.copilot_available,
                self.codex_available,
            );
        } else {
            self.add_agents = Agent::primary().to_vec();
        }
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
                if self.add_plugin {
                    self.cancel_add();
                } else {
                    self.mode = Mode::AddBackend;
                }
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
        if self.add_plugin {
            self.add_package = self.add_query.trim().to_string();
            self.add_skill.clear();
            self.enter_add_agent();
            self.status = format!("plugin {} → エージェント選択", self.add_package);
            return Ok(());
        }
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
                    self.add_result_idx = (self.add_result_idx + 1) % self.add_results.len();
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
        let available: &[Agent] = if self.add_plugin {
            plugin_cli_agents()
        } else {
            Agent::all()
        };
        if handle_agent_list_keys(key, &mut self.add_agents, available, &mut self.agent_focus) {
            return;
        }
        match key.code {
            KeyCode::Char('q') => self.cancel_add(),
            KeyCode::Esc => {
                // gh flow has a results step; npx goes straight from query.
                self.mode =
                    if self.add_backend == AddBackend::GhSkill && !self.add_results.is_empty() {
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

    fn handle_add_scope_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.cancel_add(),
            KeyCode::Esc => self.mode = Mode::AddAgent,
            KeyCode::Char('p') => {
                if self.block_project_add_when_active_is_home() {
                    return;
                }
                self.add_scope = Scope::Project;
                self.finish_add();
            }
            KeyCode::Char('u') => {
                self.add_scope = Scope::User;
                self.finish_add();
            }
            _ => {}
        }
    }

    /// Queue the add as a pending operation; the executor runs the CLI, does the
    /// full reload, and composes the result message.
    fn finish_add(&mut self) {
        if self.add_scope == Scope::Project && self.block_project_add_when_active_is_home() {
            return;
        }
        let agents = std::mem::take(&mut self.add_agents);
        let scope = self.add_scope;
        let add_plugin = self.add_plugin;
        let package = std::mem::take(&mut self.add_package);
        let skill = if self.add_skill.is_empty() {
            "*".to_string()
        } else {
            std::mem::take(&mut self.add_skill)
        };
        let backend = self.add_backend;
        self.add_plugin = false;
        self.add_query.clear();
        self.add_results.clear();
        self.add_result_idx = 0;
        self.mode = Mode::List;
        if add_plugin {
            self.pending_action = Some(PendingAction::PluginAdd {
                spec: package,
                agents,
                scope,
            });
        } else {
            self.pending_action = Some(PendingAction::Add {
                backend,
                package,
                skill,
                agents,
                scope,
            });
        }
    }

    fn begin_delete(&mut self) {
        match self.list_view {
            ListView::Mcp => {
                let mcps = self.operation_mcp();
                if mcps.is_empty() {
                    return;
                }
                let mut plugins = Vec::new();
                let mut missing = Vec::new();
                let mut standalone = Vec::new();
                for mcp in &mcps {
                    let Some(plugin_name) = mcp.plugin.as_deref() else {
                        standalone.push(mcp.name.clone());
                        continue;
                    };
                    let found: Vec<PluginRecord> = self
                        .plugins
                        .iter()
                        .filter(|p| p.name == plugin_name || p.id == plugin_name)
                        .cloned()
                        .collect();
                    if found.is_empty() {
                        missing.push(format!("{} ({plugin_name})", mcp.name));
                    } else {
                        for plugin in found {
                            if !plugins
                                .iter()
                                .any(|p: &PluginRecord| p.key() == plugin.key())
                            {
                                plugins.push(plugin);
                            }
                        }
                    }
                }
                if plugins.is_empty() {
                    let mut msg = String::new();
                    if !standalone.is_empty() {
                        msg.push_str("standalone MCP configs are not deleted by skls");
                    }
                    if !missing.is_empty() {
                        if !msg.is_empty() {
                            msg.push('\n');
                        }
                        msg.push_str(&format!(
                            "parent plugin not in inventory: {}",
                            missing.join(", ")
                        ));
                    }
                    self.show_message(if msg.is_empty() {
                        "nothing to delete".into()
                    } else {
                        msg
                    });
                    return;
                }
                self.delete_plugins = plugins;
                self.delete_skills.clear();
                self.delete_agents = union_plugin_agents(self.delete_plugins.iter());
                self.agent_focus = 0;
                self.refresh_delete_plans();
                self.mode = Mode::DeleteConfirm;
                self.status =
                    "MCP servers are bundled in plugins — uninstall the parent plugin?".into();
            }
            ListView::Plugins => {
                let targets = self.operation_plugins();
                if targets.is_empty() {
                    return;
                }
                self.delete_plugins = targets;
                self.delete_skills.clear();
                self.delete_agents = union_plugin_agents(self.delete_plugins.iter());
                self.agent_focus = 0;
                self.refresh_delete_plans();
                self.mode = Mode::DeleteConfirm;
            }
            ListView::Skills => {
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
                self.delete_plugins.clear();
                self.delete_agents = union_agents(deletable.iter());
                self.delete_skills = deletable;
                self.agent_focus = 0;
                self.refresh_delete_plans();
                self.mode = Mode::DeleteConfirm;
            }
        }
    }

    fn operation_plugins(&self) -> Vec<PluginRecord> {
        if self.checked_plugins.is_empty() {
            return self.selected_plugin().cloned().into_iter().collect();
        }
        self.plugins
            .iter()
            .filter(|p| self.checked_plugins.contains(&p.key()))
            .cloned()
            .collect()
    }

    fn operation_mcp(&self) -> Vec<McpServerRecord> {
        if self.checked_mcp.is_empty() {
            return self.selected_mcp().cloned().into_iter().collect();
        }
        self.mcp_servers
            .iter()
            .filter(|m| self.checked_mcp.contains(&m.key()))
            .cloned()
            .collect()
    }

    fn handle_delete_key(&mut self, key: KeyEvent) -> Result<()> {
        let available = self.delete_available_agents();
        if handle_agent_list_keys(
            key,
            &mut self.delete_agents,
            &available,
            &mut self.agent_focus,
        ) {
            self.refresh_delete_plans();
            return Ok(());
        }
        match key.code {
            // Footer still advertises `q`; accept it as cancel too.
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Char('q') => {
                self.cancel_delete();
            }
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                if !self.delete_plugins.is_empty() {
                    let plans = std::mem::take(&mut self.plugin_delete_plans_cache);
                    self.delete_plugins.clear();
                    self.delete_skills.clear();
                    self.delete_agents.clear();
                    if plans.is_empty() {
                        self.show_message("nothing to delete for selected agents".into());
                    } else {
                        self.mode = Mode::List;
                        self.pending_action = Some(PendingAction::PluginDelete(plans));
                    }
                    return Ok(());
                }
                let plans = std::mem::take(&mut self.delete_plans_cache);
                self.delete_skills.clear();
                self.delete_agents.clear();
                if plans.is_empty() {
                    self.show_message("nothing to delete for selected agents".into());
                } else {
                    self.mode = Mode::List;
                    self.pending_action = Some(PendingAction::Delete(plans));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn begin_update(&mut self) {
        match self.list_view {
            ListView::Mcp => {
                self.show_message(
                    "Update MCP by updating the parent plugin (t → plugins, then u).".into(),
                );
            }
            ListView::Plugins => {
                if !self.claude_available && !self.copilot_available && !self.codex_available {
                    self.show_message(
                        "no plugin catalog CLI on PATH (claude / copilot / codex)".into(),
                    );
                    return;
                }
                let targets = self.operation_plugins();
                if targets.is_empty() {
                    return;
                }
                self.update_plugins = targets;
                self.update_skills.clear();
                self.update_agents = union_plugin_agents(self.update_plugins.iter());
                self.agent_focus = 0;
                self.mode = Mode::UpdateAgents;
            }
            ListView::Skills => {
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
                self.update_plugins.clear();
                self.update_suggested = suggested_update_backend_for(&with_agents);
                self.update_agents = union_agents(with_agents.iter());
                self.update_skills = with_agents;
                self.update_jobs.clear();
                self.agent_focus = 0;
                self.mode = Mode::UpdateAgents;
            }
        }
    }

    fn cancel_update(&mut self) {
        self.update_skills.clear();
        self.update_jobs.clear();
        self.update_agents.clear();
        self.update_plugins.clear();
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
        if !self.update_plugins.is_empty() {
            let plugins = std::mem::take(&mut self.update_plugins);
            let agents = std::mem::take(&mut self.update_agents);
            self.update_skills.clear();
            self.mode = Mode::List;
            self.pending_action = Some(PendingAction::PluginUpdate { plugins, agents });
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
                project: s.project.clone(),
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
        self.mode = Mode::List;
        self.pending_action = Some(PendingAction::Update { backend, jobs });
    }

    fn block_project_add_when_active_is_home(&mut self) -> bool {
        if crate::config::active_is_home(&self.project_root, &self.home) {
            self.show_message(
                "project scope needs a project directory; run from a repo or pass --project-root"
                    .into(),
            );
            true
        } else {
            false
        }
    }

    pub fn show_message(&mut self, msg: String) {
        self.message = crate::adapters::command::strip_ansi(&msg);
        self.mode = Mode::Message;
    }

    /// Current / total step numbers for the add wizard, owned by the state
    /// machine (rendering just displays them).
    pub fn add_wizard_step(&self) -> (u8, u8) {
        if self.add_plugin {
            let current = match self.mode {
                Mode::AddQuery => 1,
                Mode::AddAgent => 2,
                Mode::AddScope => 3,
                _ => 1,
            };
            return (current, 3);
        }
        let total = match self.add_backend {
            AddBackend::GhSkill => 5,
            AddBackend::NpxSkills => 4,
        };
        let current = match (self.mode, self.add_backend) {
            (Mode::AddBackend, _) => 1,
            (Mode::AddQuery, _) => 2,
            (Mode::AddResults, AddBackend::GhSkill) => 3,
            (Mode::AddAgent, AddBackend::GhSkill) => 4,
            (Mode::AddAgent, AddBackend::NpxSkills) => 3,
            (Mode::AddScope, AddBackend::GhSkill) => 5,
            (Mode::AddScope, AddBackend::NpxSkills) => 4,
            _ => 1,
        };
        (current, total)
    }

    pub fn dump_json_value(&self) -> serde_json::Value {
        serde_json::json!({
            "skills": self.skills.iter().map(|s| serde_json::json!({
                "id": s.id,
                "name": s.name,
                "scope": s.scope.as_str(),
                "project": s.project.as_ref().map(|p| p.to_string_lossy().into_owned()),
                "agents": s.agents.iter().map(|a| a.as_str()).collect::<Vec<_>>(),
                "source": s.source.as_str(),
                "author": s.author,
                "activation_rate": s.stats.activation_rate,
                "delete_score": s.stats.delete_score,
                "hits": s.stats.hits,
                "sessions_total": s.stats.sessions_total,
            })).collect::<Vec<_>>(),
            "plugins": self.plugins.iter().map(|p| serde_json::json!({
                "id": p.id,
                "name": p.name,
                "spec": p.spec,
                "scope": p.scope.as_str(),
                "agents": p.agents.iter().map(|a| a.as_str()).collect::<Vec<_>>(),
                "marketplace": p.marketplace,
                "author": p.author,
                "version": p.version,
                "skills": p.skill_names,
                "mcp": p.mcp_names,
            })).collect::<Vec<_>>(),
            "mcp_servers": self.mcp_servers.iter().map(|m| serde_json::json!({
                "id": m.id,
                "name": m.name,
                "transport": m.transport.as_str(),
                "command": m.command,
                "args": m.args,
                "url": m.url,
                "plugin": m.plugin,
                "scope": m.scope.as_str(),
                "agents": m.agents.iter().map(|a| a.as_str()).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        })
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
    Agent::all()
        .iter()
        .copied()
        .filter(|a| agents.contains(a))
        .collect()
}

fn union_plugin_agents<'a>(plugins: impl Iterator<Item = &'a PluginRecord>) -> Vec<Agent> {
    let mut agents = Vec::new();
    for plugin in plugins {
        for agent in &plugin.agents {
            if !agents.contains(agent) {
                agents.push(*agent);
            }
        }
    }
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
    use crate::model::{InstallKind, InstallSource, NavItem, SkillStats};

    fn sample_app() -> App {
        let mut app = App::new("/tmp/proj".into(), "/tmp/home".into());
        app.skills = vec![
            SkillRecord {
                id: "a".into(),
                name: "alpha".into(),
                description: "A".into(),
                scope: Scope::User,
                project: None,
                agents: vec![Agent::Cursor],
                locations: vec![],
                install_kind: InstallKind::Copy,
                source: InstallSource::Manual,
                source_url: None,
                author: None,
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
                project: None,
                agents: vec![Agent::Codex],
                locations: vec![],
                install_kind: InstallKind::Symlink,
                source: InstallSource::Manual,
                source_url: Some("https://x".into()),
                author: None,
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
    fn sort_by_author_orders_unknown_last() {
        let mut app = sample_app();
        app.skills[0].author = None;
        app.skills[1].author = Some("abc".into());
        app.sort_key = SortKey::Author;
        app.sort_dir = SortKey::Author.default_dir();
        app.recompute_view();
        let names: Vec<&str> = app
            .filtered_indices
            .iter()
            .map(|&i| app.skills[i].name.as_str())
            .collect();
        assert_eq!(names, vec!["beta", "alpha"]);
    }

    #[test]
    fn sort_by_author_desc_keeps_unknown_last() {
        let mut app = sample_app();
        app.skills[0].author = None;
        app.skills[1].author = Some("abc".into());
        let mut gamma = app.skills[1].clone();
        gamma.id = "c".into();
        gamma.name = "gamma".into();
        gamma.author = Some("zzz".into());
        app.skills.push(gamma);
        app.sort_key = SortKey::Author;
        app.sort_dir = SortDir::Desc;
        app.recompute_view();
        let names: Vec<&str> = app
            .filtered_indices
            .iter()
            .map(|&i| app.skills[i].name.as_str())
            .collect();
        assert_eq!(names, vec!["gamma", "beta", "alpha"]);
    }

    #[test]
    fn sort_by_source_ranks_managed_first() {
        let mut app = sample_app();
        app.skills[0].source = InstallSource::Plugin;
        app.skills[1].source = InstallSource::Manual;
        app.sort_key = SortKey::Source;
        app.sort_dir = SortKey::Source.default_dir();
        app.recompute_view();
        let names: Vec<&str> = app
            .filtered_indices
            .iter()
            .map(|&i| app.skills[i].name.as_str())
            .collect();
        assert_eq!(names, vec!["alpha", "beta"]);
    }

    #[test]
    fn sort_cycle_covers_all_keys() {
        let mut key = SortKey::Name;
        let mut seen = vec![key];
        for _ in 0..SortKey::Source as usize {
            key = key.next();
            seen.push(key);
        }
        assert_eq!(seen.len(), 6);
        assert!(seen.contains(&SortKey::Author));
        assert!(seen.contains(&SortKey::Source));
        assert_eq!(SortKey::Source.next(), SortKey::Name);
        assert_eq!(SortKey::Score.default_dir(), SortDir::Desc);
        assert_eq!(SortKey::Name.default_dir(), SortDir::Asc);
        assert_eq!(SortDir::Desc.toggle(), SortDir::Asc);
        assert_eq!(SortDir::Desc.marker(), "↓");
    }

    #[test]
    fn toggle_sort_dir_reverses_score_order() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = sample_app();
        assert_eq!(app.sort_key, SortKey::Score);
        assert_eq!(app.sort_dir, SortDir::Desc);
        assert_eq!(app.selected_skill().unwrap().name, "alpha");
        app.handle_key(KeyEvent::new(KeyCode::Char('S'), KeyModifiers::SHIFT))
            .unwrap();
        assert_eq!(app.sort_dir, SortDir::Asc);
        assert_eq!(app.mode, Mode::List);
        let names: Vec<&str> = app
            .filtered_indices
            .iter()
            .map(|&i| app.skills[i].name.as_str())
            .collect();
        assert_eq!(names, vec!["beta", "alpha"]);
    }

    #[test]
    fn cycle_sort_key_resets_direction_to_default() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = sample_app();
        app.sort_dir = SortDir::Asc;
        app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.sort_key, SortKey::LastHit);
        assert_eq!(app.sort_dir, SortDir::Desc);
        app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.sort_key, SortKey::Author);
        assert_eq!(app.sort_dir, SortDir::Asc);
    }

    fn named_skills_app(n: usize) -> App {
        let mut app = App::new("/tmp/proj".into(), "/tmp/home".into());
        app.skills = (0..n)
            .map(|i| SkillRecord {
                id: format!("s{i}"),
                name: format!("s{i:02}"),
                description: String::new(),
                scope: Scope::User,
                project: None,
                agents: vec![Agent::Cursor],
                locations: vec![],
                install_kind: InstallKind::Copy,
                source: InstallSource::Manual,
                source_url: None,
                author: None,
                version: None,
                pinned: false,
                stats: SkillStats {
                    hits: 0,
                    sessions_total: 0,
                    last_hit_at: None,
                    activation_rate: None,
                    delete_score: i as f64,
                },
            })
            .collect();
        app.sort_key = SortKey::Name;
        app.sort_dir = SortDir::Asc;
        app.list_page_rows = 5;
        app.recompute_view();
        app
    }

    #[test]
    fn ctrl_f_pages_down_without_opening_filter() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = named_skills_app(20);
        assert_eq!(app.selected_skill().unwrap().name, "s00");
        app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL))
            .unwrap();
        assert_eq!(app.mode, Mode::List);
        assert_eq!(app.selected_skill().unwrap().name, "s04");
        app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.mode, Mode::Filter);
    }

    #[test]
    fn page_keys_clamp_at_ends() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = named_skills_app(20);
        app.handle_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.selected, 0);
        app.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL))
            .unwrap();
        assert_eq!(app.selected, 0);
        for _ in 0..10 {
            app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE))
                .unwrap();
        }
        assert_eq!(app.selected, 19);
        app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL))
            .unwrap();
        assert_eq!(app.selected, 19);
    }

    #[test]
    fn gg_and_shift_l_jump_to_list_ends() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = named_skills_app(20);
        app.selected = 10;
        app.sync_list_state();
        app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.selected, 10);
        app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.mode, Mode::List);
        assert_eq!(app.selected, 0);
        app.selected = 5;
        app.sync_list_state();
        app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE))
            .unwrap();
        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.selected, 6);
        app.handle_key(KeyEvent::new(KeyCode::Char('L'), KeyModifiers::SHIFT))
            .unwrap();
        assert_eq!(app.selected, 19);
        app.selected = 3;
        app.sync_list_state();
        app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL))
            .unwrap();
        assert_eq!(app.selected, 19);
        app.selected = 8;
        app.sync_list_state();
        app.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::CONTROL))
            .unwrap();
        assert_eq!(app.selected, 8);
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
    fn delete_confirm_queues_pending_action() {
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
        assert!(app.delete_skills.is_empty());
        assert!(app.delete_plans_cache.is_empty());
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

    fn sample_plugin() -> PluginRecord {
        PluginRecord {
            id: "fmt".into(),
            name: "fmt".into(),
            description: "format".into(),
            version: Some("1.0".into()),
            author: None,
            marketplace: Some("claude-plugins-official".into()),
            spec: "fmt@claude-plugins-official".into(),
            agents: vec![Agent::ClaudeCode],
            locations: vec![crate::model::SkillLocation {
                agent: Agent::ClaudeCode,
                scope: Scope::User,
                path: std::path::PathBuf::from("/tmp/.claude/plugins/fmt"),
                kind: InstallKind::Copy,
                resolved: None,
            }],
            skill_names: vec!["fmt".into()],
            mcp_names: vec!["docs".into()],
            source_url: None,
            scope: Scope::User,
        }
    }

    #[test]
    fn t_cycles_sidebar_nav() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = sample_app();
        app.plugins = vec![sample_plugin()];
        app.mcp_servers = vec![crate::model::McpServerRecord {
            id: "docs".into(),
            name: "docs".into(),
            transport: crate::model::McpTransport::Stdio,
            command: Some("npx".into()),
            args: vec!["-y".into()],
            url: None,
            plugin: Some("fmt".into()),
            agents: vec![Agent::ClaudeCode],
            locations: vec![],
            scope: Scope::User,
        }];
        assert_eq!(app.nav, NavItem::Manual);
        app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.nav, NavItem::Gh);
        assert_eq!(app.list_view, ListView::Skills);
        app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.nav, NavItem::Npx);
        app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.nav, NavItem::Plugins);
        assert_eq!(app.list_view, ListView::Plugins);
        assert_eq!(app.selected_plugin().unwrap().name, "fmt");
        app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.nav, NavItem::Mcp);
        assert_eq!(app.list_view, ListView::Mcp);
        assert_eq!(app.selected_mcp().unwrap().name, "docs");
        app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.nav, NavItem::Manual);
        assert_eq!(app.list_view, ListView::Skills);
    }

    #[test]
    fn sidebar_jk_switches_nav_and_hl_moves_focus() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = sample_app();
        app.skills[1].source = InstallSource::Gh;
        app.recompute_view();
        assert_eq!(app.focus, FocusPane::List);
        assert_eq!(app.filtered_indices.len(), 1);
        assert_eq!(app.selected_skill().unwrap().name, "alpha");

        app.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.focus, FocusPane::Sidebar);
        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.nav, NavItem::Gh);
        assert_eq!(app.selected_skill().unwrap().name, "beta");
        app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.focus, FocusPane::List);
        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.selected_skill().unwrap().name, "beta");
    }

    #[test]
    fn nav_hides_other_sources() {
        let mut app = sample_app();
        app.skills[1].source = InstallSource::Npx;
        app.apply_nav(NavItem::Manual);
        let names: Vec<&str> = app
            .filtered_indices
            .iter()
            .map(|&i| app.skills[i].name.as_str())
            .collect();
        assert_eq!(names, vec!["alpha"]);
        app.apply_nav(NavItem::Npx);
        let names: Vec<&str> = app
            .filtered_indices
            .iter()
            .map(|&i| app.skills[i].name.as_str())
            .collect();
        assert_eq!(names, vec!["beta"]);
        assert_eq!(app.nav_count(NavItem::Manual), 1);
        assert_eq!(app.nav_count(NavItem::Npx), 1);
        assert_eq!(app.nav_count(NavItem::Gh), 0);
    }

    #[test]
    fn gh_nav_add_skips_backend_picker() {
        let mut app = sample_app();
        app.gh_available = true;
        app.apply_nav(NavItem::Gh);
        app.begin_add();
        assert_eq!(app.mode, Mode::AddQuery);
        assert_eq!(app.add_backend, AddBackend::GhSkill);
        assert!(!app.add_plugin);
    }

    #[test]
    fn plugins_and_mcp_display_name_asc_sort() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = sample_app();
        let mut zebra = sample_plugin();
        zebra.id = "zebra".into();
        zebra.name = "zebra".into();
        let mut apple = sample_plugin();
        apple.id = "apple".into();
        apple.name = "apple".into();
        app.plugins = vec![zebra, apple];
        app.mcp_servers = vec![
            crate::model::McpServerRecord {
                id: "zeta".into(),
                name: "zeta".into(),
                transport: crate::model::McpTransport::Stdio,
                command: Some("npx".into()),
                args: vec![],
                url: None,
                plugin: Some("fmt".into()),
                agents: vec![Agent::ClaudeCode],
                locations: vec![],
                scope: Scope::User,
            },
            crate::model::McpServerRecord {
                id: "alpha".into(),
                name: "alpha".into(),
                transport: crate::model::McpTransport::Stdio,
                command: Some("npx".into()),
                args: vec![],
                url: None,
                plugin: Some("fmt".into()),
                agents: vec![Agent::ClaudeCode],
                locations: vec![],
                scope: Scope::User,
            },
        ];
        app.sort_key = SortKey::Score;
        app.sort_dir = SortDir::Desc;
        app.apply_nav(NavItem::Plugins);
        assert_eq!(app.displayed_sort(), (SortKey::Name, SortDir::Asc));
        let plugin_names: Vec<&str> = app
            .filtered_indices
            .iter()
            .map(|&i| app.plugins[i].name.as_str())
            .collect();
        assert_eq!(plugin_names, vec!["apple", "zebra"]);
        app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.sort_key, SortKey::Score);
        app.apply_nav(NavItem::Mcp);
        assert_eq!(app.displayed_sort(), (SortKey::Name, SortDir::Asc));
        let mcp_names: Vec<&str> = app
            .filtered_indices
            .iter()
            .map(|&i| app.mcp_servers[i].name.as_str())
            .collect();
        assert_eq!(mcp_names, vec!["alpha", "zeta"]);
    }

    #[test]
    fn plugin_add_flow_queues_pending_action() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = sample_app();
        app.apply_nav(NavItem::Plugins);
        app.claude_available = true;
        app.copilot_available = false;
        app.codex_available = false;
        app.begin_add();
        assert_eq!(app.mode, Mode::AddQuery);
        assert!(app.add_plugin);
        for c in "fmt@claude-plugins-official".chars() {
            app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
                .unwrap();
        }
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.mode, Mode::AddAgent);
        assert_eq!(app.add_agents, vec![Agent::ClaudeCode]);
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.mode, Mode::AddScope);
        app.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE))
            .unwrap();
        match app.pending_action {
            Some(PendingAction::PluginAdd {
                spec,
                agents,
                scope,
            }) => {
                assert_eq!(spec, "fmt@claude-plugins-official");
                assert_eq!(agents, vec![Agent::ClaudeCode]);
                assert_eq!(scope, Scope::User);
            }
            other => panic!("unexpected pending action: {other:?}"),
        }
    }

    #[test]
    fn mcp_add_redirects_to_plugins_message() {
        let mut app = sample_app();
        app.apply_nav(NavItem::Mcp);
        app.begin_add();
        assert_eq!(app.mode, Mode::Message);
        assert!(app.message.contains("sidebar"));
    }

    #[test]
    fn plugin_delete_queues_pending_action() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = sample_app();
        app.plugins = vec![sample_plugin()];
        app.apply_nav(NavItem::Plugins);
        app.begin_delete();
        assert_eq!(app.mode, Mode::DeleteConfirm);
        assert!(!app.plugin_delete_plans_cache.is_empty());
        app.handle_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE))
            .unwrap();
        assert!(matches!(
            app.pending_action,
            Some(PendingAction::PluginDelete(_))
        ));
    }

    #[test]
    fn reload_keeps_config_warnings() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let proj = tmp.path().join("proj");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&proj).unwrap();
        let mut app = App::new(proj, home);
        app.scan_roots = vec![];
        app.config_warnings = vec!["skip relative project path: foo".into()];
        app.reload_light().unwrap();
        assert!(app.warnings.iter().any(|w| w.contains("relative")));
    }

    #[test]
    fn project_add_blocked_when_active_is_home() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let mut app = App::new(home.clone(), home.clone());
        app.add_backend = AddBackend::NpxSkills;
        app.add_package = "owner/repo".into();
        app.add_agents = vec![Agent::Cursor];
        app.add_scope = Scope::Project;
        app.finish_add();
        assert!(app.pending_action.is_none());
        assert!(
            app.message.contains("project")
                || app.status.contains("project")
                || app.message.contains("--project-root")
        );
    }

    #[test]
    fn dump_json_includes_project_field() {
        let mut app = sample_app();
        app.skills[1].project = Some(PathBuf::from("/tmp/proj"));
        let value = app.dump_json_value();
        assert_eq!(value["skills"][0]["project"], serde_json::Value::Null);
        assert_eq!(value["skills"][1]["project"], "/tmp/proj");
    }

    #[test]
    fn dump_json_includes_plugins_and_mcp() {
        let mut app = sample_app();
        app.plugins = vec![sample_plugin()];
        let value = app.dump_json_value();
        assert!(value.get("skills").and_then(|v| v.as_array()).is_some());
        assert_eq!(value["plugins"][0]["spec"], "fmt@claude-plugins-official");
        assert!(
            value
                .get("mcp_servers")
                .and_then(|v| v.as_array())
                .is_some()
        );
    }
}
