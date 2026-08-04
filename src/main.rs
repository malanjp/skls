use anyhow::{Context, Result};
use clap::Parser;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use skillui::app::App;
use skillui::ui::draw;
use std::io::{self, stdout};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Parser)]
#[command(name = "skillui", about = "TUI for managing agent skills")]
struct Cli {
    /// Project root for project-scope skills (default: cwd)
    #[arg(long)]
    project_root: Option<PathBuf>,

    /// Activation window in days
    #[arg(long, default_value_t = 30)]
    window_days: i64,

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
    app.reload().context("initial inventory load")?;

    if cli.dump_json {
        println!("{}", serde_json::to_string_pretty(&dump_records(&app))?);
        return Ok(());
    }

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
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    app.handle_key(key)?;
                }
            }
        }
        if app.should_quit {
            break;
        }
    }
    Ok(())
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
