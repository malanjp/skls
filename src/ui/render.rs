//! ratatui rendering for skls.

use crate::app::{App, Mode};
use crate::ops::AddBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(3),
        ])
        .split(frame.area());

    draw_header(frame, app, chunks[0]);
    draw_body(frame, app, chunks[1]);
    draw_footer(frame, app, chunks[2]);

    match app.mode {
        Mode::Help => draw_help_modal(frame),
        Mode::Message => draw_message_modal(frame, &app.message),
        Mode::Busy => draw_busy_modal(frame, &app.busy_message),
        Mode::Filter => draw_filter_modal(frame, app),
        Mode::Search => draw_search_modal(frame, app),
        Mode::AddBackend => draw_add_backend_modal(frame, app),
        Mode::AddQuery => draw_add_query_modal(frame, app),
        Mode::AddResults => draw_add_results_modal(frame, app),
        Mode::AddAgent => draw_add_agent_modal(frame, app),
        Mode::AddScope => draw_add_scope_modal(frame, app),
        Mode::UpdateBackend => draw_update_backend_modal(frame, app),
        Mode::DeleteConfirm => draw_delete_modal(frame, app),
        Mode::List => {}
    }
}

fn scope_label(scope: Option<crate::model::Scope>) -> &'static str {
    match scope {
        Some(crate::model::Scope::Project) => "project",
        Some(crate::model::Scope::User) => "user",
        None => "all",
    }
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let agents = if app.filters.agents.is_empty() {
        "all".into()
    } else {
        app.filters
            .agents
            .iter()
            .map(|a| a.as_str())
            .collect::<Vec<_>>()
            .join(",")
    };
    let sample = if app.analyze_limits.is_unlimited() {
        "full".to_string()
    } else {
        format!(
            "≤{}sess/{}KiB",
            app.analyze_limits.max_files_per_agent,
            app.analyze_limits.max_bytes_per_file / 1024
        )
    };
    let selected = if app.checked_count() > 0 {
        format!("  sel:{}", app.checked_count())
    } else {
        String::new()
    };
    let title = format!(
        " skls  scope:{}  agents:{}  sort:{}  window:{}d  sample:{}{selected} ",
        scope_label(app.filters.scope),
        agents,
        app.sort_key.as_str(),
        app.window_days,
        sample
    );
    let status = if !app.filters.query.is_empty() {
        format!("filter: {}   |  {}", app.filters.query, app.status)
    } else {
        app.status.clone()
    };
    let block = Block::default().borders(Borders::ALL).title(title);
    let p = Paragraph::new(status).block(block);
    frame.render_widget(p, area);
}

fn draw_body(frame: &mut Frame, app: &mut App, area: Rect) {
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    let items: Vec<ListItem> = app
        .filtered_indices
        .iter()
        .map(|&skill_i| {
            let s = &app.skills[skill_i];
            let mark = if app.is_checked(s) { "[x]" } else { "[ ]" };
            let rate = s
                .stats
                .activation_rate
                .map(|r| format!("{:>5.1}%", r * 100.0))
                .unwrap_or_else(|| "  n/a".into());
            let line = format!(
                "{mark} {:<20} {:7} {rate} {:>5.0}",
                truncate(&s.name, 20),
                s.scope.as_str(),
                s.stats.delete_score
            );
            ListItem::new(Line::from(line))
        })
        .collect();

    let list_title = if app.checked_count() > 0 {
        format!(
            " NAME                 SCOPE   RATE  SCORE  ({} selected) ",
            app.checked_count()
        )
    } else {
        " NAME                 SCOPE   RATE  SCORE ".into()
    };
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(list_title))
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");
    frame.render_stateful_widget(list, panes[0], &mut app.list_state);

    let detail = match app.selected_skill() {
        Some(s) => {
            let paths = s
                .locations
                .iter()
                .map(|l| {
                    format!("  · {} ({})  {}", l.agent, l.kind, l.path.display())
                })
                .collect::<Vec<_>>()
                .join("\n");
            let last = s
                .stats
                .last_hit_at
                .map(|t| t.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| "-".into());
            let rate = s
                .stats
                .activation_rate
                .map(|r| format!("{:.1}%", r * 100.0))
                .unwrap_or_else(|| "n/a".into());
            let advice = if s.stats.delete_score >= 60.0 {
                "consider delete"
            } else if s.stats.delete_score >= 35.0 {
                "review"
            } else {
                "keep"
            };
            let checked = if app.is_checked(s) { "yes" } else { "-" };
            format!(
                "{}\n\
                 ────────────────\n\
                 scope      {}\n\
                 agents     {}\n\
                 source     {}  ({})\n\
                 url        {}\n\
                 version    {}   pin:{}\n\
                 selected   {}\n\
                 \n\
                 activation\n\
                   hits     {} / {} sessions ({}d)\n\
                   rate     {}\n\
                   last     {}\n\
                   score    {:.0}  →  {}\n\
                 \n\
                 paths\n\
                 {}\n\
                 \n\
                 {}",
                s.name,
                s.scope,
                s.agents_label(),
                s.source,
                s.install_kind,
                s.source_url.as_deref().unwrap_or("-"),
                s.version.as_deref().unwrap_or("-"),
                s.pinned,
                checked,
                s.stats.hits,
                s.stats.sessions_total,
                app.window_days,
                rate,
                last,
                s.stats.delete_score,
                advice,
                paths,
                s.description
            )
        }
        None => "No skill selected".into(),
    };

    let detail_widget = Paragraph::new(detail)
        .wrap(Wrap { trim: false })
        .block(Block::default().borders(Borders::ALL).title(" detail "));
    frame.render_widget(detail_widget, panes[1]);
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let warn = if app.warnings.is_empty() {
        String::new()
    } else {
        format!("  !{}", truncate(&app.warnings[0], 40))
    };
    let text = match app.mode {
        Mode::DeleteConfirm => {
            " [y]/Enter confirm  [n]/Esc cancel  [1]/[2]/[3] narrow  [0] all ".to_string()
        }
        Mode::Help | Mode::Message => " Enter / Esc / q close ".to_string(),
        Mode::Busy => " working — please wait … ".to_string(),
        Mode::Filter => {
            " [p]/[u]/[a] scope  [1]/[2]/[3]/[0] agents  [c] clear  Esc back ".to_string()
        }
        Mode::Search => " type  Enter=apply  Esc=cancel ".to_string(),
        Mode::AddBackend => " [1] gh  [2] npx   Esc/q cancel ".to_string(),
        Mode::UpdateBackend => " [1] gh  [2] npx  Enter=suggested  Esc cancel ".to_string(),
        Mode::AddQuery => " type  Enter=next  Esc=back  q=cancel ".to_string(),
        Mode::AddResults => " j/k select  Enter=next  Esc=back  q=cancel ".to_string(),
        Mode::AddAgent => " [1]cursor [2]claude [3]codex  Esc=back ".to_string(),
        Mode::AddScope => " [p]project [u]user  Esc=back  q=cancel ".to_string(),
        Mode::List => format!(
            " j/k  Space/* /x select  / search  f filter  s sort  a add  d del  u upd  r/R refresh  ?  q{warn} "
        ),
    };
    let p = Paragraph::new(text).block(Block::default().borders(Borders::ALL));
    frame.render_widget(p, area);
}

fn draw_panel(frame: &mut Frame, title: &str, body: String, border: Color) {
    let area = centered(frame.area(), 72, 55);
    frame.render_widget(Clear, area);
    let p = Paragraph::new(body)
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {title} "))
                .border_style(Style::default().fg(border)),
        );
    frame.render_widget(p, area);
}

fn draw_help_modal(frame: &mut Frame) {
    let text = "\
Keys
  j / k     move up/down
  Space     toggle select
  *         select/clear all visible
  x         clear selection
  /         search name/description
  f         filter (scope · agents)
  s         cycle sort (name → rate → score → last)
  a         add skill
  d         delete (selection or current row)
  u         update (pick gh / npx)
  r         light rescan
  R         recompute activation stats
  ?         this help
  q         quit

Update (u)
  [1] gh skill   [2] npx skills
  Enter uses suggested backend

Add (a)
  backend → source → (gh results) → agent → scope

Delete (d)
  [y] confirm   [n]/Esc cancel
  [1]/[2]/[3] narrow agents   [0] all

CLI sampling
  --max-sessions N   sessions/agent (default 80)
  --max-bytes N      bytes/file (default 256KiB)
  --full-scan        no limits
";
    let area = centered(frame.area(), 72, 75);
    frame.render_widget(Clear, area);
    let p = Paragraph::new(text).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" help ")
            .border_style(Style::default().fg(Color::Cyan)),
    );
    frame.render_widget(p, area);
}

fn draw_message_modal(frame: &mut Frame, message: &str) {
    let body = format!(
        "Result\n\
         ────────────────────────\n\
         {message}\n\
         \n\
         Keys  Enter / Esc / q to close"
    );
    draw_panel(frame, "result", body, Color::Green);
}

fn draw_busy_modal(frame: &mut Frame, message: &str) {
    let body = format!(
        "Working\n\
         ────────────────────────\n\
         {message}\n\
         \n\
         Input is ignored until this finishes."
    );
    draw_panel(frame, "busy", body, Color::Cyan);
}

fn draw_search_modal(frame: &mut Frame, app: &App) {
    let body = format!(
        "Search skills\n\
         ────────────────────────\n\
         Filter by name or description.\n\
         \n\
         Input\n\
         ┌──────────────────────────────────────┐\n\
         │ /{}_│\n\
         └──────────────────────────────────────┘\n\
         \n\
         Keys  Enter=apply · Esc=cancel",
        app.input
    );
    draw_panel(frame, "search", body, Color::Cyan);
}

fn draw_filter_modal(frame: &mut Frame, app: &App) {
    let agents = if app.filters.agents.is_empty() {
        "all".into()
    } else {
        app.filters
            .agents
            .iter()
            .map(|a| a.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let body = format!(
        "Filter list\n\
         ────────────────────────\n\
         Current\n\
           scope    {}\n\
           agents   {agents}\n\
         \n\
         Scope\n\
           [p]  project (this repo)\n\
           [u]  user (global)\n\
           [a]  all\n\
         \n\
         Agents\n\
           [1] cursor   [2] claude-code   [3] codex\n\
           [0] all\n\
         \n\
         [c] clear filters\n\
         \n\
         Keys  toggle above · Esc=back",
        scope_label(app.filters.scope),
    );
    draw_panel(frame, "filter", body, Color::Cyan);
}

fn add_total_steps(backend: AddBackend) -> u8 {
    match backend {
        AddBackend::GhSkill => 5,
        AddBackend::NpxSkills => 4,
    }
}

fn add_source_summary(app: &App) -> String {
    if app.add_package.is_empty() && app.add_query.is_empty() {
        return "-".into();
    }
    if !app.add_package.is_empty() {
        if app.add_skill.is_empty() {
            app.add_package.clone()
        } else {
            format!("{}@{}", app.add_package, app.add_skill)
        }
    } else {
        app.add_query.clone()
    }
}

fn draw_add_panel(frame: &mut Frame, title: &str, body: String) {
    draw_panel(frame, title, body, Color::Cyan);
}

fn draw_add_backend_modal(frame: &mut Frame, app: &App) {
    let gh = if app.gh_available {
        "available"
    } else {
        "missing"
    };
    let npx = if app.npx_available {
        "available"
    } else {
        "missing"
    };
    let body = format!(
        "Add a skill\n\
         ────────────────────────\n\
         Step 1  ·  choose install backend\n\
         (total steps: 4–5 depending on backend)\n\
         \n\
         [1]  gh skill     search GitHub then install   ({gh})\n\
         [2]  npx skills   install by package id        ({npx})\n\
         \n\
         Difference\n\
           gh   keyword search → pick from results\n\
           npx  type owner/repo (or owner/repo@skill)\n\
         \n\
         Keys  [1]/[g] or [2]/[n] · Esc/q cancel"
    );
    draw_add_panel(frame, "add · backend", body);
}

fn draw_add_query_modal(frame: &mut Frame, app: &App) {
    let total = add_total_steps(app.add_backend);
    let (what, examples) = match app.add_backend {
        AddBackend::GhSkill => (
            "enter search keywords",
            "e.g.  tdd\ne.g.  cloudflare",
        ),
        AddBackend::NpxSkills => (
            "enter package (source)",
            "e.g.  vercel-labs/skills\ne.g.  mattpocock/skills@tdd\n      └ owner/repo or owner/repo@skill",
        ),
    };
    let body = format!(
        "Add a skill  ·  {}\n\
         ────────────────────────\n\
         Step 2 / {total}  ·  {what}\n\
         \n\
         {examples}\n\
         \n\
         Input\n\
         ┌──────────────────────────────────────┐\n\
         │ {}_│\n\
         └──────────────────────────────────────┘\n\
         \n\
         Keys  Enter=next · Esc=back · q=cancel",
        app.add_backend.as_str(),
        app.input
    );
    draw_add_panel(frame, &format!("add 2/{total}"), body);
}

fn draw_add_results_modal(frame: &mut Frame, app: &mut App) {
    let area = centered(frame.area(), 78, 70);
    frame.render_widget(Clear, area);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Min(6),
            Constraint::Length(3),
        ])
        .split(area);

    let header = Paragraph::new(format!(
        "Add a skill  ·  gh skill\n\
         ────────────────────────\n\
         Step 3 / 5  ·  pick from search results\n\
         query: \"{}\"   ({} hits)",
        app.add_query,
        app.add_results.len()
    ))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" add 3/5 ")
            .border_style(Style::default().fg(Color::Cyan)),
    );
    frame.render_widget(header, chunks[0]);

    let items: Vec<ListItem> = app
        .add_results
        .iter()
        .map(|item| {
            let line = format!(
                "{:<22}  {:<28}  ★{:<5}  {}",
                truncate(&item.skill_name, 22),
                truncate(&item.repo, 28),
                item.stars,
                truncate(&item.description, 40)
            );
            ListItem::new(line)
        })
        .collect();
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" skill / repo / ★ / description "),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Cyan)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");
    frame.render_stateful_widget(list, chunks[1], &mut app.add_list_state);

    let footer = Paragraph::new("Keys  j/k=move  Enter=next with this skill  Esc=back  q=cancel")
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(footer, chunks[2]);
}

fn draw_add_agent_modal(frame: &mut Frame, app: &App) {
    let total = add_total_steps(app.add_backend);
    let step = match app.add_backend {
        AddBackend::GhSkill => 4,
        AddBackend::NpxSkills => 3,
    };
    let body = format!(
        "Add a skill  ·  {}\n\
         ────────────────────────\n\
         Step {step} / {total}  ·  choose target agent\n\
         \n\
         Current\n\
           source   {}\n\
         \n\
         [1]  cursor\n\
         [2]  claude-code\n\
         [3]  codex\n\
         \n\
         Keys  1/2/3=next · Esc=back · q=cancel",
        app.add_backend.as_str(),
        add_source_summary(app),
    );
    draw_add_panel(frame, &format!("add {step}/{total}"), body);
}

fn draw_add_scope_modal(frame: &mut Frame, app: &App) {
    let total = add_total_steps(app.add_backend);
    let step = match app.add_backend {
        AddBackend::GhSkill => 5,
        AddBackend::NpxSkills => 4,
    };
    let body = format!(
        "Add a skill  ·  {}\n\
         ────────────────────────\n\
         Step {step} / {total}  ·  choose scope and run\n\
         \n\
         Current\n\
           source   {}\n\
           agent    {}\n\
         \n\
         [p]  project   this repository only\n\
         [u]  user      all projects for this user\n\
         \n\
         Keys  p/u=run · Esc=back · q=cancel",
        app.add_backend.as_str(),
        add_source_summary(app),
        app.add_agent.as_str(),
    );
    draw_add_panel(frame, &format!("add {step}/{total}"), body);
}

fn draw_update_backend_modal(frame: &mut Frame, app: &App) {
    let names: Vec<&str> = app
        .update_jobs
        .iter()
        .take(8)
        .map(|j| j.name.as_str())
        .collect();
    let more = if app.update_jobs.len() > 8 {
        format!("\n  … and {} more", app.update_jobs.len() - 8)
    } else {
        String::new()
    };
    let suggested = match app.update_suggested {
        Some(b) => format!("suggested: {}  (Enter to use)", b.as_str()),
        None => "suggested: (none — pick manually)".into(),
    };
    let gh = if app.gh_available {
        "available"
    } else {
        "missing"
    };
    let npx = if app.npx_available {
        "available"
    } else {
        "missing"
    };
    let body = format!(
        "Update skills\n\
         ────────────────────────\n\
         Choose update backend\n\
         \n\
         {suggested}\n\
         \n\
         Targets\n\
           {}{more}\n\
         \n\
         [1]  gh skill     ({gh})\n\
         [2]  npx skills   ({npx})\n\
         \n\
         Keys  [1]/[g] or [2]/[n] · Enter=suggested · Esc=cancel",
        names.join("\n  "),
    );
    draw_panel(frame, "update · backend", body, Color::Cyan);
}

fn draw_delete_modal(frame: &mut Frame, app: &App) {
    let plans = app.delete_plans();
    let body = if plans.is_empty() {
        "Nothing to delete.\n\
         Press [0] for all agents, or Esc to cancel."
            .into()
    } else {
        let filter = match &app.delete_agent_filter {
            Some(agents) => agents
                .iter()
                .map(|a| a.as_str())
                .collect::<Vec<_>>()
                .join(","),
            None => "all".into(),
        };
        let mut lines = Vec::new();
        lines.push("Delete skills".into());
        lines.push("────────────────────────".into());
        if plans.len() == 1 {
            lines.push(format!(
                "Target  {} ({})   agents: {filter}",
                plans[0].skill_name, plans[0].scope
            ));
        } else {
            lines.push(format!(
                "Target  {} skills   agents: {filter}",
                plans.len()
            ));
        }
        lines.push(String::new());
        lines.push("Skills".into());
        for plan in plans.iter().take(12) {
            lines.push(format!(
                "  · {} ({}) [{}]",
                plan.skill_name,
                plan.scope,
                plan.agents
                    .iter()
                    .map(|a| a.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            ));
        }
        if plans.len() > 12 {
            lines.push(format!("  … and {} more", plans.len() - 12));
        }
        lines.push(String::new());
        lines.push("Paths".into());
        let mut path_count = 0usize;
        for plan in &plans {
            for path in &plan.paths {
                if path_count >= 8 {
                    break;
                }
                lines.push(format!("  {}", path.display()));
                path_count += 1;
            }
            if path_count >= 8 {
                break;
            }
        }
        let total_paths: usize = plans.iter().map(|p| p.paths.len()).sum();
        if total_paths > path_count {
            lines.push(format!("  … and {} more paths", total_paths - path_count));
        }
        let warns: Vec<&str> = plans
            .iter()
            .filter_map(|p| p.shared_warning.as_deref())
            .collect();
        if !warns.is_empty() {
            lines.push(String::new());
            lines.push(format!("Warning  {}", warns[0]));
            if warns.len() > 1 {
                lines.push(format!("  (+{} more shared-path warnings)", warns.len() - 1));
            }
        }
        lines.push(String::new());
        lines.push(
            "Keys  [y]/Enter=delete · [n]/Esc=cancel · [1]/[2]/[3]=narrow · [0]=all".into(),
        );
        lines.join("\n")
    };
    draw_panel(frame, "confirm delete", body, Color::Yellow);
}

fn centered(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let popup = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area)[1];
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup)[1]
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}
