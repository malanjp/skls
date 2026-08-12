//! Parse `npx skills` lockfile (`.agents/.skill-lock.json`).

use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SkillLock {
    #[serde(default)]
    pub skills: HashMap<String, SkillLockEntry>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SkillLockEntry {
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub source_url: String,
    #[serde(default)]
    pub source_type: String,
}

/// Candidate lockfile paths for user / project scopes.
pub fn skill_lock_paths(project_root: &Path, home: &Path) -> Vec<(crate::model::Scope, PathBuf)> {
    use crate::model::Scope;
    vec![
        (Scope::User, home.join(".agents/.skill-lock.json")),
        (
            Scope::Project,
            project_root.join(".agents/.skill-lock.json"),
        ),
        (Scope::Project, project_root.join("skills-lock.json")),
    ]
}

pub fn load_skill_lock(path: &Path) -> Option<SkillLock> {
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

pub fn load_locks(project_root: &Path, home: &Path) -> Vec<(crate::model::Scope, SkillLock)> {
    let mut out = Vec::new();
    for (scope, path) in skill_lock_paths(project_root, home) {
        if let Some(lock) = load_skill_lock(&path) {
            out.push((scope, lock));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_skill_lock_json() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(".skill-lock.json");
        fs::write(
            &path,
            r#"{"version":3,"skills":{"find-skills":{"source":"vercel-labs/skills","sourceType":"github","sourceUrl":"https://github.com/vercel-labs/skills.git"}}}"#,
        )
        .unwrap();
        let lock = load_skill_lock(&path).unwrap();
        assert!(lock.skills.contains_key("find-skills"));
        assert_eq!(
            lock.skills["find-skills"].source_url,
            "https://github.com/vercel-labs/skills.git"
        );
    }
}
