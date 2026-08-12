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

/// Canonical skill identity: normalized id + scope. One type shared by the
/// inventory merge, the multi-select marks, and the stats-preserving reload.
pub type SkillKey = (String, Scope);

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

/// One row of the single agent table. Order must match the `Agent` enum
/// declaration order — `as_str` indexes into it by discriminant and the
/// consistency test (`agent_tables_are_consistent`) guards the pairing.
struct AgentDef {
    agent: Agent,
    /// Canonical host string returned by `as_str` / accepted by `parse`.
    host: &'static str,
    /// Extra aliases accepted by `parse` (the host itself is always accepted).
    aliases: &'static [&'static str],
}

const AGENT_DEFS: &[AgentDef] = &[
    AgentDef {
        agent: Agent::Cursor,
        host: "cursor",
        aliases: &[],
    },
    AgentDef {
        agent: Agent::ClaudeCode,
        host: "claude-code",
        aliases: &["claude"],
    },
    AgentDef {
        agent: Agent::Codex,
        host: "codex",
        aliases: &[],
    },
    AgentDef {
        agent: Agent::GeminiCli,
        host: "gemini-cli",
        aliases: &["gemini"],
    },
    AgentDef {
        agent: Agent::Antigravity,
        host: "antigravity",
        aliases: &[],
    },
    AgentDef {
        agent: Agent::AntigravityCli,
        host: "antigravity-cli",
        aliases: &[],
    },
    AgentDef {
        agent: Agent::Antigravity2,
        host: "antigravity2.0",
        aliases: &["antigravity2"],
    },
    AgentDef {
        agent: Agent::GitHubCopilot,
        host: "github-copilot",
        aliases: &["copilot"],
    },
    AgentDef {
        agent: Agent::OpenCode,
        host: "opencode",
        aliases: &[],
    },
    AgentDef {
        agent: Agent::Pi,
        host: "pi",
        aliases: &[],
    },
    AgentDef {
        agent: Agent::Amp,
        host: "amp",
        aliases: &[],
    },
    AgentDef {
        agent: Agent::KimiCli,
        host: "kimi-cli",
        aliases: &["kimi"],
    },
    AgentDef {
        agent: Agent::Replit,
        host: "replit",
        aliases: &[],
    },
    AgentDef {
        agent: Agent::QwenCode,
        host: "qwen-code",
        aliases: &["qwen"],
    },
    AgentDef {
        agent: Agent::Augment,
        host: "augment",
        aliases: &[],
    },
    AgentDef {
        agent: Agent::Continue,
        host: "continue",
        aliases: &[],
    },
    AgentDef {
        agent: Agent::Droid,
        host: "droid",
        aliases: &["factory"],
    },
    AgentDef {
        agent: Agent::Kilo,
        host: "kilo",
        aliases: &["kilocode"],
    },
    AgentDef {
        agent: Agent::Qoder,
        host: "qoder",
        aliases: &[],
    },
    AgentDef {
        agent: Agent::Roo,
        host: "roo",
        aliases: &[],
    },
    AgentDef {
        agent: Agent::Trae,
        host: "trae",
        aliases: &[],
    },
    AgentDef {
        agent: Agent::CodeBuddy,
        host: "codebuddy",
        aliases: &[],
    },
    AgentDef {
        agent: Agent::Grok,
        host: "grok",
        aliases: &[],
    },
    AgentDef {
        agent: Agent::Cline,
        host: "cline",
        aliases: &[],
    },
    AgentDef {
        agent: Agent::Warp,
        host: "warp",
        aliases: &[],
    },
    AgentDef {
        agent: Agent::Universal,
        host: "universal",
        aliases: &[],
    },
    AgentDef {
        agent: Agent::Devin,
        host: "devin",
        aliases: &[],
    },
];

impl Agent {
    pub fn as_str(self) -> &'static str {
        AGENT_DEFS[self as usize].host
    }

    /// All known agents (inventory / filter order).
    pub fn all() -> &'static [Agent] {
        const ALL: [Agent; AGENT_DEFS.len()] = {
            let mut out = [Agent::Cursor; AGENT_DEFS.len()];
            let mut i = 0;
            while i < AGENT_DEFS.len() {
                out[i] = AGENT_DEFS[i].agent;
                i += 1;
            }
            out
        };
        &ALL
    }

    /// Default targets for add flow (common trio).
    pub fn primary() -> &'static [Agent] {
        &[Agent::Cursor, Agent::ClaudeCode, Agent::Codex]
    }

    pub fn parse(s: &str) -> Option<Self> {
        let idx = AGENT_DEFS
            .iter()
            .position(|d| d.host == s || d.aliases.contains(&s))?;
        Some(Agent::all()[idx])
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
    /// The canonical inventory identity for this skill.
    pub fn key(&self) -> SkillKey {
        (self.id.clone(), self.scope)
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

/// Shared id normalization used everywhere a skill name/id is compared:
/// inventory merge keys, gh matching, and log-stat tying. Keeps one rule so
/// lookups can never disagree about whether two strings name the same skill.
pub fn normalize_skill_id(id: &str, name: &str) -> String {
    let base = if id.is_empty() { name } else { id };
    base.trim_start_matches('.').to_lowercase()
}

/// Comma-joined agent ids for modal summaries (empty => `(none)`).
pub fn agents_label(agents: &[Agent]) -> String {
    if agents.is_empty() {
        "(none)".into()
    } else {
        agents
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
}

impl SkillFilters {
    pub fn matches(&self, skill: &SkillRecord) -> bool {
        if let Some(scope) = self.scope
            && skill.scope != scope
        {
            return false;
        }
        if !self.agents.is_empty() && !skill.agents.iter().any(|a| self.agents.contains(a)) {
            return false;
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

    #[test]
    fn agent_tables_are_consistent() {
        let all = Agent::all();
        for (i, agent) in all.iter().enumerate() {
            assert_eq!(
                AGENT_DEFS[i].agent, *agent,
                "AGENT_DEFS order must match enum order at index {i}"
            );
            assert_eq!(agent.as_str(), AGENT_DEFS[i].host);
            assert_eq!(
                Agent::parse(agent.as_str()),
                Some(*agent),
                "host round-trips"
            );
        }
        for def in AGENT_DEFS {
            for alias in def.aliases {
                assert_eq!(Agent::parse(alias), Some(def.agent), "alias {alias}");
            }
        }
    }

    #[test]
    fn normalize_skill_id_strips_dots_and_lowercases() {
        assert_eq!(normalize_skill_id(".FooBar", ".FooBar"), "foobar");
        assert_eq!(normalize_skill_id("", "fallback"), "fallback");
    }

    #[test]
    fn skill_key_uses_id_and_scope() {
        let rec = SkillRecord {
            id: "tdd".into(),
            name: "TDD".into(),
            description: String::new(),
            scope: Scope::Project,
            agents: vec![],
            locations: vec![],
            install_kind: InstallKind::Copy,
            source: InstallSource::Manual,
            source_url: None,
            version: None,
            pinned: false,
            stats: SkillStats::default(),
        };
        assert_eq!(rec.key(), (String::from("tdd"), Scope::Project));
    }

    #[test]
    fn filters_match_scope_agents_and_query() {
        let rec = SkillRecord {
            id: "find-skills".into(),
            name: "find-skills".into(),
            description: "Discover agent skills".into(),
            scope: Scope::User,
            agents: vec![Agent::Cursor, Agent::ClaudeCode],
            locations: vec![],
            install_kind: InstallKind::Copy,
            source: InstallSource::Npx,
            source_url: None,
            version: None,
            pinned: false,
            stats: SkillStats::default(),
        };
        let all = SkillFilters::default();
        assert!(all.matches(&rec));

        let project = SkillFilters {
            scope: Some(Scope::Project),
            ..Default::default()
        };
        assert!(!project.matches(&rec));

        let agent = SkillFilters {
            agents: vec![Agent::Codex],
            ..Default::default()
        };
        assert!(!agent.matches(&rec));

        let agent2 = SkillFilters {
            agents: vec![Agent::Cursor, Agent::Codex],
            ..Default::default()
        };
        assert!(agent2.matches(&rec));

        let query = SkillFilters {
            query: "DISCOVER".into(),
            ..Default::default()
        };
        assert!(query.matches(&rec));
        let miss = SkillFilters {
            query: "zzz".into(),
            ..Default::default()
        };
        assert!(!miss.matches(&rec));
    }
}
