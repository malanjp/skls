//! Parse agent transcripts to count skill activations.

use crate::model::{Agent, SkillRecord, normalize_skill_id};
use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Clone)]
pub struct LogPaths {
    pub cursor_transcripts: PathBuf,
    pub claude_projects: PathBuf,
    pub codex_sessions: PathBuf,
}

impl LogPaths {
    pub fn from_home(home: &Path) -> Self {
        Self {
            cursor_transcripts: home.join(".cursor/projects"),
            claude_projects: home.join(".claude/projects"),
            codex_sessions: home.join(".codex/sessions"),
        }
    }
}

/// Limits that keep activation analysis interactive even with huge transcript trees.
#[derive(Debug, Clone, Copy)]
pub struct AnalyzeLimits {
    /// Max sessions (jsonl files) per agent, newest first.
    pub max_files_per_agent: usize,
    /// Bytes read from each jsonl (head only).
    pub max_bytes_per_file: u64,
}

impl Default for AnalyzeLimits {
    fn default() -> Self {
        Self {
            max_files_per_agent: 80,
            max_bytes_per_file: 256 * 1024,
        }
    }
}

impl AnalyzeLimits {
    /// No caps — may take tens of seconds on large transcript trees.
    pub fn unlimited() -> Self {
        Self {
            max_files_per_agent: usize::MAX,
            max_bytes_per_file: u64::MAX,
        }
    }

    pub fn is_unlimited(&self) -> bool {
        self.max_files_per_agent == usize::MAX && self.max_bytes_per_file == u64::MAX
    }
}

#[derive(Debug, Clone, Default)]
pub struct ActivationIndex {
    /// skill_id (lowercase) -> set of session ids that hit
    pub hits_by_skill: HashMap<String, HashSet<String>>,
    /// skill_id -> last hit timestamp
    pub last_hit: HashMap<String, DateTime<Utc>>,
    /// agent -> total sessions scanned in window
    pub sessions_by_agent: HashMap<Agent, u64>,
    /// How many candidate files were skipped due to the per-agent cap.
    pub truncated_files: u64,
}

pub fn analyze_logs(
    paths: &LogPaths,
    window_days: i64,
    now: DateTime<Utc>,
) -> Result<ActivationIndex> {
    analyze_logs_with_limits(paths, window_days, now, AnalyzeLimits::default())
}

pub fn analyze_logs_with_limits(
    paths: &LogPaths,
    window_days: i64,
    now: DateTime<Utc>,
    limits: AnalyzeLimits,
) -> Result<ActivationIndex> {
    let cutoff = now - Duration::days(window_days);
    let cutoff_sys =
        SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(cutoff.timestamp().max(0) as u64);
    let mut index = ActivationIndex::default();

    scan_agent_files(
        Agent::Cursor,
        collect_cursor_jsonl(&paths.cursor_transcripts),
        cutoff_sys,
        limits,
        &mut index,
    )?;
    scan_agent_files(
        Agent::ClaudeCode,
        collect_jsonl_tree(&paths.claude_projects, 8),
        cutoff_sys,
        limits,
        &mut index,
    )?;
    scan_agent_files(
        Agent::Codex,
        collect_jsonl_tree(&paths.codex_sessions, 8),
        cutoff_sys,
        limits,
        &mut index,
    )?;

    Ok(index)
}

pub fn apply_stats(records: &mut [SkillRecord], index: &ActivationIndex) {
    let total_sessions: u64 = index.sessions_by_agent.values().sum();
    for rec in records.iter_mut() {
        let key = normalize_skill_id(&rec.id, &rec.name);
        let name_key = normalize_skill_id(&rec.name, &rec.name);
        let mut hits = index
            .hits_by_skill
            .get(&key)
            .map(|s| s.len() as u64)
            .unwrap_or(0);
        if hits == 0 {
            hits = index
                .hits_by_skill
                .get(&name_key)
                .map(|s| s.len() as u64)
                .unwrap_or(0);
        }
        let last = index
            .last_hit
            .get(&key)
            .copied()
            .or_else(|| index.last_hit.get(&name_key).copied());
        let sessions_total = if total_sessions == 0 {
            0
        } else {
            let mut sum = 0u64;
            for agent in &rec.agents {
                sum += index.sessions_by_agent.get(agent).copied().unwrap_or(0);
            }
            if sum == 0 { total_sessions } else { sum }
        };
        rec.stats.hits = hits;
        rec.stats.sessions_total = sessions_total;
        rec.stats.last_hit_at = last;
        rec.stats.activation_rate = if sessions_total == 0 {
            None
        } else {
            Some(hits as f64 / sessions_total as f64)
        };
    }
}

fn scan_agent_files(
    agent: Agent,
    files: Vec<PathBuf>,
    cutoff: SystemTime,
    limits: AnalyzeLimits,
    index: &mut ActivationIndex,
) -> Result<()> {
    let mut dated: Vec<(SystemTime, PathBuf)> = Vec::new();
    for path in files {
        let Ok(meta) = fs::metadata(&path) else {
            continue;
        };
        let Ok(modified) = meta.modified() else {
            continue;
        };
        if modified < cutoff {
            continue;
        }
        dated.push((modified, path));
    }
    dated.sort_by(|a, b| b.0.cmp(&a.0));
    if dated.len() > limits.max_files_per_agent {
        index.truncated_files += (dated.len() - limits.max_files_per_agent) as u64;
        dated.truncate(limits.max_files_per_agent);
    }

    let mut sessions = 0u64;
    for (modified, path) in dated {
        if process_file(&path, modified, limits.max_bytes_per_file, index)? {
            sessions += 1;
        }
    }
    *index.sessions_by_agent.entry(agent).or_insert(0) += sessions;
    Ok(())
}

fn collect_cursor_jsonl(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if !root.is_dir() {
        return out;
    }
    let Ok(projects) = fs::read_dir(root) else {
        return out;
    };
    for project in projects.flatten() {
        let transcripts = project.path().join("agent-transcripts");
        if !transcripts.is_dir() {
            continue;
        }
        let Ok(entries) = fs::read_dir(&transcripts) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                out.push(path);
                continue;
            }
            if path.is_dir() {
                if let Ok(nested) = fs::read_dir(&path) {
                    for n in nested.flatten() {
                        let npath = n.path();
                        if npath.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                            out.push(npath);
                        }
                    }
                }
            }
        }
    }
    out
}

fn collect_jsonl_tree(root: &Path, max_depth: usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn rec(path: &Path, depth: usize, max_depth: usize, out: &mut Vec<PathBuf>) {
        if depth > max_depth {
            return;
        }
        let Ok(rd) = fs::read_dir(path) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                // Skip bulky caches that never hold session transcripts.
                let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if matches!(name, "node_modules" | ".git" | "vendor" | "cache") {
                    continue;
                }
                rec(&p, depth + 1, max_depth, out);
            } else if p.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                out.push(p);
            }
        }
    }
    if root.is_dir() {
        rec(root, 0, max_depth, &mut out);
    }
    out
}

fn process_file(
    path: &Path,
    modified: SystemTime,
    max_bytes: u64,
    index: &mut ActivationIndex,
) -> Result<bool> {
    let mut file = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return Ok(false),
    };
    let mut buf = Vec::new();
    let mut limited = file.by_ref().take(max_bytes);
    limited.read_to_end(&mut buf)?;
    let lower = String::from_utf8_lossy(&buf).to_lowercase();
    // Cheap reject: no skill-ish path markers.
    if !lower.contains("skills/") && !lower.contains("skill.md") {
        // Still counts as a scanned session for the denominator.
        return Ok(true);
    }

    let when = DateTime::<Utc>::from(modified);
    let session_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();
    record_hits_from_text(&lower, &session_id, when, index);
    Ok(true)
}

fn record_hits_from_text(
    lower: &str,
    session_id: &str,
    when: DateTime<Utc>,
    index: &mut ActivationIndex,
) {
    for segment in
        lower.split(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_' && c != '/')
    {
        if let Some(name) = extract_skill_name(segment) {
            note_hit(index, &name, session_id, when);
        }
    }
}

fn extract_skill_name(segment: &str) -> Option<String> {
    let s = segment.trim_matches('/');
    if let Some(rest) = s.strip_prefix("skills/") {
        let name = rest.split('/').next().unwrap_or("");
        if is_plausible_skill_id(name) {
            return Some(name.to_string());
        }
    }
    if let Some(idx) = s.rfind("/skills/") {
        let rest = &s[idx + "/skills/".len()..];
        let name = rest.split('/').next().unwrap_or("");
        if is_plausible_skill_id(name) {
            return Some(name.to_string());
        }
    }
    if s.ends_with("/skill.md") {
        let parent = s.trim_end_matches("/skill.md");
        let name = parent.rsplit('/').next().unwrap_or("");
        if is_plausible_skill_id(name) {
            return Some(name.to_string());
        }
    }
    None
}

fn is_plausible_skill_id(name: &str) -> bool {
    !name.is_empty()
        && name.len() < 80
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && name != "skills"
        && name != "agents"
}

fn note_hit(index: &mut ActivationIndex, skill: &str, session_id: &str, when: DateTime<Utc>) {
    let key = skill.to_lowercase();
    index
        .hits_by_skill
        .entry(key.clone())
        .or_default()
        .insert(session_id.to_string());
    let entry = index.last_hit.entry(key).or_insert(when);
    if when > *entry {
        *entry = when;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn extracts_skill_from_path_mentions() {
        let mut index = ActivationIndex::default();
        let text =
            r#"read /users/me/.claude/skills/brainstorming/skill.md and also skills/tdd/SKILL.md"#
                .to_lowercase();
        record_hits_from_text(&text, "sess1", Utc::now(), &mut index);
        assert!(index.hits_by_skill.contains_key("brainstorming"));
        assert!(index.hits_by_skill.contains_key("tdd"));
    }

    #[test]
    fn counts_session_once_per_skill() {
        let tmp = tempfile::tempdir().unwrap();
        let transcripts = tmp.path().join("proj/agent-transcripts");
        fs::create_dir_all(&transcripts).unwrap();
        let mut f = fs::File::create(transcripts.join("abc.jsonl")).unwrap();
        writeln!(
            f,
            r#"{{"text":"using ~/.cursor/skills/find-skills/SKILL.md and again skills/find-skills/x"}}"#
        )
        .unwrap();

        let paths = LogPaths {
            cursor_transcripts: tmp.path().to_path_buf(),
            claude_projects: tmp.path().join("none"),
            codex_sessions: tmp.path().join("none"),
        };
        let index = analyze_logs(&paths, 30, Utc::now()).unwrap();
        let hits = index.hits_by_skill.get("find-skills").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(index.sessions_by_agent.get(&Agent::Cursor), Some(&1));
    }

    #[test]
    fn respects_file_cap() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("projects/p1");
        fs::create_dir_all(&root).unwrap();
        for i in 0..20 {
            let p = root.join(format!("s{i}.jsonl"));
            fs::write(&p, format!(r#"{{"t":"skills/foo/SKILL.md {i}"}}"#)).unwrap();
        }
        let paths = LogPaths {
            cursor_transcripts: tmp.path().join("none"),
            claude_projects: tmp.path().join("projects"),
            codex_sessions: tmp.path().join("none"),
        };
        let index = analyze_logs_with_limits(
            &paths,
            30,
            Utc::now(),
            AnalyzeLimits {
                max_files_per_agent: 5,
                max_bytes_per_file: 64 * 1024,
            },
        )
        .unwrap();
        assert_eq!(index.sessions_by_agent.get(&Agent::ClaudeCode), Some(&5));
        assert!(index.truncated_files >= 15);
    }

    #[test]
    fn apply_stats_ties_index_to_records_by_shared_normalization() {
        use crate::model::{InstallKind, InstallSource, Scope, SkillStats};
        let mut index = ActivationIndex::default();
        note_hit(&mut index, "find-skills", "s1", Utc::now());
        index.sessions_by_agent.insert(Agent::Cursor, 10);

        let mut records = vec![SkillRecord {
            id: "find-skills".into(),
            name: "Find Skills".into(),
            description: String::new(),
            scope: Scope::User,
            project: None,
            agents: vec![Agent::Cursor],
            locations: vec![],
            install_kind: InstallKind::Copy,
            source: InstallSource::Npx,
            source_url: None,
            author: None,
            version: None,
            pinned: false,
            stats: SkillStats::default(),
        }];
        apply_stats(&mut records, &index);
        assert_eq!(records[0].stats.hits, 1);
        assert_eq!(records[0].stats.sessions_total, 10);
        assert_eq!(records[0].stats.activation_rate, Some(0.1));
    }
}
