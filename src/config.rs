//! User config (`~/.config/skls/config.toml`) for extra project-scope roots.

use serde::Deserialize;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoadedConfig {
    pub projects: Vec<PathBuf>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
struct RawConfig {
    #[serde(default)]
    projects: Vec<String>,
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
            return LoadedConfig::default();
        }
        Err(err) => {
            return LoadedConfig {
                projects: Vec::new(),
                warnings: vec![format!("{}: {err}", path.display())],
            };
        }
    };
    let parsed: RawConfig = match toml::from_str(&raw) {
        Ok(v) => v,
        Err(err) => {
            return LoadedConfig {
                projects: Vec::new(),
                warnings: vec![format!("{}: {err}", path.display())],
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
            warnings.push(format!("skip unreadable project path: {}", expanded.display()));
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
    LoadedConfig { projects, warnings }
}

pub fn resolve_scan_roots(
    config_projects: &[PathBuf],
    active: &Path,
    home: &Path,
) -> (Vec<PathBuf>, Vec<String>) {
    let mut roots = config_projects.to_vec();
    let warnings = Vec::new();
    let active_abs = absolutize(active);
    if paths_eq_canonical(&active_abs, home) {
        return (roots, warnings);
    }
    if !roots.iter().any(|p| paths_eq_canonical(p, &active_abs)) {
        roots.push(active_abs);
    }
    (roots, warnings)
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
        let repo_tilde = format!("~/repo");
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
        let (roots, warnings) = resolve_scan_roots(
            &[repo.canonicalize().unwrap()],
            &home,
            &home,
        );
        assert_eq!(roots, vec![repo.canonicalize().unwrap()]);
        assert!(warnings.is_empty());
    }

    #[test]
    fn resolve_scan_roots_adds_active_and_dedups() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let repo = tmp.path().join("repo");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&repo).unwrap();
        let repo_c = repo.canonicalize().unwrap();
        let (roots, _) = resolve_scan_roots(&[repo_c.clone()], &repo, &home);
        assert_eq!(roots, vec![repo_c.clone()]);
        let extra = tmp.path().join("other");
        fs::create_dir_all(&extra).unwrap();
        let extra_c = extra.canonicalize().unwrap();
        let (roots, _) = resolve_scan_roots(&[repo_c.clone()], &extra, &home);
        assert_eq!(roots, vec![repo_c, extra_c]);
    }
}
