use anyhow::{Context, Result};
use clap::Parser;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use skls::adapters::command::SystemCommandRunner;
use skls::analytics::AnalyzeLimits;
use skls::app::{App, PendingAction};
use skls::executor;
use skls::ui::draw;
use std::io::{self, stdout};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Parser)]
#[command(name = "skls", about = "TUI for listing and managing agent skills")]
struct Cli {
    /// Project root for project-scope skills (default: cwd)
    #[arg(long)]
    project_root: Option<PathBuf>,

    /// Activation window in days
    #[arg(long, default_value_t = 30)]
    window_days: i64,

    /// Max session files per agent when computing activations (newest first)
    #[arg(long, default_value_t = 80)]
    max_sessions: usize,

    /// Max bytes to read from each session file
    #[arg(long, default_value_t = 262_144)]
    max_bytes: u64,

    /// Disable session/byte caps (slow on large log trees)
    #[arg(long)]
    full_scan: bool,

    /// Skip launching TUI and print inventory as JSON
    #[arg(long)]
    dump_json: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let project_root = match cli.project_root {
        Some(p) => p,
        None => std::env::current_dir().context("current_dir")?,
    };
    let home = dirs_home().context("HOME not set")?;

    let mut app = App::new(project_root, home);
    app.window_days = cli.window_days;
    app.analyze_limits = if cli.full_scan {
        AnalyzeLimits::unlimited()
    } else {
        AnalyzeLimits {
            max_files_per_agent: cli.max_sessions,
            max_bytes_per_file: cli.max_bytes,
        }
    };

    if cli.dump_json {
        // JSON dump needs stats in one shot.
        app.reload().context("inventory load")?;
        println!("{}", serde_json::to_string_pretty(&dump_records(&app))?);
        return Ok(());
    }

    // Show the list ASAP; activation analysis runs after the first paint.
    app.bootstrap_fast().context("initial inventory load")?;

    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(out);
    let mut terminal = Terminal::new(backend)?;

    let result = run_loop(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn run_loop(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| draw(f, app))?;

        // Run deferred work after a redraw so the Busy modal is visible first.
        if let Some(action) = app.pending_action.take() {
            run_pending_action(terminal, app, action)?;
            continue;
        }

        if event::poll(Duration::from_millis(250))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            app.handle_key(key)?;
        }
        if app.should_quit {
            break;
        }
    }
    Ok(())
}

fn run_pending_action(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    action: PendingAction,
) -> Result<()> {
    let runner = SystemCommandRunner;
    executor::run_pending_action(app, action, &runner, &mut |app| {
        terminal.draw(|f| draw(f, app))?;
        Ok(())
    })
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn dump_records(app: &App) -> Vec<serde_json::Value> {
    app.skills
        .iter()
        .map(|s| {
            serde_json::json!({
                "id": s.id,
                "name": s.name,
                "scope": s.scope.as_str(),
                "agents": s.agents.iter().map(|a| a.as_str()).collect::<Vec<_>>(),
                "source": s.source.as_str(),
                "activation_rate": s.stats.activation_rate,
                "delete_score": s.stats.delete_score,
                "hits": s.stats.hits,
                "sessions_total": s.stats.sessions_total,
            })
        })
        .collect()
}
