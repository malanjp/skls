//! Plugin packages and MCP servers shown alongside skills.

use crate::model::{Agent, Scope, SkillLocation, agents_label};

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
