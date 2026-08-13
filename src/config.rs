//! User config (`~/.config/skls/config.toml`) for extra project roots
//! and analysis defaults.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Project-scope skill directories used as discovery markers.
const PROJECT_SKILL_MARKERS: &[&str] = &[
    ".agents/skills",
    ".cursor/skills",
    ".claude/skills",
    ".codex/skills",
];

const DISCOVER_MAX_DEPTH: usize = 6;
const DISCOVER_MAX_VISITS: usize = 4_000;
const DISCOVER_MAX_PROJECTS: usize = 80;
/// Only these home children are walked. A full `$HOME` walk hits Desktop /
/// cloud folders and makes first launch very slow.
const DISCOVER_SEEDS: &[&str] = &[
    "repos",
    "src",
    "dev",
    "code",
    "work",
    "projects",
    "orca",
    "Documents",
    "Developer",
    "ghq",
    "git",
];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoadedConfig {
    pub projects: Vec<PathBuf>,
    pub window_days: Option<i64>,
    pub max_sessions: Option<usize>,
    pub max_bytes: Option<u64>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
struct RawConfig {
    #[serde(default)]
    projects: Vec<String>,
    window_days: Option<i64>,
    max_sessions: Option<usize>,
    max_bytes: Option<u64>,
}

pub fn default_config_path(home: &Path) -> PathBuf {
    let xdg = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
    config_path_for(home, xdg.as_deref())
}

pub fn config_path_for(home: &Path, xdg_config_home: Option<&Path>) -> PathBuf {
    match xdg_config_home {
        Some(xdg) if !xdg.as_os_str().is_empty() => xdg.join("skls/config.toml"),
        _ => home.join(".config/skls/config.toml"),
    }
}

pub fn paths_eq_canonical(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

/// Prefix check without syscalls. Strips a macOS `/private` prefix so
/// `/var/...` and `/private/var/...` compare equal.
pub fn path_is_under(path: &Path, root: &Path) -> bool {
    let path = cmp_path(path);
    let root = cmp_path(root);
    Path::new(path).starts_with(Path::new(root))
}

fn cmp_path(path: &Path) -> &str {
    let raw = path.to_str().unwrap_or("");
    raw.strip_prefix("/private").unwrap_or(raw)
}

pub fn active_is_home(active: &Path, home: &Path) -> bool {
    paths_eq_canonical(active, home)
}

fn expand_tilde(raw: &str, home: &Path) -> PathBuf {
    if raw == "~" {
        home.to_path_buf()
    } else if let Some(rest) = raw.strip_prefix("~/") {
        home.join(rest)
    } else {
        PathBuf::from(raw)
    }
}

fn absolutize(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(path)
        }
    })
}

pub fn load_config(path: &Path, home: &Path) -> LoadedConfig {
    let raw = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            return bootstrap_missing_config(path, home);
        }
        Err(err) => {
            return LoadedConfig {
                warnings: vec![format!("{}: {err}", path.display())],
                ..LoadedConfig::default()
            };
        }
    };
    let parsed: RawConfig = match toml::from_str(&raw) {
        Ok(v) => v,
        Err(err) => {
            return LoadedConfig {
                warnings: vec![format!("{}: {err}", path.display())],
                ..LoadedConfig::default()
            };
        }
    };
    let mut projects = Vec::new();
    let mut warnings = Vec::new();
    for entry in parsed.projects {
        let expanded = expand_tilde(&entry, home);
        if !expanded.is_absolute() {
            warnings.push(format!("skip relative project path: {entry}"));
            continue;
        }
        if !expanded.exists() {
            warnings.push(format!("skip missing project path: {}", expanded.display()));
            continue;
        }
        let Ok(canon) = expanded.canonicalize() else {
            warnings.push(format!(
                "skip unreadable project path: {}",
                expanded.display()
            ));
            continue;
        };
        if paths_eq_canonical(&canon, home) {
            warnings.push(format!(
                "skip project path that resolves to home: {}",
                expanded.display()
            ));
            continue;
        }
        if !projects.iter().any(|p| p == &canon) {
            projects.push(canon);
        }
    }
    LoadedConfig {
        projects,
        window_days: parsed.window_days.filter(|d| *d > 0),
        max_sessions: parsed.max_sessions.filter(|n| *n > 0),
        max_bytes: parsed.max_bytes.filter(|n| *n > 0),
        warnings,
    }
}

fn bootstrap_missing_config(path: &Path, home: &Path) -> LoadedConfig {
    let projects = discover_project_roots(home);
    if projects.is_empty() {
        return LoadedConfig::default();
    }
    let mut warnings = Vec::new();
    match write_generated_config(path, &projects, home) {
        Ok(()) => warnings.push(format!(
            "wrote {} project(s) to {}",
            projects.len(),
            path.display()
        )),
        Err(err) => warnings.push(format!(
            "discovered {} project(s) but failed to write {}: {err}",
            projects.len(),
            path.display()
        )),
    }
    LoadedConfig {
        projects,
        warnings,
        ..LoadedConfig::default()
    }
}

fn is_project_root(dir: &Path) -> bool {
    PROJECT_SKILL_MARKERS
        .iter()
        .any(|rel| dir.join(rel).is_dir())
}

fn skip_dir_name(name: &str) -> bool {
    name.starts_with('.')
        || matches!(
            name,
            "node_modules"
                | "target"
                | "dist"
                | "build"
                | "vendor"
                | "Library"
                | "Applications"
                | "Pictures"
                | "Movies"
                | "Music"
                | "Public"
                | "Downloads"
                | "__pycache__"
                | ".venv"
                | "venv"
                | "site-packages"
        )
}

/// Walk well-known home children for directories that contain project-scope
/// skill folders. Does not walk the whole home tree.
pub fn discover_project_roots(home: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut visits = 0usize;
    for seed in DISCOVER_SEEDS {
        let dir = home.join(seed);
        if dir.is_dir() {
            walk_for_projects(home, &dir, 1, &mut found, &mut visits);
        }
    }
    found.sort();
    found
}

fn walk_for_projects(
    home: &Path,
    dir: &Path,
    depth: usize,
    found: &mut Vec<PathBuf>,
    visits: &mut usize,
) {
    if *visits >= DISCOVER_MAX_VISITS || found.len() >= DISCOVER_MAX_PROJECTS {
        return;
    }
    *visits += 1;

    if depth > 0
        && is_project_root(dir)
        && let Ok(canon) = dir.canonicalize()
        && !paths_eq_canonical(&canon, home)
        && !found.iter().any(|p| p == &canon)
    {
        found.push(canon);
    }
    if depth >= DISCOVER_MAX_DEPTH {
        return;
    }

    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        if ft.is_symlink() || !ft.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if skip_dir_name(name) {
            continue;
        }
        walk_for_projects(home, &entry.path(), depth + 1, found, visits);
    }
}

fn project_entry(path: &Path, home: &Path) -> String {
    let home = home.canonicalize().unwrap_or_else(|_| home.to_path_buf());
    match path.strip_prefix(&home) {
        Ok(rel) if !rel.as_os_str().is_empty() => format!("~/{}", rel.display()),
        _ => path.display().to_string(),
    }
}

#[derive(Serialize)]
struct WritableProjects {
    projects: Vec<String>,
}

pub fn write_generated_config(path: &Path, projects: &[PathBuf], home: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let payload = WritableProjects {
        projects: projects.iter().map(|p| project_entry(p, home)).collect(),
    };
    let serialized =
        toml::to_string(&payload).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    let body = format!(
        "# Generated by skls because config.toml was missing.\n\
         # Edit this list to add or remove project-scope scan roots.\n\
         {serialized}"
    );
    fs::write(path, body)
}

/// Config projects plus the active cwd / `--project-root`.
/// Home as the active root is not scanned as a project.
pub fn resolve_scan_roots(config_projects: &[PathBuf], active: &Path, home: &Path) -> Vec<PathBuf> {
    let mut roots = config_projects.to_vec();
    let active_abs = absolutize(active);
    if paths_eq_canonical(&active_abs, home) {
        return roots;
    }
    if !roots.iter().any(|p| paths_eq_canonical(p, &active_abs)) {
        roots.push(active_abs);
    }
    roots
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn config_path_prefers_xdg_then_home_config() {
        let home = Path::new("/Users/me");
        assert_eq!(
            config_path_for(home, None),
            PathBuf::from("/Users/me/.config/skls/config.toml")
        );
        assert_eq!(
            config_path_for(home, Some(Path::new("/xdg/config"))),
            PathBuf::from("/xdg/config/skls/config.toml")
        );
        assert_eq!(
            config_path_for(home, Some(Path::new(""))),
            PathBuf::from("/Users/me/.config/skls/config.toml")
        );
    }

    #[test]
    fn load_config_missing_file_is_empty_without_warning() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let loaded = load_config(&tmp.path().join("nope.toml"), &home);
        assert!(loaded.projects.is_empty());
        assert!(loaded.warnings.is_empty());
        assert!(loaded.window_days.is_none());
        assert!(!tmp.path().join("nope.toml").exists());
    }

    fn write_marker(root: &Path, marker: &str) {
        fs::create_dir_all(root.join(marker)).unwrap();
    }

    #[test]
    fn discover_project_roots_finds_skill_markers_and_skips_junk() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let app = home.join("repos/github.com/me/app");
        let nested_junk = home.join("repos/github.com/me/app/node_modules/pkg");
        let hidden = home.join(".hidden/proj");
        let user_skills = home.join(".cursor/skills");
        fs::create_dir_all(&home).unwrap();
        write_marker(&app, ".cursor/skills");
        write_marker(&nested_junk, ".cursor/skills");
        write_marker(&hidden, ".claude/skills");
        fs::create_dir_all(&user_skills).unwrap();

        let found = discover_project_roots(&home);
        let app_c = app.canonicalize().unwrap();
        assert_eq!(found, vec![app_c]);
    }

    #[test]
    fn discover_project_roots_skips_non_seed_home_children() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let stray = home.join("Desktop/random-app");
        fs::create_dir_all(&home).unwrap();
        write_marker(&stray, ".cursor/skills");
        assert!(discover_project_roots(&home).is_empty());
    }

    #[test]
    fn load_config_missing_discovers_and_writes_tilde_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let app = home.join("src/my-app");
        fs::create_dir_all(&home).unwrap();
        write_marker(&app, ".claude/skills");
        let cfg = home.join(".config/skls/config.toml");
        let loaded = load_config(&cfg, &home);
        assert_eq!(loaded.projects, vec![app.canonicalize().unwrap()]);
        assert!(loaded.warnings.iter().any(|w| w.contains("wrote 1")));
        let raw = fs::read_to_string(&cfg).unwrap();
        assert!(raw.contains("~/src/my-app"));
        assert!(raw.contains("Generated by skls"));

        fs::write(&cfg, "projects = []\n").unwrap();
        let again = load_config(&cfg, &home);
        assert!(again.projects.is_empty());
        assert_eq!(fs::read_to_string(&cfg).unwrap(), "projects = []\n");
    }

    #[test]
    fn load_config_unreadable_path_warns_and_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let cfg = tmp.path().join("config.toml");
        fs::create_dir_all(&cfg).unwrap();
        let loaded = load_config(&cfg, &home);
        assert!(loaded.projects.is_empty());
        assert_eq!(loaded.warnings.len(), 1);
        assert!(loaded.warnings[0].contains("config.toml"));
    }

    #[test]
    fn load_config_broken_toml_warns_and_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let cfg = tmp.path().join("config.toml");
        fs::write(&cfg, "<<<not toml").unwrap();
        let loaded = load_config(&cfg, &home);
        assert!(loaded.projects.is_empty());
        assert_eq!(loaded.warnings.len(), 1);
        assert!(loaded.warnings[0].contains("config.toml"));
    }

    #[test]
    fn load_config_expands_tilde_skips_relative_missing_and_home() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let repo = tmp.path().join("repo");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&repo).unwrap();
        let cfg = tmp.path().join("config.toml");
        let repo_tilde = "~/repo";
        let tilde_repo = home.join("repo");
        fs::create_dir_all(&tilde_repo).unwrap();
        fs::write(
            &cfg,
            format!(
                "projects = [\n  \"{repo_tilde}\",\n  \"relative/path\",\n  \"{missing}\",\n  \"{home_s}\",\n  \"{abs}\"\n]\nunknown = 1\n",
                missing = tmp.path().join("missing-dir").display(),
                home_s = home.display(),
                abs = repo.display(),
            ),
        )
        .unwrap();
        let loaded = load_config(&cfg, &home);
        let canon: Vec<_> = loaded
            .projects
            .iter()
            .map(|p| p.canonicalize().unwrap())
            .collect();
        assert!(canon.contains(&tilde_repo.canonicalize().unwrap()));
        assert!(canon.contains(&repo.canonicalize().unwrap()));
        assert_eq!(canon.len(), 2);
        assert!(loaded.warnings.iter().any(|w| w.contains("relative")));
        assert!(loaded.warnings.iter().any(|w| w.contains("missing-dir")));
        assert!(loaded.warnings.iter().any(|w| w.contains("home")));
    }

    #[test]
    fn load_config_reads_analysis_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let cfg = tmp.path().join("config.toml");
        fs::write(
            &cfg,
            "window_days = 14\nmax_sessions = 40\nmax_bytes = 1024\nwindow_days_bad = 0\n",
        )
        .unwrap();
        let loaded = load_config(&cfg, &home);
        assert_eq!(loaded.window_days, Some(14));
        assert_eq!(loaded.max_sessions, Some(40));
        assert_eq!(loaded.max_bytes, Some(1024));
    }

    #[test]
    fn load_config_drops_non_positive_analysis_values() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let cfg = tmp.path().join("config.toml");
        fs::write(&cfg, "window_days = 0\nmax_sessions = 0\nmax_bytes = 0\n").unwrap();
        let loaded = load_config(&cfg, &home);
        assert!(loaded.window_days.is_none());
        assert!(loaded.max_sessions.is_none());
        assert!(loaded.max_bytes.is_none());
    }

    #[test]
    fn path_is_under_accepts_prefix_and_canonical() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("proj");
        let child = root.join(".cursor/skills/x");
        fs::create_dir_all(&child).unwrap();
        assert!(path_is_under(&child, &root));
        assert!(!path_is_under(&root, &child));
        assert!(!path_is_under(&tmp.path().join("other"), &root));
    }

    #[test]
    fn active_is_home_uses_canonical_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let proj = tmp.path().join("proj");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&proj).unwrap();
        assert!(active_is_home(&home, &home));
        assert!(!active_is_home(&proj, &home));
    }

    #[test]
    fn resolve_scan_roots_skips_active_when_it_is_home() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let repo = tmp.path().join("repo");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&repo).unwrap();
        let roots = resolve_scan_roots(&[repo.canonicalize().unwrap()], &home, &home);
        assert_eq!(roots, vec![repo.canonicalize().unwrap()]);
    }

    #[test]
    fn resolve_scan_roots_adds_active_and_dedups() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let repo = tmp.path().join("repo");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&repo).unwrap();
        let repo_c = repo.canonicalize().unwrap();
        let roots = resolve_scan_roots(std::slice::from_ref(&repo_c), &repo, &home);
        assert_eq!(roots, vec![repo_c.clone()]);
        let extra = tmp.path().join("other");
        fs::create_dir_all(&extra).unwrap();
        let extra_c = extra.canonicalize().unwrap();
        let roots = resolve_scan_roots(std::slice::from_ref(&repo_c), &extra, &home);
        assert_eq!(roots, vec![repo_c, extra_c]);
    }
}
