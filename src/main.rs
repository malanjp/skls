use anyhow::{Context, Result};
use clap::parser::ValueSource;
use clap::{CommandFactory, FromArgMatches, Parser};
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
use skls::config::LoadedConfig;
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
    let matches = Cli::command().get_matches();
    let cli = Cli::from_arg_matches(&matches).context("cli")?;
    let project_root = match cli.project_root.clone() {
        Some(p) => p,
        None => std::env::current_dir().context("current_dir")?,
    };
    let home = dirs_home().context("HOME not set")?;

    let loaded = skls::config::load_config(&skls::config::default_config_path(&home), &home);
    let mut app = App::new(project_root, home);
    apply_config(&mut app, &loaded, &cli, &matches);

    if cli.dump_json {
        // JSON dump needs stats in one shot.
        app.reload().context("inventory load")?;
        println!("{}", serde_json::to_string_pretty(&app.dump_json_value())?);
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

fn apply_config(app: &mut App, loaded: &LoadedConfig, cli: &Cli, matches: &clap::ArgMatches) {
    app.scan_roots =
        skls::config::resolve_scan_roots(&loaded.projects, &app.project_root, &app.home);
    app.config_project_count = loaded.projects.len();
    app.config_warnings.extend(loaded.warnings.iter().cloned());

    app.window_days = pick_value(matches, "window_days", loaded.window_days, cli.window_days);
    if cli.full_scan {
        app.analyze_limits = AnalyzeLimits::unlimited();
    } else {
        app.analyze_limits = AnalyzeLimits {
            max_files_per_agent: pick_value(
                matches,
                "max_sessions",
                loaded.max_sessions,
                cli.max_sessions,
            ),
            max_bytes_per_file: pick_value(matches, "max_bytes", loaded.max_bytes, cli.max_bytes),
        };
    }
}

fn pick_value<T: Copy>(
    matches: &clap::ArgMatches,
    id: &str,
    from_config: Option<T>,
    from_cli: T,
) -> T {
    if matches.value_source(id) == Some(ValueSource::CommandLine) {
        from_cli
    } else {
        from_config.unwrap_or(from_cli)
    }
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}
