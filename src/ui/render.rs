//! ratatui rendering for skls.

use crate::analytics::delete_advice;
use crate::app::{App, Mode};
use crate::model::{Agent, ListView, agents_label, plugin_cli_agents};
use crate::ops::AddBackend;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, HighlightSpacing, List, ListItem, Paragraph, Wrap};

/// Highlight symbol for skill / search lists. ASCII so display width is stable
/// across fonts (▶ is Ambiguous-width and often shifts columns in screenshots).
const LIST_HIGHLIGHT: &str = "> ";

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
        Mode::UpdateAgents => draw_update_agents_modal(frame, app),
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
        " skls  view:{}  scope:{}  agents:{}  sort:{}{}  window:{}d  sample:{}{selected} ",
        app.list_view.as_str(),
        scope_label(app.filters.scope),
        agents,
        app.sort_key.as_str(),
        app.sort_dir.marker(),
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

    app.list_page_rows = panes[0].height.saturating_sub(2).max(1) as usize;

    let (items, list_title, detail) = match app.list_view {
        ListView::Skills => skill_list_content(app),
        ListView::Plugins => plugin_list_content(app),
        ListView::Mcp => mcp_list_content(app),
    };

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(list_title))
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(LIST_HIGHLIGHT)
        .highlight_spacing(HighlightSpacing::Always);
    frame.render_stateful_widget(list, panes[0], &mut app.list_state);

    let detail_widget = Paragraph::new(detail)
        .wrap(Wrap { trim: false })
        .block(Block::default().borders(Borders::ALL).title(" detail "));
    frame.render_widget(detail_widget, panes[1]);
}

fn skill_list_content(app: &App) -> (Vec<ListItem<'static>>, String, String) {
    let items: Vec<ListItem> = app
        .filtered_indices
        .iter()
        .map(|&skill_i| {
            let s = &app.skills[skill_i];
            let mark = if app.is_checked(s) { "[x]" } else { "[ ]" };
            let line = format_skill_list_row(
                mark,
                &s.name,
                s.scope.as_str(),
                s.source.label(),
                s.author.as_deref().unwrap_or("-"),
                s.stats.activation_rate,
                s.stats.delete_score,
            );
            ListItem::new(Line::from(line))
        })
        .collect();
    let detail = match app.selected_skill() {
        Some(s) => {
            let paths = s
                .locations
                .iter()
                .map(|l| format!("  · {} ({})  {}", l.agent, l.kind, l.path.display()))
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
            let advice = delete_advice(s.stats.delete_score);
            let checked = if app.is_checked(s) { "yes" } else { "-" };
            format!(
                "{}\n\
                 ────────────────\n\
                 scope      {}\n\
                 agents     {}\n\
                 source     {}\n\
                 kind       {}\n\
                 url        {}\n\
                 author     {}\n\
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
                s.author.as_deref().unwrap_or("-"),
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
    (items, skill_list_title(app.checked_count()), detail)
}

fn plugin_list_content(app: &App) -> (Vec<ListItem<'static>>, String, String) {
    let items: Vec<ListItem> = app
        .filtered_indices
        .iter()
        .map(|&i| {
            let p = &app.plugins[i];
            let mark = if app.is_plugin_checked(p) {
                "[x]"
            } else {
                "[ ]"
            };
            ListItem::new(Line::from(format_plugin_list_row(
                mark,
                &p.name,
                p.scope.as_str(),
                p.marketplace.as_deref().unwrap_or("-"),
                p.skill_names.len(),
                p.mcp_names.len(),
            )))
        })
        .collect();
    let detail = match app.selected_plugin() {
        Some(p) => {
            let paths = p
                .locations
                .iter()
                .map(|l| format!("  · {} ({})  {}", l.agent, l.kind, l.path.display()))
                .collect::<Vec<_>>()
                .join("\n");
            let skills = if p.skill_names.is_empty() {
                "-".into()
            } else {
                p.skill_names.join(", ")
            };
            let mcp = if p.mcp_names.is_empty() {
                "-".into()
            } else {
                p.mcp_names.join(", ")
            };
            format!(
                "{}\n\
                 ────────────────\n\
                 spec       {}\n\
                 scope      {}\n\
                 agents     {}\n\
                 market     {}\n\
                 author     {}\n\
                 version    {}\n\
                 url        {}\n\
                 skills     {}\n\
                 mcp        {}\n\
                 selected   {}\n\
                 \n\
                 paths\n\
                 {}\n\
                 \n\
                 {}",
                p.name,
                p.spec,
                p.scope,
                p.agents_label(),
                p.marketplace.as_deref().unwrap_or("-"),
                p.author.as_deref().unwrap_or("-"),
                p.version.as_deref().unwrap_or("-"),
                p.source_url.as_deref().unwrap_or("-"),
                skills,
                mcp,
                if app.is_plugin_checked(p) { "yes" } else { "-" },
                paths,
                p.description
            )
        }
        None => "No plugin selected".into(),
    };
    (items, plugin_list_title(app.checked_count()), detail)
}

fn mcp_list_content(app: &App) -> (Vec<ListItem<'static>>, String, String) {
    let items: Vec<ListItem> = app
        .filtered_indices
        .iter()
        .map(|&i| {
            let m = &app.mcp_servers[i];
            let mark = if app.is_mcp_checked(m) { "[x]" } else { "[ ]" };
            ListItem::new(Line::from(format_mcp_list_row(
                mark,
                &m.name,
                m.transport.as_str(),
                m.plugin.as_deref().unwrap_or("-"),
                &m.agents_label(),
            )))
        })
        .collect();
    let detail = match app.selected_mcp() {
        Some(m) => {
            let paths = m
                .locations
                .iter()
                .map(|l| format!("  · {} ({})  {}", l.agent, l.kind, l.path.display()))
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                "{}\n\
                 ────────────────\n\
                 transport  {}\n\
                 plugin     {}\n\
                 scope      {}\n\
                 agents     {}\n\
                 command    {}\n\
                 url        {}\n\
                 selected   {}\n\
                 \n\
                 paths\n\
                 {}\n\
                 \n\
                 MCP configs live inside plugins. Add/update from the plugins view (t).",
                m.name,
                m.transport.as_str(),
                m.plugin.as_deref().unwrap_or("-"),
                m.scope,
                m.agents_label(),
                m.endpoint_label(),
                m.url.as_deref().unwrap_or("-"),
                if app.is_mcp_checked(m) { "yes" } else { "-" },
                paths
            )
        }
        None => "No MCP server selected".into(),
    };
    (items, mcp_list_title(app.checked_count()), detail)
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let warn = if app.warnings.is_empty() {
        String::new()
    } else {
        format!("  !{}", truncate(&app.warnings[0], 40))
    };
    let text = match app.mode {
        Mode::DeleteConfirm => " j/k  Space  */x all/none  [y]/Enter confirm  [n]/Esc ".to_string(),
        Mode::Help | Mode::Message => " Enter / Esc / q close ".to_string(),
        Mode::Busy => " working — please wait … ".to_string(),
        Mode::Filter => {
            " [p]/[u]/[a] scope  j/k Space agents  */0 all  [c] clear  Esc ".to_string()
        }
        Mode::Search => " type  Enter=apply  Esc=cancel ".to_string(),
        Mode::AddBackend => " [1] gh  [2] npx   Esc/q cancel ".to_string(),
        Mode::UpdateAgents => " j/k  Space  */x all/none  Enter=next  Esc/q ".to_string(),
        Mode::UpdateBackend => " [1] gh  [2] npx  Enter=suggested  Esc=back  q=cancel ".to_string(),
        Mode::AddQuery => " type  Enter=next  Esc=back  q=cancel ".to_string(),
        Mode::AddResults => " j/k select  Enter=next  Esc=back  q=cancel ".to_string(),
        Mode::AddAgent => " j/k  Space  */x all/none  Enter=next  Esc=back ".to_string(),
        Mode::AddScope => " [p]project [u]user  Esc=back  q=cancel ".to_string(),
        Mode::List => format!(
            " j/k  C-f/C-b page  C-h/C-l home/end  t view  Space/* /x select  / search  f filter  s sort  S dir  a add  d del  u upd  r/R refresh  ?  q{warn} "
        ),
    };
    let p = Paragraph::new(text).block(Block::default().borders(Borders::ALL));
    frame.render_widget(p, area);
}

fn draw_panel(frame: &mut Frame, title: &str, body: String, border: Color) {
    let area = centered(frame.area(), 72, 55);
    frame.render_widget(Clear, area);
    let p = Paragraph::new(body).wrap(Wrap { trim: false }).block(
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
  C-f / PgDn  page down
  C-b / PgUp  page up
  C-h / Home  first
  C-l / End   last
  t         cycle view (skills → plugins → mcp)
  Space     toggle select
  *         select/clear all visible
  x         clear selection
  /         search name/description
  f         filter (scope · agents)
  s         cycle sort key (skills view)
  S         toggle sort direction (asc / desc)
  a         add (skills: gh/npx · plugins: catalog CLI)
  d         delete (selection or current row)
  u         update
  r         light rescan
  R         recompute activation stats
  ?         this help
  q         quit

Plugins (t)
  a  claude / copilot / codex plugin install  SPEC
  u  catalog update (codex re-runs plugin add)
  d  catalog uninstall (CLI first; path fallback)
  Cursor has no catalog CLI — install from the host marketplace

MCP (t)
  Bundled in plugins (mcp.json). a/u go to the plugins view.
  d uninstalls the parent plugin.

Update (u) on skills
  agents (j/k · Space · */x) → [1] gh skill / [2] npx skills
  Enter uses suggested backend

Add skills (a)
  backend → source → (gh results) → agents (j/k · Space · */x) → scope

Delete (d)
  [y] confirm   [n]/Esc cancel
  j/k · Space toggle · * all · x none
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
        "Search\n\
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
    let agents_summary = if app.filters.agents.is_empty() {
        "all".into()
    } else {
        app.filters
            .agents
            .iter()
            .map(|a| a.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let selected = if app.filters.agents.is_empty() {
        Agent::all().to_vec()
    } else {
        app.filters.agents.clone()
    };
    let toggles = format_agent_toggles(&selected, Agent::all(), app.agent_focus);
    let body = format!(
        "Filter list\n\
         ────────────────────────\n\
         Current\n\
           scope    {}\n\
           agents   {agents_summary}\n\
         \n\
         Scope\n\
           [p]  project (this repo)\n\
           [u]  user (global)\n\
           [a]  all\n\
         \n\
         Agents  (j/k · Space · *=all · x=clear)\n\
         {toggles}\n\
         \n\
         [c] clear filters\n\
         \n\
         Keys  toggle above · Esc=back",
        scope_label(app.filters.scope),
    );
    draw_panel(frame, "filter", body, Color::Cyan);
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
    let (step, total) = app.add_wizard_step();
    let body = if app.add_plugin {
        format!(
            "Add a plugin\n\
             ────────────────────────\n\
             Step {step} / {total}  ·  catalog spec\n\
             \n\
             e.g.  frontend-design@claude-plugins-official\n\
             e.g.  linear@openai-curated\n\
                   └ name@marketplace\n\
             \n\
             Input\n\
             ┌──────────────────────────────────────┐\n\
             │ {}_│\n\
             └──────────────────────────────────────┘\n\
             \n\
             Keys  Enter=next · Esc/q=cancel",
            app.input
        )
    } else {
        let (what, examples) = match app.add_backend {
            AddBackend::GhSkill => ("enter search keywords", "e.g.  tdd\ne.g.  cloudflare"),
            AddBackend::NpxSkills => (
                "enter package (source)",
                "e.g.  vercel-labs/skills\ne.g.  mattpocock/skills@tdd\n      └ owner/repo or owner/repo@skill",
            ),
        };
        format!(
            "Add a skill  ·  {}\n\
             ────────────────────────\n\
             Step {step} / {total}  ·  {what}\n\
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
        )
    };
    draw_add_panel(frame, &format!("add {step}/{total}"), body);
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

    let (step, total) = app.add_wizard_step();
    let header = Paragraph::new(format!(
        "Add a skill  ·  gh skill\n\
         ────────────────────────\n\
         Step {step} / {total}  ·  pick from search results\n\
         query: \"{}\"   ({} hits)",
        app.add_query,
        app.add_results.len()
    ))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" add {step}/{total} "))
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
        .highlight_symbol(LIST_HIGHLIGHT)
        .highlight_spacing(HighlightSpacing::Always);
    frame.render_stateful_widget(list, chunks[1], &mut app.add_list_state);

    let footer = Paragraph::new("Keys  j/k=move  Enter=next with this skill  Esc=back  q=cancel")
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(footer, chunks[2]);
}

fn draw_add_agent_modal(frame: &mut Frame, app: &App) {
    let (step, total) = app.add_wizard_step();
    let available: &[Agent] = if app.add_plugin {
        plugin_cli_agents()
    } else {
        Agent::all()
    };
    let toggles = format_agent_toggles(&app.add_agents, available, app.agent_focus);
    let kind = if app.add_plugin { "plugin" } else { "skill" };
    let backend = if app.add_plugin {
        "claude / copilot / codex"
    } else {
        app.add_backend.as_str()
    };
    let body = format!(
        "Add a {kind}  ·  {backend}\n\
         ────────────────────────\n\
         Step {step} / {total}  ·  choose target agents\n\
         \n\
         Current\n\
           source   {}\n\
         \n\
         {toggles}\n\
         \n\
         Keys  j/k=move · Space=toggle · *=all · x=none · Enter=next · Esc=back · q=cancel",
        add_source_summary(app),
    );
    draw_add_panel(frame, &format!("add {step}/{total}"), body);
}

fn draw_add_scope_modal(frame: &mut Frame, app: &App) {
    let (step, total) = app.add_wizard_step();
    let agents = agents_label(&app.add_agents);
    let kind = if app.add_plugin { "plugin" } else { "skill" };
    let backend = if app.add_plugin {
        "claude / copilot / codex"
    } else {
        app.add_backend.as_str()
    };
    let body = format!(
        "Add a {kind}  ·  {backend}\n\
         ────────────────────────\n\
         Step {step} / {total}  ·  choose scope and run\n\
         \n\
         Current\n\
           source   {}\n\
           agents   {agents}\n\
         \n\
         [p]  project   this repository only\n\
         [u]  user      all projects for this user\n\
         \n\
         Keys  p/u=run · Esc=back · q=cancel",
        add_source_summary(app),
    );
    draw_add_panel(frame, &format!("add {step}/{total}"), body);
}

fn draw_update_agents_modal(frame: &mut Frame, app: &App) {
    let available = app.update_available_agents();
    let toggles = format_agent_toggles(&app.update_agents, &available, app.agent_focus);
    let plugin_mode = !app.update_plugins.is_empty();
    let names: Vec<&str> = if plugin_mode {
        app.update_plugins
            .iter()
            .take(8)
            .map(|p| p.name.as_str())
            .collect()
    } else {
        app.update_skills
            .iter()
            .take(8)
            .map(|s| s.name.as_str())
            .collect()
    };
    let total = if plugin_mode {
        app.update_plugins.len()
    } else {
        app.update_skills.len()
    };
    let more = if total > 8 {
        format!("\n  … and {} more", total - 8)
    } else {
        String::new()
    };
    let heading = if plugin_mode {
        "Update plugins"
    } else {
        "Update skills"
    };
    let body = format!(
        "{heading}\n\
         ────────────────────────\n\
         Choose target agents\n\
         \n\
         Targets\n\
           {}{more}\n\
         \n\
         {toggles}\n\
         \n\
         Keys  j/k=move · Space=toggle · *=all · x=none · Enter=next · Esc/q=cancel",
        names.join("\n  "),
    );
    draw_panel(frame, "update · agents", body, Color::Cyan);
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
    let available = app.delete_available_agents();
    let toggles = format_agent_toggles(&app.delete_agents, &available, app.agent_focus);
    let body = if !app.delete_plugins.is_empty() {
        plugin_delete_modal_body(app, &toggles)
    } else {
        skill_delete_modal_body(app, &toggles)
    };
    draw_panel(frame, "confirm delete", body, Color::Yellow);
}

fn plugin_delete_modal_body(app: &App, toggles: &str) -> String {
    let plans = &app.plugin_delete_plans_cache;
    if plans.is_empty() {
        return format!(
            "Nothing to uninstall.\n\
             \n\
             {toggles}\n\
             \n\
             Keys  j/k · Space · *=all · x=none · Esc=cancel"
        );
    }
    let filter = agents_label(&app.delete_agents);
    let mut lines = Vec::new();
    lines.push("Uninstall plugins".into());
    lines.push("────────────────────────".into());
    if plans.len() == 1 {
        lines.push(format!(
            "Target  {} ({})   agents: {filter}",
            plans[0].spec, plans[0].scope
        ));
    } else {
        lines.push(format!(
            "Target  {} plugins   agents: {filter}",
            plans.len()
        ));
    }
    lines.push(String::new());
    lines.push("Agents".into());
    for line in toggles.lines() {
        lines.push(format!("  {line}"));
    }
    lines.push(String::new());
    lines.push("Plugins".into());
    for plan in plans.iter().take(12) {
        lines.push(format!(
            "  · {}  {} [{}]",
            plan.spec,
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
    lines.push("Runs claude/copilot/codex plugin uninstall when a CLI exists.".into());
    lines.push("Cursor has no catalog CLI (install from the host marketplace).".into());
    lines.push("If every CLI call fails, inventory paths are removed as fallback.".into());
    lines.push(String::new());
    lines.push("Paths (fallback)".into());
    let mut path_count = 0usize;
    for plan in plans {
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
    lines.push(String::new());
    lines.push(
        "Keys  j/k=move · Space=toggle · *=all · x=none · [y]/Enter=uninstall · [n]/Esc".into(),
    );
    lines.join("\n")
}

fn skill_delete_modal_body(app: &App, toggles: &str) -> String {
    let plans = app.delete_plans();
    if plans.is_empty() {
        return format!(
            "Nothing to delete.\n\
             \n\
             {toggles}\n\
             \n\
             Keys  j/k · Space · *=all · x=none · Esc=cancel"
        );
    }
    let filter = agents_label(&app.delete_agents);
    let mut lines = Vec::new();
    lines.push("Delete skills".into());
    lines.push("────────────────────────".into());
    if plans.len() == 1 {
        lines.push(format!(
            "Target  {} ({})   agents: {filter}",
            plans[0].skill_name, plans[0].scope
        ));
    } else {
        lines.push(format!("Target  {} skills   agents: {filter}", plans.len()));
    }
    lines.push(String::new());
    lines.push("Agents".into());
    for line in toggles.lines() {
        lines.push(format!("  {line}"));
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
    for plan in plans {
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
    let shared_warns: Vec<&str> = plans
        .iter()
        .filter_map(|p| p.shared_warning.as_deref())
        .collect();
    let plugin_warns: Vec<&str> = plans
        .iter()
        .filter_map(|p| p.plugin_warning.as_deref())
        .collect();
    if !shared_warns.is_empty() {
        lines.push(String::new());
        lines.push(format!("Warning  {}", shared_warns[0]));
        if shared_warns.len() > 1 {
            lines.push(format!(
                "  (+{} more shared-path warnings)",
                shared_warns.len() - 1
            ));
        }
    }
    if !plugin_warns.is_empty() {
        lines.push(String::new());
        lines.push(format!("Warning  {}", plugin_warns[0]));
        if plugin_warns.len() > 1 {
            lines.push(format!(
                "  (+{} more plugin-path warnings)",
                plugin_warns.len() - 1
            ));
        }
    }
    lines.push(String::new());
    lines
        .push("Keys  j/k=move · Space=toggle · *=all · x=none · [y]/Enter=delete · [n]/Esc".into());
    lines.join("\n")
}

fn format_agent_toggles(selected: &[Agent], available: &[Agent], focus: usize) -> String {
    if available.is_empty() {
        return "  (no agents)".into();
    }
    available
        .iter()
        .enumerate()
        .map(|(i, agent)| {
            let cursor = if i == focus { '>' } else { ' ' };
            let mark = if selected.contains(agent) { 'x' } else { ' ' };
            format!("{cursor} [{mark}]  {}", agent.as_str())
        })
        .collect::<Vec<_>>()
        .join("\n")
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

/// Format one skill inventory row.
/// Columns: mark(3) name(16) scope(7) src(10) author(12) rate(6) score(5).
fn format_skill_list_row(
    mark: &str,
    name: &str,
    scope: &str,
    source: &str,
    author: &str,
    activation_rate: Option<f64>,
    delete_score: f64,
) -> String {
    let rate = format_rate_column(activation_rate);
    format!(
        "{mark} {:<16} {:7} {:<10} {:<12} {rate} {:>5.0}",
        truncate(name, 16),
        scope,
        truncate(source, 10),
        truncate(author, 12),
        delete_score
    )
}

fn format_rate_column(activation_rate: Option<f64>) -> String {
    activation_rate
        .map(|r| format!("{:>5.1}%", r * 100.0))
        .unwrap_or_else(|| format!("{:>6}", "n/a"))
}

/// Block title aligned with [`format_skill_list_row`] under [`LIST_HIGHLIGHT`] spacing.
fn skill_list_title(checked_count: usize) -> String {
    // Leading cells match LIST_HIGHLIGHT width so headers sit over row text
    // (HighlightSpacing::Always reserves that gutter for every row).
    let highlight_pad = " ".repeat(LIST_HIGHLIGHT.chars().count());
    let cols = format!(
        "{highlight_pad}{:<3} {:<16} {:7} {:<10} {:<12} {:>6} {:>5}",
        "", "NAME", "SCOPE", "SRC", "AUTHOR", "RATE", "SCORE"
    );
    if checked_count > 0 {
        format!("{cols}  ({checked_count} selected) ")
    } else {
        format!("{cols} ")
    }
}

fn format_plugin_list_row(
    mark: &str,
    name: &str,
    scope: &str,
    marketplace: &str,
    skills: usize,
    mcp: usize,
) -> String {
    format!(
        "{mark} {:<16} {:7} {:<16} {:>2} {:>3}",
        truncate(name, 16),
        scope,
        truncate(marketplace, 16),
        skills,
        mcp
    )
}

fn plugin_list_title(checked_count: usize) -> String {
    let highlight_pad = " ".repeat(LIST_HIGHLIGHT.chars().count());
    let cols = format!(
        "{highlight_pad}{:<3} {:<16} {:7} {:<16} {:>2} {:>3}",
        "", "NAME", "SCOPE", "MARKET", "SK", "MCP"
    );
    if checked_count > 0 {
        format!("{cols}  ({checked_count} selected) ")
    } else {
        format!("{cols} ")
    }
}

fn format_mcp_list_row(
    mark: &str,
    name: &str,
    transport: &str,
    plugin: &str,
    agents: &str,
) -> String {
    format!(
        "{mark} {:<16} {:<6} {:<16} {}",
        truncate(name, 16),
        truncate(transport, 6),
        truncate(plugin, 16),
        truncate(agents, 18)
    )
}

fn mcp_list_title(checked_count: usize) -> String {
    let highlight_pad = " ".repeat(LIST_HIGHLIGHT.chars().count());
    let cols = format!(
        "{highlight_pad}{:<3} {:<16} {:<6} {:<16} {}",
        "", "NAME", "TRANS", "PLUGIN", "AGENTS"
    );
    if checked_count > 0 {
        format!("{cols}  ({checked_count} selected) ")
    } else {
        format!("{cols} ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_list_headers_align_with_row_columns() {
        let highlight_pad = " ".repeat(LIST_HIGHLIGHT.chars().count());
        let row = format!(
            "{highlight_pad}{}",
            format_skill_list_row(
                "[ ]",
                "agent-reach",
                "user",
                "npx skills",
                "vercel-labs",
                Some(0.0),
                85.0
            )
        );
        // Same skeleton as the title, with data values in each column.
        let expected_header = format!(
            "{highlight_pad}{:<3} {:<16} {:7} {:<10} {:<12} {:>6} {:>5} ",
            "", "NAME", "SCOPE", "SRC", "AUTHOR", "RATE", "SCORE"
        );
        assert_eq!(skill_list_title(0), expected_header);

        // Column starts: name / scope / src / author / rate / score.
        let name_at = highlight_pad.len() + 4; // "[ ] "
        assert_eq!(&row[name_at..name_at + 11], "agent-reach");
        assert_eq!(&expected_header[name_at..name_at + 4], "NAME");

        let scope_at = name_at + 16 + 1;
        assert_eq!(&row[scope_at..scope_at + 4], "user");
        assert_eq!(&expected_header[scope_at..scope_at + 5], "SCOPE");

        let src_at = scope_at + 7 + 1;
        assert_eq!(&row[src_at..src_at + 10], "npx skills");
        assert_eq!(&expected_header[src_at..src_at + 3], "SRC");

        let author_at = src_at + 10 + 1;
        assert_eq!(&row[author_at..author_at + 11], "vercel-labs");
        assert_eq!(&expected_header[author_at..author_at + 6], "AUTHOR");

        let rate_at = author_at + 12 + 1;
        assert_eq!(&row[rate_at..rate_at + 6], "  0.0%");
        assert_eq!(&expected_header[rate_at..rate_at + 6], "  RATE");

        let score_at = rate_at + 6 + 1;
        assert_eq!(&row[score_at..score_at + 5], "   85");
        assert_eq!(&expected_header[score_at..score_at + 5], "SCORE");
    }

    #[test]
    fn skill_list_rate_column_is_fixed_width() {
        assert_eq!(format_rate_column(Some(0.012)).len(), 6);
        assert_eq!(format_rate_column(None).len(), 6);
        let with_rate =
            format_skill_list_row("[ ]", "a", "user", "gh skill", "-", Some(0.012), 10.0);
        let without = format_skill_list_row("[ ]", "a", "user", "gh skill", "-", None, 10.0);
        assert_eq!(with_rate.len(), without.len());
    }

    #[test]
    fn plugin_list_headers_align_with_row_columns() {
        let highlight_pad = " ".repeat(LIST_HIGHLIGHT.chars().count());
        let row = format!(
            "{highlight_pad}{}",
            format_plugin_list_row("[ ]", "context7", "user", "cursor-public", 1, 1)
        );
        let expected_header = format!(
            "{highlight_pad}{:<3} {:<16} {:7} {:<16} {:>2} {:>3} ",
            "", "NAME", "SCOPE", "MARKET", "SK", "MCP"
        );
        assert_eq!(plugin_list_title(0), expected_header);
        let name_at = highlight_pad.len() + 4;
        assert_eq!(&row[name_at..name_at + 8], "context7");
        assert_eq!(&expected_header[name_at..name_at + 4], "NAME");
    }

    #[test]
    fn mcp_list_headers_align_with_row_columns() {
        let highlight_pad = " ".repeat(LIST_HIGHLIGHT.chars().count());
        let row = format!(
            "{highlight_pad}{}",
            format_mcp_list_row("[ ]", "docs", "stdio", "context7", "claude-code")
        );
        let expected_header = format!(
            "{highlight_pad}{:<3} {:<16} {:<6} {:<16} {} ",
            "", "NAME", "TRANS", "PLUGIN", "AGENTS"
        );
        assert_eq!(mcp_list_title(0), expected_header);
        let name_at = highlight_pad.len() + 4;
        assert_eq!(&row[name_at..name_at + 4], "docs");
        assert_eq!(&expected_header[name_at..name_at + 4], "NAME");
    }
}
