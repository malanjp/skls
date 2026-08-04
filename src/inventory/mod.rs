//! Merge FS discoveries with gh skill metadata into SkillRecords.

use crate::adapters::fs::{scan_skills, DiscoveredSkill};
use crate::adapters::gh_skill::{GhSkillCli, GhSkillListItem};
use crate::adapters::skill_lock::{load_locks, SkillLock};
use crate::adapters::CommandRunner;
use crate::model::{
    Agent, InstallKind, InstallSource, Scope, SkillLocation, SkillRecord, SkillStats,
};
use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct InventoryOptions {
    pub use_gh: bool,
}

pub fn build_inventory(
    project_root: &Path,
    home: &Path,
    runner: &impl CommandRunner,
    opts: &InventoryOptions,
) -> Result<(Vec<SkillRecord>, Vec<String>)> {
    let mut warnings = Vec::new();
    let (discovered, scan_warnings) = scan_skills(project_root, home)?;
    warnings.extend(scan_warnings);
    let mut records = merge_discovered(discovered);

    // Fast path: npx skills lockfile (no process spawn).
    for (scope, lock) in load_locks(project_root, home) {
        enrich_with_skill_lock(&mut records, &lock, scope);
    }

    if opts.use_gh {
        let cli = GhSkillCli { runner };
        match cli.list(None, None) {
            Ok(items) => enrich_with_gh(&mut records, &items),
            Err(err) => warnings.push(format!("gh skill list unavailable: {err}")),
        }
    }

    Ok((records, warnings))
}

pub fn merge_discovered(discovered: Vec<DiscoveredSkill>) -> Vec<SkillRecord> {
    let mut map: HashMap<(String, Scope), SkillRecord> = HashMap::new();

    for d in discovered {
        let key = (normalize_id(&d.id, &d.name), d.location.scope);
        let entry = map.entry(key.clone()).or_insert_with(|| SkillRecord {
            id: key.0.clone(),
            name: d.name.clone(),
            description: d.description.clone(),
            scope: d.location.scope,
            agents: Vec::new(),
            locations: Vec::new(),
            install_kind: d.location.kind,
            source: infer_source(&d),
            source_url: d.source_url.clone(),
            version: d.version.clone(),
            pinned: d.pinned,
            stats: SkillStats::default(),
        });

        if !entry.agents.contains(&d.location.agent) {
            entry.agents.push(d.location.agent);
        }
        entry.locations.push(d.location.clone());
        entry.install_kind = prefer_kind(entry.install_kind, d.location.kind);
        if entry.source_url.is_none() {
            entry.source_url = d.source_url.clone();
        }
        if entry.version.is_none() {
            entry.version = d.version.clone();
        }
        if entry.description.is_empty() && !d.description.is_empty() {
            entry.description = d.description.clone();
        }
        if entry.name == entry.id && d.name != d.id {
            entry.name = d.name.clone();
        }
        entry.source = prefer_source(entry.source, infer_source(&d));
        entry.pinned = entry.pinned || d.pinned;
    }

    let mut records: Vec<SkillRecord> = map.into_values().collect();
    for r in &mut records {
        r.agents.sort_by_key(|a| a.as_str());
        r.locations
            .sort_by(|a, b| a.agent.as_str().cmp(b.agent.as_str()));
    }
    records.sort_by(|a, b| a.name.cmp(&b.name).then(a.scope.as_str().cmp(b.scope.as_str())));
    records
}

pub fn enrich_with_gh(records: &mut [SkillRecord], items: &[GhSkillListItem]) {
    for item in items {
        let scope = match item.scope.as_str() {
            "project" => Scope::Project,
            "user" => Scope::User,
            _ => continue,
        };
        if let Some(rec) = records
            .iter_mut()
            .find(|r| gh_item_matches(r, item, scope))
        {
            apply_gh_item(rec, item, scope);
        }
    }
}

fn gh_item_matches(rec: &SkillRecord, item: &GhSkillListItem, scope: Scope) -> bool {
    if rec.scope != scope {
        return false;
    }
    let leaf = item
        .skill_name
        .rsplit('/')
        .next()
        .unwrap_or(&item.skill_name);
    let full_id = normalize_id(&item.skill_name, &item.skill_name);
    let leaf_id = normalize_id(leaf, leaf);
    if rec.id == full_id
        || rec.id == leaf_id
        || rec.name == item.skill_name
        || rec.name == leaf
    {
        return true;
    }
    if item.path.is_empty() {
        return false;
    }
    let item_path = std::path::PathBuf::from(&item.path);
    rec.locations.iter().any(|l| {
        l.path == item_path
            || l.resolved.as_ref() == Some(&item_path)
            || (l.path.file_name().is_some() && l.path.file_name() == item_path.file_name())
    })
}

pub fn enrich_with_skill_lock(records: &mut [SkillRecord], lock: &SkillLock, scope: Scope) {
    for rec in records.iter_mut() {
        if rec.scope != scope {
            continue;
        }
        let entry = lock
            .skills
            .get(&rec.name)
            .or_else(|| lock.skills.get(&rec.id));
        let Some(entry) = entry else {
            continue;
        };
        // Don't override stronger gh provenance with lock membership alone.
        if rec.source == InstallSource::Manual {
            rec.source = InstallSource::Npx;
        }
        if rec.source_url.is_none() && !entry.source_url.is_empty() {
            rec.source_url = Some(entry.source_url.clone());
        }
    }
}

fn apply_gh_item(rec: &mut SkillRecord, item: &GhSkillListItem, scope: Scope) {
    // Prefer non-empty provenance; never clobber an existing URL with empty.
    if !item.source_url.is_empty() {
        rec.source_url = Some(item.source_url.clone());
        rec.source = InstallSource::Gh;
    }
    if !item.version.is_empty() {
        rec.version = Some(item.version.clone());
    }
    rec.pinned = rec.pinned || item.pinned;
    for host in &item.agent_hosts {
        if let Some(agent) = Agent::parse(host) {
            if !rec.agents.contains(&agent) {
                rec.agents.push(agent);
            }
            if !rec.locations.iter().any(|l| l.agent == agent) && !item.path.is_empty() {
                rec.locations.push(SkillLocation {
                    agent,
                    scope,
                    path: item.path.clone().into(),
                    kind: InstallKind::Unknown,
                    resolved: None,
                });
            }
        }
    }
    rec.agents.sort_by_key(|a| a.as_str());
}

fn normalize_id(id: &str, name: &str) -> String {
    let base = if id.is_empty() { name } else { id };
    base.trim_start_matches('.').to_lowercase()
}

fn infer_source(d: &DiscoveredSkill) -> InstallSource {
    if d.source_url.as_deref().is_some_and(|u| !u.is_empty()) {
        // github-repo / sourceURL in SKILL.md is the gh-skill metadata shape.
        InstallSource::Gh
    } else {
        InstallSource::Manual
    }
}

fn prefer_kind(a: InstallKind, b: InstallKind) -> InstallKind {
    match (a, b) {
        (InstallKind::Symlink, _) | (_, InstallKind::Symlink) => InstallKind::Symlink,
        (InstallKind::Copy, _) | (_, InstallKind::Copy) => InstallKind::Copy,
        _ => InstallKind::Unknown,
    }
}

fn prefer_source(a: InstallSource, b: InstallSource) -> InstallSource {
    match (a, b) {
        (InstallSource::Gh, _) | (_, InstallSource::Gh) => InstallSource::Gh,
        (InstallSource::Npx, _) | (_, InstallSource::Npx) => InstallSource::Npx,
        _ => InstallSource::Manual,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::fs::DiscoveredSkill;
    use crate::model::SkillLocation;
    use std::path::PathBuf;

    fn disc(name: &str, agent: Agent, scope: Scope) -> DiscoveredSkill {
        DiscoveredSkill {
            id: name.into(),
            name: name.into(),
            description: format!("{name} desc"),
            location: SkillLocation {
                agent,
                scope,
                path: PathBuf::from(format!("/{name}")),
                kind: InstallKind::Copy,
                resolved: None,
            },
            source_url: None,
            version: None,
            pinned: false,
        }
    }

    #[test]
    fn merges_same_skill_across_agents() {
        let records = merge_discovered(vec![
            disc("tdd", Agent::Cursor, Scope::User),
            disc("tdd", Agent::ClaudeCode, Scope::User),
        ]);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].agents.len(), 2);
    }

    #[test]
    fn enrich_sets_gh_provenance() {
        let mut records = merge_discovered(vec![disc("tdd", Agent::Cursor, Scope::User)]);
        enrich_with_gh(
            &mut records,
            &[GhSkillListItem {
                skill_name: "tdd".into(),
                path: "/x/tdd".into(),
                scope: "user".into(),
                source_url: "https://github.com/ex/skills".into(),
                version: "v2".into(),
                pinned: true,
                agent_hosts: vec!["cursor".into()],
            }],
        );
        assert_eq!(records[0].source, InstallSource::Gh);
        assert_eq!(records[0].version.as_deref(), Some("v2"));
        assert!(records[0].pinned);
    }

    #[test]
    fn enrich_skill_lock_marks_npx_without_overriding_gh() {
        let mut records = merge_discovered(vec![
            disc("find-skills", Agent::ClaudeCode, Scope::User),
            {
                let mut d = disc("tdd", Agent::Cursor, Scope::User);
                d.source_url = Some("https://github.com/ex/skills".into());
                d
            },
        ]);
        let mut lock = SkillLock::default();
        lock.skills.insert(
            "find-skills".into(),
            crate::adapters::skill_lock::SkillLockEntry {
                source: "vercel-labs/skills".into(),
                source_url: "https://github.com/vercel-labs/skills.git".into(),
                source_type: "github".into(),
            },
        );
        lock.skills.insert(
            "tdd".into(),
            crate::adapters::skill_lock::SkillLockEntry {
                source: "mattpocock/skills".into(),
                source_url: "https://github.com/mattpocock/skills.git".into(),
                source_type: "github".into(),
            },
        );
        enrich_with_skill_lock(&mut records, &lock, Scope::User);
        let find = records.iter().find(|r| r.name == "find-skills").unwrap();
        let tdd = records.iter().find(|r| r.name == "tdd").unwrap();
        assert_eq!(find.source, InstallSource::Npx);
        assert_eq!(tdd.source, InstallSource::Gh);
    }

    #[test]
    fn enrich_matches_prefixed_gh_skill_name_by_path() {
        let mut records = merge_discovered(vec![DiscoveredSkill {
            id: "tdd".into(),
            name: "tdd".into(),
            description: String::new(),
            location: SkillLocation {
                agent: Agent::Cursor,
                scope: Scope::User,
                path: PathBuf::from("/home/.cursor/skills/tdd"),
                kind: InstallKind::Copy,
                resolved: None,
            },
            source_url: None,
            version: None,
            pinned: false,
        }]);
        enrich_with_gh(
            &mut records,
            &[
                GhSkillListItem {
                    skill_name: "tdd".into(),
                    path: "/home/.agents/skills/tdd".into(),
                    scope: "user".into(),
                    source_url: String::new(),
                    version: String::new(),
                    pinned: false,
                    agent_hosts: vec!["universal".into()],
                },
                GhSkillListItem {
                    skill_name: "engineering/tdd".into(),
                    path: "/home/.cursor/skills/tdd".into(),
                    scope: "user".into(),
                    source_url: "https://github.com/ex/skills".into(),
                    version: "v1".into(),
                    pinned: false,
                    agent_hosts: vec!["cursor".into()],
                },
            ],
        );
        assert_eq!(
            records[0].source_url.as_deref(),
            Some("https://github.com/ex/skills")
        );
        assert_eq!(records[0].source, InstallSource::Gh);
    }
}
