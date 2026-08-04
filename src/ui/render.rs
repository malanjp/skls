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
        Mode::Filter => draw_filter_modal(frame, app),
        Mode::Search => {}
        Mode::AddBackend => draw_add_backend_modal(frame),
        Mode::AddQuery => {}
        Mode::AddResults => draw_add_results_modal(frame, app),
        Mode::AddAgent => draw_add_agent_modal(frame),
        Mode::AddScope => draw_add_scope_modal(frame),
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
    let title = format!(
        " skillui  scope:{scope}  agents:{agents}  sort:{}  window:{}d ",
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
            let rate = s
                .stats
                .activation_rate
                .map(|r| format!("{:>5.1}%", r * 100.0))
                .unwrap_or_else(|| "  n/a".into());
            let line = format!(
                "{:<22} {:7} {rate} {:>5.0}",
                truncate(&s.name, 22),
                s.scope.as_str(),
                s.stats.delete_score
            );
            ListItem::new(Line::from(line))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" NAME                 SCOPE   RATE  SCORE "),
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
    let text = format!(
        " j/k move  / search  f filter  s sort  a add  d delete  u update  r refresh  ? help  q quit{warn} "
    );
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
  a    add skill     d  delete
  u    gh update     r  refresh
  ?    help          q  quit

Filter mode
  p project  u user  a all scopes
  1 cursor  2 claude-code  3 codex  0 all agents
  c clear filters

Add flow
  pick backend (gh / npx) → query → select → agent → scope

Delete
  y confirm all agents in plan
  1/2/3 narrow to one agent first
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
    let area = centered(frame.area(), 70, 50);
    frame.render_widget(Clear, area);
    let body = if let Some(plan) = &app.delete_plan {
        let paths = plan
            .paths
            .iter()
            .map(|p| format!("  {}", p.display()))
            .collect::<Vec<_>>()
            .join("\n");
        let warn = plan
            .shared_warning
            .as_deref()
            .unwrap_or("");
        format!(
            "Delete '{}' ({}) from {:?}?\n\npaths:\n{}\n\n{}\n\ny confirm  n cancel\n1/2/3 narrow agents",
            plan.skill_name,
            plan.scope,
            plan.agents.iter().map(|a| a.as_str()).collect::<Vec<_>>(),
            paths,
            warn
        )
    } else {
        "nothing to delete".into()
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
