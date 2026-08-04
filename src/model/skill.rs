//! Skill domain types for inventory, analytics, and TUI.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Project,
    User,
}

impl Scope {
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::Project => "project",
            Scope::User => "user",
        }
    }
}

impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Agent hosts that can own skill directories.
///
/// String ids match `gh skill list` `agentHosts` / `npx skills -a` where possible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Agent {
    Cursor,
    ClaudeCode,
    Codex,
    GeminiCli,
    Antigravity,
    AntigravityCli,
    #[serde(rename = "antigravity2.0")]
    Antigravity2,
    GitHubCopilot,
    OpenCode,
    Pi,
    Amp,
    KimiCli,
    Replit,
    QwenCode,
    Augment,
    Continue,
    /// Factory Droid (`gh` host: `droid`, path: `~/.factory/skills`)
    Droid,
    Kilo,
    Qoder,
    Roo,
    Trae,
    CodeBuddy,
    Grok,
    Cline,
    Warp,
    Universal,
    Devin,
}

impl Agent {
    pub fn as_str(self) -> &'static str {
        match self {
            Agent::Cursor => "cursor",
            Agent::ClaudeCode => "claude-code",
            Agent::Codex => "codex",
            Agent::GeminiCli => "gemini-cli",
            Agent::Antigravity => "antigravity",
            Agent::AntigravityCli => "antigravity-cli",
            Agent::Antigravity2 => "antigravity2.0",
            Agent::GitHubCopilot => "github-copilot",
            Agent::OpenCode => "opencode",
            Agent::Pi => "pi",
            Agent::Amp => "amp",
            Agent::KimiCli => "kimi-cli",
            Agent::Replit => "replit",
            Agent::QwenCode => "qwen-code",
            Agent::Augment => "augment",
            Agent::Continue => "continue",
            Agent::Droid => "droid",
            Agent::Kilo => "kilo",
            Agent::Qoder => "qoder",
            Agent::Roo => "roo",
            Agent::Trae => "trae",
            Agent::CodeBuddy => "codebuddy",
            Agent::Grok => "grok",
            Agent::Cline => "cline",
            Agent::Warp => "warp",
            Agent::Universal => "universal",
            Agent::Devin => "devin",
        }
    }

    /// All known agents (inventory / filter order).
    pub fn all() -> &'static [Agent] {
        &[
            Agent::Cursor,
            Agent::ClaudeCode,
            Agent::Codex,
            Agent::GeminiCli,
            Agent::Antigravity,
            Agent::AntigravityCli,
            Agent::Antigravity2,
            Agent::GitHubCopilot,
            Agent::OpenCode,
            Agent::Pi,
            Agent::Amp,
            Agent::KimiCli,
            Agent::Replit,
            Agent::QwenCode,
            Agent::Augment,
            Agent::Continue,
            Agent::Droid,
            Agent::Kilo,
            Agent::Qoder,
            Agent::Roo,
            Agent::Trae,
            Agent::CodeBuddy,
            Agent::Grok,
            Agent::Cline,
            Agent::Warp,
            Agent::Universal,
            Agent::Devin,
        ]
    }

    /// Default targets for add flow (common trio).
    pub fn primary() -> &'static [Agent] {
        &[Agent::Cursor, Agent::ClaudeCode, Agent::Codex]
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "cursor" => Some(Agent::Cursor),
            "claude-code" | "claude" => Some(Agent::ClaudeCode),
            "codex" => Some(Agent::Codex),
            "gemini-cli" | "gemini" => Some(Agent::GeminiCli),
            "antigravity" => Some(Agent::Antigravity),
            "antigravity-cli" => Some(Agent::AntigravityCli),
            "antigravity2.0" | "antigravity2" => Some(Agent::Antigravity2),
            "github-copilot" | "copilot" => Some(Agent::GitHubCopilot),
            "opencode" => Some(Agent::OpenCode),
            "pi" => Some(Agent::Pi),
            "amp" => Some(Agent::Amp),
            "kimi-cli" | "kimi" => Some(Agent::KimiCli),
            "replit" => Some(Agent::Replit),
            "qwen-code" | "qwen" => Some(Agent::QwenCode),
            "augment" => Some(Agent::Augment),
            "continue" => Some(Agent::Continue),
            "droid" | "factory" => Some(Agent::Droid),
            "kilo" | "kilocode" => Some(Agent::Kilo),
            "qoder" => Some(Agent::Qoder),
            "roo" => Some(Agent::Roo),
            "trae" => Some(Agent::Trae),
            "codebuddy" => Some(Agent::CodeBuddy),
            "grok" => Some(Agent::Grok),
            "cline" => Some(Agent::Cline),
            "warp" => Some(Agent::Warp),
            "universal" => Some(Agent::Universal),
            "devin" => Some(Agent::Devin),
            _ => None,
        }
    }
}

impl std::fmt::Display for Agent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstallKind {
    Symlink,
    Copy,
    Unknown,
}

impl InstallKind {
    pub fn as_str(self) -> &'static str {
        match self {
            InstallKind::Symlink => "symlink",
            InstallKind::Copy => "copy",
            InstallKind::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for InstallKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstallSource {
    Gh,
    Npx,
    Manual,
}

impl InstallSource {
    pub fn as_str(self) -> &'static str {
        match self {
            InstallSource::Gh => "gh",
            InstallSource::Npx => "npx",
            InstallSource::Manual => "manual",
        }
    }
}

impl std::fmt::Display for InstallSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillLocation {
    pub agent: Agent,
    pub scope: Scope,
    pub path: PathBuf,
    pub kind: InstallKind,
    /// Canonical target when `path` is a symlink.
    pub resolved: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SkillStats {
    pub hits: u64,
    pub sessions_total: u64,
    pub last_hit_at: Option<DateTime<Utc>>,
    pub activation_rate: Option<f64>,
    pub delete_score: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkillRecord {
    pub id: String,
    pub name: String,
    pub description: String,
    pub scope: Scope,
    pub agents: Vec<Agent>,
    pub locations: Vec<SkillLocation>,
    pub install_kind: InstallKind,
    pub source: InstallSource,
    pub source_url: Option<String>,
    pub version: Option<String>,
    pub pinned: bool,
    pub stats: SkillStats,
}

impl SkillRecord {
    pub fn primary_path(&self) -> Option<&PathBuf> {
        self.locations.first().map(|l| &l.path)
    }

    pub fn agents_label(&self) -> String {
        if self.agents.is_empty() {
            "-".into()
        } else {
            self.agents
                .iter()
                .map(|a| a.as_str())
                .collect::<Vec<_>>()
                .join(",")
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Name,
    Rate,
    Score,
    LastHit,
}

impl SortKey {
    pub fn next(self) -> Self {
        match self {
            SortKey::Name => SortKey::Rate,
            SortKey::Rate => SortKey::Score,
            SortKey::Score => SortKey::LastHit,
            SortKey::LastHit => SortKey::Name,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            SortKey::Name => "name",
            SortKey::Rate => "rate",
            SortKey::Score => "delete_score",
            SortKey::LastHit => "last_hit",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SkillFilters {
    pub scope: Option<Scope>,
    pub agents: Vec<Agent>,
    pub query: String,
    pub source: Option<InstallSource>,
    pub install_kind: Option<InstallKind>,
}

impl SkillFilters {
    pub fn matches(&self, skill: &SkillRecord) -> bool {
        if let Some(scope) = self.scope {
            if skill.scope != scope {
                return false;
            }
        }
        if !self.agents.is_empty()
            && !skill.agents.iter().any(|a| self.agents.contains(a))
        {
            return false;
        }
        if let Some(source) = self.source {
            if skill.source != source {
                return false;
            }
        }
        if let Some(kind) = self.install_kind {
            if skill.install_kind != kind {
                return false;
            }
        }
        if !self.query.is_empty() {
            let q = self.query.to_lowercase();
            let hay = format!(
                "{} {} {}",
                skill.name.to_lowercase(),
                skill.description.to_lowercase(),
                skill.id.to_lowercase()
            );
            if !hay.contains(&q) {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gh_agent_host_aliases() {
        assert_eq!(Agent::parse("github-copilot"), Some(Agent::GitHubCopilot));
        assert_eq!(Agent::parse("copilot"), Some(Agent::GitHubCopilot));
        assert_eq!(Agent::parse("qwen-code"), Some(Agent::QwenCode));
        assert_eq!(Agent::parse("droid"), Some(Agent::Droid));
        assert_eq!(Agent::parse("factory"), Some(Agent::Droid));
        assert_eq!(Agent::parse("antigravity2.0"), Some(Agent::Antigravity2));
        assert_eq!(Agent::parse("kilo"), Some(Agent::Kilo));
        assert_eq!(Agent::parse("universal"), Some(Agent::Universal));
    }

    #[test]
    fn all_agents_have_unique_ids() {
        let mut ids: Vec<_> = Agent::all().iter().map(|a| a.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), Agent::all().len());
    }
}
