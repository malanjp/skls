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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Agent {
    Cursor,
    ClaudeCode,
    Codex,
}

impl Agent {
    pub fn as_str(self) -> &'static str {
        match self {
            Agent::Cursor => "cursor",
            Agent::ClaudeCode => "claude-code",
            Agent::Codex => "codex",
        }
    }

    pub fn all() -> &'static [Agent] {
        &[Agent::Cursor, Agent::ClaudeCode, Agent::Codex]
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "cursor" => Some(Agent::Cursor),
            "claude-code" | "claude" => Some(Agent::ClaudeCode),
            "codex" => Some(Agent::Codex),
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
        self.agents
            .iter()
            .map(|a| a.as_str())
            .collect::<Vec<_>>()
            .join(",")
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
