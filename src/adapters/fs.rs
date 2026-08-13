//! Filesystem skill scanner for Cursor / Claude Code / Codex.

use crate::model::{Agent, InstallKind, InstallSource, Scope, SkillLocation};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

/// Agents that read the shared `~/.agents` store. Used to attribute
/// `~/.agents/skills` and `~/.agents/plugins` skills to every consumer host.
pub(crate) const AGENTS_SHARED_STORE: &[Agent] =
    &[Agent::Cursor, Agent::Cline, Agent::Warp, Agent::Universal];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub location: SkillLocation,
    pub source_url: Option<String>,
    pub author: Option<String>,
    pub version: Option<String>,
    pub pinned: bool,
    /// Explicit provenance override (e.g. plugin-bundled skills). `None`
    /// means "infer from `source_url` / metadata" in the inventory merge.
    pub source: Option<InstallSource>,
    /// Project root that produced this discovery. `None` for user-scope
    /// (home) skills. Tagged with the path as passed to `skill_roots`,
    /// without canonicalization.
    pub project: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
struct Frontmatter {
    name: Option<String>,
    description: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(rename = "sourceURL", default)]
    source_url: Option<String>,
    author: Option<String>,
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

/// Paths relative to project roots / home for each agent.
///
/// Project-scope entries are emitted once per `project_roots` item and tagged
/// with that root (as passed, not canonicalized). User-scope (home) entries
/// are emitted once with `project = None`.
///
/// Shared stores (e.g. `~/.agents/skills`) may appear under multiple agents;
/// inventory merge collapses them onto one skill row with several locations.
pub fn skill_roots(
    project_roots: &[PathBuf],
    home: &Path,
) -> Vec<(Agent, Scope, PathBuf, Option<PathBuf>)> {
    let mut roots = Vec::new();

    for project_root in project_roots {
        let tag = Some(project_root.to_path_buf());
        roots.extend([
            (
                Agent::Cursor,
                Scope::Project,
                project_root.join(".agents/skills"),
                tag.clone(),
            ),
            (
                Agent::Cursor,
                Scope::Project,
                project_root.join(".cursor/skills"),
                tag.clone(),
            ),
            (
                Agent::ClaudeCode,
                Scope::Project,
                project_root.join(".claude/skills"),
                tag.clone(),
            ),
            (
                Agent::Codex,
                Scope::Project,
                project_root.join(".agents/skills"),
                tag.clone(),
            ),
            (
                Agent::Codex,
                Scope::Project,
                project_root.join(".codex/skills"),
                tag,
            ),
        ]);
    }

    roots.extend([
        (
            Agent::Cursor,
            Scope::User,
            home.join(".cursor/skills"),
            None,
        ),
        (
            Agent::Cursor,
            Scope::User,
            home.join(".agents/skills"),
            None,
        ),
        (
            Agent::Cursor,
            Scope::User,
            home.join(".cursor/skills-cursor"),
            None,
        ),
        (
            Agent::ClaudeCode,
            Scope::User,
            home.join(".claude/skills"),
            None,
        ),
        (Agent::Codex, Scope::User, home.join(".codex/skills"), None),
        (
            Agent::GeminiCli,
            Scope::User,
            home.join(".gemini/skills"),
            None,
        ),
        (
            Agent::Antigravity,
            Scope::User,
            home.join(".gemini/antigravity/skills"),
            None,
        ),
        (
            Agent::AntigravityCli,
            Scope::User,
            home.join(".gemini/antigravity-cli/skills"),
            None,
        ),
        (
            Agent::Antigravity2,
            Scope::User,
            home.join(".gemini/config/skills"),
            None,
        ),
        (
            Agent::GitHubCopilot,
            Scope::User,
            home.join(".copilot/skills"),
            None,
        ),
        (
            Agent::OpenCode,
            Scope::User,
            home.join(".config/opencode/skills"),
            None,
        ),
        (Agent::Pi, Scope::User, home.join(".pi/agent/skills"), None),
        (
            Agent::Amp,
            Scope::User,
            home.join(".config/agents/skills"),
            None,
        ),
        (
            Agent::KimiCli,
            Scope::User,
            home.join(".config/agents/skills"),
            None,
        ),
        (
            Agent::Replit,
            Scope::User,
            home.join(".config/agents/skills"),
            None,
        ),
        (
            Agent::QwenCode,
            Scope::User,
            home.join(".qwen/skills"),
            None,
        ),
        (
            Agent::Augment,
            Scope::User,
            home.join(".augment/skills"),
            None,
        ),
        (
            Agent::Continue,
            Scope::User,
            home.join(".continue/skills"),
            None,
        ),
        (
            Agent::Droid,
            Scope::User,
            home.join(".factory/skills"),
            None,
        ),
        (
            Agent::Kilo,
            Scope::User,
            home.join(".kilocode/skills"),
            None,
        ),
        (Agent::Qoder, Scope::User, home.join(".qoder/skills"), None),
        (Agent::Roo, Scope::User, home.join(".roo/skills"), None),
        (Agent::Trae, Scope::User, home.join(".trae/skills"), None),
        (
            Agent::CodeBuddy,
            Scope::User,
            home.join(".codebuddy/skills"),
            None,
        ),
        (Agent::Grok, Scope::User, home.join(".grok/skills"), None),
        (Agent::Warp, Scope::User, home.join(".warp/skills"), None),
        (
            Agent::Devin,
            Scope::User,
            home.join(".config/devin/skills"),
            None,
        ),
    ]);
    // Shared `~/.agents/skills` also attributed by gh to these hosts
    for agent in AGENTS_SHARED_STORE {
        roots.push((*agent, Scope::User, home.join(".agents/skills"), None));
    }

    // Stable order for tests / debugging.
    roots.sort_by(|a, b| {
        a.0.as_str()
            .cmp(b.0.as_str())
            .then(a.1.as_str().cmp(b.1.as_str()))
            .then(a.2.cmp(&b.2))
            .then(a.3.cmp(&b.3))
    });
    roots
}

pub fn parse_skill_md(
    content: &str,
) -> Result<(
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    bool,
)> {
    let fm = extract_frontmatter(content)?;
    let name = fm.name.clone().unwrap_or_else(|| "unnamed".to_string());
    let description = fm.description.unwrap_or_default();
    let meta_repo = fm.metadata.as_ref().and_then(|m| m.github_repo.clone());
    let source_url = fm.source_url.or(fm.source).or(meta_repo);
    let author = fm.author.filter(|a| !a.is_empty());
    let version = fm.version;
    let pinned = fm.pinned.unwrap_or(false);
    Ok((name, description, source_url, author, version, pinned))
}

fn extract_frontmatter(content: &str) -> Result<Frontmatter> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return Ok(Frontmatter {
            name: None,
            description: None,
            source: None,
            source_url: None,
            author: None,
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
        author: None,
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
            "author" => fm.author = Some(value),
            "github-repo" => {
                let meta = fm
                    .metadata
                    .get_or_insert(SkillMetadata { github_repo: None });
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

/// Whether a skill dir carries GitHub provenance in its SKILL.md frontmatter.
/// Single source of truth for the "gh-installed" read used by update dir
/// preference and backend guessing.
pub fn skill_path_has_github_metadata(skill_dir: &Path) -> bool {
    let Ok(content) = fs::read_to_string(skill_dir.join("SKILL.md")) else {
        return false;
    };
    matches!(parse_skill_md(&content), Ok((_, _, Some(url), _, _, _)) if !url.is_empty())
}

pub fn detect_install_kind(path: &Path) -> (InstallKind, Option<PathBuf>) {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            let resolved = fs::read_link(path).ok().map(|target| {
                if target.is_absolute() {
                    target
                } else {
                    path.parent().unwrap_or_else(|| Path::new(".")).join(target)
                }
            });
            (InstallKind::Symlink, resolved)
        }
        Ok(_) => (InstallKind::Copy, None),
        Err(_) => (InstallKind::Unknown, None),
    }
}

pub fn scan_skills(
    project_roots: &[PathBuf],
    home: &Path,
) -> Result<(Vec<DiscoveredSkill>, Vec<String>)> {
    let mut out = Vec::new();
    let mut warnings = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();

    for (agent, scope, root, project) in skill_roots(project_roots, home) {
        if !root.is_dir() {
            continue;
        }
        collect_skills_in_dir(
            &root,
            agent,
            scope,
            project,
            &mut out,
            &mut seen_paths,
            &mut warnings,
        )?;
    }
    Ok((out, warnings))
}

pub(crate) fn collect_skills_in_dir(
    root: &Path,
    agent: Agent,
    scope: Scope,
    project: Option<PathBuf>,
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
            match read_discovered(&path, agent, scope, project.clone()) {
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
                    match read_discovered(&nested_path, agent, scope, project.clone()) {
                        Ok(Some(discovered)) => out.push(discovered),
                        Ok(None) => {}
                        Err(err) => warnings.push(format!("skip {}: {err}", nested_path.display())),
                    }
                }
            }
        }
    }
    Ok(())
}

fn read_discovered(
    path: &Path,
    agent: Agent,
    scope: Scope,
    project: Option<PathBuf>,
) -> Result<Option<DiscoveredSkill>> {
    let skill_md = path.join("SKILL.md");
    let content = match fs::read_to_string(&skill_md) {
        Ok(c) => c,
        Err(_) => return Ok(None),
    };
    let (name, description, source_url, author, version, pinned) = match parse_skill_md(&content) {
        Ok(v) => v,
        Err(_) => {
            // Fallback: tolerate invalid YAML by using directory name.
            let id = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("unnamed")
                .to_string();
            (id.clone(), String::new(), None, None, None, false)
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
        author,
        version,
        pinned,
        source: None,
        project,
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
        let (name, desc, _, _, _, _) = parse_skill_md(content).unwrap();
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
        let (name, desc, _, _, _, _) = parse_skill_md(content).unwrap();
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
        let (name, _, source_url, _, _, _) = parse_skill_md(content).unwrap();
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

        let (found, _warnings) = scan_skills(&[project], &home).unwrap();
        assert!(found.iter().any(|s| s.name == "alpha"));
        let beta = found.iter().find(|s| s.name == "beta").unwrap();
        assert_eq!(beta.location.kind, InstallKind::Symlink);
        assert_eq!(beta.source_url.as_deref(), Some("https://example.com/beta"));
    }

    #[test]
    fn scan_cursor_shared_and_managed_roots() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("proj");
        let home = tmp.path().join("home");
        fs::create_dir_all(project.join(".git")).unwrap();

        fs::create_dir_all(home.join(".agents/skills/shared-only")).unwrap();
        fs::write(
            home.join(".agents/skills/shared-only/SKILL.md"),
            "---\nname: shared-only\ndescription: From agents store\n---\n",
        )
        .unwrap();

        fs::create_dir_all(home.join(".cursor/skills-cursor/managed")).unwrap();
        fs::write(
            home.join(".cursor/skills-cursor/managed/SKILL.md"),
            "---\nname: managed\ndescription: Cursor managed\n---\n",
        )
        .unwrap();

        let (found, _warnings) = scan_skills(&[project], &home).unwrap();
        let shared: Vec<_> = found.iter().filter(|s| s.name == "shared-only").collect();
        assert!(
            shared.len() >= 3,
            "shared store should be attributed to cursor/cline/warp/universal"
        );
        assert!(shared.iter().any(|s| s.location.agent == Agent::Cursor));
        assert!(shared.iter().any(|s| s.location.agent == Agent::Universal));
        assert!(
            shared
                .iter()
                .all(|s| s.location.path.ends_with(".agents/skills/shared-only"))
        );

        let managed = found
            .iter()
            .find(|s| s.name == "managed")
            .expect("skills-cursor skill");
        assert_eq!(managed.location.agent, Agent::Cursor);
        assert!(
            managed
                .location
                .path
                .ends_with(".cursor/skills-cursor/managed")
        );
    }

    #[test]
    fn skill_roots_include_cursor_extra_user_paths() {
        let roots = skill_roots(&[PathBuf::from("/proj")], Path::new("/home"));
        let cursor_user: Vec<_> = roots
            .iter()
            .filter(|(a, s, _, _)| *a == Agent::Cursor && *s == Scope::User)
            .map(|(_, _, p, _)| p.clone())
            .collect();
        assert!(cursor_user.contains(&PathBuf::from("/home/.cursor/skills")));
        assert!(cursor_user.contains(&PathBuf::from("/home/.agents/skills")));
        assert!(cursor_user.contains(&PathBuf::from("/home/.cursor/skills-cursor")));
    }

    #[test]
    fn skill_roots_cover_extended_agents() {
        let roots = skill_roots(&[PathBuf::from("/proj")], Path::new("/home"));
        let has = |agent: Agent, path: &str| {
            roots
                .iter()
                .any(|(a, _, p, _)| *a == agent && p == Path::new(path))
        };
        assert!(has(Agent::GeminiCli, "/home/.gemini/skills"));
        assert!(has(Agent::GitHubCopilot, "/home/.copilot/skills"));
        assert!(has(Agent::OpenCode, "/home/.config/opencode/skills"));
        assert!(has(Agent::Pi, "/home/.pi/agent/skills"));
        assert!(has(Agent::QwenCode, "/home/.qwen/skills"));
        assert!(has(Agent::Droid, "/home/.factory/skills"));
        assert!(has(Agent::Devin, "/home/.config/devin/skills"));
        assert!(has(Agent::Universal, "/home/.agents/skills"));
        assert!(has(Agent::Cursor, "/proj/.cursor/skills"));
    }

    #[test]
    fn skill_roots_tag_project_and_keep_user_untagged() {
        let roots = skill_roots(&[PathBuf::from("/proj")], Path::new("/home"));
        let proj = roots
            .iter()
            .find(|(a, s, p, _)| {
                *a == Agent::Cursor
                    && *s == Scope::Project
                    && p == Path::new("/proj/.cursor/skills")
            })
            .unwrap();
        assert_eq!(proj.3.as_deref(), Some(Path::new("/proj")));
        assert!(roots.iter().any(|(a, s, p, proj)| {
            *a == Agent::Cursor
                && *s == Scope::User
                && p == Path::new("/home/.agents/skills")
                && proj.is_none()
        }));
    }

    #[test]
    fn skill_roots_repeat_project_paths_for_each_root() {
        let roots = skill_roots(
            &[PathBuf::from("/a"), PathBuf::from("/b")],
            Path::new("/home"),
        );
        let cursor_proj: Vec<_> = roots
            .iter()
            .filter(|(a, s, p, _)| {
                *a == Agent::Cursor && *s == Scope::Project && p.ends_with(".cursor/skills")
            })
            .collect();
        assert_eq!(cursor_proj.len(), 2);
    }

    #[test]
    fn scan_skills_sets_project_on_project_scope_discoveries() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let proj = tmp.path().join("proj");
        fs::create_dir_all(home.join(".agents/skills")).unwrap();
        fs::create_dir_all(proj.join(".cursor/skills/foo")).unwrap();
        fs::write(
            proj.join(".cursor/skills/foo/SKILL.md"),
            "---\nname: foo\ndescription: d\n---\n",
        )
        .unwrap();
        let (found, _) = scan_skills(&[proj.clone()], &home).unwrap();
        let foo = found.iter().find(|s| s.name == "foo").unwrap();
        assert_eq!(foo.location.scope, Scope::Project);
        assert_eq!(foo.project.as_ref(), Some(&proj));
    }
}
