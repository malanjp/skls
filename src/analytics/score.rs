//! Delete-recommendation scoring.

use crate::model::{InstallSource, SkillRecord};
use chrono::{Duration, Utc};

/// Higher score => stronger delete candidate.
pub fn compute_delete_score(skill: &SkillRecord, now: chrono::DateTime<Utc>) -> f64 {
    let mut score = 0.0;

    match skill.stats.activation_rate {
        None => score += 25.0, // unknown / no sessions
        Some(rate) if rate <= 0.0 => score += 40.0,
        Some(rate) if rate < 0.05 => score += 30.0,
        Some(rate) if rate < 0.15 => score += 15.0,
        Some(rate) if rate < 0.30 => score += 5.0,
        Some(_) => score += 0.0,
    }

    match skill.stats.last_hit_at {
        None => score += 20.0,
        Some(ts) => {
            let age = now.signed_duration_since(ts);
            if age > Duration::days(60) {
                score += 25.0;
            } else if age > Duration::days(30) {
                score += 15.0;
            } else if age > Duration::days(14) {
                score += 8.0;
            }
        }
    }

    // Many host installs of an unused skill = more cleanup value / clutter
    if skill.agents.len() >= 3 {
        score += 10.0;
    } else if skill.agents.len() == 2 {
        score += 5.0;
    }

    if skill.source == InstallSource::Manual && skill.source_url.is_none() {
        score += 5.0;
    }

    if skill.stats.hits == 0 {
        score += 10.0;
    }

    score
}

pub fn apply_scores(records: &mut [SkillRecord], now: chrono::DateTime<Utc>) {
    for rec in records.iter_mut() {
        rec.stats.delete_score = compute_delete_score(rec, now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{InstallKind, Scope, SkillStats};

    fn base() -> SkillRecord {
        SkillRecord {
            id: "x".into(),
            name: "x".into(),
            description: String::new(),
            scope: Scope::User,
            agents: vec![],
            locations: vec![],
            install_kind: InstallKind::Copy,
            source: InstallSource::Manual,
            source_url: None,
            version: None,
            pinned: false,
            stats: SkillStats {
                hits: 0,
                sessions_total: 100,
                last_hit_at: None,
                activation_rate: Some(0.0),
                delete_score: 0.0,
            },
        }
    }

    #[test]
    fn unused_skills_score_high() {
        let now = Utc::now();
        let unused = base();
        let mut used = base();
        used.stats.hits = 20;
        used.stats.activation_rate = Some(0.5);
        used.stats.last_hit_at = Some(now);
        used.source = InstallSource::Gh;
        used.source_url = Some("https://x".into());

        let s_unused = compute_delete_score(&unused, now);
        let s_used = compute_delete_score(&used, now);
        assert!(s_unused > s_used);
        assert!(s_unused >= 60.0);
    }
}
