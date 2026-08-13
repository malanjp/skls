//! Merge FS discoveries with gh skill metadata into SkillRecords.

use crate::adapters::CommandRunner;
use crate::adapters::fs::{DiscoveredSkill, scan_skills};
use crate::adapters::gh_skill::{GhSkillCli, GhSkillListItem};
use crate::adapters::mcp::DiscoveredMcp;
use crate::adapters::plugin::{DiscoveredPlugin, scan_plugin_inventory};
use crate::adapters::skill_lock::{SkillLock, load_locks};
use crate::model::{
    Agent, InstallKind, InstallSource, McpServerRecord, PluginRecord, Scope, SkillKey,
    SkillLocation, SkillRecord, SkillStats, github_owner, normalize_skill_id,
};
use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct InventoryOptions {
    pub use_gh: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Inventory {
    pub skills: Vec<SkillRecord>,
    pub plugins: Vec<PluginRecord>,
    pub mcp: Vec<McpServerRecord>,
    pub warnings: Vec<String>,
}

pub fn build_inventory(
    project_root: &Path,
    home: &Path,
    runner: &impl CommandRunner,
    opts: &InventoryOptions,
) -> Result<Inventory> {
    let mut warnings = Vec::new();
    let (discovered, scan_warnings) = scan_skills(project_root, home)?;
    warnings.extend(scan_warnings);
    let plugin_scan = scan_plugin_inventory(project_root, home)?;
    warnings.extend(plugin_scan.warnings);
    let mut skills = merge_discovered(discovered.into_iter().chain(plugin_scan.skills).collect());
    let plugins = merge_plugins(plugin_scan.plugins);
    let mcp = merge_mcp(plugin_scan.mcp);

    // Fast path: npx skills lockfile (no process spawn).
    for (scope, lock) in load_locks(project_root, home) {
        enrich_with_skill_lock(&mut skills, &lock, scope);
    }

    if opts.use_gh {
        let cli = GhSkillCli { runner };
        match cli.list(None, None) {
            Ok(items) => enrich_with_gh(&mut skills, &items),
            Err(err) => warnings.push(format!("gh skill list unavailable: {err}")),
        }
    }

    for rec in &mut skills {
        retain_plugin_source(rec);
    }

    Ok(Inventory {
        skills,
        plugins,
        mcp,
        warnings,
    })
}

pub fn merge_discovered(discovered: Vec<DiscoveredSkill>) -> Vec<SkillRecord> {
    let mut map: HashMap<SkillKey, SkillRecord> = HashMap::new();

    for d in discovered {
        let key = (normalize_skill_id(&d.id, &d.name), d.location.scope, None);
        let entry = map.entry(key.clone()).or_insert_with(|| SkillRecord {
            id: key.0.clone(),
            name: d.name.clone(),
            description: d.description.clone(),
            scope: d.location.scope,
            project: None,
            agents: Vec::new(),
            locations: Vec::new(),
            install_kind: d.location.kind,
            source: infer_source(&d),
            source_url: d.source_url.clone(),
            author: d.author.clone(),
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
        if entry.author.is_none() {
            entry.author = d
                .author
                .clone()
                .or_else(|| d.source_url.as_deref().and_then(github_owner));
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
        retain_plugin_source(r);
        r.agents.sort_by_key(|a| a.as_str());
        r.locations
            .sort_by(|a, b| a.agent.as_str().cmp(b.agent.as_str()));
    }
    records.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then(a.scope.as_str().cmp(b.scope.as_str()))
    });
    records
}

fn merge_plugins(discovered: Vec<DiscoveredPlugin>) -> Vec<PluginRecord> {
    let mut map: HashMap<(String, Scope), PluginRecord> = HashMap::new();
    for d in discovered {
        let id = normalize_skill_id(&d.name, &d.name);
        let key = (id.clone(), d.location.scope);
        let entry = map.entry(key.clone()).or_insert_with(|| PluginRecord {
            id: key.0.clone(),
            name: d.name.clone(),
            description: d.description.clone(),
            version: d.version.clone(),
            author: d.author.clone(),
            marketplace: d.marketplace.clone(),
            spec: d.spec.clone(),
            agents: Vec::new(),
            locations: Vec::new(),
            skill_names: d.skill_names.clone(),
            mcp_names: d.mcp_names.clone(),
            source_url: d.source_url.clone(),
            scope: d.location.scope,
        });
        if !entry.agents.contains(&d.location.agent) {
            entry.agents.push(d.location.agent);
        }
        if !entry
            .locations
            .iter()
            .any(|l| l.path == d.location.path && l.agent == d.location.agent)
        {
            entry.locations.push(d.location.clone());
        }
        if entry.source_url.is_none() {
            entry.source_url = d.source_url.clone();
        }
        if entry.author.is_none() {
            entry.author = d
                .author
                .clone()
                .or_else(|| d.source_url.as_deref().and_then(github_owner));
        }
        if entry.version.is_none() {
            entry.version = d.version.clone();
        }
        if entry.description.is_empty() && !d.description.is_empty() {
            entry.description = d.description.clone();
        }
        if entry.marketplace.is_none() {
            entry.marketplace = d.marketplace.clone();
        }
        if !entry.spec.contains('@') && d.spec.contains('@') {
            entry.spec = d.spec.clone();
        }
        for name in d.skill_names {
            if !entry.skill_names.contains(&name) {
                entry.skill_names.push(name);
            }
        }
        for name in d.mcp_names {
            if !entry.mcp_names.contains(&name) {
                entry.mcp_names.push(name);
            }
        }
    }
    let mut records: Vec<PluginRecord> = map.into_values().collect();
    for r in &mut records {
        r.agents.sort_by_key(|a| a.as_str());
        r.skill_names.sort();
        r.mcp_names.sort();
        r.locations
            .sort_by(|a, b| a.agent.as_str().cmp(b.agent.as_str()));
    }
    records.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then(a.scope.as_str().cmp(b.scope.as_str()))
    });
    records
}

fn merge_mcp(discovered: Vec<DiscoveredMcp>) -> Vec<McpServerRecord> {
    let mut map: HashMap<(String, Scope, String), McpServerRecord> = HashMap::new();
    for d in discovered {
        let plugin = d.plugin.clone().unwrap_or_default();
        let id = normalize_skill_id(&d.name, &d.name);
        let key = (id.clone(), d.location.scope, plugin.clone());
        let entry = map.entry(key.clone()).or_insert_with(|| McpServerRecord {
            id: key.0.clone(),
            name: d.name.clone(),
            transport: d.transport,
            command: d.command.clone(),
            args: d.args.clone(),
            url: d.url.clone(),
            plugin: d.plugin.clone(),
            agents: Vec::new(),
            locations: Vec::new(),
            scope: d.location.scope,
        });
        if !entry.agents.contains(&d.location.agent) {
            entry.agents.push(d.location.agent);
        }
        if !entry
            .locations
            .iter()
            .any(|l| l.path == d.location.path && l.agent == d.location.agent)
        {
            entry.locations.push(d.location.clone());
        }
    }
    let mut records: Vec<McpServerRecord> = map.into_values().collect();
    for r in &mut records {
        r.agents.sort_by_key(|a| a.as_str());
        r.locations
            .sort_by(|a, b| a.agent.as_str().cmp(b.agent.as_str()));
    }
    records.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then(a.plugin.cmp(&b.plugin))
            .then(a.scope.as_str().cmp(b.scope.as_str()))
    });
    records
}

pub fn enrich_with_gh(records: &mut [SkillRecord], items: &[GhSkillListItem]) {
    for item in items {
        let scope = match item.scope.as_str() {
            "project" => Scope::Project,
            "user" => Scope::User,
            _ => continue,
        };
        if let Some(rec) = records.iter_mut().find(|r| gh_item_matches(r, item, scope)) {
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
    let full_id = normalize_skill_id(&item.skill_name, &item.skill_name);
    let leaf_id = normalize_skill_id(leaf, leaf);
    if rec.id == full_id || rec.id == leaf_id || rec.name == item.skill_name || rec.name == leaf {
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
        let entry = lock_entry_for(lock, rec);
        let Some(entry) = entry else {
            continue;
        };
        // Lock membership is the npx-skills signal. It outranks a plugin copy of
        // the same name, but must not clobber gh-skill provenance.
        if rec.source != InstallSource::Gh {
            rec.source = InstallSource::Npx;
        }
        if rec.source_url.is_none() && !entry.source_url.is_empty() {
            rec.source_url = Some(entry.source_url.clone());
        }
        if rec.author.is_none() {
            // Lock `source` is `owner/repo`.
            let owner_from_source = entry
                .source
                .split_once('/')
                .map(|(owner, _)| owner.trim().to_string())
                .filter(|owner| !owner.is_empty());
            rec.author = owner_from_source.or_else(|| github_owner(&entry.source_url));
        }
    }
}

fn lock_entry_for<'a>(
    lock: &'a SkillLock,
    rec: &SkillRecord,
) -> Option<&'a crate::adapters::skill_lock::SkillLockEntry> {
    lock.skills
        .get(&rec.name)
        .or_else(|| lock.skills.get(&rec.id))
        .or_else(|| {
            rec.locations.iter().find_map(|l| {
                l.path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .and_then(|n| lock.skills.get(n))
            })
        })
}

fn apply_gh_item(rec: &mut SkillRecord, item: &GhSkillListItem, scope: Scope) {
    // Prefer non-empty provenance; never clobber an existing URL with empty.
    if !item.source_url.is_empty() {
        rec.source_url = Some(item.source_url.clone());
        if rec.source != InstallSource::Plugin || has_non_plugin_location(rec) {
            rec.source = InstallSource::Gh;
        }
        if rec.author.is_none() {
            rec.author = github_owner(&item.source_url);
        }
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

fn infer_source(d: &DiscoveredSkill) -> InstallSource {
    if let Some(source) = d.source {
        return source;
    }
    let path = d
        .location
        .resolved
        .as_deref()
        .unwrap_or(d.location.path.as_path());
    if crate::ops::is_plugin_path(path) {
        return InstallSource::Plugin;
    }
    if d.source_url.as_deref().is_some_and(|u| !u.is_empty()) {
        // github-repo / sourceURL in SKILL.md is the gh-skill metadata shape.
        return InstallSource::Gh;
    }
    if crate::ops::is_agents_skills_path(path) {
        return InstallSource::Npx;
    }
    InstallSource::Manual
}

fn prefer_kind(a: InstallKind, b: InstallKind) -> InstallKind {
    match (a, b) {
        (InstallKind::Symlink, _) | (_, InstallKind::Symlink) => InstallKind::Symlink,
        (InstallKind::Copy, _) | (_, InstallKind::Copy) => InstallKind::Copy,
        _ => InstallKind::Unknown,
    }
}

fn prefer_source(a: InstallSource, b: InstallSource) -> InstallSource {
    // Gh > Npx > Plugin > Manual. Plugin-only rows are forced back to Plugin
    // after merge/enrich via [`retain_plugin_source`].
    let rank = |s: InstallSource| match s {
        InstallSource::Gh => 3,
        InstallSource::Npx => 2,
        InstallSource::Plugin => 1,
        InstallSource::Manual => 0,
    };
    if rank(b) > rank(a) { b } else { a }
}

/// Skills whose inventory paths all live inside a plugin stay `plugin`,
/// but only when no stronger installer (gh / npx) is already known.
fn retain_plugin_source(rec: &mut SkillRecord) {
    if rec.source != InstallSource::Manual {
        return;
    }
    if rec.locations.is_empty() {
        return;
    }
    let all_plugin = rec.locations.iter().all(location_is_plugin);
    if all_plugin {
        rec.source = InstallSource::Plugin;
    }
}

fn location_is_plugin(l: &SkillLocation) -> bool {
    crate::ops::is_plugin_path(&l.path)
        || l.resolved
            .as_ref()
            .is_some_and(|p| crate::ops::is_plugin_path(p))
}

fn has_non_plugin_location(rec: &SkillRecord) -> bool {
    rec.locations.iter().any(|l| !location_is_plugin(l))
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
            author: None,
            version: None,
            pinned: false,
            source: None,
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
    fn merge_derives_author_from_github_url() {
        let mut d = disc("tdd", Agent::Cursor, Scope::User);
        d.source_url = Some("https://github.com/mattpocock/skills".into());
        let records = merge_discovered(vec![d]);
        assert_eq!(records[0].author.as_deref(), Some("mattpocock"));
        assert_eq!(records[0].source, InstallSource::Gh);
    }

    #[test]
    fn agents_skills_store_infers_npx_even_with_plugin_copy() {
        let mut agents = disc("find-skills", Agent::Cursor, Scope::User);
        agents.location.path = PathBuf::from("/home/.agents/skills/find-skills");
        let mut plugin = disc("find-skills", Agent::ClaudeCode, Scope::User);
        plugin.location.path =
            PathBuf::from("/home/.claude/plugins/cache/m/sp/1.0.0/skills/find-skills");
        plugin.source = Some(InstallSource::Plugin);
        let records = merge_discovered(vec![agents, plugin]);
        assert_eq!(records[0].source, InstallSource::Npx);
    }

    #[test]
    fn lockfile_marks_npx_over_plugin_copy() {
        let mut plugin = disc("find-skills", Agent::ClaudeCode, Scope::User);
        plugin.location.path =
            PathBuf::from("/home/.claude/plugins/cache/m/sp/1.0.0/skills/find-skills");
        plugin.source = Some(InstallSource::Plugin);
        let mut host = disc("find-skills", Agent::Cursor, Scope::User);
        host.location.path = PathBuf::from("/home/.cursor/skills/find-skills");
        let mut records = merge_discovered(vec![plugin, host]);
        assert_eq!(records[0].source, InstallSource::Plugin);
        let mut lock = SkillLock::default();
        lock.skills.insert(
            "find-skills".into(),
            crate::adapters::skill_lock::SkillLockEntry {
                source: "vercel-labs/skills".into(),
                source_url: "https://github.com/vercel-labs/skills.git".into(),
                source_type: "github".into(),
            },
        );
        enrich_with_skill_lock(&mut records, &lock, Scope::User);
        retain_plugin_source(&mut records[0]);
        assert_eq!(records[0].source, InstallSource::Npx);
    }

    #[test]
    fn enrich_skill_lock_sets_author_from_source() {
        let mut records = merge_discovered(vec![disc("find-skills", Agent::Cursor, Scope::User)]);
        let mut lock = SkillLock::default();
        lock.skills.insert(
            "find-skills".into(),
            crate::adapters::skill_lock::SkillLockEntry {
                source: "vercel-labs/skills".into(),
                source_url: "https://github.com/vercel-labs/skills.git".into(),
                source_type: "github".into(),
            },
        );
        enrich_with_skill_lock(&mut records, &lock, Scope::User);
        assert_eq!(records[0].author.as_deref(), Some("vercel-labs"));
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
    fn enrich_does_not_override_plugin_source() {
        let mut d = disc("tdd", Agent::ClaudeCode, Scope::User);
        d.location.path = PathBuf::from("/home/.claude/plugins/cache/m/sp/1.0.0/skills/tdd");
        d.source = Some(InstallSource::Plugin);
        d.source_url = Some("https://github.com/ex/skills".into());
        let mut records = merge_discovered(vec![d]);
        assert_eq!(records[0].source, InstallSource::Plugin);
        enrich_with_gh(
            &mut records,
            &[GhSkillListItem {
                skill_name: "tdd".into(),
                path: "/home/.claude/plugins/cache/m/sp/1.0.0/skills/tdd".into(),
                scope: "user".into(),
                source_url: "https://github.com/ex/skills".into(),
                version: "v2".into(),
                pinned: false,
                agent_hosts: vec!["claude-code".into()],
            }],
        );
        retain_plugin_source(&mut records[0]);
        assert_eq!(records[0].source, InstallSource::Plugin);
        assert_eq!(
            records[0].source_url.as_deref(),
            Some("https://github.com/ex/skills")
        );
    }

    #[test]
    fn gh_enrich_sets_gh_when_host_copy_exists_beside_plugin() {
        let mut host = disc("tdd", Agent::Cursor, Scope::User);
        host.location.path = PathBuf::from("/home/.cursor/skills/tdd");
        let mut plugin = disc("tdd", Agent::ClaudeCode, Scope::User);
        plugin.location.path = PathBuf::from("/home/.claude/plugins/cache/m/sp/1.0.0/skills/tdd");
        plugin.source = Some(InstallSource::Plugin);
        let mut records = merge_discovered(vec![host, plugin]);
        assert_eq!(records[0].source, InstallSource::Plugin);
        enrich_with_gh(
            &mut records,
            &[GhSkillListItem {
                skill_name: "tdd".into(),
                path: "/home/.cursor/skills/tdd".into(),
                scope: "user".into(),
                source_url: "https://github.com/ex/skills".into(),
                version: "v2".into(),
                pinned: false,
                agent_hosts: vec!["cursor".into()],
            }],
        );
        retain_plugin_source(&mut records[0]);
        assert_eq!(records[0].source, InstallSource::Gh);
    }

    #[test]
    fn enrich_skill_lock_marks_npx_without_overriding_gh() {
        let mut records =
            merge_discovered(vec![disc("find-skills", Agent::ClaudeCode, Scope::User), {
                let mut d = disc("tdd", Agent::Cursor, Scope::User);
                d.source_url = Some("https://github.com/ex/skills".into());
                d
            }]);
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
    fn build_inventory_merges_plugin_skills_with_regular_roots() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("proj");
        let home = tmp.path().join("home");

        // Regular install for claude-code.
        std::fs::create_dir_all(home.join(".claude/skills/tdd")).unwrap();
        std::fs::write(
            home.join(".claude/skills/tdd/SKILL.md"),
            "---\nname: tdd\ndescription: TDD\n---\n",
        )
        .unwrap();

        // Same skill bundled inside a claude plugin.
        let plugin = home.join(".claude/plugins/cache/claude-plugins-official/superpowers/6.2.0");
        std::fs::create_dir_all(plugin.join("skills/tdd")).unwrap();
        std::fs::write(
            plugin.join("skills/tdd/SKILL.md"),
            "---\nname: tdd\ndescription: TDD via plugin\n---\n",
        )
        .unwrap();
        std::fs::create_dir_all(home.join(".claude/plugins")).unwrap();
        std::fs::write(
            home.join(".claude/plugins/installed_plugins.json"),
            format!(
                r#"{{"version":2,"plugins":{{"superpowers@m":[
                  {{"scope":"user","installPath":"{}","version":"6.2.0"}}
                ]}}}}"#,
                plugin.display()
            ),
        )
        .unwrap();

        let runner = crate::adapters::command::FakeCommandRunner::default();
        let inventory =
            build_inventory(&project, &home, &runner, &InventoryOptions::default()).unwrap();
        let records = inventory.skills;

        let tdd = records.iter().find(|r| r.name == "tdd").unwrap();
        assert_eq!(tdd.locations.len(), 2);
        assert!(
            tdd.locations
                .iter()
                .any(|l| l.agent == Agent::ClaudeCode && l.path.ends_with(".claude/skills/tdd"))
        );
        assert!(tdd.locations.iter().any(
            |l| l.agent == Agent::ClaudeCode && l.path.to_string_lossy().contains("/plugins/")
        ));
        assert_eq!(tdd.source, InstallSource::Plugin);
    }

    #[test]
    fn build_inventory_includes_plugins_and_mcp() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("proj");
        let home = tmp.path().join("home");
        let plugin = home.join(".cursor/plugins/cache/cursor-public/context7/abc123");
        std::fs::create_dir_all(plugin.join("skills/context7")).unwrap();
        std::fs::write(
            plugin.join("skills/context7/SKILL.md"),
            "---\nname: context7\ndescription: docs\n---\n",
        )
        .unwrap();
        std::fs::write(
            plugin.join("mcp.json"),
            r#"{"mcpServers":{"docs":{"type":"stdio","command":"npx"}}}"#,
        )
        .unwrap();

        let runner = crate::adapters::command::FakeCommandRunner::default();
        let inventory =
            build_inventory(&project, &home, &runner, &InventoryOptions::default()).unwrap();
        assert!(inventory.plugins.iter().any(|p| p.name == "context7"));
        assert!(inventory.mcp.iter().any(|m| m.name == "docs"));
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
            author: None,
            version: None,
            pinned: false,
            source: None,
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
