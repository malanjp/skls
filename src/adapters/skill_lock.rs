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
pub fn skill_lock_paths(
    project_roots: &[PathBuf],
    home: &Path,
) -> Vec<(crate::model::Scope, Option<PathBuf>, PathBuf)> {
    use crate::model::Scope;
    let mut paths = vec![(Scope::User, None, home.join(".agents/.skill-lock.json"))];
    if let Some(xdg) = std::env::var_os("XDG_STATE_HOME") {
        let xdg_lock = PathBuf::from(xdg).join("skills/.skill-lock.json");
        if !paths.iter().any(|(_, _, p)| *p == xdg_lock) {
            paths.push((Scope::User, None, xdg_lock));
        }
    }
    for project_root in project_roots {
        paths.push((
            Scope::Project,
            Some(project_root.clone()),
            project_root.join(".agents/.skill-lock.json"),
        ));
        paths.push((
            Scope::Project,
            Some(project_root.clone()),
            project_root.join("skills-lock.json"),
        ));
    }
    paths
}

pub fn load_skill_lock(path: &Path) -> Option<SkillLock> {
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

pub fn load_locks(
    project_roots: &[PathBuf],
    home: &Path,
) -> Vec<(crate::model::Scope, Option<PathBuf>, SkillLock)> {
    let mut out = Vec::new();
    for (scope, project, path) in skill_lock_paths(project_roots, home) {
        if let Some(lock) = load_skill_lock(&path) {
            out.push((scope, project, lock));
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

    #[test]
    fn load_locks_reads_each_project_lock() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let a = tmp.path().join("a");
        let b = tmp.path().join("b");
        fs::create_dir_all(home.join(".agents")).unwrap();
        fs::create_dir_all(a.join(".agents")).unwrap();
        fs::create_dir_all(&b).unwrap();
        fs::write(
            home.join(".agents/.skill-lock.json"),
            r#"{"skills":{"user-skill":{"source":"o/r","sourceUrl":"https://github.com/o/r.git"}}}"#,
        )
        .unwrap();
        fs::write(
            a.join(".agents/.skill-lock.json"),
            r#"{"skills":{"a-skill":{"source":"o/a","sourceUrl":"https://github.com/o/a.git"}}}"#,
        )
        .unwrap();
        fs::write(
            b.join("skills-lock.json"),
            r#"{"skills":{"b-skill":{"source":"o/b","sourceUrl":"https://github.com/o/b.git"}}}"#,
        )
        .unwrap();
        let locks = load_locks(&[a.clone(), b.clone()], &home);
        assert!(locks.iter().any(|(s, p, l)| {
            *s == crate::model::Scope::User && p.is_none() && l.skills.contains_key("user-skill")
        }));
        assert!(locks.iter().any(|(s, p, l)| {
            *s == crate::model::Scope::Project
                && p.as_ref() == Some(&a)
                && l.skills.contains_key("a-skill")
        }));
        assert!(locks.iter().any(|(s, p, l)| {
            *s == crate::model::Scope::Project
                && p.as_ref() == Some(&b)
                && l.skills.contains_key("b-skill")
        }));
    }
}
