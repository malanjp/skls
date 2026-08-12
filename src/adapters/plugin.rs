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

use crate::adapters::fs::{AGENTS_SHARED_STORE, DiscoveredSkill, collect_skills_in_dir};
use crate::model::{Agent, InstallSource, Scope};
use anyhow::Result;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

type SeenPaths = HashSet<(Agent, PathBuf)>;

/// Scan every plugin store for skills.
pub fn scan_plugin_skills(
    _project_root: &Path,
    home: &Path,
) -> Result<(Vec<DiscoveredSkill>, Vec<String>)> {
    let mut out = Vec::new();
    let mut warnings = Vec::new();

    scan_claude_plugins(home, &mut out, &mut warnings)?;
    scan_cache_plugins(
        &home.join(".cursor/plugins/cache"),
        Agent::Cursor,
        &mut out,
        &mut warnings,
    )?;
    scan_cache_plugins(
        &home.join(".codex/plugins/cache"),
        Agent::Codex,
        &mut out,
        &mut warnings,
    )?;
    scan_agents_plugins(home, &mut out, &mut warnings)?;

    out.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then(a.location.agent.as_str().cmp(b.location.agent.as_str()))
    });
    Ok((out, warnings))
}

/// Claude Code plugins: read the installed list, scan each `install_path/skills/`.
fn scan_claude_plugins(
    home: &Path,
    out: &mut Vec<DiscoveredSkill>,
    warnings: &mut Vec<String>,
) -> Result<()> {
    let json_path = home.join(".claude/plugins/installed_plugins.json");
    let content = match fs::read_to_string(&json_path) {
        Ok(c) => c,
        Err(_) => return Ok(()),
    };
    let installed: InstalledPluginsJson = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(err) => {
            warnings.push(format!("skip {}: {err}", json_path.display()));
            return Ok(());
        }
    };

    // Same path can be enabled under both scopes; keep a seen set per scope.
    let mut seen_user = SeenPaths::new();
    let mut seen_project = SeenPaths::new();

    for entries in installed.plugins.values() {
        for entry in entries {
            let scope = match entry.scope.as_str() {
                "user" => Scope::User,
                "project" | "local" => Scope::Project,
                other => {
                    warnings.push(format!(
                        "skip claude plugin {}: unknown scope {other:?}",
                        entry.install_path
                    ));
                    continue;
                }
            };
            let plugin_dir = PathBuf::from(&entry.install_path);
            let meta = plugin_meta(&plugin_dir);
            let seen = match scope {
                Scope::User => &mut seen_user,
                Scope::Project => &mut seen_project,
            };
            let mut local = Vec::new();
            collect_skills_in_dir(
                &plugin_dir.join("skills"),
                Agent::ClaudeCode,
                scope,
                &mut local,
                seen,
                warnings,
            )?;
            for skill in &mut local {
                skill.source = Some(InstallSource::Plugin);
                if skill.version.is_none() {
                    skill.version = entry.version.clone();
                }
                if skill.source_url.is_none() {
                    skill.source_url = meta.source_url.clone();
                }
                if skill.author.is_none() {
                    skill.author = meta.author.clone();
                }
            }
            out.extend(local);
        }
    }
    Ok(())
}

/// Cursor / Codex plugins: walk `cache/<marketplace>/<plugin>/<version>/skills/`.
fn scan_cache_plugins(
    cache_root: &Path,
    agent: Agent,
    out: &mut Vec<DiscoveredSkill>,
    warnings: &mut Vec<String>,
) -> Result<()> {
    let mut seen = SeenPaths::new();
    let marketplaces = match fs::read_dir(cache_root) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };
    for marketplace in marketplaces.flatten() {
        if !marketplace.path().is_dir() {
            continue;
        }
        let plugins = match fs::read_dir(marketplace.path()) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for plugin in plugins.flatten() {
            if !plugin.path().is_dir() {
                continue;
            }
            let versions = match fs::read_dir(plugin.path()) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for version in versions.flatten() {
                if !version.path().is_dir() {
                    continue;
                }
                let source_url = plugin_meta(&version.path());
                let mut local = Vec::new();
                collect_skills_in_dir(
                    &version.path().join("skills"),
                    agent,
                    Scope::User,
                    &mut local,
                    &mut seen,
                    warnings,
                )?;
                for skill in &mut local {
                    skill.source = Some(InstallSource::Plugin);
                    if skill.source_url.is_none() {
                        skill.source_url = source_url.source_url.clone();
                    }
                    if skill.author.is_none() {
                        skill.author = source_url.author.clone();
                    }
                }
                out.extend(local);
            }
        }
    }
    Ok(())
}

/// Shared agents store: find `skills/` dirs up to a bounded depth and attribute
/// each to every host that reads `~/.agents`.
fn scan_agents_plugins(
    home: &Path,
    out: &mut Vec<DiscoveredSkill>,
    warnings: &mut Vec<String>,
) -> Result<()> {
    let root = home.join(".agents/plugins");
    if !root.is_dir() {
        return Ok(());
    }
    let mut skills_dirs = Vec::new();
    walk_skills_dirs(&root, 0, 5, &mut skills_dirs);

    let mut seen = SeenPaths::new();
    for skills_dir in skills_dirs {
        for &agent in AGENTS_SHARED_STORE {
            let mut local = Vec::new();
            collect_skills_in_dir(
                &skills_dir,
                agent,
                Scope::User,
                &mut local,
                &mut seen,
                warnings,
            )?;
            for skill in &mut local {
                skill.source = Some(InstallSource::Plugin);
            }
            out.extend(local);
        }
    }
    Ok(())
}

fn walk_skills_dirs(dir: &Path, depth: usize, max_depth: usize, out: &mut Vec<PathBuf>) {
    if depth > max_depth {
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if path.file_name().is_some_and(|n| n == "skills") {
            out.push(path);
            continue;
        }
        walk_skills_dirs(&path, depth + 1, max_depth, out);
    }
}

/// Best-effort provenance from a plugin manifest (`repository` / `homepage` /
/// `author`).
fn plugin_meta(plugin_dir: &Path) -> PluginMeta {
    let manifests = [
        plugin_dir.join(".claude-plugin/plugin.json"),
        plugin_dir.join(".codex-plugin/plugin.json"),
        plugin_dir.join("plugin.json"),
    ];
    for manifest in manifests {
        let Ok(content) = fs::read_to_string(&manifest) else {
            continue;
        };
        if let Ok(m) = serde_json::from_str::<PluginManifest>(&content) {
            let source_url = m.repository.or(m.homepage).filter(|u| !u.is_empty());
            let author = match &m.author {
                Some(ManifestAuthor::Named { name }) => name.clone(),
                Some(ManifestAuthor::Plain(name)) => Some(name.clone()),
                None => None,
            }
            .filter(|a| !a.is_empty());
            if source_url.is_some() || author.is_some() {
                return PluginMeta { source_url, author };
            }
        }
    }
    PluginMeta {
        source_url: None,
        author: None,
    }
}

#[derive(Debug, Clone, Default)]
struct PluginMeta {
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
}
