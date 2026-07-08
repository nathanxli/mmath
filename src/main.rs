mod game;
mod model;
mod results;
mod setup;

use std::env;
use std::error::Error;
use std::io;

use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::game::run_game;
use crate::model::App;
use crate::results::{RecentAttempt, ResultsAction, run_results};
use crate::setup::run_setup;

fn main() -> Result<(), Box<dyn Error>> {
    let use_small_text = env::args().any(|arg| arg == "-s");
    let mut terminal = init_terminal()?;
    let result = run(&mut terminal, use_small_text);
    restore_terminal(&mut terminal)?;

    match result {
        Ok(Some(_)) => Ok(()),
        Ok(None) => {
            println!("Canceled.");
            Ok(())
        }
        Err(err) => Err(err),
    }
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    use_small_text: bool,
) -> Result<Option<App>, Box<dyn Error>> {
    let setup = match run_setup(terminal)? {
        Some(config) => config,
        None => return Ok(None),
    };

    let mut game_config = setup.game;
    let mut duration = setup.duration;
    let mut recent_attempts: Vec<RecentAttempt> = Vec::new();

    loop {
        let app = run_game(terminal, game_config.clone(), duration, use_small_text)?;
        recent_attempts.push(RecentAttempt {
            score: app.score,
        });
        if recent_attempts.len() > 10 {
            let overflow = recent_attempts.len() - 10;
            recent_attempts.drain(0..overflow);
        }

        match run_results(terminal, &app, &recent_attempts)? {
            ResultsAction::Restart => {
                game_config = app.config.clone();
                duration = app.duration;
            }
            ResultsAction::Exit => return Ok(Some(app)),
        }
    }
}

fn init_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>, Box<dyn Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

fn restore_terminal(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<(), Box<dyn Error>> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}
