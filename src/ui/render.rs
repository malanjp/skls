//! ratatui rendering for skillui.

use crate::app::{App, Mode};
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
        Mode::Search => {}
        Mode::AddBackend => draw_add_backend_modal(frame),
        Mode::AddQuery => {}
        Mode::AddResults => draw_add_results_modal(frame, app),
        Mode::AddAgent => draw_add_agent_modal(frame),
        Mode::AddScope => draw_add_scope_modal(frame),
        Mode::UpdateBackend => draw_update_backend_modal(frame, app),
        Mode::DeleteConfirm => draw_delete_modal(frame, app),
        Mode::List => {}
    }
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let scope = app
        .filters
        .scope
        .map(|s| s.as_str().to_string())
        .unwrap_or_else(|| "all".into());
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
    let title = format!(
        " skillui  scope:{scope}  agents:{agents}  sort:{}  window:{}d  sample:{sample} ",
        app.sort_key.as_str(),
        app.window_days
    );
    let search = if app.mode == Mode::Search {
        format!("/{}", app.input)
    } else if app.mode == Mode::AddQuery {
        format!("add query> {}", app.input)
    } else if !app.filters.query.is_empty() {
        format!("filter: {}", app.filters.query)
    } else {
        app.status.clone()
    };
    let block = Block::default().borders(Borders::ALL).title(title);
    let p = Paragraph::new(search).block(block);
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
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(list_title),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("");
    frame.render_stateful_widget(list, panes[0], &mut app.list_state);

    let detail = match app.selected_skill() {
        Some(s) => {
            let paths = s
                .locations
                .iter()
                .map(|l| {
                    format!(
                        "  - {} ({}) {}",
                        l.agent,
                        l.kind,
                        l.path.display()
                    )
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
            format!(
                "{}\n\nscope: {}\nagents: {}\nsource: {}  kind: {}\nsource_url: {}\nversion: {}  pinned: {}\n\nhits: {} / {} sessions ({}d)\nrate: {}\nlast: {}\ndelete_score: {:.0}  ({})\n\npaths:\n{}\n\n{}",
                s.name,
                s.scope,
                s.agents_label(),
                s.source,
                s.install_kind,
                s.source_url.as_deref().unwrap_or("-"),
                s.version.as_deref().unwrap_or("-"),
                s.pinned,
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
        format!(" | !{}", app.warnings[0])
    };
    let text = match app.mode {
        Mode::DeleteConfirm => {
            " y/Enter confirm  n/q/Esc cancel  1/2/3 narrow  0 all agents ".to_string()
        }
        Mode::Help | Mode::Message => " Enter/Esc/q back ".to_string(),
        Mode::Busy => " working — please wait … ".to_string(),
        Mode::Filter => " p/u/a scope  1/2/3/0 agents  c clear  Esc back ".to_string(),
        Mode::Search => format!(" /{}  Enter apply  Esc cancel ", app.input),
        Mode::AddBackend => " 1/g gh skill  2/n npx skills  Esc cancel ".to_string(),
        Mode::UpdateBackend => {
            " 1/g gh skill  2/n npx skills  Enter suggested  Esc cancel ".to_string()
        }
        Mode::AddQuery => format!(" query> {}  Enter  Esc cancel ", app.input),
        Mode::AddResults => " j/k select  Enter  Esc cancel ".to_string(),
        Mode::AddAgent => " 1 cursor  2 claude-code  3 codex  Esc cancel ".to_string(),
        Mode::AddScope => " p project  u user  Esc cancel ".to_string(),
        Mode::List => format!(
            " j/k  Space/* /x select  d/u on selection  / f s a r R ? q{warn} "
        ),
    };
    let p = Paragraph::new(text).block(Block::default().borders(Borders::ALL));
    frame.render_widget(p, area);
}

fn draw_help_modal(frame: &mut Frame) {
    let area = centered(frame.area(), 70, 60);
    frame.render_widget(Clear, area);
    let text = "\
Keys
  j/k  move          /  search
  f    filter        s  cycle sort
  a    add skill     d  delete (selection or row)
  u    update        r  light refresh
  R    recompute activation stats
  ?    help          q  quit

Update
  pick gh skill or npx skills (Enter = suggested)

Multi-select
  Space        toggle row
  *            select/clear all visible
  x            clear selection
  d / u        apply to selection (or current row)

CLI (sampling)
  --max-sessions N   sessions/agent (default 80)
  --max-bytes N      bytes/file (default 262144)
  --full-scan        no caps (slow)

Filter mode
  p project  u user  a all scopes
  1 cursor  2 claude-code  3 codex  0 all agents
  c clear filters

Add flow
  pick backend (gh / npx) → query → select → agent → scope

Delete
  y confirm
  1/2/3 narrow agents  0 all agents
  n/Esc cancel
";
    let p = Paragraph::new(text).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" help (Esc) ")
            .style(Style::default().fg(Color::White)),
    );
    frame.render_widget(p, area);
}

fn draw_message_modal(frame: &mut Frame, message: &str) {
    let area = centered(frame.area(), 70, 50);
    frame.render_widget(Clear, area);
    let p = Paragraph::new(message.to_string())
        .wrap(Wrap { trim: false })
        .block(Block::default().borders(Borders::ALL).title(" result (Enter) "));
    frame.render_widget(p, area);
}

fn draw_busy_modal(frame: &mut Frame, message: &str) {
    let area = centered(frame.area(), 60, 30);
    frame.render_widget(Clear, area);
    let body = format!(
        "{message}\n\nPlease wait.\nThe UI is blocked until this finishes."
    );
    let p = Paragraph::new(body)
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" working ")
                .style(Style::default().fg(Color::Cyan)),
        );
    frame.render_widget(p, area);
}

fn draw_filter_modal(frame: &mut Frame, app: &App) {
    let area = centered(frame.area(), 50, 40);
    frame.render_widget(Clear, area);
    let text = format!(
        "Filter\n\nscope: {:?}\nagents: {:?}\n\np/u/a scope\n1/2/3/0 agents\nc clear  Esc back",
        app.filters.scope, app.filters.agents
    );
    let p = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title(" filter "));
    frame.render_widget(p, area);
}

fn draw_add_backend_modal(frame: &mut Frame) {
    let area = centered(frame.area(), 50, 30);
    frame.render_widget(Clear, area);
    let text = "Add skill — choose backend\n\n1 / g  gh skill\n2 / n  npx skills\n\nEsc cancel";
    let p = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title(" add "));
    frame.render_widget(p, area);
}

fn draw_update_backend_modal(frame: &mut Frame, app: &App) {
    let area = centered(frame.area(), 60, 45);
    frame.render_widget(Clear, area);
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
        Some(b) => format!("suggested: {}  (Enter)", b.as_str()),
        None => "suggested: (none — pick manually)".into(),
    };
    let text = format!(
        "Update — choose backend\n\n{suggested}\n\nskills:\n  {}{more}\n\n1 / g  gh skill{}\n2 / n  npx skills{}\n\nEsc cancel",
        names.join("\n  "),
        if app.gh_available { "" } else { "  (missing)" },
        if app.npx_available { "" } else { "  (missing)" },
    );
    let p = Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .block(Block::default().borders(Borders::ALL).title(" update "));
    frame.render_widget(p, area);
}

fn draw_add_results_modal(frame: &mut Frame, app: &mut App) {
    let area = centered(frame.area(), 80, 60);
    frame.render_widget(Clear, area);
    let items: Vec<ListItem> = app
        .add_results
        .iter()
        .map(|item| {
            let line = format!(
                "{}  {}  ★{}  {}",
                item.skill_name, item.repo, item.stars, item.description
            );
            ListItem::new(truncate(&line, 90))
        })
        .collect();
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" search results (Enter select) "),
        )
        .highlight_style(Style::default().bg(Color::Cyan).fg(Color::Black))
        .highlight_symbol("");
    frame.render_stateful_widget(list, area, &mut app.add_list_state);
}

fn draw_add_agent_modal(frame: &mut Frame) {
    let area = centered(frame.area(), 40, 30);
    frame.render_widget(Clear, area);
    let text = "Target agent\n\n1 cursor\n2 claude-code\n3 codex";
    let p = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title(" agent "));
    frame.render_widget(p, area);
}

fn draw_add_scope_modal(frame: &mut Frame) {
    let area = centered(frame.area(), 40, 25);
    frame.render_widget(Clear, area);
    let text = "Scope\n\np project\nu user";
    let p = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title(" scope "));
    frame.render_widget(p, area);
}

fn draw_delete_modal(frame: &mut Frame, app: &App) {
    let area = centered(frame.area(), 70, 55);
    frame.render_widget(Clear, area);
    let plans = app.delete_plans();
    let body = if plans.is_empty() {
        "nothing to delete (try 0 for all agents)".into()
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
        if plans.len() == 1 {
            let plan = &plans[0];
            lines.push(format!(
                "Delete '{}' ({}) — agents: {filter}",
                plan.skill_name, plan.scope
            ));
        } else {
            lines.push(format!("Delete {} skills — agents: {filter}", plans.len()));
        }
        lines.push(String::new());
        for plan in plans.iter().take(12) {
            lines.push(format!(
                "  • {} ({}) [{}]",
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
        lines.push("paths:".into());
        let mut path_count = 0usize;
        for plan in &plans {
            for path in &plan.paths {
                if path_count >= 10 {
                    break;
                }
                lines.push(format!("  {}", path.display()));
                path_count += 1;
            }
            if path_count >= 10 {
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
            lines.push(warns[0].into());
            if warns.len() > 1 {
                lines.push(format!("(+{} more shared-path warnings)", warns.len() - 1));
            }
        }
        lines.push(String::new());
        lines.push("y/Enter confirm · n/q/Esc cancel · 1/2/3 narrow · 0 all".into());
        lines.join("\n")
    };
    let p = Paragraph::new(body)
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" confirm delete ")
                .style(Style::default().fg(Color::Yellow)),
        );
    frame.render_widget(p, area);
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
