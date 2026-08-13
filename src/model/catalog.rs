//! Plugin packages and MCP servers shown alongside skills.

use crate::config::path_is_under;
use crate::model::{Agent, InstallSource, Scope, SkillLocation, SkillRecord, agents_label};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListView {
    Skills,
    Plugins,
    Mcp,
}

impl ListView {
    pub fn next(self) -> Self {
        match self {
            ListView::Skills => ListView::Plugins,
            ListView::Plugins => ListView::Mcp,
            ListView::Mcp => ListView::Skills,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ListView::Skills => "skills",
            ListView::Plugins => "plugins",
            ListView::Mcp => "mcp",
        }
    }
}

/// Left-sidebar categories. Skills are partitioned by install source so each
/// row lives in one place: manual (plus plugin-bundled), gh, or npx.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavItem {
    Manual,
    Gh,
    Npx,
    Plugins,
    Mcp,
}

impl NavItem {
    pub const ALL: [NavItem; 5] = [
        NavItem::Manual,
        NavItem::Gh,
        NavItem::Npx,
        NavItem::Plugins,
        NavItem::Mcp,
    ];

    pub fn next(self) -> Self {
        Self::from_index(self.index() + 1)
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|&item| item == self).unwrap_or(0)
    }

    pub fn from_index(i: usize) -> Self {
        Self::ALL[i % Self::ALL.len()]
    }

    pub fn as_str(self) -> &'static str {
        match self {
            NavItem::Manual => "manual",
            NavItem::Gh => "gh",
            NavItem::Npx => "npx",
            NavItem::Plugins => "plugins",
            NavItem::Mcp => "mcp",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            NavItem::Manual => "manual",
            NavItem::Gh => "gh",
            NavItem::Npx => "npx",
            NavItem::Plugins => "plugins",
            NavItem::Mcp => "mcp",
        }
    }

    pub fn list_view(self) -> ListView {
        match self {
            NavItem::Manual | NavItem::Gh | NavItem::Npx => ListView::Skills,
            NavItem::Plugins => ListView::Plugins,
            NavItem::Mcp => ListView::Mcp,
        }
    }

    /// Whether a skill belongs in this sidebar category.
    pub fn matches_skill(self, skill: &SkillRecord) -> bool {
        match self {
            NavItem::Manual => {
                matches!(skill.source, InstallSource::Manual | InstallSource::Plugin)
            }
            NavItem::Gh => skill.source == InstallSource::Gh,
            NavItem::Npx => skill.source == InstallSource::Npx,
            NavItem::Plugins | NavItem::Mcp => false,
        }
    }
}

/// One row in the left sidebar: a source category or a scan-root project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidebarSel {
    Source(NavItem),
    Project(usize),
}

impl SidebarSel {
    pub fn list_view(&self) -> ListView {
        match self {
            SidebarSel::Source(item) => item.list_view(),
            SidebarSel::Project(_) => ListView::Skills,
        }
    }

    pub fn matches_skill(&self, skill: &SkillRecord, scan_roots: &[PathBuf]) -> bool {
        match self {
            SidebarSel::Source(item) => item.matches_skill(skill),
            SidebarSel::Project(i) => scan_roots
                .get(*i)
                .is_some_and(|root| skill_in_project(skill, root)),
        }
    }
}

pub fn skill_in_project(skill: &SkillRecord, root: &Path) -> bool {
    if skill
        .project
        .as_ref()
        .is_some_and(|p| p == root || path_is_under(p, root))
    {
        return true;
    }
    skill.locations.iter().any(|loc| {
        path_is_under(&loc.path, root)
            || loc
                .resolved
                .as_deref()
                .is_some_and(|p| path_is_under(p, root))
    })
}

pub fn project_dir_label(root: &Path) -> String {
    root.file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("-")
        .to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpTransport {
    Stdio,
    Http,
    Sse,
}

impl McpTransport {
    pub fn as_str(self) -> &'static str {
        match self {
            McpTransport::Stdio => "stdio",
            McpTransport::Http => "http",
            McpTransport::Sse => "sse",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "stdio" => Some(McpTransport::Stdio),
            "streamable-http" | "http" => Some(McpTransport::Http),
            "sse" => Some(McpTransport::Sse),
            _ => None,
        }
    }
}

impl std::fmt::Display for McpTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginRecord {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: Option<String>,
    pub author: Option<String>,
    pub marketplace: Option<String>,
    /// `name@marketplace` when known; otherwise just `name`.
    pub spec: String,
    pub agents: Vec<Agent>,
    pub locations: Vec<SkillLocation>,
    pub skill_names: Vec<String>,
    pub mcp_names: Vec<String>,
    pub source_url: Option<String>,
    pub scope: Scope,
}

impl PluginRecord {
    pub fn key(&self) -> (String, Scope) {
        (self.id.clone(), self.scope)
    }

    pub fn agents_label(&self) -> String {
        agents_label(&self.agents)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServerRecord {
    pub id: String,
    pub name: String,
    pub transport: McpTransport,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    pub plugin: Option<String>,
    pub agents: Vec<Agent>,
    pub locations: Vec<SkillLocation>,
    pub scope: Scope,
}

impl McpServerRecord {
    pub fn key(&self) -> (String, Scope, String) {
        (
            self.id.clone(),
            self.scope,
            self.plugin.clone().unwrap_or_default(),
        )
    }

    pub fn agents_label(&self) -> String {
        agents_label(&self.agents)
    }

    pub fn endpoint_label(&self) -> String {
        if let Some(url) = &self.url {
            return url.clone();
        }
        match &self.command {
            Some(cmd) if self.args.is_empty() => cmd.clone(),
            Some(cmd) => format!("{cmd} {}", self.args.join(" ")),
            None => "-".into(),
        }
    }
}

/// Hosts that expose a plugin catalog CLI (Cursor is IDE-managed).
pub fn plugin_cli_agents() -> &'static [Agent] {
    &[Agent::ClaudeCode, Agent::Codex, Agent::GitHubCopilot]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginBackend {
    Claude,
    Codex,
    Copilot,
}

impl PluginBackend {
    pub fn program(self) -> &'static str {
        match self {
            PluginBackend::Claude => "claude",
            PluginBackend::Codex => "codex",
            PluginBackend::Copilot => "copilot",
        }
    }

    pub fn as_str(self) -> &'static str {
        self.program()
    }

    pub fn agent(self) -> Agent {
        match self {
            PluginBackend::Claude => Agent::ClaudeCode,
            PluginBackend::Codex => Agent::Codex,
            PluginBackend::Copilot => Agent::GitHubCopilot,
        }
    }

    pub fn from_agent(agent: Agent) -> Option<Self> {
        match agent {
            Agent::ClaudeCode => Some(PluginBackend::Claude),
            Agent::Codex => Some(PluginBackend::Codex),
            Agent::GitHubCopilot => Some(PluginBackend::Copilot),
            _ => None,
        }
    }
}

/// Parse `name@marketplace` or a bare plugin name.
pub fn split_plugin_spec(spec: &str) -> (String, Option<String>) {
    let spec = spec.trim();
    match spec.split_once('@') {
        Some((name, market)) if !name.is_empty() && !market.is_empty() => {
            (name.to_string(), Some(market.to_string()))
        }
        _ => (spec.to_string(), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_view_cycles_skills_plugins_mcp() {
        assert_eq!(ListView::Skills.next(), ListView::Plugins);
        assert_eq!(ListView::Plugins.next(), ListView::Mcp);
        assert_eq!(ListView::Mcp.next(), ListView::Skills);
    }

    #[test]
    fn nav_item_cycles_and_maps_list_view() {
        assert_eq!(NavItem::Manual.next(), NavItem::Gh);
        assert_eq!(NavItem::Gh.next(), NavItem::Npx);
        assert_eq!(NavItem::Npx.next(), NavItem::Plugins);
        assert_eq!(NavItem::Plugins.next(), NavItem::Mcp);
        assert_eq!(NavItem::Mcp.next(), NavItem::Manual);
        assert_eq!(NavItem::Manual.list_view(), ListView::Skills);
        assert_eq!(NavItem::Gh.list_view(), ListView::Skills);
        assert_eq!(NavItem::Plugins.list_view(), ListView::Plugins);
        assert_eq!(NavItem::Mcp.list_view(), ListView::Mcp);
        assert_eq!(NavItem::from_index(NavItem::Npx.index()), NavItem::Npx);
    }

    #[test]
    fn nav_item_partitions_skills_by_source() {
        let mut skill = SkillRecord {
            id: "x".into(),
            name: "x".into(),
            description: String::new(),
            scope: Scope::User,
            project: None,
            agents: vec![],
            locations: vec![],
            install_kind: crate::model::InstallKind::Copy,
            source: InstallSource::Manual,
            source_url: None,
            author: None,
            version: None,
            pinned: false,
            stats: crate::model::SkillStats::default(),
        };
        assert!(NavItem::Manual.matches_skill(&skill));
        assert!(!NavItem::Gh.matches_skill(&skill));
        skill.source = InstallSource::Plugin;
        assert!(NavItem::Manual.matches_skill(&skill));
        skill.source = InstallSource::Gh;
        assert!(NavItem::Gh.matches_skill(&skill));
        assert!(!NavItem::Manual.matches_skill(&skill));
        skill.source = InstallSource::Npx;
        assert!(NavItem::Npx.matches_skill(&skill));
        assert!(!NavItem::Plugins.matches_skill(&skill));
    }

    #[test]
    fn split_plugin_spec_parses_name_and_marketplace() {
        assert_eq!(
            split_plugin_spec("frontend-design@claude-plugins-official"),
            (
                "frontend-design".into(),
                Some("claude-plugins-official".into())
            )
        );
        assert_eq!(split_plugin_spec("linear"), ("linear".into(), None));
    }

    #[test]
    fn plugin_backend_maps_agents() {
        assert_eq!(
            PluginBackend::from_agent(Agent::ClaudeCode),
            Some(PluginBackend::Claude)
        );
        assert_eq!(PluginBackend::from_agent(Agent::Cursor), None);
        assert_eq!(PluginBackend::Copilot.program(), "copilot");
    }
}
