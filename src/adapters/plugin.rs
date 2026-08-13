//! Plugin-bundled skill scanner.
//!
//! Finds `skills/` directories inside installed agent plugins:
//!
//! - Claude Code: `~/.claude/plugins/` (installed list from
//!   `installed_plugins.json`, which also carries scope / version)
//! - Cursor / Codex: `~/.cursor/plugins/cache/*/*/*/` and
//!   `~/.codex/plugins/cache/*/*/*/` (walked directly, deduped by path)
//! - Shared agents store: `~/.agents/plugins/` (lenient depth-bounded walk,
//!   attributed to every host that reads `~/.agents`)
//!
//! Attribution follows where the files physically live: Claude Code plugins →
//! `claude-code`, Cursor plugins → `cursor`, Codex plugins → `codex`, and the
//! `~/.agents` store → the shared-store agent set.

use crate::adapters::fs::{
    AGENTS_SHARED_STORE, DiscoveredSkill, collect_skills_in_dir, detect_install_kind,
};
use crate::adapters::mcp::{DiscoveredMcp, read_mcp_files};
use crate::model::{Agent, InstallSource, Scope, SkillLocation};
use anyhow::Result;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

type SeenPaths = HashSet<(Agent, PathBuf)>;

#[derive(Debug, Clone, Default)]
pub struct PluginScan {
    pub skills: Vec<DiscoveredSkill>,
    pub plugins: Vec<DiscoveredPlugin>,
    pub mcp: Vec<DiscoveredMcp>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredPlugin {
    pub name: String,
    pub description: String,
    pub version: Option<String>,
    pub author: Option<String>,
    pub source_url: Option<String>,
    pub marketplace: Option<String>,
    pub spec: String,
    pub location: SkillLocation,
    pub skill_names: Vec<String>,
    pub mcp_names: Vec<String>,
}

/// Scan every plugin store for skills.
pub fn scan_plugin_skills(
    project_root: &Path,
    home: &Path,
) -> Result<(Vec<DiscoveredSkill>, Vec<String>)> {
    let scan = scan_plugin_inventory(project_root, home)?;
    Ok((scan.skills, scan.warnings))
}

pub fn scan_plugin_inventory(project_root: &Path, home: &Path) -> Result<PluginScan> {
    let mut scan = PluginScan::default();

    scan_claude_plugins(home, &mut scan)?;
    scan_cache_plugins(
        &home.join(".cursor/plugins/cache"),
        Agent::Cursor,
        &mut scan,
    )?;
    scan_cache_plugins(&home.join(".codex/plugins/cache"), Agent::Codex, &mut scan)?;
    scan_agents_plugins(home, &mut scan)?;
    let _ = project_root;

    scan.skills.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then(a.location.agent.as_str().cmp(b.location.agent.as_str()))
    });
    Ok(scan)
}

/// Claude Code plugins: read the installed list, scan each `install_path/skills/`.
fn scan_claude_plugins(home: &Path, scan: &mut PluginScan) -> Result<()> {
    let json_path = home.join(".claude/plugins/installed_plugins.json");
    let content = match fs::read_to_string(&json_path) {
        Ok(c) => c,
        Err(_) => return Ok(()),
    };
    let installed: InstalledPluginsJson = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(err) => {
            scan.warnings
                .push(format!("skip {}: {err}", json_path.display()));
            return Ok(());
        }
    };

    // Same path can be enabled under both scopes; keep a seen set per scope.
    let mut seen_user = SeenPaths::new();
    let mut seen_project = SeenPaths::new();

    for (key, entries) in installed.plugins {
        let marketplace = key.split_once('@').map(|(_, m)| m.to_string());
        for entry in entries {
            let scope = match entry.scope.as_str() {
                "user" => Scope::User,
                "project" | "local" => Scope::Project,
                other => {
                    scan.warnings.push(format!(
                        "skip claude plugin {}: unknown scope {other:?}",
                        entry.install_path
                    ));
                    continue;
                }
            };
            let plugin_dir = PathBuf::from(&entry.install_path);
            let seen = match scope {
                Scope::User => &mut seen_user,
                Scope::Project => &mut seen_project,
            };
            ingest_plugin_dir(
                &plugin_dir,
                Agent::ClaudeCode,
                scope,
                marketplace.clone(),
                key.clone(),
                entry.version.clone(),
                seen,
                scan,
            )?;
        }
    }
    Ok(())
}

/// Cursor / Codex plugins: walk `cache/<marketplace>/<plugin>/<version>/skills/`.
fn scan_cache_plugins(cache_root: &Path, agent: Agent, scan: &mut PluginScan) -> Result<()> {
    let mut seen = SeenPaths::new();
    let marketplaces = match fs::read_dir(cache_root) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };
    for marketplace in marketplaces.flatten() {
        if !marketplace.path().is_dir() {
            continue;
        }
        let market_name = marketplace.file_name().to_string_lossy().into_owned();
        let plugins = match fs::read_dir(marketplace.path()) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for plugin in plugins.flatten() {
            if !plugin.path().is_dir() {
                continue;
            }
            let plugin_name = plugin.file_name().to_string_lossy().into_owned();
            let versions = match fs::read_dir(plugin.path()) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for version in versions.flatten() {
                if !version.path().is_dir() {
                    continue;
                }
                let spec = format!("{plugin_name}@{market_name}");
                ingest_plugin_dir(
                    &version.path(),
                    agent,
                    Scope::User,
                    Some(market_name.clone()),
                    spec,
                    None,
                    &mut seen,
                    scan,
                )?;
            }
        }
    }
    Ok(())
}

/// Shared agents store: find plugin roots (skills/ or mcp.json) up to a bounded
/// depth and attribute each to every host that reads `~/.agents`.
fn scan_agents_plugins(home: &Path, scan: &mut PluginScan) -> Result<()> {
    let root = home.join(".agents/plugins");
    if !root.is_dir() {
        return Ok(());
    }
    let mut plugin_dirs = Vec::new();
    walk_plugin_roots(&root, 0, 5, &mut plugin_dirs);

    let mut seen = SeenPaths::new();
    for plugin_dir in plugin_dirs {
        let name = plugin_dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "plugin".into());
        for &agent in AGENTS_SHARED_STORE {
            ingest_plugin_dir(
                &plugin_dir,
                agent,
                Scope::User,
                None,
                name.clone(),
                None,
                &mut seen,
                scan,
            )?;
        }
    }
    Ok(())
}

fn walk_plugin_roots(dir: &Path, depth: usize, max_depth: usize, out: &mut Vec<PathBuf>) {
    if depth > max_depth {
        return;
    }
    if depth > 0 && is_plugin_root(dir) {
        out.push(dir.to_path_buf());
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_plugin_roots(&path, depth + 1, max_depth, out);
        }
    }
}

fn is_plugin_root(dir: &Path) -> bool {
    dir.join("skills").is_dir()
        || dir.join("mcp.json").is_file()
        || dir.join(".mcp.json").is_file()
        || dir.join("plugin.json").is_file()
        || dir.join(".claude-plugin").is_dir()
        || dir.join(".codex-plugin").is_dir()
}

fn ingest_plugin_dir(
    plugin_dir: &Path,
    agent: Agent,
    scope: Scope,
    marketplace: Option<String>,
    spec: String,
    version_hint: Option<String>,
    seen: &mut SeenPaths,
    scan: &mut PluginScan,
) -> Result<()> {
    let meta = plugin_meta(plugin_dir);
    let mut local = Vec::new();
    collect_skills_in_dir(
        &plugin_dir.join("skills"),
        agent,
        scope,
        &mut local,
        seen,
        &mut scan.warnings,
    )?;
    for skill in &mut local {
        skill.source = Some(InstallSource::Plugin);
        if skill.version.is_none() {
            skill.version = version_hint.clone().or_else(|| meta.version.clone());
        }
        if skill.source_url.is_none() {
            skill.source_url = meta.source_url.clone();
        }
        if skill.author.is_none() {
            skill.author = meta.author.clone();
        }
    }
    let mut skill_names: Vec<String> = local.iter().map(|s| s.name.clone()).collect();
    skill_names.sort();
    skill_names.dedup();

    let plugin_name = meta
        .name
        .clone()
        .or_else(|| {
            spec.split_once('@')
                .map(|(n, _)| n.to_string())
                .or_else(|| Some(spec.clone()))
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "plugin".into());

    let (parsed_mcp, mcp_warns) = read_mcp_files(plugin_dir)?;
    scan.warnings.extend(mcp_warns);
    let (kind, resolved) = detect_install_kind(plugin_dir);
    let location = SkillLocation {
        agent,
        scope,
        path: plugin_dir.to_path_buf(),
        kind,
        resolved,
    };
    let mut mcp_names = Vec::new();
    for server in parsed_mcp {
        mcp_names.push(server.name.clone());
        scan.mcp.push(DiscoveredMcp {
            name: server.name,
            transport: server.transport,
            command: server.command,
            args: server.args,
            url: server.url,
            plugin: Some(plugin_name.clone()),
            location: location.clone(),
        });
    }
    mcp_names.sort();
    mcp_names.dedup();

    scan.plugins.push(DiscoveredPlugin {
        name: plugin_name,
        description: meta.description.unwrap_or_default(),
        version: version_hint.or(meta.version),
        author: meta.author,
        source_url: meta.source_url,
        marketplace,
        spec,
        location,
        skill_names,
        mcp_names,
    });
    scan.skills.extend(local);
    Ok(())
}

/// Best-effort provenance from a plugin manifest (`repository` / `homepage` /
/// `author`).
fn plugin_meta(plugin_dir: &Path) -> PluginMeta {
    let mut meta = PluginMeta::default();
    let mut repository = None;
    let mut homepage = None;
    let manifests = [
        plugin_dir.join(".claude-plugin/plugin.json"),
        plugin_dir.join(".codex-plugin/plugin.json"),
        plugin_dir.join("plugin.json"),
    ];
    for manifest in manifests {
        let Ok(content) = fs::read_to_string(&manifest) else {
            continue;
        };
        let Ok(m) = serde_json::from_str::<PluginManifest>(&content) else {
            continue;
        };
        if meta.name.is_none() {
            meta.name = nonempty(m.name);
        }
        if meta.description.is_none() {
            meta.description = nonempty(m.description);
        }
        if meta.version.is_none() {
            meta.version = nonempty(m.version);
        }
        if repository.is_none() {
            repository = nonempty(m.repository);
        }
        if homepage.is_none() {
            homepage = nonempty(m.homepage);
        }
        if meta.author.is_none() {
            meta.author = match m.author {
                Some(ManifestAuthor::Named { name }) => nonempty(name),
                Some(ManifestAuthor::Plain(name)) => nonempty(Some(name)),
                None => None,
            };
        }
    }
    meta.source_url = repository.or(homepage);
    meta
}

fn nonempty(value: Option<String>) -> Option<String> {
    value.filter(|s| !s.is_empty())
}

#[derive(Debug, Clone, Default)]
struct PluginMeta {
    name: Option<String>,
    description: Option<String>,
    version: Option<String>,
    source_url: Option<String>,
    author: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InstalledPluginsJson {
    #[serde(default)]
    plugins: HashMap<String, Vec<InstalledPluginEntry>>,
}

#[derive(Debug, Deserialize)]
struct InstalledPluginEntry {
    scope: String,
    #[serde(rename = "installPath")]
    install_path: String,
    version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PluginManifest {
    name: Option<String>,
    description: Option<String>,
    version: Option<String>,
    repository: Option<String>,
    homepage: Option<String>,
    author: Option<ManifestAuthor>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ManifestAuthor {
    Named { name: Option<String> },
    Plain(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::InstallKind;

    fn write_skill(dir: &Path, name: &str) {
        fs::create_dir_all(dir.join(name)).unwrap();
        fs::write(
            dir.join(name).join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {name} skill\n---\n"),
        )
        .unwrap();
    }

    #[test]
    fn scan_claude_plugins_uses_installed_list_and_scope() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let sp = home.join(".claude/plugins/cache/claude-plugins-official/superpowers/6.2.0");
        let serena = home.join(".claude/plugins/cache/claude-plugins-official/serena/unknown");
        write_skill(&sp.join("skills"), "tdd");
        write_skill(&sp.join("skills/engineering"), "debugging");
        write_skill(&serena.join("skills"), "serena");
        fs::create_dir_all(sp.join(".claude-plugin")).unwrap();
        fs::write(
            sp.join(".claude-plugin/plugin.json"),
            r#"{"name":"superpowers","version":"6.2.0","author":{"name":"Jesse Vincent"},"repository":"https://github.com/obra/superpowers"}"#,
        )
        .unwrap();
        let plugins_dir = home.join(".claude/plugins");
        fs::create_dir_all(&plugins_dir).unwrap();
        fs::write(
            plugins_dir.join("installed_plugins.json"),
            format!(
                r#"{{
  "version": 2,
  "plugins": {{
    "superpowers@claude-plugins-official": [
      {{"scope": "user", "installPath": "{}", "version": "6.2.0"}}
    ],
    "serena@claude-plugins-official": [
      {{"scope": "project", "installPath": "{}", "version": "unknown"}}
    ]
  }}
}}"#,
                sp.display(),
                serena.display()
            ),
        )
        .unwrap();

        let (found, _warnings) = scan_plugin_skills(Path::new("/proj"), &home).unwrap();
        let by_name = |n: &str| found.iter().find(|s| s.name == n).unwrap();

        let tdd = by_name("tdd");
        assert_eq!(tdd.location.agent, Agent::ClaudeCode);
        assert_eq!(tdd.location.scope, Scope::User);
        assert_eq!(tdd.source, Some(InstallSource::Plugin));
        assert_eq!(tdd.version.as_deref(), Some("6.2.0"));
        assert_eq!(
            tdd.source_url.as_deref(),
            Some("https://github.com/obra/superpowers")
        );
        assert_eq!(tdd.author.as_deref(), Some("Jesse Vincent"));

        let debugging = by_name("debugging");
        assert_eq!(debugging.location.agent, Agent::ClaudeCode);
        assert_eq!(debugging.location.scope, Scope::User);

        let serena_skill = by_name("serena");
        assert_eq!(serena_skill.location.scope, Scope::Project);
    }

    #[test]
    fn scan_cursor_and_codex_cache_plugins() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        write_skill(
            &home.join(".cursor/plugins/cache/claude-plugins-official/context7/abc123/skills"),
            "context7",
        );
        write_skill(
            &home.join(".codex/plugins/cache/openai-curated/linear/abc123/skills"),
            "linear",
        );

        let (found, _warnings) = scan_plugin_skills(Path::new("/proj"), &home).unwrap();
        let context7 = found.iter().find(|s| s.name == "context7").unwrap();
        assert_eq!(context7.location.agent, Agent::Cursor);
        assert_eq!(context7.location.scope, Scope::User);
        assert_eq!(context7.source, Some(InstallSource::Plugin));
        assert_eq!(context7.location.kind, InstallKind::Copy);

        let linear = found.iter().find(|s| s.name == "linear").unwrap();
        assert_eq!(linear.location.agent, Agent::Codex);
        assert_eq!(linear.location.scope, Scope::User);
    }

    #[test]
    fn scan_agents_plugins_attributes_to_shared_store() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        write_skill(
            &home.join(".agents/plugins/mattpocock/market/skills"),
            "foo",
        );
        write_skill(&home.join(".agents/plugins/other/bar/skills"), "bar");

        let (found, _warnings) = scan_plugin_skills(Path::new("/proj"), &home).unwrap();
        for name in ["foo", "bar"] {
            let matches: Vec<_> = found.iter().filter(|s| s.name == name).collect();
            assert_eq!(matches.len(), AGENTS_SHARED_STORE.len(), "{name}");
            for skill in &matches {
                assert!(
                    AGENTS_SHARED_STORE.contains(&skill.location.agent),
                    "{name} on {}",
                    skill.location.agent
                );
                assert_eq!(skill.location.scope, Scope::User);
                assert_eq!(skill.source, Some(InstallSource::Plugin));
            }
            let agents: Vec<&'static str> =
                matches.iter().map(|s| s.location.agent.as_str()).collect();
            let mut dedup = agents.clone();
            dedup.sort_unstable();
            dedup.dedup();
            assert_eq!(dedup.len(), AGENTS_SHARED_STORE.len(), "{name} distinct");
        }
    }

    #[test]
    fn scan_plugin_skills_ignores_missing_stores() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let (found, warnings) = scan_plugin_skills(Path::new("/proj"), &home).unwrap();
        assert!(found.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn scan_plugin_inventory_collects_mcp_and_plugin_records() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let plugin = home.join(".cursor/plugins/cache/cursor-public/context7/abc123");
        write_skill(&plugin.join("skills"), "context7");
        fs::write(
            plugin.join("mcp.json"),
            r#"{
              "$schema": "https://agent-plugins.org/schemas/1.0.0/mcp.schema.json",
              "mcpServers": {
                "docs": {
                  "type": "stdio",
                  "command": "npx",
                  "args": ["-y", "@upstash/context7-mcp"]
                }
              }
            }"#,
        )
        .unwrap();
        fs::write(
            plugin.join("plugin.json"),
            r#"{"name":"context7","description":"Library docs"}"#,
        )
        .unwrap();

        let scan = scan_plugin_inventory(Path::new("/proj"), &home).unwrap();
        assert_eq!(scan.mcp.len(), 1);
        assert_eq!(scan.mcp[0].name, "docs");
        assert_eq!(scan.mcp[0].plugin.as_deref(), Some("context7"));
        assert_eq!(scan.mcp[0].location.agent, Agent::Cursor);
        let pkg = scan.plugins.iter().find(|p| p.name == "context7").unwrap();
        assert_eq!(pkg.spec, "context7@cursor-public");
        assert_eq!(pkg.mcp_names, vec!["docs"]);
        assert!(pkg.skill_names.contains(&"context7".into()));
    }

    #[test]
    fn scan_agents_plugins_finds_mcp_only_plugin() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let plugin = home.join(".agents/plugins/mcp-only");
        fs::create_dir_all(&plugin).unwrap();
        fs::write(
            plugin.join("mcp.json"),
            r#"{"mcpServers":{"github":{"command":"npx","args":["-y","@modelcontextprotocol/server-github"]}}}"#,
        )
        .unwrap();

        let scan = scan_plugin_inventory(Path::new("/proj"), &home).unwrap();
        assert!(scan.skills.is_empty());
        assert_eq!(scan.mcp.len(), AGENTS_SHARED_STORE.len());
        assert!(scan.mcp.iter().all(|m| m.name == "github"));
        let pkg = scan.plugins.iter().find(|p| p.name == "mcp-only").unwrap();
        assert_eq!(pkg.mcp_names, vec!["github"]);
        assert!(pkg.skill_names.is_empty());
    }
}
