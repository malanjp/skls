//! Parse agent transcripts to count skill activations.

use crate::model::{Agent, SkillRecord};
use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

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

#[derive(Debug, Clone, Default)]
pub struct ActivationIndex {
    /// skill_id (lowercase) -> set of session ids that hit
    pub hits_by_skill: HashMap<String, HashSet<String>>,
    /// skill_id -> last hit timestamp
    pub last_hit: HashMap<String, DateTime<Utc>>,
    /// agent -> total sessions scanned in window
    pub sessions_by_agent: HashMap<Agent, u64>,
}

pub fn analyze_logs(
    paths: &LogPaths,
    window_days: i64,
    now: DateTime<Utc>,
) -> Result<ActivationIndex> {
    let cutoff = now - Duration::days(window_days);
    let mut index = ActivationIndex::default();

    scan_cursor(&paths.cursor_transcripts, cutoff, &mut index)?;
    scan_jsonl_tree(
        &paths.claude_projects,
        Agent::ClaudeCode,
        cutoff,
        &mut index,
        "session",
    )?;
    scan_jsonl_tree(
        &paths.codex_sessions,
        Agent::Codex,
        cutoff,
        &mut index,
        "rollout",
    )?;

    Ok(index)
}

pub fn apply_stats(records: &mut [SkillRecord], index: &ActivationIndex) {
    let total_sessions: u64 = index.sessions_by_agent.values().sum();
    for rec in records.iter_mut() {
        let key = rec.id.to_lowercase();
        let name_key = rec.name.to_lowercase();
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
            // Prefer sessions from agents where the skill is installed.
            let mut sum = 0u64;
            for agent in &rec.agents {
                sum += index.sessions_by_agent.get(agent).copied().unwrap_or(0);
            }
            if sum == 0 {
                total_sessions
            } else {
                sum
            }
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

fn scan_cursor(root: &Path, cutoff: DateTime<Utc>, index: &mut ActivationIndex) -> Result<()> {
    if !root.is_dir() {
        return Ok(());
    }
    let mut sessions = 0u64;
    for project in walk_dirs(root, 2) {
        let transcripts = project.join("agent-transcripts");
        if !transcripts.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&transcripts).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                // transcript may be a directory with jsonl inside
                if path.is_dir() {
                    for nested in fs::read_dir(&path).into_iter().flatten().flatten() {
                        let npath = nested.path();
                        if npath.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                            if process_file(&npath, cutoff, index)? {
                                sessions += 1;
                            }
                        }
                    }
                }
                continue;
            }
            if process_file(&path, cutoff, index)? {
                sessions += 1;
            }
        }
    }
    *index
        .sessions_by_agent
        .entry(Agent::Cursor)
        .or_insert(0) += sessions;
    Ok(())
}

fn scan_jsonl_tree(
    root: &Path,
    agent: Agent,
    cutoff: DateTime<Utc>,
    index: &mut ActivationIndex,
    _hint: &str,
) -> Result<()> {
    if !root.is_dir() {
        return Ok(());
    }
    let mut sessions = 0u64;
    for path in walk_files(root, 6) {
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        if process_file(&path, cutoff, index)? {
            sessions += 1;
        }
    }
    *index.sessions_by_agent.entry(agent).or_insert(0) += sessions;
    Ok(())
}

fn process_file(
    path: &Path,
    cutoff: DateTime<Utc>,
    index: &mut ActivationIndex,
) -> Result<bool> {
    let meta = match fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return Ok(false),
    };
    let modified = meta
        .modified()
        .ok()
        .map(DateTime::<Utc>::from)
        .unwrap_or(Utc::now());
    if modified < cutoff {
        return Ok(false);
    }

    let file = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return Ok(false),
    };
    // Cap read size to avoid huge logs
    let reader = BufReader::new(file.take(2_000_000));
    let session_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    let mut body = String::new();
    for line in reader.lines().flatten() {
        body.push_str(&line);
        body.push('\n');
        if body.len() > 2_000_000 {
            break;
        }
    }
    let lower = body.to_lowercase();
    record_hits_from_text(&lower, &session_id, modified, index);
    Ok(true)
}

fn record_hits_from_text(
    lower: &str,
    session_id: &str,
    when: DateTime<Utc>,
    index: &mut ActivationIndex,
) {
    // Heuristic: look for common skill activation markers and bare skill folder names
    // that appear near "skill" tokens. Also match explicit path segments.
    const MARKERS: &[&str] = &[
        "skill.md",
        "skills/",
        "using skill",
        "read skill",
        "invoked skill",
        "\"skill\"",
        "skill_name",
        "skillname",
    ];

    // Extract plausible skill ids from path-like mentions: skills/<name> or /skills/<name>/
    for segment in lower.split(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_' && c != '/')
    {
        if let Some(name) = extract_skill_name(segment) {
            note_hit(index, &name, session_id, when);
        }
    }

    // If the transcript mentions skill tooling heavily, also score explicit folder names
    // after "skills/" already handled above. Fallback: no-op.
    let _ = MARKERS;
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
    // Direct skill file read: .../<skill>/skill.md
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

fn walk_dirs(root: &Path, max_depth: usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn rec(path: &Path, depth: usize, max_depth: usize, out: &mut Vec<PathBuf>) {
        if depth > max_depth {
            return;
        }
        out.push(path.to_path_buf());
        if let Ok(rd) = fs::read_dir(path) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    rec(&p, depth + 1, max_depth, out);
                }
            }
        }
    }
    if root.is_dir() {
        rec(root, 0, max_depth, &mut out);
    }
    out
}

fn walk_files(root: &Path, max_depth: usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn rec(path: &Path, depth: usize, max_depth: usize, out: &mut Vec<PathBuf>) {
        if depth > max_depth {
            return;
        }
        if let Ok(rd) = fs::read_dir(path) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    rec(&p, depth + 1, max_depth, out);
                } else {
                    out.push(p);
                }
            }
        }
    }
    if root.is_dir() {
        rec(root, 0, max_depth, &mut out);
    }
    out
}

// BufRead take helper
trait TakeExt: Sized {
    fn take(self, limit: u64) -> std::io::Take<Self>;
}
impl<R: std::io::Read> TakeExt for R {
    fn take(self, limit: u64) -> std::io::Take<Self> {
        std::io::Read::take(self, limit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn extracts_skill_from_path_mentions() {
        let mut index = ActivationIndex::default();
        let text = r#"read /users/me/.claude/skills/brainstorming/skill.md and also skills/tdd/SKILL.md"#
            .to_lowercase();
        record_hits_from_text(&text, "sess1", Utc::now(), &mut index);
        assert!(index.hits_by_skill.contains_key("brainstorming"));
        assert!(index.hits_by_skill.contains_key("tdd"));
    }

    #[test]
    fn counts_session_once_per_skill() {
        let tmp = tempfile::tempdir().unwrap();
        let transcripts = tmp
            .path()
            .join("proj/agent-transcripts");
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
}
