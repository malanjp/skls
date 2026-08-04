//! Filesystem skill scanner for Cursor / Claude Code / Codex.

use crate::model::{Agent, InstallKind, Scope, SkillLocation};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub location: SkillLocation,
    pub source_url: Option<String>,
    pub version: Option<String>,
    pub pinned: bool,
}

#[derive(Debug, Deserialize)]
struct Frontmatter {
    name: Option<String>,
    description: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(rename = "sourceURL", default)]
    source_url: Option<String>,
    version: Option<String>,
    pinned: Option<bool>,
    #[serde(default)]
    metadata: Option<SkillMetadata>,
}

#[derive(Debug, Deserialize)]
struct SkillMetadata {
    #[serde(rename = "github-repo", default)]
    github_repo: Option<String>,
}

/// Paths relative to project root / home for each agent.
pub fn skill_roots(project_root: &Path, home: &Path) -> Vec<(Agent, Scope, PathBuf)> {
    vec![
        (
            Agent::Cursor,
            Scope::Project,
            project_root.join(".agents/skills"),
        ),
        (
            Agent::Cursor,
            Scope::User,
            home.join(".cursor/skills"),
        ),
        (
            Agent::ClaudeCode,
            Scope::Project,
            project_root.join(".claude/skills"),
        ),
        (
            Agent::ClaudeCode,
            Scope::User,
            home.join(".claude/skills"),
        ),
        (
            Agent::Codex,
            Scope::Project,
            project_root.join(".agents/skills"),
        ),
        (Agent::Codex, Scope::User, home.join(".codex/skills")),
    ]
}

pub fn parse_skill_md(content: &str) -> Result<(String, String, Option<String>, Option<String>, bool)> {
    let fm = extract_frontmatter(content)?;
    let name = fm
        .name
        .clone()
        .unwrap_or_else(|| "unnamed".to_string());
    let description = fm.description.unwrap_or_default();
    let meta_repo = fm
        .metadata
        .as_ref()
        .and_then(|m| m.github_repo.clone());
    let source_url = fm.source_url.or(fm.source).or(meta_repo);
    let version = fm.version;
    let pinned = fm.pinned.unwrap_or(false);
    Ok((name, description, source_url, version, pinned))
}

fn extract_frontmatter(content: &str) -> Result<Frontmatter> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return Ok(Frontmatter {
            name: None,
            description: None,
            source: None,
            source_url: None,
            version: None,
            pinned: None,
            metadata: None,
        });
    }
    let rest = &trimmed[3..];
    let end = rest
        .find("\n---")
        .context("unterminated YAML frontmatter")?;
    let yaml = &rest[..end];
    match serde_yaml::from_str::<Frontmatter>(yaml) {
        Ok(fm) => Ok(fm),
        Err(_) => Ok(parse_frontmatter_lenient(yaml)),
    }
}

/// Best-effort key: value parser for SKILL.md files with unquoted colons.
fn parse_frontmatter_lenient(yaml: &str) -> Frontmatter {
    let mut fm = Frontmatter {
        name: None,
        description: None,
        source: None,
        source_url: None,
        version: None,
        pinned: None,
        metadata: None,
    };
    for line in yaml.lines() {
        let raw = line.trim_end();
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let value = value
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .to_string();
        match key {
            "name" => fm.name = Some(value),
            "description" => fm.description = Some(value),
            "source" => fm.source = Some(value),
            "sourceURL" | "source_url" => fm.source_url = Some(value),
            "github-repo" => {
                let meta = fm.metadata.get_or_insert(SkillMetadata {
                    github_repo: None,
                });
                meta.github_repo = Some(value);
            }
            "version" => fm.version = Some(value),
            "pinned" => {
                fm.pinned = Some(matches!(
                    value.to_lowercase().as_str(),
                    "true" | "yes" | "1"
                ));
            }
            _ => {}
        }
    }
    fm
}

pub fn detect_install_kind(path: &Path) -> (InstallKind, Option<PathBuf>) {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            let resolved = fs::read_link(path).ok().map(|target| {
                if target.is_absolute() {
                    target
                } else {
                    path.parent()
                        .unwrap_or_else(|| Path::new("."))
                        .join(target)
                }
            });
            (InstallKind::Symlink, resolved)
        }
        Ok(_) => (InstallKind::Copy, None),
        Err(_) => (InstallKind::Unknown, None),
    }
}

pub fn scan_skills(project_root: &Path, home: &Path) -> Result<(Vec<DiscoveredSkill>, Vec<String>)> {
    let mut out = Vec::new();
    let mut warnings = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();

    for (agent, scope, root) in skill_roots(project_root, home) {
        if !root.is_dir() {
            continue;
        }
        collect_skills_in_dir(
            &root,
            agent,
            scope,
            &mut out,
            &mut seen_paths,
            &mut warnings,
        )?;
    }
    Ok((out, warnings))
}

fn collect_skills_in_dir(
    root: &Path,
    agent: Agent,
    scope: Scope,
    out: &mut Vec<DiscoveredSkill>,
    seen_paths: &mut std::collections::HashSet<(Agent, PathBuf)>,
    warnings: &mut Vec<String>,
) -> Result<()> {
    let entries = match fs::read_dir(root) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() && !path.is_symlink() {
            // Allow symlink-to-dir: check metadata
            let meta = fs::symlink_metadata(&path);
            if !matches!(meta, Ok(m) if m.file_type().is_symlink() || m.is_dir()) {
                continue;
            }
        }

        // Nested system skills (e.g. .system/imagegen)
        let skill_md = path.join("SKILL.md");
        if skill_md.is_file() || fs::symlink_metadata(&skill_md).is_ok() {
            if !seen_paths.insert((agent, path.clone())) {
                continue;
            }
            match read_discovered(&path, agent, scope) {
                Ok(Some(discovered)) => out.push(discovered),
                Ok(None) => {}
                Err(err) => warnings.push(format!("skip {}: {err}", path.display())),
            }
            continue;
        }

        // One level of nesting for namespaced skills
        if path.is_dir() {
            let nested = match fs::read_dir(&path) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for nested_entry in nested.flatten() {
                let nested_path = nested_entry.path();
                let nested_md = nested_path.join("SKILL.md");
                if nested_md.is_file() || fs::symlink_metadata(&nested_md).is_ok() {
                    if !seen_paths.insert((agent, nested_path.clone())) {
                        continue;
                    }
                    match read_discovered(&nested_path, agent, scope) {
                        Ok(Some(discovered)) => out.push(discovered),
                        Ok(None) => {}
                        Err(err) => {
                            warnings.push(format!("skip {}: {err}", nested_path.display()))
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn read_discovered(path: &Path, agent: Agent, scope: Scope) -> Result<Option<DiscoveredSkill>> {
    let skill_md = path.join("SKILL.md");
    let content = match fs::read_to_string(&skill_md) {
        Ok(c) => c,
        Err(_) => return Ok(None),
    };
    let (name, description, source_url, version, pinned) = match parse_skill_md(&content) {
        Ok(v) => v,
        Err(_) => {
            // Fallback: tolerate invalid YAML by using directory name.
            let id = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("unnamed")
                .to_string();
            (id.clone(), String::new(), None, None, false)
        }
    };
    let id = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(&name)
        .to_string();
    let (kind, resolved) = detect_install_kind(path);
    Ok(Some(DiscoveredSkill {
        id: id.clone(),
        name: if name == "unnamed" { id } else { name },
        description,
        location: SkillLocation {
            agent,
            scope,
            path: path.to_path_buf(),
            kind,
            resolved,
        },
        source_url,
        version,
        pinned,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn parse_frontmatter_basic() {
        let content = r#"---
name: brainstorming
description: Explore ideas before coding
---
# Body
"#;
        let (name, desc, _, _, _) = parse_skill_md(content).unwrap();
        assert_eq!(name, "brainstorming");
        assert!(desc.contains("Explore"));
    }

    #[test]
    fn parse_frontmatter_with_unquoted_colon() {
        let content = r#"---
name: weird
description: Use this when: you need colons
---
"#;
        let (name, desc, _, _, _) = parse_skill_md(content).unwrap();
        assert_eq!(name, "weird");
        assert!(desc.contains("colons"));
    }

    #[test]
    fn parse_frontmatter_github_repo_metadata() {
        let content = r#"---
name: tdd
description: Test-driven development
metadata:
    github-path: skills/engineering/tdd
    github-repo: https://github.com/mattpocock/skills
    github-tree-sha: abc123
---
"#;
        let (name, _, source_url, _, _) = parse_skill_md(content).unwrap();
        assert_eq!(name, "tdd");
        assert_eq!(
            source_url.as_deref(),
            Some("https://github.com/mattpocock/skills")
        );
    }

    #[test]
    fn scan_project_and_user_skills() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("proj");
        let home = tmp.path().join("home");
        fs::create_dir_all(project.join(".claude/skills/alpha")).unwrap();
        fs::write(
            project.join(".claude/skills/alpha/SKILL.md"),
            "---\nname: alpha\ndescription: Alpha skill\n---\n",
        )
        .unwrap();
        fs::create_dir_all(home.join(".cursor/skills")).unwrap();
        let target = tmp.path().join("canonical/beta");
        fs::create_dir_all(&target).unwrap();
        fs::write(
            target.join("SKILL.md"),
            "---\nname: beta\ndescription: Beta skill\nsourceURL: https://example.com/beta\n---\n",
        )
        .unwrap();
        symlink(&target, home.join(".cursor/skills/beta")).unwrap();

        let (found, _warnings) = scan_skills(&project, &home).unwrap();
        assert!(found.iter().any(|s| s.name == "alpha"));
        let beta = found.iter().find(|s| s.name == "beta").unwrap();
        assert_eq!(beta.location.kind, InstallKind::Symlink);
        assert_eq!(
            beta.source_url.as_deref(),
            Some("https://example.com/beta")
        );
    }
}
